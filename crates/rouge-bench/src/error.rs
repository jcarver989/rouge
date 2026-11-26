use rouge::core::testing::TestError;
use serde_json::Error as JsonError;
use std::io::Error as IoError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BenchError {
    #[error("no scenario matches {0:?}; `rouge-bench list` names them")]
    NoScenario(String),
    #[error("`{0}` has no size; only scenarios listed with `@<size>` take one")]
    Unsized(String),
    #[error("`{0}` is not a size; write it as `rows_swap@1000`")]
    BadSize(String),
    #[error("{name}@{size} needs a size of at least {minimum}")]
    BelowMinimum {
        name: &'static str,
        size: usize,
        minimum: usize,
    },
    #[error("the scenario measured before mounting a view")]
    NotMounted,
    #[error("invalid benchmark report: {0}")]
    Report(String),
    #[error("worker failed for {0}")]
    Worker(String),
    #[error(transparent)]
    Ui(#[from] TestError),
    #[error("failed to read or write: {0}")]
    Io(#[from] IoError),
    #[error("a report did not parse: {0}")]
    Json(#[from] JsonError),
}
