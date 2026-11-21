# Publish your app

1. Commit `.framer/`, the generated files, and `Cargo.lock`. Pin Rust in `rust-toolchain.toml`.
2. On GitHub, open **Actions → Publish Rouge app → Run workflow**. Select the default branch and set `channel` to `check`. Where available, download inspection packages from the run's **Artifacts** section. This mode does not publish or need release secrets.
3. Follow the platform setup below. Add variables and secrets to the listed environments under **Settings → Environments**. Restrict publishing environments to the default branch.
4. Run `preview` to publish a test build. Use `target` to select one platform, or `all` for every enabled platform. Find the download or store link in the run summary.

Bump the Cargo package version for each production desktop release. When publishing a desktop build locally, run `build` and `publish` with the same target, channel, build number, and Git commit. A `check` package cannot be published. For iOS, use a version like `1.2.3` with no suffix. Mobile `check` and `preview` builds use the workflow run number unless you supply a higher unused `build_number`. Mobile jobs run for `check` and `preview` only: to release, promote the build you tested in the store's preview track from App Store Connect or Play Console. Non-mobile `production` still builds and publishes.

Run `cargo rouge doctor <target>` to check your local setup. Set `ROUGE_CHANNEL=preview` to also check for release credentials. After editing `.framer/src/main.rs`, run `cargo framer` to update the generated files.
