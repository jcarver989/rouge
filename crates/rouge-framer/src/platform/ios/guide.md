## iOS · TestFlight and App Store

1. Configure an opaque 1024 × 1024 PNG icon (no alpha channel).
2. Register your bundle ID in Apple Developer and create the app in App Store Connect.
3. Export an **Apple Distribution** certificate (`.p12`), an App Store distribution provisioning profile (`.mobileprovision`) for your bundle ID, and an App Store Connect team API key (`.p8`) with permission to upload builds and submit for review.
4. In **Settings → Environments**, add these secrets to `rouge-ios-preview` and `rouge-ios-production`:
   - `APPLE_CERTIFICATE_BASE64`: base64-encoded `.p12` file
   - `APPLE_CERTIFICATE_PASSWORD`: `.p12` export password
   - `IOS_PROFILE_BASE64`: base64-encoded `.mobileprovision` file
   - `APPLE_API_KEY`: raw `.p8` file contents
   - `APPLE_KEY_ID`: API key ID
   - `APPLE_ISSUER_ID`: App Store Connect issuer ID
5. Before running `production`, create the matching version in App Store Connect and complete screenshots, metadata, privacy details, and export compliance.
