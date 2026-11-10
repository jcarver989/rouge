use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, Thread};
use tokio::sync::oneshot::channel;

pub fn background<T: Send + 'static>(
    future: impl Future<Output = T> + Send + 'static,
) -> impl Future<Output = T> + Send {
    let (tx, rx) = channel();
    thread::spawn(move || {
        let _ = tx.send(block_on(future));
    });
    async move {
        rx.await.unwrap_or_else(|_| {
            panic!("rouge-reactive: a `background` thread panicked before producing its result")
        })
    }
}

struct Unpark(Thread);

impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

fn block_on<T>(future: impl Future<Output = T>) -> T {
    let mut future = pin!(future);
    let waker = Waker::from(Arc::new(Unpark(thread::current())));
    let mut cx = Context::from_waker(&waker);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => return value,
            Poll::Pending => thread::park(),
        }
    }
}
