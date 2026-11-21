# Mobile Framer smoke app

Run from this directory on a configured host. The first command generates the `ios/` and `android/` projects, which are ignored here:

```sh
mkdir -p .framer && cargo run --locked -p rouge-framer --example mobile_smoke

cargo run --locked -p rouge-framer --example mobile_smoke -- setup ios
cargo run --locked -p rouge-framer --example mobile_smoke -- build ios
cargo run --locked -p rouge-framer --example mobile_smoke -- run ios

cargo run --locked -p rouge-framer --example mobile_smoke -- setup android
cargo run --locked -p rouge-framer --example mobile_smoke -- build android
cargo run --locked -p rouge-framer --example mobile_smoke -- run android --device <adb-serial>
```

Leave the channel on `check`. The fixture has placeholder store IDs and must not be published. The preset lives in `crates/rouge-framer/examples/mobile_smoke.rs`; CI generates the projects the same way rather than checking them in.
