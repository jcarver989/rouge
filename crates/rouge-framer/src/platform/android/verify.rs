use super::Keystore;
use crate::error::{PublishError, Result, ensure, invalid};
use crate::packager::Build;
use crate::secrets::ANDROID;
use std::fs;
use std::path::{Path, PathBuf};
use xshell::cmd;

const PAGE: u64 = 16 * 1024;

pub(super) fn signer(build: &Build, bundle: &Path, keystore: &Keystore) -> Result<()> {
    let sh = &build.sh;
    let (key, alias, password) = (&keystore.path, &keystore.alias, ANDROID[1]);
    build
        .isolated(cmd!(sh, "jarsigner -verify {bundle}"))
        .ignore_stdout()
        .run()?;
    let expected = build
        .isolated(cmd!(
            sh,
            "keytool -list -v -keystore {key} -alias {alias} -storepass:env {password}"
        ))
        .env(password, &keystore.store_password)
        .read()?;
    let signed = build
        .isolated(cmd!(sh, "keytool -printcert -jarfile {bundle}"))
        .read()?;
    let expected = fingerprints(&expected);
    ensure(
        !expected.is_empty() && fingerprints(&signed) == expected,
        "The app bundle is not signed with the configured upload key",
    )
}

pub(super) fn alignment(build: &Build, ndk: &Path, libraries: &Path) -> Result<()> {
    let sh = &build.sh;
    let readelf = readelf(ndk)?;
    for library in fs::read_dir(libraries)? {
        let library = library?.path();
        let segments = build.isolated(cmd!(sh, "{readelf} -lW {library}")).read()?;
        verify_segments(&library.display().to_string(), &segments)?;
    }
    Ok(())
}

fn fingerprints(output: &str) -> Vec<&str> {
    output
        .lines()
        .filter_map(|line| line.trim().strip_prefix("SHA256: "))
        .collect()
}

fn readelf(ndk: &Path) -> Result<PathBuf> {
    for host in fs::read_dir(ndk.join("toolchains/llvm/prebuilt"))? {
        let path = host?.path().join("bin/llvm-readelf");
        if path.is_file() {
            return Ok(path);
        }
    }
    invalid("The configured NDK does not contain llvm-readelf")
}

fn verify_segments(name: &str, output: &str) -> Result<()> {
    let mut loads = Vec::new();
    let mut relro = None;
    for line in output.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if !matches!(fields.first().copied(), Some("LOAD" | "GNU_RELRO")) {
            continue;
        }
        let hex = |index: usize| {
            fields
                .get(index)
                .and_then(|part| part.strip_prefix("0x"))
                .and_then(|part| u64::from_str_radix(part, 16).ok())
                .ok_or_else(|| {
                    PublishError::Invalid(format!("Invalid ELF program headers for {name}"))
                })
        };
        let start = hex(2)?;
        let end = start.checked_add(hex(5)?).ok_or_else(|| {
            PublishError::Invalid(format!("Invalid ELF virtual address for {name}"))
        })?;
        if fields[0] == "LOAD" {
            let alignment = hex(fields.len() - 1)?;
            ensure(
                alignment >= PAGE && hex(1)? % PAGE == start % PAGE,
                format!("{name} has an ELF LOAD segment without 16 KB alignment"),
            )?;
            loads.push((
                start,
                end,
                fields[6..fields.len() - 1]
                    .iter()
                    .any(|flag| flag.contains('W')),
            ));
        } else {
            relro = Some((start, end));
        }
    }
    ensure(
        !loads.is_empty(),
        format!("No ELF LOAD segments for {name}"),
    )?;
    if let Some((start, end)) = relro {
        let trailing_writable = loads
            .iter()
            .any(|(lo, hi, writable)| *writable && *lo < end && start < *hi && end < *hi);
        ensure(
            !trailing_writable || end % PAGE == 0,
            format!("{name} has writable data directly after an unaligned ELF GNU_RELRO segment"),
        )?;
    }
    Ok(())
}
