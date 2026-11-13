use crate::signal::{ReadSignal, signal};
use crate::task::spawn;
use crate::timer::sleep;
use std::time::Duration;

#[track_caller]
pub fn interval(period: Duration) -> ReadSignal<u64> {
    let ticks = signal(0);
    spawn(async move {
        loop {
            sleep(period).await;
            if ticks.try_update(|ticks| *ticks += 1).is_none() {
                return;
            }
        }
    });
    ticks.read_only()
}
