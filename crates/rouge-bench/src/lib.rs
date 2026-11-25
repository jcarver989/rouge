#![cfg(not(target_arch = "wasm32"))]

pub mod bench;
pub mod builds;
pub mod compare;
pub mod error;
pub mod heap;
pub mod metrics;
pub mod passes;
#[cfg(all(unix, feature = "profile"))]
pub mod sampler;
pub mod scale;
pub mod scenarios;
pub mod stacks;
