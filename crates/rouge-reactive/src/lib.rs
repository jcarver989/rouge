#[cfg(not(target_arch = "wasm32"))]
mod blocking;
mod effect;
mod emitter;
mod handle;
mod interval;
mod memo;
mod resource;
mod runtime;
mod scope;
mod sender;
mod signal;
mod stats;
mod stored;
mod task;
#[cfg(not(target_arch = "wasm32"))]
pub mod testing;
#[cfg_attr(target_arch = "wasm32", path = "timer_web.rs")]
mod timer;
mod watch;

#[cfg(not(target_arch = "wasm32"))]
pub use blocking::background;
pub use effect::{Effect, effect};
pub use emitter::{Emitter, emitter};
pub use interval::interval;
pub use memo::{Memo, memo};
pub use resource::{Resource, resource};
pub use runtime::{Entered, Runtime};
pub use scope::{
    Scope, batch, expect_context, on_cleanup, provide_context, run_in, untrack, use_context,
};
pub use sender::SignalSender;
pub use signal::{ReadSignal, Signal, signal};
pub use stats::{ReactiveStats, stats};
pub use stored::{Callback, Stored, stored};
pub use task::{Task, TaskError, poll_tasks, spawn};
#[cfg(feature = "tokio-rt")]
pub use task::{set_tokio, spawn_blocking, spawn_tokio};
pub use timer::{Sleep, sleep};
pub use watch::watch;
