## Android · Google Play

1. Create the app in Google Play Console, enroll in **Play App Signing**, and create an upload keystore.
2. Upload the first `.aab` manually in Play Console (required before API uploads work).
3. Create a Google service account and grant it release permissions for the app in Play Console.
4. In **Settings → Environments**, add these secrets to `rouge-android-preview` and `rouge-android-production`:
   - `PLAY_SERVICE_ACCOUNT_JSON`: raw service account JSON
   - `ANDROID_UPLOAD_KEYSTORE_BASE64`: base64-encoded upload keystore
   - `ANDROID_UPLOAD_STORE_PASSWORD`: keystore password
   - `ANDROID_UPLOAD_KEY_ALIAS`: key alias
   - `ANDROID_UPLOAD_KEY_PASSWORD`: key password
5. Before running `production`, complete the Play Console store listing, policy forms, and any required testing prerequisites.
