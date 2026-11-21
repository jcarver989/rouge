# rouge-framer

`rouge-framer` generates a GitHub Actions publishing workflow, a `cargo rouge` command for running and packaging your app, and a `PUBLISHING.md` setup guide.

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

Run `cargo framer` from the repository root and commit the generated files. For mobile targets these include native projects in `ios/` and `android/` whose builds call cargo, so Xcode, Android Studio and Gradle work on them directly. Follow `PUBLISHING.md` to test and publish on GitHub. The `check` channel builds without release credentials; mobile store setup, signing gates, and which upload/promotion steps remain manual are documented in `PUBLISHING.md` under iOS and Android. To build locally, run `cargo rouge build web` or replace `web` with another enabled target. Run `cargo framer --check` in CI to catch stale generated files.

## Targets

| Configuration | Publishes to |
| --- | --- |
| `.web(GitHubPages)` | GitHub Pages |
| `.linux(GitHubReleases)` | GitHub Releases (x86-64 Linux) |
| `.macos(GitHubReleases)` | GitHub Releases (Apple Silicon) |
| `.ios(TestFlight::new("1234567890"))` | `TestFlight` and the App Store. Use your app's numeric Apple ID from App Store Connect. |
| `.android(PlayInternal)` | Google Play internal testing and production |

Use `.desktop(GitHubReleases)` to enable both Linux and macOS; each can be selected independently in the workflow. For iOS, set `.icon("assets/app-icon.png")` to an opaque 1024 × 1024 PNG. For Android, provide a `cdylib` that exports Rouge's `android_main` entry point.

If the workspace has more than one default package, select one with `.package(...)`. Use `.binary(...)` or `.android_library(...)` if the selected package has multiple targets of the required kind.

Pass a platform builder to change its defaults. For example, `.linux(Linux::new(GitHubReleases).apt_packages(["libasound2-dev"]))` adds a Linux build dependency. Every platform builder accepts `runner`, `timeout_minutes`, and `setup`.

## Run on the iOS Simulator

Apps with `.ios(...)` get a `just ios-sim` recipe after Framer synthesis. Use an Apple Silicon Mac with full Xcode selected and a compatible iOS simulator runtime installed in Xcode.

```sh
just ios-sim
ROUGE_IOS_DEVICE="iPhone 17 Pro" just ios-sim
cargo rouge run ios --device "iPhone 17 Pro"  # without Just
```

The command builds the `ios/` project for the simulator with `xcodebuild` (output in `.rouge/out/ios-sim/`), opens Simulator, then installs and launches the app. By default it prefers a compatible booted simulator, then the newest compatible runtime. Use a name or UDID to override the selection; use a UDID if the name is ambiguous (`xcrun simctl list devices available`). No Apple account, valid App Store Connect ID, or distribution signing is needed. `cargo rouge build ios` still builds a separate device package.

## Run on Android

Set `ANDROID_HOME`, install the Android SDK command-line tools and JDK, and connect an arm64 device or emulator with debugging enabled.

```sh
cargo rouge setup android
cargo rouge run android --device <adb-serial>
```

You can omit `--device` when exactly one device is online, or set `ANDROID_SERIAL`. This runs the `android/` project's `assembleDebug`, then installs and launches the debug APK; it does not sign or publish a Play release.

## Promote a tested mobile build

`preview` uploads a signed build to App Store Connect or Play internal testing. Wait for store processing and test that build, then select the same build in the store console to submit and release it. The mobile jobs do not run for `production`, so nothing is rebuilt or uploaded twice. Desktop and web publishing are unchanged.

When upgrading an existing preset, regenerate with `cargo framer` and read the new `PUBLISHING.md` before enabling the workflow. Verify Apple automatic signing/export and Google Play federation on dedicated canary app records before switching your publishing workflows.
