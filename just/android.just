set shell := ["bash", "-euo", "pipefail", "-c"]
set working-directory := ".."

# Type-check the Android target (needs the aarch64-linux-android target, the Android NDK and cargo-ndk)
check:
    cargo ndk -t arm64-v8a check --locked -p rouge --all-features
    cargo ndk -t arm64-v8a check --locked -p rouge --tests --all-features
    cargo ndk -t arm64-v8a check --locked -p rouge --example android_todomvc

# Package the Android example into target/android/<example>/<example>.apk (needs ANDROID_HOME with build-tools and a platform, and a Java runtime on PATH for keytool and apksigner)
build example="android_todomvc" package="rouge":
    cargo ndk -t arm64-v8a build --locked -p {{package}} --example {{example}}
    scripts/android-apk {{example}} "dev.rouge.$(echo {{example}} | tr - _)"

# Install the Android example on the connected device or emulator, start it and stream its log
[private]
run example="android_todomvc" package="rouge": (build example package)
    adb install -r target/android/{{example}}/{{example}}.apk
    adb shell am start -n "dev.rouge.$(echo {{example}} | tr - _)/android.app.NativeActivity"
    adb logcat -v color RustStdoutStderr:V AndroidRuntime:E '*:S'
