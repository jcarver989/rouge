# rouge-framer

`rouge-framer` generates a GitHub Actions publishing workflow, a `cargo rouge` command for packaging your app, and a `PUBLISHING.md` setup guide.

## Setup

1. Install Framer with `cargo install cargo-framer`. If your repository has no `.framer/` directory, run `cargo framer init` from the root.
2. In `.framer/Cargo.toml`, set `framer = "0.1.3"` and add `rouge-framer` under `[dependencies]`. Do not add it to your app's manifest.
3. Apply `RougePublish` in `.framer/src/main.rs`. Call `rouge_framer::run(&project)` instead of `project.synth()` so the entry point can also handle `cargo rouge` commands.

```rust,no_run
use framer::Project;
use rouge_framer::{GitHubPages, GitHubReleases, RougePublish};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut project = Project::new();
    project.apply(
        RougePublish::new("com.example.myapp")
            .web(GitHubPages)
            .linux(GitHubReleases),
    );
    rouge_framer::run(&project)
}
```

Run `cargo framer` from the repository root and commit the generated files. Follow `PUBLISHING.md` to test and publish on GitHub. To build locally, run `cargo rouge build web` or replace `web` with another enabled target. Run `cargo framer --check` in CI to catch stale generated files.

## Targets

| Configuration | Publishes to |
| --- | --- |
| `.web(GitHubPages)` | GitHub Pages |
| `.linux(GitHubReleases)` | GitHub Releases (x86-64 Linux) |
| `.macos(GitHubReleases)` | GitHub Releases (Apple Silicon) |
| `.ios(TestFlight::new("1234567890"))` | TestFlight and the App Store. Use your app's numeric Apple ID from App Store Connect. |
| `.android(PlayInternal)` | Google Play internal testing and production |

Use `.desktop(GitHubReleases)` to enable both Linux and macOS. For iOS, set `.icon("assets/app-icon.png")` to an opaque 1024 × 1024 PNG. For Android, provide a `cdylib` that exports Rouge's `android_main` entry point.

If the workspace has more than one default package, select one with `.package(...)`. Use `.binary(...)` or `.android_library(...)` if the selected package has multiple targets of the required kind.

Pass a platform builder to change its defaults. For example, `.linux(Linux::new(GitHubReleases).apt_packages(["libasound2-dev"]))` adds a Linux build dependency. Every platform builder accepts `runner`, `timeout_minutes`, and `setup`.
