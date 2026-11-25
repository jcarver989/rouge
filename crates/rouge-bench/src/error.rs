use rouge::core::testing::TestError;
use rouge::render::headless::EncodeError;
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
    #[error("{0} scenarios match; name exactly one of: {1}")]
    Ambiguous(usize, String),
    #[error("a scale needs at least two different sizes above zero")]
    TooFewSizes,
    #[error("the scenario measured before it mounted a view")]
    NotMounted,
    #[error(
        "no merge base with main or origin/main; fetch main and its history or specify --base <revision>"
    )]
    NoMergeBase,
    #[error("`{0}` is neither a saved build nor a git revision")]
    UnknownBuild(String),
    #[error("`{0}` is not a name for a saved build; use letters, digits, `-`, `_` and `.`")]
    BadName(String),
    #[error(
        "{0} did not build with the working tree's scenarios; if they use an API it lacks, check it out and `save` a build there instead"
    )]
    BuildFailed(String),
    #[error("`{command}` failed: {stderr}")]
    Command { command: String, stderr: String },
    #[error("the {build} build failed to run {case}: {stderr}")]
    Pass {
        build: String,
        case: String,
        stderr: String,
    },
    #[error("profiling is not compiled in; use `mise run perf:profile` or `mise run perf:heap`")]
    NoProfiler,
    #[error("the sampler failed: {0}")]
    Sampler(String),
    #[error("no measured frame was sampled; try a larger `--seconds`")]
    NoSamples,
    #[error(transparent)]
    Ui(#[from] TestError),
    #[error("failed to encode an image: {0}")]
    Image(#[from] EncodeError),
    #[error("failed to read or write: {0}")]
    Io(#[from] IoError),
    #[error("a report did not parse: {0}")]
    Json(#[from] JsonError),
}
