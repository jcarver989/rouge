use framer::Project;
use framer::github::Step;
use framer::rust::CargoWorkspace;
use framer::toml_edit::Array;
use framer_presets::{Ci, Release, ReleasePlz, Rust, RustMonorepo, Tools, Workspace};
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

const TEST_ON_LINUX: &str = "github.job == 'test' && runner.os == 'Linux'";

fn main() -> ExitCode {
    let mut project = Project::new();
    project.apply(
        RustMonorepo::new()
            .rust(Rust::new("1.100").targets([
                "aarch64-apple-ios",
                "aarch64-apple-ios-sim",
                "aarch64-linux-android",
            ]))
            .workspace(
                Workspace::new()
                    .license("MIT")
                    .members(["crates/*", "crates/rouge-framer/tests/fixtures/mobile"]),
            )
            .tools(Tools::new().mise_toml())
            .ci(Ci::new()
                .setup(install_system_libraries())
                .setup(
                    Step::run("sudo apt-get install -y libvulkan1 mesa-vulkan-drivers")
                        .name("Install Vulkan software rendering")
                        .condition(TEST_ON_LINUX),
                )
                .setup(
                    Step::run(
                        "sudo apt-get install -y xvfb && (Xvfb :99 -screen 0 1280x1024x24 &) && echo \"DISPLAY=:99\" >> \"$GITHUB_ENV\"",
                    )
                    .name("Start a virtual X display")
                    .condition(TEST_ON_LINUX),
                ))
            .release(
                Release::new().release_plz(
                    ReleasePlz::new()
                        .publish_to_crates_io()
                        .github_token()
                        .setup(install_system_libraries()),
                ),
            )
            .without_conventions(),
    );
    if let Some(workspace) = project.get_mut::<CargoWorkspace>() {
        let lints = workspace.lints_mut();
        *lints = mem::take(lints).allow_clippy(ALLOWED_PEDANTIC_LINTS);
    }
    project.toml("mise.toml").set(
        "tasks.test-ci.run",
        Array::from_iter([
            "cargo test --locked -p rouge-syntax --no-default-features --test features_test",
            "cargo test --locked -p rouge-syntax --no-default-features --features lang-rust --test features_test",
            "cargo test --locked -p rouge-syntax --no-default-features --features lang-sql --test features_test",
        ]),
    );
    project.synth()
}

fn install_system_libraries() -> Step {
    Step::run(
        "sudo apt-get update && sudo apt-get install -y libfontconfig-dev libwebkit2gtk-4.1-dev",
    )
    .name("Install Fontconfig and WebKitGTK development files")
}
