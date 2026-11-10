use crate::runtime::{Graph, NodeId, scoped, try_with_rt, with_rt};
use slotmap::{SlotMap, new_key_type};
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::marker::PhantomData;
use std::mem::take;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Wake, Waker};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

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

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Task {
    id: TaskId,
    marker: PhantomData<*const ()>,
}

pub fn spawn(future: impl Future<Output = ()> + 'static) -> Task {
    Task {
        id: with_rt(|rt| rt.spawn(Box::pin(future))),
        marker: PhantomData,
    }
}

pub fn poll_tasks() -> bool {
    with_rt(Graph::poll_tasks)
}

impl Task {
    pub fn cancel(&self) {
        try_with_rt(|rt| rt.cancel_task(self.id));
    }

    pub fn is_finished(&self) -> bool {
        try_with_rt(|rt| rt.is_task_finished(self.id)).unwrap_or(true)
    }
}
