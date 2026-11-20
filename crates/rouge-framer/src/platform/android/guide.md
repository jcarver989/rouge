## Android · Google Play

1. Create the app in Google Play Console, enroll in **Play App Signing**, and create an upload keystore.
2. For the first upload, export `ANDROID_UPLOAD_KEYSTORE_BASE64`, `ANDROID_UPLOAD_STORE_PASSWORD`, `ANDROID_UPLOAD_KEY_ALIAS`, and `ANDROID_UPLOAD_KEY_PASSWORD` locally using that keystore. Run `cargo rouge setup android`, then `cargo rouge build android --channel production --build-number 1` (choose a higher unused number for an existing app). Manually upload `.rouge/out/android/app.aab` in Play Console. This build does not contact Google Play. Never register a `check` artifact: it is signed with a disposable key, not your upload key.
3. Create a Google service account and grant it release permissions for the app in Play Console.
4. In **Settings → Environments**, add these secrets to `rouge-android-preview` and `rouge-android-production`:
   - `PLAY_SERVICE_ACCOUNT_JSON`: raw service account JSON
   - `ANDROID_UPLOAD_KEYSTORE_BASE64`: base64-encoded upload keystore
   - `ANDROID_UPLOAD_STORE_PASSWORD`: keystore password
   - `ANDROID_UPLOAD_KEY_ALIAS`: key alias
   - `ANDROID_UPLOAD_KEY_PASSWORD`: key password
5. Before running `production`, complete the Play Console store listing, policy forms, and any required testing prerequisites.
