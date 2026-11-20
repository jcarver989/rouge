# Publish your app

1. Commit `.framer/`, the generated files, and `Cargo.lock`. Pin Rust in `rust-toolchain.toml`.
2. On GitHub, open **Actions → Publish Rouge app → Run workflow**. Select the default branch and set `channel` to `check`. Download the test packages from the run's **Artifacts** section. This mode does not publish or need release secrets.
3. Follow the platform setup below. Add secrets to the listed environments under **Settings → Environments**. Restrict publishing environments to the default branch.
4. Run the workflow with `channel` set to `preview` or `production`. Use `target` to build only one platform, or select `all` for every enabled platform. Find the download or store link in the run summary.

Bump the Cargo package version for each production desktop release. For iOS, use a version like `1.2.3` with no suffix. Mobile builds use the workflow run number by default. Set `build_number` if the store needs a higher number.

Run `cargo rouge doctor <target>` to check your local setup. Set `ROUGE_CHANNEL=preview` to also check for release credentials. After editing `.framer/src/main.rs`, run `cargo framer` to update the generated files.
