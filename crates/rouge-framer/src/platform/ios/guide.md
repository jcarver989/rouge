## iOS · TestFlight and App Store

`cargo framer` generates an Xcode project in `ios/`; commit it. Its build phase runs `ios/cargo.sh`, which builds the Rust binary with cargo, so the project also builds and runs from Xcode. To do that without `cargo rouge`, set `RougePublish::binary` or `package` so the project knows the executable name.

On Apple Silicon with full Xcode and a compatible simulator runtime, run `just ios-sim` or `cargo rouge run ios`. Use `--device` or `ROUGE_IOS_DEVICE` to select a simulator by name or UDID. No Apple credentials are needed.

Device packaging needs Xcode 26 or later and the iOS 26 SDK or later. The generated workflow uses Xcode 26.6 on `macos-26`; locally use `DEVELOPER_DIR` or `xcode-select` to choose Xcode. `cargo rouge setup ios` installs the Rust device target. Configure an opaque 1024 × 1024 PNG icon. The app ships an empty privacy manifest unless you pass a repository-relative `PrivacyInfo.xcprivacy` path to `Ios::privacy_manifest`.

1. Register the bundle ID in Apple Developer and create the app in App Store Connect. Create a team API key with permission to manage automatic signing and upload builds for the app.
2. In GitHub **Settings → Environments → rouge-ios-preview**, set the `APPLE_TEAM_ID` variable (the 10-character Apple Developer team ID) and these secrets:
   - `APPLE_API_KEY`: raw `.p8` contents
   - `APPLE_KEY_ID`: API key ID
   - `APPLE_ISSUER_ID`: App Store Connect issuer ID
3. Run `check` to build an unsigned `.xcarchive` and inspection ZIP. `preview` archives with Xcode automatic signing, retains matching crash symbols as a workflow artifact, and uploads the archive once with `xcodebuild -exportArchive`. Wait for Apple processing and complete any compliance and tester setup before testing in TestFlight.
4. Prepare the matching App Store version, screenshots, metadata and privacy details. Select the processed TestFlight build in App Store Connect, then submit it for review and choose when to release. The workflow does not run iOS jobs for `production`.

Before switching an existing app, verify automatic signing and the archive upload on a fresh runner using a dedicated App Store Connect record. If this fails, keep the previous preset pinned until your signing setup is validated. The minimum deployment OS remains iOS 16 by default. Do not switch to the iOS 27 SDK until your runtime implements `UIScene` and you have verified visible rendering, rotation and background/resume on a real iPhone; adding a scene manifest or passing the simulator workflow is not sufficient.
