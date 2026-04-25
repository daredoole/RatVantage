use std::fs;
use std::path::{Path, PathBuf};

use legion_common::{
    BatteryChargeTypeCapability, BatteryTelemetry, Capability, CapabilityRegistry,
    CapabilityStatus, FanCurveCapability, FirmwareAttributeCapability, HardwareSummary,
    HwmonSensor, IdeapadToggleCapability, LedCapability, PlatformProfileCapability, RiskLevel,
    TelemetrySnapshot,
};

#[derive(Debug, Clone)]
pub struct ProbeOptions {
    pub sysfs_root: PathBuf,
}

impl Default for ProbeOptions {
    fn default() -> Self {
        Self {
            sysfs_root: PathBuf::from("/"),
        }
    }
}

pub fn probe(options: &ProbeOptions) -> CapabilityRegistry {
    let platform_profile = detect_platform_profile(&options.sysfs_root);
    let battery_charge_type = detect_battery_charge_type(&options.sysfs_root);
    let battery_telemetry = detect_battery_telemetry(&options.sysfs_root);
    let hwmon_sensors = detect_hwmon_sensors(&options.sysfs_root);
    let fan_curves = detect_fan_curves(&options.sysfs_root);
    let leds = detect_leds(&options.sysfs_root);
    let firmware_attributes = detect_firmware_attributes(&options.sysfs_root);
    let ideapad_toggles = detect_ideapad_toggles(&options.sysfs_root);

    let mut capabilities = Vec::new();
    push_capability(
        &mut capabilities,
        "platform_profile",
        "Platform profiles",
        platform_profile.is_some(),
    );
    push_capability(
        &mut capabilities,
        "battery_charge_type",
        "Battery charge types",
        battery_charge_type.is_some(),
    );
    push_capability(
        &mut capabilities,
        "hwmon",
        "Hardware monitor sensors",
        !hwmon_sensors.is_empty(),
    );
    push_capability(
        &mut capabilities,
        "firmware_attributes",
        "Lenovo firmware attributes",
        !firmware_attributes.is_empty(),
    );
    push_capability(
        &mut capabilities,
        "fan_curves",
        "Fan curve nodes",
        !fan_curves.is_empty(),
    );
    push_capability(&mut capabilities, "leds", "LED nodes", !leds.is_empty());
    push_capability(
        &mut capabilities,
        "ideapad_toggles",
        "Ideapad toggles",
        !ideapad_toggles.is_empty(),
    );

    CapabilityRegistry {
        hardware: detect_hardware(&options.sysfs_root),
        capabilities,
        platform_profile,
        battery_charge_type,
        telemetry: TelemetrySnapshot {
            sensors: hwmon_sensors.clone(),
            battery: battery_telemetry,
        },
        hwmon_sensors,
        fan_curves,
        leds,
        firmware_attributes,
        ideapad_toggles,
        ..CapabilityRegistry::default()
    }
}

pub fn parse_choices(raw: &str) -> Vec<String> {
    raw.split_whitespace()
        .map(|choice| choice.trim_matches(['[', ']']).to_owned())
        .collect()
}

pub fn parse_marked_current_choice(raw: &str) -> Option<String> {
    raw.split_whitespace().find_map(|choice| {
        choice
            .strip_prefix('[')?
            .strip_suffix(']')
            .map(str::to_owned)
    })
}

fn push_capability(capabilities: &mut Vec<Capability>, id: &str, label: &str, detected: bool) {
    capabilities.push(Capability {
        id: id.to_owned(),
        label: label.to_owned(),
        status: if detected {
            CapabilityStatus::ProbeOnly
        } else {
            CapabilityStatus::Missing
        },
        risk: RiskLevel::ReadOnly,
        evidence: Vec::new(),
        details: serde_json::Value::Null,
    });
}

fn detect_hardware(root: &Path) -> HardwareSummary {
    HardwareSummary {
        sysfs_root: root.display().to_string(),
        vendor: read_trim(root.join("sys/class/dmi/id/sys_vendor")),
        product_name: read_trim(root.join("sys/class/dmi/id/product_name")),
        product_version: read_trim(root.join("sys/class/dmi/id/product_version")),
        product_sku: read_trim(root.join("sys/class/dmi/id/product_sku")),
    }
}

fn detect_platform_profile(root: &Path) -> Option<PlatformProfileCapability> {
    let path = root.join("sys/firmware/acpi/platform_profile");
    let choices_path = root.join("sys/firmware/acpi/platform_profile_choices");
    let current = read_trim(&path);
    let choices = read_trim(&choices_path).map_or_else(Vec::new, |value| parse_choices(&value));

    if current.is_none() && choices.is_empty() {
        return None;
    }

    Some(PlatformProfileCapability {
        current,
        choices,
        path: path.display().to_string(),
    })
}

fn detect_battery_charge_type(root: &Path) -> Option<BatteryChargeTypeCapability> {
    let path = root.join("sys/class/power_supply/BAT0/charge_type");
    let choices_path = root.join("sys/class/power_supply/BAT0/charge_types");
    let choices_raw = read_trim(&choices_path);
    let current =
        read_trim(&path).or_else(|| choices_raw.as_deref().and_then(parse_marked_current_choice));
    let choices = choices_raw.map_or_else(Vec::new, |value| parse_choices(&value));

    if current.is_none() && choices.is_empty() {
        return None;
    }

    Some(BatteryChargeTypeCapability {
        current,
        choices,
        path: path.display().to_string(),
    })
}

fn detect_battery_telemetry(root: &Path) -> Option<BatteryTelemetry> {
    let path = root.join("sys/class/power_supply/BAT0");
    let capacity_percent = read_i64(path.join("capacity"));
    let status = read_trim(path.join("status"));
    let health = read_trim(path.join("health"));

    if capacity_percent.is_none() && status.is_none() && health.is_none() {
        return None;
    }

    Some(BatteryTelemetry {
        name: "BAT0".to_owned(),
        path: path.display().to_string(),
        capacity_percent,
        status,
        health,
    })
}

fn detect_hwmon_sensors(root: &Path) -> Vec<HwmonSensor> {
    let hwmon_root = root.join("sys/class/hwmon");
    let Ok(entries) = fs::read_dir(hwmon_root) else {
        return Vec::new();
    };

    let mut sensors = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let hwmon_name = read_trim(dir.join("name"));
        for index in 1..=16 {
            for kind in ["fan", "temp"] {
                let input_path = dir.join(format!("{kind}{index}_input"));
                if !input_path.exists() {
                    continue;
                }
                sensors.push(HwmonSensor {
                    hwmon_name: hwmon_name.clone(),
                    label: read_trim(dir.join(format!("{kind}{index}_label"))),
                    kind: kind.to_owned(),
                    value: read_trim(&input_path).and_then(|value| value.parse().ok()),
                    input_path: input_path.display().to_string(),
                });
            }
        }
    }
    sensors
}

fn detect_fan_curves(root: &Path) -> Vec<FanCurveCapability> {
    let hwmon_root = root.join("sys/class/hwmon");
    let Ok(entries) = fs::read_dir(hwmon_root) else {
        return Vec::new();
    };

    let mut fan_curves = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }

        let Ok(files) = fs::read_dir(&dir) else {
            continue;
        };
        let mut point_paths: Vec<String> = files
            .flatten()
            .map(|file| file.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(is_fan_curve_point)
            })
            .map(|path| path.display().to_string())
            .collect();
        point_paths.sort();

        if point_paths.is_empty() {
            continue;
        }

        let id = read_trim(dir.join("name")).unwrap_or_else(|| {
            dir.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("hwmon")
                .to_owned()
        });
        fan_curves.push(FanCurveCapability {
            id,
            status: CapabilityStatus::ProbeOnly,
            path: Some(dir.display().to_string()),
            point_paths,
        });
    }
    fan_curves.sort_by(|left, right| left.id.cmp(&right.id));
    fan_curves
}

fn is_fan_curve_point(name: &str) -> bool {
    name.starts_with("pwm") && name.contains("_auto_point")
}

fn detect_leds(root: &Path) -> Vec<LedCapability> {
    let leds_root = root.join("sys/class/leds");
    let Ok(entries) = fs::read_dir(leds_root) else {
        return Vec::new();
    };

    let mut leds = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        let brightness_path = dir.join("brightness");
        if !brightness_path.exists() {
            continue;
        }
        let Some(name) = dir
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
        else {
            continue;
        };

        leds.push(LedCapability {
            name,
            path: brightness_path.display().to_string(),
            brightness: read_i64(&brightness_path),
            max_brightness: read_i64(dir.join("max_brightness")),
        });
    }
    leds.sort_by(|left, right| left.name.cmp(&right.name));
    leds
}

fn detect_firmware_attributes(root: &Path) -> Vec<FirmwareAttributeCapability> {
    let attributes_root = root.join("sys/class/firmware-attributes");
    let Ok(providers) = fs::read_dir(attributes_root) else {
        return Vec::new();
    };

    let mut attributes = Vec::new();
    for provider in providers.flatten() {
        let provider_attributes = provider.path().join("attributes");
        let Ok(entries) = fs::read_dir(provider_attributes) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.join("current_value").exists() || !path.join("display_name").exists() {
                continue;
            }
            let Some(name) = path
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
            else {
                continue;
            };
            attributes.push(FirmwareAttributeCapability {
                name,
                current_value: read_trim(path.join("current_value")),
                display_name: read_trim(path.join("display_name")),
                path: path.display().to_string(),
            });
        }
    }
    attributes
}

fn detect_ideapad_toggles(root: &Path) -> Vec<IdeapadToggleCapability> {
    let base = root.join("sys/bus/platform/drivers/ideapad_acpi");
    let Ok(entries) = fs::read_dir(base) else {
        return Vec::new();
    };

    let toggle_names = [
        "fn_lock",
        "touchpad",
        "camera_power",
        "usb_charging",
        "conservation_mode",
        "fan_mode",
    ];
    let mut toggles = Vec::new();
    for entry in entries.flatten() {
        let device = entry.path();
        if !device.is_dir() {
            continue;
        }
        for name in toggle_names {
            let path = device.join(name);
            if !path.exists() {
                continue;
            }
            toggles.push(IdeapadToggleCapability {
                name: name.to_owned(),
                status: CapabilityStatus::ProbeOnly,
                current_value: read_trim(&path),
                path: Some(path.display().to_string()),
            });
        }
    }
    toggles.sort_by(|left, right| left.name.cmp(&right.name));
    toggles
}

fn read_i64(path: impl AsRef<Path>) -> Option<i64> {
    read_trim(path).and_then(|value| value.parse().ok())
}

fn read_trim(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn parses_platform_profile_choices() {
        assert_eq!(
            parse_choices("low-power balanced performance\n"),
            ["low-power", "balanced", "performance"]
        );
    }

    #[test]
    fn parses_battery_charge_type_choices() {
        assert_eq!(
            parse_choices("Standard Conservation Fast"),
            ["Standard", "Conservation", "Fast"]
        );
        assert_eq!(
            parse_choices("Fast Standard [Long_Life]"),
            ["Fast", "Standard", "Long_Life"]
        );
        assert_eq!(
            parse_marked_current_choice("Fast Standard [Long_Life]").as_deref(),
            Some("Long_Life")
        );
    }

    #[test]
    fn detects_fixture_without_hardcoded_hwmon_number() {
        let registry = probe(&ProbeOptions {
            sysfs_root: fixture_root(),
        });

        assert_eq!(
            registry.hardware.product_version.as_deref(),
            Some("Legion Pro 5 16ARX8")
        );
        assert_eq!(
            registry
                .platform_profile
                .as_ref()
                .map(|capability| capability.choices.clone()),
            Some(vec![
                "low-power".to_owned(),
                "balanced".to_owned(),
                "performance".to_owned()
            ])
        );
        assert!(registry.hwmon_sensors.iter().any(|sensor| {
            sensor.hwmon_name.as_deref() == Some("legion-hwmon")
                && sensor.label.as_deref() == Some("CPU Fan")
                && sensor.kind == "fan"
        }));
        assert!(registry.fan_curves.iter().any(|fan_curve| {
            fan_curve.id == "legion-hwmon"
                && fan_curve
                    .point_paths
                    .iter()
                    .any(|path| path.ends_with("pwm1_auto_point1_pwm"))
        }));
        assert!(registry.leds.iter().any(|led| {
            led.name == "platform::ylogo"
                && led.brightness == Some(1)
                && led.max_brightness == Some(1)
        }));
        assert!(registry.ideapad_toggles.iter().any(|toggle| {
            toggle.name == "conservation_mode" && toggle.current_value.as_deref() == Some("1")
        }));
        let battery = registry.telemetry.battery.as_ref().unwrap();
        assert_eq!(battery.name, "BAT0");
        assert_eq!(battery.capacity_percent, Some(79));
        assert_eq!(battery.status.as_deref(), Some("Charging"));
        assert_eq!(battery.health.as_deref(), Some("Good"));
    }

    #[test]
    fn detects_runtime_fixture_with_bracketed_charge_type_current() {
        let registry = probe(&ProbeOptions {
            sysfs_root: runtime_fixture_root(),
        });

        assert_eq!(
            registry
                .platform_profile
                .as_ref()
                .and_then(|capability| capability.current.as_deref()),
            Some("performance")
        );
        assert_eq!(
            registry
                .platform_profile
                .as_ref()
                .map(|capability| capability.choices.clone()),
            Some(vec![
                "quiet".to_owned(),
                "balanced".to_owned(),
                "balanced-performance".to_owned(),
                "performance".to_owned()
            ])
        );
        assert_eq!(
            registry
                .battery_charge_type
                .as_ref()
                .and_then(|capability| capability.current.as_deref()),
            Some("Long_Life")
        );
        assert_eq!(
            registry
                .battery_charge_type
                .as_ref()
                .map(|capability| capability.choices.clone()),
            Some(vec![
                "Fast".to_owned(),
                "Standard".to_owned(),
                "Long_Life".to_owned()
            ])
        );
        assert!(registry.hwmon_sensors.len() > 10);
        assert!(registry.fan_curves.iter().any(|fan_curve| {
            fan_curve.id == "legion_hwmon" && fan_curve.point_paths.len() >= 20
        }));
        assert!(registry
            .leds
            .iter()
            .any(|led| led.name == "platform::ylogo"));
    }

    #[test]
    fn handles_missing_sysfs_paths_cleanly() {
        let root = std::env::temp_dir().join(format!(
            "legion-probe-empty-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let registry = probe(&ProbeOptions {
            sysfs_root: root.clone(),
        });

        assert!(registry.platform_profile.is_none());
        assert!(registry.battery_charge_type.is_none());
        assert!(registry.hwmon_sensors.is_empty());
        assert!(registry.fan_curves.is_empty());
        assert!(registry.leds.is_empty());
        assert!(registry.ideapad_toggles.is_empty());
        assert!(registry.telemetry.battery.is_none());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detects_firmware_attributes_only_with_metadata() {
        let registry = probe(&ProbeOptions {
            sysfs_root: fixture_root(),
        });

        assert!(registry
            .firmware_attributes
            .iter()
            .any(|attribute| attribute.name == "CustomMode"));
    }

    fn fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("tests/fixtures/sysfs-82wm-confirmed")
    }

    fn runtime_fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("tests/fixtures/sysfs-82wm-runtime-capture")
    }
}
