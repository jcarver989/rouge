use framer::Project;
use framer::github::Step;
use framer::rust::CargoWorkspace;
use framer_presets::{Release, ReleasePlz, Rust, RustMonorepo, Workspace};
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
            .rust(Rust::new("1.100").targets(["aarch64-apple-ios", "aarch64-apple-ios-sim"]))
            .workspace(Workspace::new().license("MIT"))
            .release(
                Release::new().release_plz(
                    ReleasePlz::new()
                        .publish_to_crates_io()
                        .github_token()
                        .setup(
                            Step::run(
                                "sudo apt-get update && sudo apt-get install -y libfontconfig-dev",
                            )
                            .name("Install Fontconfig development files"),
                        ),
                ),
            )
            .without_conventions(),
    );
    if let Some(workspace) = project.get_mut::<CargoWorkspace>() {
        let lints = workspace.lints_mut();
        *lints = mem::take(lints).allow_clippy(ALLOWED_PEDANTIC_LINTS);
    }
    project.synth()
}
