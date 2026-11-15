use rouge_reactive::{Runtime, effect, memo, signal};
use std::hint::black_box;
use std::time::{Duration, Instant};

fn main() {
    let _entered = Runtime::new().enter();
    let reads = best(|| {
        let value = signal(1u64);
        let start = Instant::now();
        let mut sum = 0;
        for _ in 0..1_000_000 {
            sum += value.get_untracked();
        }
        black_box(sum);
        start.elapsed()
    });
    let tracked = best(|| {
        let value = signal(1u64);
        let derived = memo(move || {
            let mut sum = 0;
            for _ in 0..1_000_000 {
                sum += value.get();
            }
            sum
        });
        let start = Instant::now();
        black_box(derived.get_untracked());
        start.elapsed()
    });
    let fan_out = best(|| {
        let source = signal(0u64);
        for _ in 0..10_000 {
            effect(move || {
                black_box(source.get());
            });
        }
        let start = Instant::now();
        for value in 1..=10 {
            source.set(value);
        }
        start.elapsed() / 10
    });
    println!("1M untracked get: {reads:?}");
    println!("1M tracked get in one memo: {tracked:?}");
    println!("10k-effect fan-out write: {fan_out:?}");
}

fn best(mut run: impl FnMut() -> Duration) -> Duration {
    (0..5).map(|_| run()).min().unwrap_or_default()
}
