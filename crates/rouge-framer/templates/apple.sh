doctor_apple() {
    [[ $(uname -s) == Darwin ]] || die 'Use macOS with Xcode selected'
    need xcrun
    if [[ $channel != check ]]; then
        secret APPLE_CERTIFICATE_BASE64
        secret APPLE_CERTIFICATE_PASSWORD
        secret APPLE_API_KEY
        secret APPLE_KEY_ID
        secret APPLE_ISSUER_ID
        if [[ $target == ios ]]; then secret IOS_PROFILE_BASE64; fi
    fi
    if [[ $target == ios ]]; then
        xcrun --sdk iphoneos --find actool >/dev/null
        [[ $(sips -g pixelWidth -g pixelHeight -g hasAlpha "$APP_ICON") == *'pixelWidth: 1024'*'pixelHeight: 1024'*'hasAlpha: no'* ]] \
            || die 'iOS requires an opaque 1024 × 1024 PNG icon'
    fi
}

apple_scratch() {
    scratch=$(mktemp -d)
    keychain=''
    original_keychains=()
    trap 'if [[ -n $keychain ]]; then security list-keychains -d user -s "${original_keychains[@]}"; security delete-keychain "$keychain"; fi; rm -rf "$scratch"' EXIT
}

apple_signing() {
    keychain="$scratch/build.keychain-db"
    local password
    password=$(openssl rand -hex 24)
    while IFS= read -r line; do
        line=${line#*\"}
        original_keychains+=("${line%\"*}")
    done < <(security list-keychains -d user)
    security create-keychain -p "$password" "$keychain"
    security set-keychain-settings -lut 3600 "$keychain"
    security unlock-keychain -p "$password" "$keychain"
    security list-keychains -d user -s "$keychain" "${original_keychains[@]}"
    umask 077
    printf '%s' "$APPLE_CERTIFICATE_BASE64" | base64 -D > "$scratch/certificate.p12"
    security import "$scratch/certificate.p12" -k "$keychain" -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign
    security set-key-partition-list -S apple-tool:,apple: -s -k "$password" "$keychain" >/dev/null
    signing_identity=$(security find-identity -v -p codesigning "$keychain" | awk -v kind="$1" 'index($0, kind) { print $2; exit }')
    [[ -n $signing_identity ]] || die "Could not find a $1 signing identity in APPLE_CERTIFICATE_BASE64"
    unset APPLE_CERTIFICATE_BASE64 APPLE_CERTIFICATE_PASSWORD
}

apple_api_key() {
    [[ $APPLE_KEY_ID =~ ^[A-Z0-9]+$ ]] || die 'Invalid APPLE_KEY_ID'
    mkdir -p "$scratch/private_keys"
    chmod 700 "$scratch/private_keys"
    (umask 077; printf '%s\n' "$APPLE_API_KEY" > "$scratch/private_keys/AuthKey_$APPLE_KEY_ID.p8")
}

make_plist() {
    python3 - "$1" "$target" <<'PY'
import os
import plistlib
import sys

path, target = sys.argv[1:]
values = dict(
    CFBundleIdentifier=os.environ['APP_ID'], CFBundleDisplayName=os.environ['APP_NAME'],
    CFBundleName=os.environ['APP_NAME'], CFBundleExecutable=os.environ['APP_TARGET'],
    CFBundlePackageType='APPL', CFBundleInfoDictionaryVersion='6.0',
    CFBundleShortVersionString=os.environ['APP_VERSION'].split('-')[0].split('+')[0],
    CFBundleVersion=os.environ['ROUGE_BUILD_NUMBER'],
)
if target == 'ios':
    values.update(MinimumOSVersion='16.0', LSRequiresIPhoneOS=True, UIDeviceFamily=[1, 2],
        CFBundleSupportedPlatforms=['iPhoneOS'], UILaunchScreen={},
        UISupportedInterfaceOrientations=['UIInterfaceOrientationPortrait', 'UIInterfaceOrientationLandscapeLeft', 'UIInterfaceOrientationLandscapeRight'],
        **{'UISupportedInterfaceOrientations~ipad': ['UIInterfaceOrientationPortrait', 'UIInterfaceOrientationPortraitUpsideDown', 'UIInterfaceOrientationLandscapeLeft', 'UIInterfaceOrientationLandscapeRight']})
elif os.environ['APP_ICON']:
    values['CFBundleIconFile'] = 'AppIcon.icns'
with open(path, 'wb') as file:
    plistlib.dump(values, file)
PY
}

build_macos() (
    local scratch keychain signing_identity
    local -a original_keychains
    apple_scratch
    rustup target add aarch64-apple-darwin
    build_binary aarch64-apple-darwin
    local app="$scratch/$APP_TARGET.app"
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
    cp "$CARGO_TARGET_DIR/aarch64-apple-darwin/release/$APP_TARGET" "$app/Contents/MacOS/"
    make_plist "$app/Contents/Info.plist"
    if [[ -n $APP_ICON ]]; then
        mkdir "$scratch/AppIcon.iconset"
        for size in 16 32 128 256 512; do
            sips -z "$size" "$size" "$APP_ICON" --out "$scratch/AppIcon.iconset/icon_${size}x${size}.png" >/dev/null
            sips -z "$((size * 2))" "$((size * 2))" "$APP_ICON" --out "$scratch/AppIcon.iconset/icon_${size}x${size}@2x.png" >/dev/null
        done
        iconutil -c icns -o "$app/Contents/Resources/AppIcon.icns" "$scratch/AppIcon.iconset"
    fi
    local dmg="$out/$APP_TARGET-$APP_VERSION-macos-arm64.dmg"
    if [[ $channel != check ]]; then
        apple_signing 'Developer ID Application'
        apple_api_key
        codesign --sign "$signing_identity" --timestamp --options runtime "$app"
        codesign --verify --strict "$app"
    fi
    hdiutil create -srcfolder "$app" -volname "$APP_TARGET" -format UDZO "$dmg"
    if [[ $channel != check ]]; then
        codesign --sign "$signing_identity" --timestamp "$dmg"
        xcrun notarytool submit "$dmg" --wait --key "$scratch/private_keys/AuthKey_$APPLE_KEY_ID.p8" \
            --key-id "$APPLE_KEY_ID" --issuer "$APPLE_ISSUER_ID"
        xcrun stapler staple "$dmg"
        xcrun stapler validate "$dmg"
    fi
)

build_ios() (
    local scratch keychain signing_identity
    local -a original_keychains
    apple_scratch
    local app="$scratch/Payload/$APP_TARGET.app"
    local catalog="$scratch/Assets.xcassets"
    local triple=aarch64-apple-ios
    export IPHONEOS_DEPLOYMENT_TARGET=16.0
    export SDKROOT
    SDKROOT=$(xcrun --sdk iphoneos --show-sdk-path)
    rustup target add "$triple"
    build_binary "$triple"
    mkdir -p "$app" "$catalog/AppIcon.appiconset"
    cp "$CARGO_TARGET_DIR/$triple/release/$APP_TARGET" "$app/"
    make_plist "$app/Info.plist"
    cp "$APP_ICON" "$catalog/AppIcon.appiconset/icon.png"
    printf '%s\n' '{"info":{"author":"xcode","version":1}}' > "$catalog/Contents.json"
    printf '%s\n' '{"images":[{"filename":"icon.png","idiom":"universal","platform":"ios","size":"1024x1024"}],"info":{"author":"xcode","version":1}}' > "$catalog/AppIcon.appiconset/Contents.json"
    xcrun --sdk iphoneos actool "$catalog" --compile "$app" --platform iphoneos --app-icon AppIcon \
        --target-device iphone --target-device ipad --minimum-deployment-target 16.0 \
        --output-partial-info-plist "$scratch/assets.plist"
    python3 - "$app/Info.plist" "$scratch/assets.plist" "$(xcrun --sdk iphoneos --show-sdk-build-version)" \
        "$(xcrun --sdk iphoneos --show-sdk-version)" "$(xcodebuild -version | sed -n '2s/^Build version //p')" <<'PY'
import plistlib
import sys
from pathlib import Path

info, assets, sdk_build, sdk, xcode_build = sys.argv[1:]
path = Path(info)
values = plistlib.loads(path.read_bytes())
values.update(plistlib.loads(Path(assets).read_bytes()))
values.update(DTPlatformName='iphoneos', DTPlatformVersion=sdk, DTSDKName=f'iphoneos{sdk}',
    DTPlatformBuild=sdk_build, DTSDKBuild=sdk_build, DTXcodeBuild=xcode_build)
path.write_bytes(plistlib.dumps(values))
PY
    if [[ $channel != check ]]; then
        printf '%s' "$IOS_PROFILE_BASE64" | base64 -D > "$app/embedded.mobileprovision"
        security cms -D -i "$app/embedded.mobileprovision" -o "$scratch/profile.plist"
        python3 - "$scratch/profile.plist" "$scratch/entitlements.plist" <<'PY'
import os
import plistlib
import sys
from pathlib import Path

profile = plistlib.loads(Path(sys.argv[1]).read_bytes())
entitlements = profile['Entitlements']
team = profile['TeamIdentifier'][0]
app_id = f"{profile['ApplicationIdentifierPrefix'][0]}.{os.environ['APP_ID']}"
if entitlements.get('application-identifier') != app_id or entitlements.get('get-task-allow') or profile.get('ProvisionedDevices') or profile.get('ProvisionsAllDevices'):
    sys.exit('IOS_PROFILE_BASE64 must be an App Store distribution profile for the configured app ID')
values = {'application-identifier': app_id, 'com.apple.developer.team-identifier': team, 'get-task-allow': False}
if entitlements.get('beta-reports-active'):
    values['beta-reports-active'] = True
Path(sys.argv[2]).write_bytes(plistlib.dumps(values))
PY
        apple_signing 'Apple Distribution'
        codesign --sign "$signing_identity" --entitlements "$scratch/entitlements.plist" "$app"
        codesign --verify --strict "$app"
        (cd "$scratch"; /usr/bin/zip -q -r "$out/app.ipa" Payload)
    else
        ditto -c -k --keepParent "$app" "$out/unsigned-app.zip"
    fi
)

upload_ios() (
    local scratch keychain
    apple_scratch
    secret APPLE_API_KEY
    secret APPLE_KEY_ID
    secret APPLE_ISSUER_ID
    apple_api_key
    local ipa="$out/app.ipa"
    if [[ $channel == preview ]]; then
        API_PRIVATE_KEYS_DIR="$scratch/private_keys" xcrun altool --upload-app --type ios --file "$ipa" \
            --apiKey "$APPLE_KEY_ID" --apiIssuer "$APPLE_ISSUER_ID"
    else
        need fastlane
        (umask 077; python3 - "$scratch/key.json" <<'PY'
import json
import os
import sys
from pathlib import Path

Path(sys.argv[1]).write_text(json.dumps(dict(key_id=os.environ['APPLE_KEY_ID'],
    issuer_id=os.environ['APPLE_ISSUER_ID'], key=os.environ['APPLE_API_KEY'])))
PY
        )
        fastlane deliver --api_key_path "$scratch/key.json" --ipa "$ipa" --app_identifier "$APP_ID" \
            --app_version "$APP_VERSION" --skip_metadata true \
            --skip_screenshots true --precheck_include_in_app_purchases false \
            --submit_for_review true --automatic_release false --force true
    fi
)
