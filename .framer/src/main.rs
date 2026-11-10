use framer::Project;
use framer::rust::CargoWorkspace;
use framer_presets::{Rust, RustMonorepo, Workspace};
use std::mem;
use std::process::ExitCode;

const ALLOWED_PEDANTIC_LINTS: [&str; 8] = [
    "cast_possible_truncation",
    "cast_precision_loss",
    "cast_sign_loss",
    "float_cmp",
    "needless_pass_by_value",
    "too_many_lines",
    "unnecessary_wraps",
    "unreadable_literal",
];

fn main() -> ExitCode {
    let mut project = Project::new();
    project.apply(
        RustMonorepo::new()
            .rust(Rust::new("1.99").toolchain("1.99.1"))
            .workspace(Workspace::new().license("MIT OR Apache-2.0"))
            .without_conventions(),
    );
    if let Some(workspace) = project.get_mut::<CargoWorkspace>() {
        let lints = workspace.lints_mut();
        *lints = mem::take(lints).allow_clippy(ALLOWED_PEDANTIC_LINTS);
    }
    project.synth()
}
