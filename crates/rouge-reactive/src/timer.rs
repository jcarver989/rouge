use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::OnceLock;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::task::{Context, Poll, Waker};
use std::thread;
use std::time::{Duration, Instant};

type Entry = (Instant, Waker);

static TIMER: OnceLock<Sender<Entry>> = OnceLock::new();

fn timer() -> &'static Sender<Entry> {
    TIMER.get_or_init(|| {
        let (tx, rx) = channel();
        thread::spawn(move || run(rx));
        tx
    })
}

fn run(rx: Receiver<Entry>) {
    let mut queue = BTreeMap::<(Instant, u64), Waker>::new();
    let mut sequence = 0u64;
    loop {
        let now = Instant::now();
        while let Some(entry) = queue.first_entry().filter(|e| e.key().0 <= now) {
            entry.remove().wake();
        }
        let received = match queue.first_key_value() {
            Some(((deadline, _), _)) => rx.recv_timeout(deadline.saturating_duration_since(now)),
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match received {
            Ok((deadline, waker)) => {
                queue.insert((deadline, sequence), waker);
                sequence += 1;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

#[must_use = "futures do nothing unless awaited"]
pub struct Sleep {
    deadline: Instant,
}

pub fn sleep(duration: Duration) -> Sleep {
    Sleep {
        deadline: Instant::now() + duration,
    }
}

impl Future for Sleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if Instant::now() >= self.deadline {
            return Poll::Ready(());
        }
        assert!(
            timer().send((self.deadline, cx.waker().clone())).is_ok(),
            "rouge-reactive: the timer thread stopped unexpectedly"
        );
        Poll::Pending
    }
}
