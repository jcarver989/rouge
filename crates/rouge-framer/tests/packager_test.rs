#![cfg(unix)]

use rouge_framer::{Action, Channel, Packager, RougePublish, Target, TestFlight};
use std::env;
use std::error::Error;
use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tempfile::tempdir;
use xshell::{Shell, cmd};

type Result = std::result::Result<(), Box<dyn Error>>;

#[test]
fn signing_tools_install_without_credentials_and_use_private_key_files() -> Result {
    in_app(
        "signing_tools_install_without_credentials_and_use_private_key_files",
        |sh| {
            let packager = Packager::new(
                RougePublish::new(APP_ID)
                    .icon("icon.png")
                    .ios(TestFlight::new("1234")),
                ".",
            )
            .channel(Channel::Preview);
            sh.write_file("icon.png", "unused during upload")?;
            tool(sh, "rcodesign", "echo 'rcodesign 0.0.0'")?;
            tool(
                sh,
                "cargo",
                r#"if [ "$1" != install ]; then exec "$REAL_CARGO" "$@"; fi
[ -z "${APPLE_API_KEY+x}${APPLE_CERTIFICATE_PASSWORD+x}${ANDROID_UPLOAD_STORE_PASSWORD+x}${GH_TOKEN+x}" ]
touch "$ROUGE_TEST_APP/installed""#,
            )?;
            packager.run(Action::Setup, Target::Ios)?;
            assert!(sh.path_exists("installed"));
            tool(
                sh,
                "xcrun",
                r"python3 - <<'PY'
import os, pathlib, stat
keys = pathlib.Path(os.environ['API_PRIVATE_KEYS_DIR'])
assert stat.S_IMODE(keys.parent.stat().st_mode) == 0o700
key = keys / 'AuthKey_TEST.p8'
assert stat.S_IMODE(key.stat().st_mode) == 0o600
assert key.read_text().strip() == 'test-only'
(pathlib.Path(os.environ['ROUGE_TEST_APP']) / 'scratch-path').write_text(str(keys.parent))
PY",
            )?;
            packager.run(Action::Publish, Target::Ios)?;
            assert!(!Path::new(&sh.read_file("scratch-path")?).exists());
            Ok(())
        },
    )
}

const APP_ID: &str = "com.example.app";

fn in_app(test: &str, run: impl FnOnce(&Shell) -> Result) -> Result {
    let sh = Shell::new()?;
    if env::var_os("ROUGE_TEST_APP").is_some() {
        return run(&sh);
    }
    let app = tempdir()?;
    let root = app.path();
    sh.change_dir(root);
    sh.write_file(
        "Cargo.toml",
        "[workspace]\n[package]\nname='app'\nversion='1.0.0'\nedition='2024'\n",
    )?;
    sh.write_file("src/main.rs", "fn main() {}")?;
    cmd!(sh, "cargo generate-lockfile --offline").run()?;
    let bin = sh.create_dir("tools")?;
    let path = env::join_paths([bin].into_iter().chain(env::split_paths(
        &env::var_os("PATH").ok_or("missing PATH")?,
    )))?;
    let exe = env::current_exe()?;
    cmd!(sh, "{exe} --exact {test} --nocapture")
        .env("ROUGE_TEST_APP", root)
        .env("PATH", path)
        .env("REAL_CARGO", env!("CARGO"))
        .env("CARGO_TARGET_DIR", root.join("target"))
        .env_remove("GITHUB_STEP_SUMMARY")
        .env("APPLE_API_KEY", "test-only")
        .env("APPLE_KEY_ID", "TEST")
        .env("APPLE_ISSUER_ID", "test")
        .env("APPLE_CERTIFICATE_PASSWORD", "test")
        .env("ANDROID_UPLOAD_STORE_PASSWORD", "test")
        .env("GH_TOKEN", "test")
        .run()?;
    Ok(())
}

fn tool(sh: &Shell, name: &str, script: &str) -> Result {
    let path = sh.current_dir().join("tools").join(name);
    sh.write_file(&path, format!("#!/bin/sh\nset -eu\n{script}\n"))?;
    fs::set_permissions(path, Permissions::from_mode(0o755))?;
    Ok(())
}
