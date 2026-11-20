android_ndk=28.2.13676358
android_sdk=36
android_tools=36.0.0
android_cargo_ndk=4.1.2

install_android() {
    need sdkmanager
    need rustup
    sdkmanager "platforms;android-$android_sdk" "build-tools;$android_tools" "ndk;$android_ndk"
    rustup target add aarch64-linux-android
    if [[ $(cargo ndk --version 2>/dev/null || true) != "cargo-ndk $android_cargo_ndk" ]]; then
        cargo install cargo-ndk --version "=$android_cargo_ndk" --locked
    fi
}

doctor_android() {
    : "${ANDROID_HOME:?Set ANDROID_HOME to the Android SDK, then see PUBLISHING.md}"
    export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/$android_ndk"
    if [[ $channel != check ]]; then
        secret ANDROID_UPLOAD_KEYSTORE_BASE64
        secret ANDROID_UPLOAD_STORE_PASSWORD
        secret ANDROID_UPLOAD_KEY_ALIAS
        secret ANDROID_UPLOAD_KEY_PASSWORD
    fi
    if [[ $action == doctor ]]; then return; fi
    need gradle
    need keytool
    need jarsigner
    [[ -d $ANDROID_NDK_HOME ]] || die "Install NDK $android_ndk with sdkmanager"
    [[ -d $ANDROID_HOME/platforms/android-$android_sdk ]] || die "Install platform android-$android_sdk with sdkmanager"
    cargo ndk --version >/dev/null || die "Install cargo-ndk $android_cargo_ndk"
    if [[ -n $APP_ICON ]] && ! command -v magick >/dev/null; then need convert; fi
}

build_android() (
    local scratch
    scratch=$(mktemp -d)
    trap 'rm -rf "$scratch"' EXIT
    export ROUGE_KEYSTORE="$scratch/upload.p12"
    if [[ $channel == check ]]; then
        export ANDROID_UPLOAD_STORE_PASSWORD=rouge-check-only ANDROID_UPLOAD_KEY_PASSWORD=rouge-check-only ANDROID_UPLOAD_KEY_ALIAS=rouge-check
        keytool -genkeypair -keystore "$ROUGE_KEYSTORE" -storetype PKCS12 -storepass:env ANDROID_UPLOAD_STORE_PASSWORD \
            -alias "$ANDROID_UPLOAD_KEY_ALIAS" -keyalg RSA -keysize 2048 -validity 1 -dname 'CN=Rouge package check' >/dev/null
    else
        umask 077
        python3 -c 'import base64, os; from pathlib import Path; Path(os.environ["ROUGE_KEYSTORE"]).write_bytes(base64.b64decode(os.environ.pop("ANDROID_UPLOAD_KEYSTORE_BASE64")))'
        unset ANDROID_UPLOAD_KEYSTORE_BASE64
    fi
    mkdir -p "$scratch/src/main/jniLibs"
    cargo ndk -t arm64-v8a -p 26 -o "$scratch/src/main/jniLibs" build --locked --release --package "$APP_PACKAGE" --lib
    [[ -f "$scratch/src/main/jniLibs/arm64-v8a/lib${APP_TARGET//-/_}.so" ]] || die 'Cargo did not build the configured Android cdylib'
    cat > "$scratch/settings.gradle" <<'GRADLE'
pluginManagement { repositories { google(); mavenCentral(); gradlePluginPortal() } }
dependencyResolutionManagement { repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS); repositories { google(); mavenCentral() } }
rootProject.name = 'RougeApp'
GRADLE
    cat > "$scratch/build.gradle" <<'GRADLE'
plugins { id 'com.android.application' version '8.11.1' }
android {
    namespace System.getenv('APP_ID')
    compileSdk 36
    defaultConfig {
        applicationId System.getenv('APP_ID')
        minSdk 26
        targetSdk 36
        versionCode System.getenv('ROUGE_BUILD_NUMBER').toInteger()
        versionName System.getenv('APP_VERSION')
        manifestPlaceholders = [appName: System.getenv('APP_NAME'), rustLibrary: System.getenv('APP_TARGET').replace('-', '_')]
    }
    signingConfigs {
        upload {
            storeFile file(System.getenv('ROUGE_KEYSTORE'))
            storePassword System.getenv('ANDROID_UPLOAD_STORE_PASSWORD')
            keyAlias System.getenv('ANDROID_UPLOAD_KEY_ALIAS')
            keyPassword System.getenv('ANDROID_UPLOAD_KEY_PASSWORD')
        }
    }
    buildTypes { release { signingConfig signingConfigs.upload } }
}
GRADLE
    icon=''
    if [[ -n $APP_ICON ]]; then
        mkdir -p "$scratch/src/main/res/drawable"
        local image_tool=magick
        if ! command -v magick >/dev/null; then image_tool=convert; fi
        "$image_tool" "$APP_ICON" -resize 192x192 "$scratch/src/main/res/drawable/rouge_icon.png"
        icon='android:icon="@drawable/rouge_icon"'
    fi
    cat > "$scratch/src/main/AndroidManifest.xml" <<MANIFEST
<manifest xmlns:android="http://schemas.android.com/apk/res/android">
    <application android:label="\${appName}" $icon>
        <activity android:name="android.app.NativeActivity" android:exported="true"
            android:configChanges="orientation|keyboardHidden|screenSize">
            <meta-data android:name="android.app.lib_name" android:value="\${rustLibrary}" />
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
    </application>
</manifest>
MANIFEST
    gradle --no-daemon --project-dir "$scratch" bundleRelease
    cp "$scratch/build/outputs/bundle/release/RougeApp-release.aab" "$out/app.aab"
    jarsigner -verify "$out/app.aab"
)
