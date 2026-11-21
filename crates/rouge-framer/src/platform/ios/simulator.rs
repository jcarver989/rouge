use super::{Device, Ios, SIMULATOR, check_icon, xcodebuild};
use crate::error::{PublishError, Result, ensure, invalid};
use crate::packager::{Build, need};
use serde::Deserialize;
use std::cmp::Reverse;
use std::collections::BTreeMap;
use xshell::cmd;

#[derive(Deserialize)]
struct Simulators {
    devices: BTreeMap<String, Vec<Simulator>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Simulator {
    udid: String,
    name: String,
    state: String,
    is_available: bool,
    device_type_identifier: String,
}

impl Simulator {
    fn family(&self) -> Option<Device> {
        let kind = self
            .device_type_identifier
            .strip_prefix("com.apple.CoreSimulator.SimDeviceType.")?;
        if kind.starts_with("iPhone-") {
            Some(Device::Phone)
        } else if kind.starts_with("iPad-") {
            Some(Device::Pad)
        } else {
            None
        }
    }
}

pub(super) fn run(ios: &Ios, build: &Build, selector: Option<&str>) -> Result<()> {
    ensure(
        cfg!(all(target_os = "macos", target_arch = "aarch64")),
        "Running the iOS Simulator requires macOS on Apple Silicon",
    )?;
    need("xcrun")?;
    let sh = &build.sh;
    build
        .isolated(cmd!(sh, "xcrun --find simctl"))
        .read()
        .map_err(|error| {
            PublishError::Invalid(format!(
                "Select a full Xcode installation with xcode-select: {error}"
            ))
        })?;
    let json = build
        .isolated(cmd!(sh, "xcrun simctl list devices available --json"))
        .read()?;
    let simulators: Simulators = serde_json::from_str(&json).map_err(|error| {
        PublishError::Invalid(format!("Could not read the simctl device list: {error}"))
    })?;
    let device = simulators.select(ios, selector)?;
    let (udid, name, id) = (&device.udid, &device.name, &build.app.id);
    println!("Building for {name} ({udid})");

    check_icon(build)?;
    build
        .isolated(cmd!(sh, "rustup target add {SIMULATOR}"))
        .run()?;
    let derived = build.out.join("DerivedData");
    let destination = format!("id={udid}");
    xcodebuild(build, &derived)?
        .args(["-configuration", "Debug", "-destination", &destination])
        .arg("build")
        .run()?;
    let bundle = derived.join("Build/Products/Debug-iphonesimulator/RougeApp.app");
    build
        .isolated(cmd!(sh, "xcrun simctl bootstatus {udid} -b"))
        .run()?;
    build.isolated(cmd!(sh, "open -a Simulator")).run()?;
    build
        .isolated(cmd!(sh, "xcrun simctl install {udid} {bundle}"))
        .run()?;
    build
        .isolated(cmd!(
            sh,
            "xcrun simctl launch --terminate-running-process {udid} {id}"
        ))
        .run()?;
    Ok(())
}

impl Simulators {
    fn select(&self, ios: &Ios, selector: Option<&str>) -> Result<&Simulator> {
        let minimum = version(&ios.minimum_os, '.')
            .ok_or_else(|| PublishError::Invalid("Invalid iOS deployment target".into()))?;
        let compatible = self
            .devices
            .iter()
            .filter_map(|(runtime, devices)| {
                let runtime = version(
                    runtime.strip_prefix("com.apple.CoreSimulator.SimRuntime.iOS-")?,
                    '-',
                )?;
                (runtime >= minimum).then_some((runtime, devices))
            })
            .flat_map(|(runtime, devices)| devices.iter().map(move |device| (runtime, device)))
            .filter_map(|(runtime, device)| {
                let family = device.family()?;
                (device.is_available && ios.devices.contains(&family))
                    .then_some((runtime, device, family))
            });
        if let Some(selector) = selector {
            let mut named = None;
            let mut ambiguous = false;
            for (_, device, _) in compatible {
                if device.udid.eq_ignore_ascii_case(selector) {
                    return Ok(device);
                }
                if device.name == selector {
                    ambiguous |= named.replace(device).is_some();
                }
            }
            if ambiguous {
                return invalid(format!(
                    "More than one compatible simulator is named {selector:?}; use a UDID from `xcrun simctl list devices available`"
                ));
            }
            if let Some(device) = named {
                return Ok(device);
            }
        } else if let Some((_, device, _)) = compatible.min_by_key(|(runtime, device, family)| {
            (
                Reverse(device.state == "Booted"),
                Reverse(*runtime),
                Reverse(*family == Device::Phone),
                &device.name,
                &device.udid,
            )
        }) {
            return Ok(device);
        }
        let requested =
            selector.map_or_else(String::new, |selector| format!(" matching {selector:?}"));
        invalid(format!(
            "No compatible iOS simulator{requested}; install an iOS {} or newer runtime in Xcode and select a simulator for a configured device family (`xcrun simctl list devices available`)",
            ios.minimum_os
        ))
    }
}

fn version(text: &str, separator: char) -> Option<[u32; 3]> {
    let mut version = [0; 3];
    for (part, number) in text.split(separator).zip(&mut version) {
        *number = part.parse().ok()?;
    }
    Some(version)
}
