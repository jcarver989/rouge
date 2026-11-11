use gloo_timers::future::TimeoutFuture;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use web_time::Instant;

#[must_use = "futures do nothing unless awaited"]
pub struct Sleep {
    deadline: Instant,
    timer: Option<TimeoutFuture>,
}

pub fn sleep(duration: Duration) -> Sleep {
    Sleep {
        deadline: Instant::now() + duration,
        timer: None,
    }
}

impl Future for Sleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        loop {
            let remaining = self.deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                self.timer = None;
                return Poll::Ready(());
            }
            let timer = self.timer.get_or_insert_with(|| {
                let ms = remaining.as_nanos().div_ceil(1_000_000);
                TimeoutFuture::new(ms.min(i32::MAX as u128) as u32)
            });
            if Pin::new(timer).poll(cx).is_pending() {
                return Poll::Pending;
            }
            self.timer = None;
        }
    }
}
