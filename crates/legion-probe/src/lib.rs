use std::fs;
use std::path::{Path, PathBuf};

use legion_common::{
    BatteryChargeTypeCapability, Capability, CapabilityRegistry, CapabilityStatus,
    FirmwareAttributeCapability, HardwareSummary, HwmonSensor, PlatformProfileCapability,
    RiskLevel, TelemetrySnapshot,
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
    let hwmon_sensors = detect_hwmon_sensors(&options.sysfs_root);
    let firmware_attributes = detect_firmware_attributes(&options.sysfs_root);

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

    CapabilityRegistry {
        hardware: detect_hardware(&options.sysfs_root),
        capabilities,
        platform_profile,
        battery_charge_type,
        telemetry: TelemetrySnapshot {
            sensors: hwmon_sensors.clone(),
        },
        hwmon_sensors,
        firmware_attributes,
        ..CapabilityRegistry::default()
    }
}

pub fn parse_choices(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(str::to_owned).collect()
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
    let current = read_trim(&path);
    let choices = read_trim(&choices_path).map_or_else(Vec::new, |value| parse_choices(&value));

    if current.is_none() && choices.is_empty() {
        return None;
    }

    Some(BatteryChargeTypeCapability {
        current,
        choices,
        path: path.display().to_string(),
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
}
