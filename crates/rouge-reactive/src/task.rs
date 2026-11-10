use crate::runtime::{Graph, NodeId, scoped, try_with_rt, with_rt};
use slotmap::{SlotMap, new_key_type};
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::marker::PhantomData;
use std::mem::take;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll, Wake, Waker};
use thiserror::Error;
#[cfg(feature = "tokio-rt")]
use tokio::runtime::Handle as TokioHandle;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::sync::oneshot::{Receiver, channel};
#[cfg(feature = "tokio-rt")]
use tokio::task::AbortHandle;

new_key_type! {
    struct TaskId;
}

pub(crate) type Job = Box<dyn FnOnce() + Send>;
pub(crate) type Hook = Box<dyn Fn() + Send + Sync>;
type LocalFuture = Pin<Box<dyn Future<Output = ()>>>;

enum Message {
    Wake(TaskId),
    Job(Job),
}

pub(crate) struct Mailbox {
    tx: UnboundedSender<Message>,
    hook: Option<Hook>,
}

impl Mailbox {
    fn send(&self, message: Message) {
        if self.tx.send(message).is_ok()
            && let Some(hook) = &self.hook
        {
            hook();
        }
    }

    pub(crate) fn post(&self, job: Job) {
        self.send(Message::Job(job));
    }
}

struct TaskWaker {
    id: TaskId,
    queued: AtomicBool,
    mailbox: Arc<Mailbox>,
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        if !self.queued.swap(true, Ordering::AcqRel) {
            self.mailbox.send(Message::Wake(self.id));
        }
    }
}

struct Entry {
    future: Option<LocalFuture>,
    owner: Option<NodeId>,
    waker: Arc<TaskWaker>,
}

pub(crate) struct Executor {
    tasks: RefCell<SlotMap<TaskId, Entry>>,
    rx: RefCell<UnboundedReceiver<Message>>,
    mailbox: Arc<Mailbox>,
    polling: Cell<bool>,
}

impl Default for Executor {
    fn default() -> Self {
        Executor::new(None)
    }
}

impl Executor {
    pub(crate) fn new(hook: Option<Hook>) -> Self {
        let (tx, rx) = unbounded_channel();
        Executor {
            tasks: RefCell::default(),
            rx: RefCell::new(rx),
            mailbox: Arc::new(Mailbox { tx, hook }),
            polling: Cell::new(false),
        }
    }
}

impl Graph {
    pub(crate) fn mailbox(&self) -> Arc<Mailbox> {
        self.executor.mailbox.clone()
    }

    fn spawn(&self, future: LocalFuture) -> TaskId {
        let owner = self.current_owner();
        let mailbox = self.mailbox();
        let mut waker = None;
        let id = self.executor.tasks.borrow_mut().insert_with_key(|id| {
            let task_waker = Arc::new(TaskWaker {
                id,
                queued: AtomicBool::new(false),
                mailbox,
            });
            waker = Some(task_waker.clone());
            Entry {
                future: Some(future),
                owner,
                waker: task_waker,
            }
        });
        self.add_cleanup(Box::new(move || {
            try_with_rt(|rt| rt.cancel_task(id));
        }));
        if let Some(waker) = waker {
            waker.wake();
        }
        id
    }

    fn cancel_task(&self, id: TaskId) {
        let entry = self.executor.tasks.borrow_mut().remove(id);
        drop(entry);
    }

    fn is_task_finished(&self, id: TaskId) -> bool {
        !self.executor.tasks.borrow().contains_key(id)
    }

    pub(crate) fn clear_tasks(&self) {
        let tasks = take(&mut *self.executor.tasks.borrow_mut());
        drop(tasks);
    }

    pub(crate) fn poll_tasks(&self) -> bool {
        if self.executor.polling.get() {
            return false;
        }
        let _polling = scoped(&self.executor.polling, true);
        let mut ran = false;
        loop {
            let message = self.executor.rx.borrow_mut().try_recv();
            match message {
                Ok(Message::Job(job)) => {
                    job();
                    ran = true;
                }
                Ok(Message::Wake(id)) => ran |= self.poll_task(id),
                Err(_) => return ran,
            }
        }
    }

    fn poll_task(&self, id: TaskId) -> bool {
        let taken = self
            .executor
            .tasks
            .borrow_mut()
            .get_mut(id)
            .and_then(|entry| Some((entry.future.take()?, entry.owner, entry.waker.clone())));
        let Some((mut future, owner, task_waker)) = taken else {
            return false;
        };
        task_waker.queued.store(false, Ordering::Release);
        let waker = Waker::from(task_waker);
        let mut cx = Context::from_waker(&waker);
        let done =
            self.batch(|| self.with_owner(owner, || future.as_mut().poll(&mut cx).is_ready()));
        let finished = {
            let mut tasks = self.executor.tasks.borrow_mut();
            match tasks.get_mut(id) {
                Some(entry) if !done => {
                    entry.future = Some(future);
                    None
                }
                _ => {
                    tasks.remove(id);
                    Some(future)
                }
            }
        };
        drop(finished);
        true
    }
}

#[derive(Debug, Error)]
pub enum TaskError {
    #[error("task was cancelled before completing")]
    Cancelled,
    #[cfg(feature = "tokio-rt")]
    #[error("Tokio task failed: {0}")]
    Tokio(#[from] tokio::task::JoinError),
}

pub struct Task<T = ()> {
    id: TaskId,
    result: Receiver<Result<T, TaskError>>,
    #[cfg(feature = "tokio-rt")]
    abort: Option<AbortHandle>,
    marker: PhantomData<*const ()>,
}

pub fn spawn<T: 'static>(future: impl Future<Output = T> + 'static) -> Task<T> {
    let (sender, result) = channel();
    Task {
        id: with_rt(|rt| {
            rt.spawn(Box::pin(async move {
                let _ = sender.send(Ok(future.await));
            }))
        }),
        result,
        #[cfg(feature = "tokio-rt")]
        abort: None,
        marker: PhantomData,
    }
}

#[cfg(feature = "tokio-rt")]
#[must_use = "Tokio bridge tasks abort when dropped; await or retain the task"]
pub fn spawn_on<T: Send + 'static>(
    handle: TokioHandle,
    future: impl Future<Output = T> + Send + 'static,
) -> Task<T> {
    let join = handle.spawn(future);
    let abort = join.abort_handle();
    let guard = AbortOnDrop(abort.clone());
    let (sender, result) = channel();
    let id = with_rt(|rt| {
        rt.spawn(Box::pin(async move {
            let _guard = guard;
            let _ = sender.send(join.await.map_err(TaskError::from));
        }))
    });
    Task {
        id,
        result,
        abort: Some(abort),
        marker: PhantomData,
    }
}

#[cfg(feature = "tokio-rt")]
struct AbortOnDrop(AbortHandle);

#[cfg(feature = "tokio-rt")]
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub fn poll_tasks() -> bool {
    with_rt(Graph::poll_tasks)
}

impl<T> Task<T> {
    pub fn cancel(&self) {
        #[cfg(feature = "tokio-rt")]
        if let Some(abort) = &self.abort {
            abort.abort();
        }
        try_with_rt(|rt| rt.cancel_task(self.id));
    }

    pub fn is_finished(&self) -> bool {
        try_with_rt(|rt| rt.is_task_finished(self.id)).unwrap_or(true)
    }
}

impl<T> Future for Task<T> {
    type Output = Result<T, TaskError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.result)
            .poll(cx)
            .map(|result| result.unwrap_or(Err(TaskError::Cancelled)))
    }
}

impl<T> Drop for Task<T> {
    fn drop(&mut self) {
        #[cfg(feature = "tokio-rt")]
        if self.abort.is_some() {
            self.cancel();
        }
    }
}
