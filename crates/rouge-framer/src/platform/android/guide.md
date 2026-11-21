## Android · Google Play

Set `ANDROID_HOME`, put Android SDK command-line tools 23.0 or later and a JDK on `PATH`, and accept the SDK licenses. Run `cargo rouge setup android` to install the pinned SDK/NDK packages, Rust target and `cargo-ndk`. `cargo framer` generates a Gradle project in `android/`; commit it. Its `cargoDebug` and `cargoRelease` tasks build the Rust library with `cargo ndk`, so `cargo rouge build android` is a wrapper around `bundleRelease` and the project also opens in Android Studio. To build there without `cargo rouge`, set `RougePublish::package` or `android_library` so the project knows the library name. The project uses the checksum-pinned Gradle Wrapper, not a system Gradle installation. `AndroidToolchain::gradle` takes a version and the official distribution SHA-256 checksum. The app icon is packaged at its original size.

Run `cargo rouge run android` with an arm64 device or emulator connected and USB debugging enabled. With multiple devices use `--device <serial>` or set `ANDROID_SERIAL` (see `adb devices`). This runs `assembleDebug`, then installs and launches the debug-signed APK, separate from Play packages. Set `ROUGE_ANDROID_LOGS=1` to stream logs.

1. Create the app in Play Console, enroll in **Play App Signing**, and create or reuse your upload keystore. Never use a `check` bundle as the first Play upload: its signing key is disposable.
2. For the first upload, export the four keystore secrets below locally and run `cargo rouge build android --channel preview --build-number 1` (or choose a higher unused version code for an existing app). Manually upload `.rouge/out/android/app.aab` in Play Console. The local build does not contact Play.
3. Grant a Google service account permission for internal releases on this app in Play Console. Configure GitHub Workload Identity Federation for that service account; restrict trust to your repository, default branch and protected preview environment. In **Settings → Environments → rouge-android-preview**, set `GOOGLE_PLAY_WORKLOAD_IDENTITY_PROVIDER` to the full provider resource name and `GOOGLE_PLAY_SERVICE_ACCOUNT` to the account email. No service-account JSON key is needed.
4. Add these secrets to `rouge-android-preview` for AAB signing:
   - `ANDROID_UPLOAD_KEYSTORE_BASE64`: base64-encoded upload keystore
   - `ANDROID_UPLOAD_STORE_PASSWORD`: keystore password
   - `ANDROID_UPLOAD_KEY_ALIAS`: key alias
   - `ANDROID_UPLOAD_KEY_PASSWORD`: key password
5. Run `preview` to upload the verified AAB to internal testing. After testing, select the same build in Play Console and complete review or rollout there; the workflow does not run Android jobs for `production`. Complete store listing, policy forms and testing prerequisites in the console.

The bundle build verifies that the bundle is signed with the configured key and that the shipped ELF libraries are 16 KB aligned. Before switching an existing app, verify that release splits launch on arm64 in 16 KB mode without compatibility mode and that a dedicated Play canary reaches internal testing via federation. Federation replaces the Play API credential, not the app upload keystore.
