#[cfg(not(target_arch = "wasm32"))]
mod cli;
#[cfg(not(target_arch = "wasm32"))]
mod compare;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> std::process::ExitCode {
    cli::main().unwrap_or_else(|error| {
        eprintln!("rouge-bench: {error}");
        std::process::ExitCode::FAILURE
    })
}

#[cfg(target_arch = "wasm32")]
fn main() {}
