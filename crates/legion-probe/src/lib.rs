use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use legion_common::{
    AcAdapterTelemetry, AmdGpuPowerDpmCapability, BatteryChargeTypeCapability, BatteryTelemetry,
    Capability, CapabilityRegistry, CapabilityStatus, CpuPowerCapability, FanCurveCapability,
    FirmwareAttributeCapability, GpuCapability, HardwareSummary, HwmonSensor,
    IdeapadToggleCapability, LedCapability, PlatformProfileCapability, RiskLevel,
    TelemetrySnapshot, ThermalZone,
};

mod power_profiles;

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
    let gpu = detect_envycontrol_gpu(&options.sysfs_root);
    let amd_gpu_power_dpm = detect_amd_gpu_power_dpm(&options.sysfs_root);
    let cpu_power = detect_cpu_power(&options.sysfs_root);
    let thermal_zones = detect_thermal_zones(&options.sysfs_root);
    let ac_adapters = detect_ac_adapters(&options.sysfs_root);
    let power_profiles = power_profiles::detect_power_profiles(&options.sysfs_root);

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
    push_capability(&mut capabilities, "gpu", "GPU mode", gpu.is_some());
    push_capability(
        &mut capabilities,
        "amd_gpu_power_dpm",
        "AMD GPU power DPM",
        amd_gpu_power_dpm.is_some(),
    );
    push_capability(
        &mut capabilities,
        "cpu_power",
        "CPU frequency scaling",
        cpu_power.is_some(),
    );
    push_capability(
        &mut capabilities,
        "thermal_zones",
        "ACPI thermal zones",
        !thermal_zones.is_empty(),
    );
    push_capability(
        &mut capabilities,
        "power_profiles",
        "Desktop PowerProfiles",
        power_profiles
            .as_ref()
            .and_then(|probe| probe.unique_owner.as_ref())
            .is_some(),
    );

    CapabilityRegistry {
        hardware: detect_hardware(&options.sysfs_root),
        capabilities,
        platform_profile,
        battery_charge_type,
        telemetry: TelemetrySnapshot {
            sensors: hwmon_sensors.clone(),
            battery: battery_telemetry,
            ac_adapters,
        },
        hwmon_sensors,
        fan_curves,
        leds,
        firmware_attributes,
        ideapad_toggles,
        gpu,
        amd_gpu_power_dpm,
        cpu_power,
        thermal_zones,
        power_profiles,
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

pub fn parse_envycontrol_mode(raw: &str) -> Option<String> {
    raw.split(|character: char| {
        !character.is_ascii_alphanumeric() && character != '-' && character != '_'
    })
    .map(str::to_ascii_lowercase)
    .find(|token| matches!(token.as_str(), "integrated" | "hybrid" | "nvidia"))
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
        choices_path: choices_path.display().to_string(),
    })
}

fn detect_battery_charge_type(root: &Path) -> Option<BatteryChargeTypeCapability> {
    let path = root.join("sys/class/power_supply/BAT0/charge_type");
    let choices_path = root.join("sys/class/power_supply/BAT0/charge_types");
    let choices_raw = read_trim(&choices_path);
    let current =
        read_trim(&path).or_else(|| choices_raw.as_deref().and_then(parse_marked_current_choice));
    let choices = choices_raw.map_or_else(Vec::new, |value| parse_choices(&value));
    let write_path = if path.exists() { &path } else { &choices_path };

    if current.is_none() && choices.is_empty() {
        return None;
    }

    Some(BatteryChargeTypeCapability {
        current,
        choices,
        path: write_path.display().to_string(),
        choices_path: choices_path.display().to_string(),
    })
}

fn detect_battery_telemetry(root: &Path) -> Option<BatteryTelemetry> {
    let path = root.join("sys/class/power_supply/BAT0");
    let capacity_percent = read_i64(path.join("capacity"));
    let status = read_trim(path.join("status"));
    let health = read_trim(path.join("health"));
    let power_now_uw = read_i64(path.join("power_now"));
    let cycle_count = read_i64(path.join("cycle_count"));
    let energy_full_uwh = read_i64(path.join("energy_full"));
    let energy_full_design_uwh = read_i64(path.join("energy_full_design"));
    let energy_now_uwh = read_i64(path.join("energy_now"));
    let voltage_now_uv = read_i64(path.join("voltage_now"));
    let capacity_level = read_trim(path.join("capacity_level"));
    let technology = read_trim(path.join("technology"));
    let model_name = read_trim(path.join("model_name"));
    let manufacturer = read_trim(path.join("manufacturer"));

    if capacity_percent.is_none()
        && status.is_none()
        && health.is_none()
        && power_now_uw.is_none()
        && cycle_count.is_none()
        && energy_full_uwh.is_none()
        && energy_now_uwh.is_none()
    {
        return None;
    }

    Some(BatteryTelemetry {
        name: "BAT0".to_owned(),
        path: path.display().to_string(),
        capacity_percent,
        status,
        health,
        power_now_uw,
        cycle_count,
        energy_full_uwh,
        energy_full_design_uwh,
        energy_now_uwh,
        voltage_now_uv,
        capacity_level,
        technology,
        model_name,
        manufacturer,
    })
}

fn detect_ac_adapters(root: &Path) -> Vec<AcAdapterTelemetry> {
    let supply_root = root.join("sys/class/power_supply");
    let Ok(entries) = fs::read_dir(supply_root) else {
        return Vec::new();
    };

    let mut adapters = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        if read_trim(dir.join("type")).as_deref() != Some("Mains") {
            continue;
        }
        let Some(name) = dir
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
        else {
            continue;
        };
        adapters.push(AcAdapterTelemetry {
            name,
            path: dir.display().to_string(),
            online: read_i64(dir.join("online")).map(|value| value != 0),
        });
    }
    adapters.sort_by(|left, right| left.name.cmp(&right.name));
    adapters
}

fn detect_thermal_zones(root: &Path) -> Vec<ThermalZone> {
    let thermal_root = root.join("sys/class/thermal");
    let Ok(entries) = fs::read_dir(thermal_root) else {
        return Vec::new();
    };

    let mut zones = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        let Some(name) = dir.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with("thermal_zone") {
            continue;
        }
        let temp_path = dir.join("temp");
        if !temp_path.exists() {
            continue;
        }
        zones.push(ThermalZone {
            name: name.to_owned(),
            zone_type: read_trim(dir.join("type")),
            temp_millicelsius: read_i64(&temp_path),
            path: dir.display().to_string(),
        });
    }
    zones.sort_by(|left, right| left.name.cmp(&right.name));
    zones
}

fn detect_cpu_power(root: &Path) -> Option<CpuPowerCapability> {
    let cpufreq = root.join("sys/devices/system/cpu/cpufreq");
    let policy = cpufreq.join("policy0");
    if !policy.is_dir() {
        return None;
    }

    let governor_path = policy.join("scaling_governor");
    let epp_path = policy.join("energy_performance_preference");
    let boost_path = cpufreq.join("boost");
    let amd_status_path = root.join("sys/devices/system/cpu/amd_pstate/status");

    let governor = read_trim(&governor_path);
    let available_governors = read_trim(policy.join("scaling_available_governors"))
        .map_or_else(Vec::new, |value| parse_choices(&value));
    let epp = read_trim(&epp_path);
    let available_epp = read_trim(policy.join("energy_performance_available_preferences"))
        .map_or_else(Vec::new, |value| parse_choices(&value));
    let boost = read_i64(&boost_path).map(|value| value != 0);

    if governor.is_none() && epp.is_none() && boost.is_none() {
        return None;
    }

    Some(CpuPowerCapability {
        status: CapabilityStatus::ProbeOnly,
        scaling_driver: read_trim(policy.join("scaling_driver")),
        amd_pstate_status: read_trim(&amd_status_path),
        governor,
        available_governors,
        epp,
        available_epp,
        boost,
        scaling_min_khz: read_i64(policy.join("scaling_min_freq")),
        scaling_max_khz: read_i64(policy.join("scaling_max_freq")),
        scaling_cur_khz: read_i64(policy.join("scaling_cur_freq")),
        cpuinfo_min_khz: read_i64(policy.join("cpuinfo_min_freq")),
        cpuinfo_max_khz: read_i64(policy.join("cpuinfo_max_freq")),
        governor_path: path_if_exists(&governor_path),
        epp_path: path_if_exists(&epp_path),
        boost_path: path_if_exists(&boost_path),
    })
}

fn path_if_exists(path: &Path) -> String {
    if path.exists() {
        path.display().to_string()
    } else {
        String::new()
    }
}

fn detect_amd_gpu_power_dpm(root: &Path) -> Option<AmdGpuPowerDpmCapability> {
    let drm_root = root.join("sys/class/drm");
    let Ok(entries) = fs::read_dir(drm_root) else {
        return None;
    };

    let mut cards = entries
        .flatten()
        .filter_map(|entry| {
            let card = entry.file_name().to_string_lossy().to_string();
            if !card.starts_with("card") || card.contains('-') {
                return None;
            }
            Some((card, entry.path().join("device")))
        })
        .collect::<Vec<_>>();
    cards.sort_by(|a, b| a.0.cmp(&b.0));

    for (card, device) in cards {
        let Some(vendor) = read_trim(device.join("vendor")) else {
            continue;
        };
        if !vendor.eq_ignore_ascii_case("0x1002") {
            continue;
        }

        let force_path = device.join("power_dpm_force_performance_level");
        if !force_path.exists() {
            continue;
        }

        return Some(AmdGpuPowerDpmCapability {
            card,
            status: CapabilityStatus::ProbeOnly,
            vendor,
            force_performance_level_path: force_path.display().to_string(),
            current_force_performance_level: read_trim(&force_path),
            power_dpm_state: read_trim(device.join("power_dpm_state")),
            current_sclk: read_marked_dpm_clock(device.join("pp_dpm_sclk")),
            current_mclk: read_marked_dpm_clock(device.join("pp_dpm_mclk")),
            choices: vec!["auto".to_owned(), "low".to_owned()],
        });
    }

    None
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
                min_value: read_trim(path.join("min_value")),
                max_value: read_trim(path.join("max_value")),
                scalar_increment: read_trim(path.join("scalar_increment")),
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

fn detect_envycontrol_gpu(root: &Path) -> Option<GpuCapability> {
    if root != Path::new("/") {
        return None;
    }

    let output = Command::new("envycontrol").arg("--query").output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mode = parse_envycontrol_mode(&stdout).or_else(|| parse_envycontrol_mode(&stderr));

    Some(GpuCapability {
        provider: "envycontrol".to_owned(),
        status: if output.status.success() && mode.is_some() {
            CapabilityStatus::ProbeOnly
        } else {
            CapabilityStatus::Unsupported
        },
        mode,
    })
}

fn read_i64(path: impl AsRef<Path>) -> Option<i64> {
    read_trim(path).and_then(|value| value.parse().ok())
}

fn read_marked_dpm_clock(path: impl AsRef<Path>) -> Option<String> {
    let raw = read_trim(path)?;
    raw.lines()
        .map(str::trim)
        .find(|line| line.ends_with('*'))
        .map(str::to_owned)
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
    fn parses_envycontrol_mode_from_common_outputs() {
        assert_eq!(
            parse_envycontrol_mode("nvidia\n").as_deref(),
            Some("nvidia")
        );
        assert_eq!(
            parse_envycontrol_mode("Current GPU mode: hybrid").as_deref(),
            Some("hybrid")
        );
        assert_eq!(
            parse_envycontrol_mode("mode = integrated").as_deref(),
            Some("integrated")
        );
        assert!(parse_envycontrol_mode("unsupported").is_none());
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
        assert!(registry
            .platform_profile
            .as_ref()
            .unwrap()
            .choices_path
            .ends_with("sys/firmware/acpi/platform_profile_choices"));
        assert!(registry.hwmon_sensors.iter().any(|sensor| {
            sensor.hwmon_name.as_deref() == Some("legion-hwmon")
                && sensor.label.as_deref() == Some("CPU Fan")
                && sensor.kind == "fan"
        }));
        assert!(registry.fan_curves.iter().any(|fan_curve| {
            fan_curve.id == "legion-hwmon"
                && fan_curve.point_paths.len() >= 20
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
        assert!(registry.gpu.is_none());
        let amd_gpu_power_dpm = registry.amd_gpu_power_dpm.as_ref().unwrap();
        assert_eq!(amd_gpu_power_dpm.card, "card1");
        assert_eq!(
            amd_gpu_power_dpm.current_force_performance_level.as_deref(),
            Some("auto")
        );
        assert!(amd_gpu_power_dpm
            .choices
            .iter()
            .any(|choice| choice == "low"));
        assert!(registry.power_profiles.is_none());
        let battery = registry.telemetry.battery.as_ref().unwrap();
        assert_eq!(battery.name, "BAT0");
        assert_eq!(battery.capacity_percent, Some(79));
        assert_eq!(battery.status.as_deref(), Some("Charging"));
        assert_eq!(battery.health.as_deref(), Some("Good"));
        assert_eq!(battery.energy_full_uwh, Some(70_970_000));
        assert_eq!(battery.technology.as_deref(), Some("Li-poly"));
        assert_eq!(battery.capacity_level.as_deref(), Some("Normal"));

        let cpu = registry.cpu_power.as_ref().unwrap();
        assert_eq!(cpu.scaling_driver.as_deref(), Some("amd-pstate-epp"));
        assert_eq!(cpu.amd_pstate_status.as_deref(), Some("active"));
        assert_eq!(cpu.governor.as_deref(), Some("powersave"));
        assert_eq!(cpu.epp.as_deref(), Some("balance_power"));
        assert!(cpu.available_epp.iter().any(|e| e == "balance_performance"));
        assert_eq!(cpu.boost, Some(true));
        assert!(cpu.governor_path.ends_with("policy0/scaling_governor"));

        assert!(registry.thermal_zones.iter().any(
            |z| z.zone_type.as_deref() == Some("acpitz") && z.temp_millicelsius == Some(52000)
        ));
        assert!(registry
            .telemetry
            .ac_adapters
            .iter()
            .any(|a| a.name == "ADP0" && a.online == Some(true)));
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
        assert!(registry
            .platform_profile
            .as_ref()
            .unwrap()
            .choices_path
            .ends_with("sys/firmware/acpi/platform_profile_choices"));
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
        assert!(registry
            .battery_charge_type
            .as_ref()
            .unwrap()
            .path
            .ends_with("sys/class/power_supply/BAT0/charge_types"));
        assert!(registry
            .battery_charge_type
            .as_ref()
            .unwrap()
            .choices_path
            .ends_with("sys/class/power_supply/BAT0/charge_types"));
        assert!(registry.hwmon_sensors.len() > 10);
        assert!(registry.fan_curves.iter().any(|fan_curve| {
            fan_curve.id == "legion_hwmon" && fan_curve.point_paths.len() >= 20
        }));
        assert!(registry
            .leds
            .iter()
            .any(|led| led.name == "platform::ylogo"));
        assert!(registry.power_profiles.is_none());
    }

    #[test]
    fn detects_amd_gpu_power_dpm_without_hardcoded_card_number() {
        let root = std::env::temp_dir().join(format!(
            "legion-probe-amdgpu-dpm-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let device = root.join("sys/class/drm/card7/device");
        fs::create_dir_all(&device).unwrap();
        fs::write(device.join("vendor"), "0x1002\n").unwrap();
        fs::write(device.join("power_dpm_force_performance_level"), "auto\n").unwrap();
        fs::write(device.join("power_dpm_state"), "performance\n").unwrap();
        fs::write(device.join("pp_dpm_sclk"), "0: 400Mhz\n1: 700Mhz *\n").unwrap();
        fs::write(device.join("pp_dpm_mclk"), "0: 1000Mhz *\n1: 1600Mhz\n").unwrap();

        let registry = probe(&ProbeOptions {
            sysfs_root: root.clone(),
        });
        let capability = registry.amd_gpu_power_dpm.as_ref().unwrap();
        assert_eq!(capability.card, "card7");
        assert_eq!(
            capability.current_force_performance_level.as_deref(),
            Some("auto")
        );
        assert_eq!(capability.power_dpm_state.as_deref(), Some("performance"));
        assert_eq!(capability.current_sclk.as_deref(), Some("1: 700Mhz *"));
        assert_eq!(capability.current_mclk.as_deref(), Some("0: 1000Mhz *"));

        fs::remove_dir_all(root).unwrap();
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
        assert!(registry.telemetry.ac_adapters.is_empty());
        assert!(registry.amd_gpu_power_dpm.is_none());
        assert!(registry.cpu_power.is_none());
        assert!(registry.thermal_zones.is_empty());
        assert!(registry.power_profiles.is_none());

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
