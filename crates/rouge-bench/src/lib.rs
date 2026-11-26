#![cfg(not(target_arch = "wasm32"))]

pub mod bench;
pub mod error;
mod heap;
pub mod scenarios;
