## macOS · GitHub Releases

1. Export a **Developer ID Application** certificate as a `.p12` file.
2. Create an App Store Connect team API key (`.p8`) with notarization permissions.
3. In **Settings → Environments**, add these secrets to `rouge-macos-preview` and `rouge-macos-production`:
   - `APPLE_CERTIFICATE_BASE64`: base64-encoded `.p12` file
   - `APPLE_CERTIFICATE_PASSWORD`: `.p12` export password
   - `APPLE_API_KEY`: raw `.p8` file contents
   - `APPLE_KEY_ID`: API key ID
   - `APPLE_ISSUER_ID`: App Store Connect issuer ID
