use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use legion_common::{
    AcAdapterTelemetry, AmdGpuPowerDpmCapability, BatteryChargeTypeCapability, BatteryTelemetry,
    Capability, CapabilityRegistry, CapabilityStatus, CpuPowerCapability, FanCurveCapability,
    FirmwareAttributeCapability, GpuCapability, GpuRuntimeCapability, GpuSwitchType,
    HardwareSummary, HwmonSensor, IdeapadToggleCapability, KeyboardRgbCandidate,
    KeyboardRgbCapability, KeyboardRgbHidReport, KeyboardRgbOpenRgbDevice,
    KeyboardRgbOpenRgbStatus, LedCapability, PlatformProfileCapability, RiskLevel,
    TelemetrySnapshot, ThermalZone, WirelessPowerCapability,
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
    probe_inner(options, false)
}

pub fn probe_with_openrgb_details(options: &ProbeOptions) -> CapabilityRegistry {
    probe_inner(options, true)
}

fn probe_inner(options: &ProbeOptions, include_openrgb_details: bool) -> CapabilityRegistry {
    let platform_profile = detect_platform_profile(&options.sysfs_root);
    let battery_charge_type = detect_battery_charge_type(&options.sysfs_root);
    let battery_telemetry = detect_battery_telemetry(&options.sysfs_root);
    let hwmon_sensors = detect_hwmon_sensors(&options.sysfs_root);
    let fan_curves = detect_fan_curves(&options.sysfs_root);
    let leds = detect_leds(&options.sysfs_root);
    let keyboard_rgb_candidates = detect_keyboard_rgb_candidates(&options.sysfs_root);
    let keyboard_rgb = detect_keyboard_rgb_capability(&keyboard_rgb_candidates);
    let keyboard_rgb_openrgb =
        detect_keyboard_rgb_openrgb(&options.sysfs_root, include_openrgb_details);
    let firmware_attributes = detect_firmware_attributes(&options.sysfs_root);
    let ideapad_toggles = detect_ideapad_toggles(&options.sysfs_root);
    let gpu = detect_envycontrol_gpu(&options.sysfs_root);
    let gpu_runtime = detect_gpu_runtime_capability(&options.sysfs_root, gpu.as_ref());
    let amd_gpu_power_dpm = detect_amd_gpu_power_dpm(&options.sysfs_root);
    let cpu_power = detect_cpu_power(&options.sysfs_root);
    let wireless_power = detect_wireless_power(&options.sysfs_root);
    let thermal_zones = detect_thermal_zones(&options.sysfs_root);
    let ac_adapters = detect_ac_adapters(&options.sysfs_root);
    let power_profiles = power_profiles::detect_power_profiles(&options.sysfs_root);
    // Raw ACPI/WMI method invocation is intentionally unsupported. Keep the
    // capability visible as unavailable until a safe kernel telemetry interface
    // is discovered.
    let wmi_sensors: Vec<HwmonSensor> = Vec::new();

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
        "wmi_sensors",
        "WMI EC sensors (fan RPM, CPU/GPU temp)",
        !wmi_sensors.is_empty(),
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
        "keyboard_rgb_candidates",
        "Keyboard RGB candidates",
        !keyboard_rgb_candidates.is_empty(),
    );
    push_capability(
        &mut capabilities,
        "ideapad_toggles",
        "Ideapad toggles",
        !ideapad_toggles.is_empty(),
    );
    push_capability(&mut capabilities, "gpu", "GPU mode", gpu.is_some());
    push_capability(
        &mut capabilities,
        "gpu_runtime",
        "GPU PCI hotswap",
        gpu_runtime.is_some(),
    );
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
    push_capability(
        &mut capabilities,
        "wireless_power",
        "Wireless power save",
        !wireless_power.is_empty(),
    );

    let mut all_sensors = hwmon_sensors;
    all_sensors.extend(wmi_sensors);

    CapabilityRegistry {
        hardware: detect_hardware(&options.sysfs_root),
        capabilities,
        platform_profile,
        battery_charge_type,
        telemetry: TelemetrySnapshot {
            sensors: all_sensors.clone(),
            battery: battery_telemetry,
            ac_adapters,
        },
        hwmon_sensors: all_sensors,
        fan_curves,
        leds,
        keyboard_rgb,
        keyboard_rgb_candidates,
        keyboard_rgb_openrgb,
        firmware_attributes,
        ideapad_toggles,
        gpu,
        gpu_runtime,
        amd_gpu_power_dpm,
        cpu_power,
        wireless_power,
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
    let custom_profile = detect_custom_platform_profile_handler(root);

    if current.is_none() && choices.is_empty() {
        return None;
    }

    Some(PlatformProfileCapability {
        current,
        choices,
        path: path.display().to_string(),
        choices_path: choices_path.display().to_string(),
        custom_profile_path: custom_profile
            .as_ref()
            .map(|profile| profile.path.display().to_string()),
        custom_profile_driver: custom_profile.map(|profile| profile.driver),
    })
}

#[derive(Debug, Clone)]
struct CustomPlatformProfileHandler {
    driver: String,
    path: PathBuf,
}

fn detect_custom_platform_profile_handler(root: &Path) -> Option<CustomPlatformProfileHandler> {
    let class_root = root.join("sys/class/platform-profile");
    let entries = std::fs::read_dir(class_root).ok()?;
    let mut fallback = None;
    for entry in entries.flatten() {
        let profile_root = entry.path();
        let driver = read_trim(profile_root.join("name"))?;
        let profile_path = profile_root.join("profile");
        let choices = read_trim(profile_root.join("choices"))
            .map(|value| parse_choices(&value))
            .unwrap_or_default();
        if !choices.iter().any(|choice| choice == "custom") || !profile_path.exists() {
            continue;
        }
        let handler = CustomPlatformProfileHandler {
            driver: driver.clone(),
            path: profile_path,
        };
        if driver == "lenovo-wmi-gamezone" {
            return Some(handler);
        }
        fallback.get_or_insert(handler);
    }
    fallback
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
    let mut scaling_max_paths = fs::read_dir(&cpufreq)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.flatten())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("policy"))
        })
        .map(|path| path.join("scaling_max_freq"))
        .filter(|path| path.exists())
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    scaling_max_paths.sort();

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
        scaling_max_paths,
    })
}

fn detect_wireless_power(root: &Path) -> Vec<WirelessPowerCapability> {
    let net_root = root.join("sys/class/net");
    let Ok(entries) = fs::read_dir(net_root) else {
        return Vec::new();
    };

    let mut interfaces = Vec::new();
    for entry in entries.flatten() {
        let iface_path = entry.path();
        if !iface_path.join("wireless").is_dir() {
            continue;
        }
        let Some(interface) = iface_path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
        else {
            continue;
        };
        let current_power_save = if root == Path::new("/") {
            read_iw_power_save(&interface)
        } else {
            read_trim(iface_path.join("wireless/power_save"))
        };
        interfaces.push(WirelessPowerCapability {
            interface,
            status: CapabilityStatus::ProbeOnly,
            driver: iface_path
                .join("device/driver")
                .read_link()
                .ok()
                .and_then(|path| {
                    path.file_name()
                        .map(|name| name.to_string_lossy().to_string())
                }),
            current_power_save,
            choices: vec!["off".to_owned(), "on".to_owned()],
            command: "iw".to_owned(),
        });
    }
    interfaces.sort_by(|left, right| left.interface.cmp(&right.interface));
    interfaces
}

fn read_iw_power_save(interface: &str) -> Option<String> {
    let output = Command::new("iw")
        .args(["dev", interface, "get", "power_save"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value = stdout.split(':').nth(1)?.trim().to_ascii_lowercase();
    match value.as_str() {
        "on" | "off" => Some(value),
        _ => None,
    }
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

fn detect_keyboard_rgb_candidates(root: &Path) -> Vec<KeyboardRgbCandidate> {
    let hidraw_root = root.join("sys/class/hidraw");
    let Ok(entries) = fs::read_dir(hidraw_root) else {
        return Vec::new();
    };

    let mut candidates = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(device_id) = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
        else {
            continue;
        };
        let device_dir = path.join("device");
        let uevent = read_uevent(&device_dir.join("uevent"));
        let hid_id = uevent.get("HID_ID").map(String::as_str);
        let name = uevent.get("HID_NAME").cloned();
        let modalias = uevent
            .get("MODALIAS")
            .cloned()
            .or_else(|| read_trim(device_dir.join("modalias")));
        let (hid_vendor, hid_product) = hid_id.and_then(parse_hid_id).unwrap_or((None, None));
        let (modalias_vendor, modalias_product) = modalias
            .as_deref()
            .and_then(parse_hid_modalias_ids)
            .unwrap_or((None, None));
        let vendor_id = hid_vendor.or(modalias_vendor);
        let product_id = hid_product.or(modalias_product);

        if !is_keyboard_rgb_candidate(vendor_id.as_deref(), name.as_deref(), modalias.as_deref()) {
            continue;
        }

        let mut evidence = Vec::new();
        if let Some(hid_id) = hid_id {
            evidence.push(format!("HID_ID={hid_id}"));
        }
        if let Some(name) = &name {
            evidence.push(format!("HID_NAME={name}"));
        }
        if let Some(modalias) = &modalias {
            evidence.push(format!("MODALIAS={modalias}"));
        }
        let descriptor = read_hid_report_descriptor(&device_dir);
        if let Some(bytes) = descriptor.bytes {
            evidence.push(format!("REPORT_DESCRIPTOR_BYTES={bytes}"));
        }
        if !descriptor.report_ids.is_empty() {
            evidence.push(format!(
                "REPORT_IDS={}",
                descriptor
                    .report_ids
                    .iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }

        candidates.push(KeyboardRgbCandidate {
            backend: "hidraw".to_owned(),
            device_id,
            path: path.display().to_string(),
            vendor_id,
            product_id,
            name,
            modalias,
            report_descriptor_bytes: descriptor.bytes,
            report_ids: descriptor.report_ids,
            hid_reports: descriptor.hid_reports,
            evidence,
        });
    }

    candidates.sort_by(|left, right| left.device_id.cmp(&right.device_id));
    candidates
}

struct HidReportDescriptorSummary {
    bytes: Option<usize>,
    report_ids: Vec<u8>,
    hid_reports: Vec<KeyboardRgbHidReport>,
}

fn read_hid_report_descriptor(device_dir: &Path) -> HidReportDescriptorSummary {
    let Ok(bytes) = fs::read(device_dir.join("report_descriptor")) else {
        return HidReportDescriptorSummary {
            bytes: None,
            report_ids: Vec::new(),
            hid_reports: Vec::new(),
        };
    };
    HidReportDescriptorSummary {
        bytes: Some(bytes.len()),
        report_ids: parse_hid_report_ids(&bytes),
        hid_reports: parse_hid_report_summaries(&bytes),
    }
}

fn parse_hid_report_ids(bytes: &[u8]) -> Vec<u8> {
    let mut ids = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        let prefix = bytes[index];
        index += 1;
        if prefix == 0xfe {
            if index + 2 > bytes.len() {
                break;
            }
            let size = bytes[index] as usize;
            index = index.saturating_add(2 + size);
            continue;
        }

        let size = match prefix & 0b11 {
            0 => 0usize,
            1 => 1,
            2 => 2,
            _ => 4,
        };
        let item_type = (prefix >> 2) & 0b11;
        let item_tag = (prefix >> 4) & 0b1111;
        if item_type == 1 && item_tag == 8 && size > 0 && index < bytes.len() {
            let id = bytes[index];
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        index = index.saturating_add(size);
    }
    ids.sort_unstable();
    ids
}

fn parse_hid_report_summaries(bytes: &[u8]) -> Vec<KeyboardRgbHidReport> {
    let mut reports = Vec::new();
    let mut index = 0usize;
    let mut report_id = None;
    let mut report_size_bits = 0u32;
    let mut report_count = 0u32;

    while index < bytes.len() {
        let prefix = bytes[index];
        index += 1;
        if prefix == 0xfe {
            if index + 2 > bytes.len() {
                break;
            }
            let size = bytes[index] as usize;
            index = index.saturating_add(2 + size);
            continue;
        }

        let size = match prefix & 0b11 {
            0 => 0usize,
            1 => 1,
            2 => 2,
            _ => 4,
        };
        if index + size > bytes.len() {
            break;
        }
        let item_type = (prefix >> 2) & 0b11;
        let item_tag = (prefix >> 4) & 0b1111;
        let value = hid_item_u32(&bytes[index..index + size]);

        match (item_type, item_tag) {
            (1, 7) => report_size_bits = value,
            (1, 8) if size > 0 => report_id = Some(bytes[index]),
            (1, 9) => report_count = value,
            (0, 8) => push_hid_report_summary(
                &mut reports,
                report_id,
                "input",
                report_size_bits,
                report_count,
            ),
            (0, 9) => push_hid_report_summary(
                &mut reports,
                report_id,
                "output",
                report_size_bits,
                report_count,
            ),
            (0, 11) => push_hid_report_summary(
                &mut reports,
                report_id,
                "feature",
                report_size_bits,
                report_count,
            ),
            _ => {}
        }

        index = index.saturating_add(size);
    }

    reports
}

fn hid_item_u32(bytes: &[u8]) -> u32 {
    bytes.iter().enumerate().fold(0u32, |value, (index, byte)| {
        value | ((*byte as u32) << (index * 8))
    })
}

fn push_hid_report_summary(
    reports: &mut Vec<KeyboardRgbHidReport>,
    report_id: Option<u8>,
    kind: &str,
    report_size_bits: u32,
    report_count: u32,
) {
    let bit_length = report_size_bits.saturating_mul(report_count);
    reports.push(KeyboardRgbHidReport {
        report_id,
        kind: kind.to_owned(),
        report_size_bits,
        report_count,
        bit_length,
        byte_length: bit_length.div_ceil(8),
    });
}

fn detect_keyboard_rgb_capability(
    candidates: &[KeyboardRgbCandidate],
) -> Option<KeyboardRgbCapability> {
    candidates.iter().find_map(|candidate| {
        let metadata_path = Path::new(&candidate.path)
            .join("device")
            .join("ratvantage-keyboard-rgb.json");
        let raw = fs::read_to_string(metadata_path).ok()?;
        let capability = serde_json::from_str::<KeyboardRgbCapability>(&raw).ok()?;
        if capability.backend == candidate.backend
            && capability.device_id == candidate.device_id
            && !capability.path.trim().is_empty()
        {
            Some(capability)
        } else {
            None
        }
    })
}

fn detect_keyboard_rgb_openrgb(
    root: &Path,
    include_details: bool,
) -> Option<KeyboardRgbOpenRgbStatus> {
    if root != Path::new("/") {
        let fixture_path = root.join("ratvantage-keyboard-rgb-openrgb.json");
        return fs::read_to_string(fixture_path)
            .ok()
            .and_then(|raw| serde_json::from_str::<KeyboardRgbOpenRgbStatus>(&raw).ok());
    }

    let openrgb_path = command_path("openrgb");
    let sdk_helper_path = command_path("ratvantage-openrgb-keyboard-rgb-sdk-helper");
    let devices = if include_details {
        openrgb_path
            .as_ref()
            .and_then(|path| openrgb_list_devices_output(path))
            .map(|stdout| parse_openrgb_keyboard_devices(&stdout))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let sdk_snapshot = if include_details {
        openrgb_path
            .as_ref()
            .zip(sdk_helper_path.as_ref())
            .and_then(|(openrgb, helper)| read_openrgb_sdk_snapshot(helper, openrgb))
    } else {
        None
    };
    let sdk_snapshot_supported = sdk_snapshot.is_some();
    let sdk_active_mode = sdk_snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.active_mode.clone());
    let sdk_color_zones = sdk_snapshot
        .as_ref()
        .map(|snapshot| snapshot.color_zones.clone())
        .unwrap_or_default();
    let sdk_colors = sdk_snapshot
        .as_ref()
        .map(|snapshot| snapshot.colors.clone())
        .unwrap_or_default();
    let backend_ready = !devices.is_empty() && sdk_snapshot_supported;

    Some(KeyboardRgbOpenRgbStatus {
        installed: openrgb_path.is_some(),
        path: openrgb_path,
        devices,
        i2c_dev_loaded: kernel_module_loaded("i2c_dev"),
        user_in_i2c_group: current_user_in_group("i2c"),
        has_i2c_rw_access: device_prefix_has_rw_access(Path::new("/dev"), "i2c-"),
        has_hidraw_rw_access: device_prefix_has_rw_access(Path::new("/dev"), "hidraw"),
        backend_ready,
        write_support_claimed: backend_ready,
        sdk_helper_installed: sdk_helper_path.is_some(),
        sdk_helper_path,
        sdk_server_running: sdk_snapshot_supported,
        sdk_snapshot_supported,
        sdk_active_mode,
        sdk_color_zones,
        sdk_colors,
    })
}

fn openrgb_list_devices_output(path: &str) -> Option<String> {
    openrgb_list_devices_output_with_timeout(path, Duration::from_secs(2))
}

fn openrgb_list_devices_output_with_timeout(path: &str, timeout: Duration) -> Option<String> {
    let mut child = Command::new(path)
        .arg("--list-devices")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = child.wait_with_output().ok()?;
                return status
                    .success()
                    .then(|| String::from_utf8_lossy(&output.stdout).into_owned());
            }
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OpenRgbSdkSnapshotProbe {
    active_mode: Option<String>,
    color_zones: Vec<String>,
    colors: BTreeMap<String, String>,
}

fn read_openrgb_sdk_snapshot(
    helper_path: &str,
    openrgb_path: &str,
) -> Option<OpenRgbSdkSnapshotProbe> {
    let output = Command::new(helper_path)
        .arg("snapshot")
        .arg(format!("openrgb-sdk:{openrgb_path}"))
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_openrgb_sdk_snapshot(&String::from_utf8_lossy(&output.stdout))
}

fn parse_openrgb_sdk_snapshot(raw: &str) -> Option<OpenRgbSdkSnapshotProbe> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    let active_mode = value
        .get("active_mode")
        .and_then(|mode| mode.as_str())
        .filter(|mode| !mode.is_empty())
        .map(ToOwned::to_owned);
    let colors: BTreeMap<String, String> = value
        .get("colors")
        .and_then(|colors| colors.as_object())
        .map(|colors| {
            colors
                .iter()
                .filter_map(|(zone, color)| {
                    color.as_str().map(|color| (zone.clone(), color.to_owned()))
                })
                .collect()
        })
        .unwrap_or_default();
    let mut color_zones: Vec<String> = colors.keys().cloned().collect();
    color_zones.sort();
    Some(OpenRgbSdkSnapshotProbe {
        active_mode,
        color_zones,
        colors,
    })
}

fn current_user_in_group(group: &str) -> bool {
    Command::new("id")
        .arg("-nG")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|groups| groups.split_whitespace().any(|name| name == group))
        .unwrap_or(false)
}

fn command_path(command: &str) -> Option<String> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {command}"))
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!path.is_empty()).then_some(path)
}

fn kernel_module_loaded(module: &str) -> bool {
    fs::read_to_string("/proc/modules")
        .map(|raw| {
            raw.lines()
                .any(|line| line.split_whitespace().next() == Some(module))
        })
        .unwrap_or(false)
}

fn device_prefix_has_rw_access(dev_root: &Path, prefix: &str) -> bool {
    let Ok(entries) = fs::read_dir(dev_root) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return false;
        };
        name.starts_with(prefix) && access_path(&path, "-r") && access_path(&path, "-w")
    })
}

fn access_path(path: &Path, flag: &str) -> bool {
    Command::new("test")
        .arg(flag)
        .arg(path)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn parse_openrgb_keyboard_devices(raw: &str) -> Vec<KeyboardRgbOpenRgbDevice> {
    let mut devices = Vec::new();
    let mut current: Option<KeyboardRgbOpenRgbDevice> = None;
    for line in raw.lines() {
        if let Some((index, name)) = parse_openrgb_device_header(line) {
            if let Some(device) = current.take() {
                if is_openrgb_keyboard_device(&device) {
                    devices.push(device);
                }
            }
            current = Some(KeyboardRgbOpenRgbDevice {
                index,
                name,
                device_type: None,
                description: None,
                modes: Vec::new(),
                current_mode: None,
                zones: Vec::new(),
                leds: Vec::new(),
            });
            continue;
        }

        let Some(device) = current.as_mut() else {
            continue;
        };
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key.trim() {
            "Type" => device.device_type = Some(value.trim().to_owned()),
            "Description" => device.description = Some(value.trim().to_owned()),
            "Modes" => {
                let (modes, current_mode) = parse_openrgb_list_with_current(value.trim());
                device.modes = modes;
                device.current_mode = current_mode;
            }
            "Zones" => device.zones = parse_openrgb_list(value.trim()),
            "LEDs" => device.leds = parse_openrgb_list(value.trim()),
            _ => {}
        }
    }

    if let Some(device) = current {
        if is_openrgb_keyboard_device(&device) {
            devices.push(device);
        }
    }
    devices
}

fn parse_openrgb_device_header(line: &str) -> Option<(u32, String)> {
    let (index, name) = line.split_once(':')?;
    let index = index.trim().parse().ok()?;
    let name = name.trim();
    (!name.is_empty()).then(|| (index, name.to_owned()))
}

fn is_openrgb_keyboard_device(device: &KeyboardRgbOpenRgbDevice) -> bool {
    let haystack = format!(
        "{} {} {} {}",
        device.name,
        device.description.as_deref().unwrap_or(""),
        device.zones.join(" "),
        device.leds.join(" ")
    )
    .to_ascii_lowercase();
    haystack.contains("lenovo")
        && (haystack.contains("keyboard")
            || haystack.contains("4-zone")
            || haystack.contains("left side"))
}

fn parse_openrgb_list_with_current(raw: &str) -> (Vec<String>, Option<String>) {
    let tokens = parse_openrgb_list(raw);
    let current = tokens.iter().find_map(|token| {
        token
            .strip_prefix('[')?
            .strip_suffix(']')
            .map(str::to_owned)
    });
    let modes = tokens
        .into_iter()
        .map(|token| {
            token
                .strip_prefix('[')
                .and_then(|token| token.strip_suffix(']'))
                .unwrap_or(&token)
                .to_owned()
        })
        .collect();
    (modes, current)
}

fn parse_openrgb_list(raw: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    for ch in raw.chars() {
        match ch {
            '\'' => {
                in_quote = !in_quote;
                if !in_quote && !current.trim().is_empty() {
                    values.push(current.trim().to_owned());
                    current.clear();
                }
            }
            ' ' | '\t' if !in_quote => {
                if !current.trim().is_empty() {
                    values.push(current.trim().to_owned());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        values.push(current.trim().to_owned());
    }
    values
}

fn read_uevent(path: &Path) -> BTreeMap<String, String> {
    let Some(raw) = read_trim(path) else {
        return BTreeMap::new();
    };
    raw.lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
}

fn parse_hid_id(raw: &str) -> Option<(Option<String>, Option<String>)> {
    let parts = raw.split(':').collect::<Vec<_>>();
    if parts.len() < 3 {
        return None;
    }
    Some((
        normalize_hex_id(parts[parts.len() - 2]),
        normalize_hex_id(parts[parts.len() - 1]),
    ))
}

fn parse_hid_modalias_ids(raw: &str) -> Option<(Option<String>, Option<String>)> {
    let vendor = extract_modalias_hex(raw, 'v').and_then(|value| normalize_hex_id(&value));
    let product = extract_modalias_hex(raw, 'p').and_then(|value| normalize_hex_id(&value));
    if vendor.is_none() && product.is_none() {
        None
    } else {
        Some((vendor, product))
    }
}

fn extract_modalias_hex(raw: &str, marker: char) -> Option<String> {
    let marker_index = raw.find(marker)?;
    let value = raw[marker_index + marker.len_utf8()..]
        .chars()
        .take_while(|ch| ch.is_ascii_hexdigit())
        .collect::<String>();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn normalize_hex_id(raw: &str) -> Option<String> {
    let hex = raw
        .trim()
        .trim_start_matches("0x")
        .trim_start_matches("0X")
        .chars()
        .filter(|ch| ch.is_ascii_hexdigit())
        .collect::<String>();
    if hex.is_empty() {
        return None;
    }
    let trimmed = hex.trim_start_matches('0');
    let normalized = if trimmed.is_empty() { "0" } else { trimmed };
    Some(format!("{normalized:0>4}").to_ascii_uppercase())
}

fn is_keyboard_rgb_candidate(
    vendor_id: Option<&str>,
    name: Option<&str>,
    modalias: Option<&str>,
) -> bool {
    vendor_id == Some("048D")
        || name.is_some_and(|name| name.contains("ITE"))
        || modalias.is_some_and(|modalias| modalias.to_ascii_uppercase().contains("V0000048D"))
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
                attribute_type: read_trim(path.join("type")),
                default_value: read_trim(path.join("default_value")),
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
        switch_type: GpuSwitchType::RebootRequired,
        switch_notes: vec![
            "EnvyControl mode changes are treated as reboot-required until a runtime mux path is detected and validated".to_owned(),
        ],
    })
}

fn detect_gpu_runtime_capability(
    root: &Path,
    gpu: Option<&GpuCapability>,
) -> Option<GpuRuntimeCapability> {
    if root != Path::new("/") {
        return None;
    }
    // PCI hotswap requires /sys/bus/pci/rescan to exist and envycontrol to be available.
    let rescan = root.join("sys/bus/pci/rescan");
    if !rescan.exists() {
        return None;
    }
    let gpu = gpu?;
    if gpu.provider != "envycontrol" || gpu.status != CapabilityStatus::ProbeOnly {
        return None;
    }
    let current_mode = gpu.mode.clone()?;
    // Only integrated and hybrid are candidates. Runtime execution remains blocked until
    // dedicated live evidence promotes this path.
    let candidate_runtime_modes = match current_mode.as_str() {
        "integrated" => vec!["hybrid".to_owned()],
        "hybrid" => vec!["integrated".to_owned()],
        _ => return None,
    };
    Some(GpuRuntimeCapability {
        status: CapabilityStatus::ProbeOnly,
        current_mode,
        candidate_runtime_modes,
        promotion_ready: false,
        evidence: vec![
            format!("{} exists", rescan.display()),
            "envycontrol reported integrated/hybrid current mode".to_owned(),
            "runtime execution requires ratvantage-review-gpu-mux-evidence --require-session-restart-confirmed or stricter evidence before promotion".to_owned(),
        ],
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
    use std::os::unix::fs::PermissionsExt;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn production_probe_does_not_invoke_raw_acpi_methods() {
        let source = include_str!("lib.rs");
        let proc_path = ["/proc/acpi/", "call"].concat();
        let raw_write = ["fs::", "write(proc_call"].concat();

        assert!(!source.contains(&proc_path));
        assert!(!source.contains(&raw_write));
    }

    #[test]
    fn parses_platform_profile_choices() {
        assert_eq!(
            parse_choices("low-power balanced performance\n"),
            ["low-power", "balanced", "performance"]
        );
    }

    #[test]
    fn detects_lenovo_gamezone_custom_platform_profile_handler() {
        let root = std::env::temp_dir().join(format!(
            "legion-probe-gamezone-platform-profile-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let firmware = root.join("sys/firmware/acpi");
        let gamezone = root.join("sys/class/platform-profile/platform-profile-0");
        fs::create_dir_all(&firmware).unwrap();
        fs::create_dir_all(&gamezone).unwrap();
        fs::write(firmware.join("platform_profile"), "low-power\n").unwrap();
        fs::write(
            firmware.join("platform_profile_choices"),
            "low-power balanced performance max-power custom\n",
        )
        .unwrap();
        fs::write(gamezone.join("name"), "lenovo-wmi-gamezone\n").unwrap();
        fs::write(gamezone.join("profile"), "low-power\n").unwrap();
        fs::write(
            gamezone.join("choices"),
            "low-power balanced performance max-power custom\n",
        )
        .unwrap();

        let registry = probe(&ProbeOptions {
            sysfs_root: root.clone(),
        });
        let capability = registry.platform_profile.as_ref().unwrap();
        assert_eq!(
            capability.custom_profile_driver.as_deref(),
            Some("lenovo-wmi-gamezone")
        );
        assert!(capability
            .custom_profile_path
            .as_deref()
            .unwrap()
            .ends_with("sys/class/platform-profile/platform-profile-0/profile"));

        fs::remove_dir_all(root).unwrap();
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
    fn parses_openrgb_lenovo_keyboard_device() {
        let devices = parse_openrgb_keyboard_devices(
            r#"Connection attempt failed
0: Lenovo 5 2023
  Type:           Laptop
  Description:    Lenovo 4-Zone device
  Modes: [Direct] Breathing 'Rainbow Wave' 'Spectrum Cycle'
  Zones: Keyboard
  LEDs: 'Left side' 'Left center' 'Right center' 'Right side'
1: Other device
  Type:           GPU
  Description:    Not keyboard
"#,
        );

        assert_eq!(devices.len(), 1);
        let device = &devices[0];
        assert_eq!(device.index, 0);
        assert_eq!(device.name, "Lenovo 5 2023");
        assert_eq!(device.device_type.as_deref(), Some("Laptop"));
        assert_eq!(device.description.as_deref(), Some("Lenovo 4-Zone device"));
        assert_eq!(
            device.modes,
            ["Direct", "Breathing", "Rainbow Wave", "Spectrum Cycle"]
        );
        assert_eq!(device.current_mode.as_deref(), Some("Direct"));
        assert_eq!(device.zones, ["Keyboard"]);
        assert_eq!(
            device.leds,
            ["Left side", "Left center", "Right center", "Right side"]
        );
    }

    #[test]
    fn openrgb_list_devices_probe_times_out() {
        let root = std::env::temp_dir().join(format!(
            "legion-probe-openrgb-timeout-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let script = root.join("openrgb-hang");
        fs::write(&script, "#!/bin/sh\nsleep 5\n").unwrap();
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).unwrap();

        let output = openrgb_list_devices_output_with_timeout(
            script.to_str().unwrap(),
            Duration::from_millis(100),
        );

        assert_eq!(output, None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parses_openrgb_sdk_snapshot_probe() {
        let snapshot = parse_openrgb_sdk_snapshot(
            r##"{"active_mode":"Direct","colors":{"right_side":"#000000","left_side":"#FFFFFF"}}"##,
        )
        .unwrap();

        assert_eq!(snapshot.active_mode.as_deref(), Some("Direct"));
        assert_eq!(
            snapshot.color_zones,
            vec!["left_side".to_owned(), "right_side".to_owned()]
        );
        assert_eq!(snapshot.colors["left_side"], "#FFFFFF");
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
    fn detects_keyboard_rgb_hidraw_candidates_without_claiming_support() {
        let root = std::env::temp_dir().join(format!(
            "legion-probe-keyboard-rgb-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let ite = root.join("sys/class/hidraw/hidraw7/device");
        let elan = root.join("sys/class/hidraw/hidraw8/device");
        fs::create_dir_all(&ite).unwrap();
        fs::create_dir_all(&elan).unwrap();
        fs::write(
            ite.join("uevent"),
            "DRIVER=hid-generic\nHID_ID=0003:0000048D:0000C985\nHID_NAME=ITE Tech. Inc. ITE Device(8295)\nMODALIAS=hid:b0003g0001v0000048Dp0000C985\n",
        )
        .unwrap();
        fs::write(
            ite.join("report_descriptor"),
            [
                0x05, 0x0c, 0x09, 0x01, 0xa1, 0x01, 0x85, 0x0f, 0x85, 0x10, 0xc0,
            ],
        )
        .unwrap();
        fs::write(
            elan.join("uevent"),
            "DRIVER=hid-generic\nHID_ID=0003:000004F3:0000327E\nHID_NAME=ELAN Touchpad\nMODALIAS=hid:b0003g0001v000004F3p0000327E\n",
        )
        .unwrap();

        let registry = probe(&ProbeOptions {
            sysfs_root: root.clone(),
        });

        assert!(registry.keyboard_rgb.is_none());
        assert_eq!(registry.keyboard_rgb_candidates.len(), 1);
        let candidate = &registry.keyboard_rgb_candidates[0];
        assert_eq!(candidate.backend, "hidraw");
        assert_eq!(candidate.device_id, "hidraw7");
        assert_eq!(candidate.vendor_id.as_deref(), Some("048D"));
        assert_eq!(candidate.product_id.as_deref(), Some("C985"));
        assert_eq!(
            candidate.name.as_deref(),
            Some("ITE Tech. Inc. ITE Device(8295)")
        );
        assert_eq!(candidate.report_descriptor_bytes, Some(11));
        assert_eq!(candidate.report_ids, [15, 16]);
        assert!(candidate.hid_reports.is_empty());
        assert!(candidate
            .evidence
            .iter()
            .any(|entry| entry == "REPORT_DESCRIPTOR_BYTES=11"));
        assert!(candidate
            .evidence
            .iter()
            .any(|entry| entry == "REPORT_IDS=15,16"));
        assert!(registry.capabilities.iter().any(|capability| {
            capability.id == "keyboard_rgb_candidates"
                && capability.status == CapabilityStatus::ProbeOnly
        }));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parses_hid_report_ids_from_short_items_only() {
        assert_eq!(parse_hid_report_ids(&[0x85, 0x01, 0x85, 0x02]), [1, 2]);
        assert_eq!(
            parse_hid_report_ids(&[0xfe, 0x04, 0x00, 0xaa, 0xbb, 0xcc, 0xdd, 0x85, 0x07]),
            [7]
        );
        assert!(parse_hid_report_ids(&[0xfe, 0x04]).is_empty());
    }

    #[test]
    fn parses_hid_report_summaries_from_main_items() {
        let reports = parse_hid_report_summaries(&[
            0x85, 0x05, // Report ID 5
            0x75, 0x08, // Report Size 8 bits
            0x95, 0x40, // Report Count 64
            0x91, 0x02, // Output
            0xb1, 0x02, // Feature
            0x95, 0x02, // Report Count 2
            0x81, 0x02, // Input
        ]);

        assert_eq!(reports.len(), 3);
        assert_eq!(reports[0].report_id, Some(5));
        assert_eq!(reports[0].kind, "output");
        assert_eq!(reports[0].report_size_bits, 8);
        assert_eq!(reports[0].report_count, 64);
        assert_eq!(reports[0].bit_length, 512);
        assert_eq!(reports[0].byte_length, 64);
        assert_eq!(reports[1].kind, "feature");
        assert_eq!(reports[1].byte_length, 64);
        assert_eq!(reports[2].kind, "input");
        assert_eq!(reports[2].byte_length, 2);
    }

    #[test]
    fn detects_keyboard_rgb_fixture_metadata_as_plan_capability() {
        let root = std::env::temp_dir().join(format!(
            "legion-probe-keyboard-rgb-metadata-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let device = root.join("sys/class/hidraw/hidraw9/device");
        fs::create_dir_all(&device).unwrap();
        fs::write(
            device.join("uevent"),
            "DRIVER=hid-generic\nHID_ID=0003:0000048D:0000C985\nHID_NAME=ITE Tech. Inc. ITE Device(8295)\nMODALIAS=hid:b0003g0001v0000048Dp0000C985\n",
        )
        .unwrap();
        let capability = KeyboardRgbCapability {
            backend: "hidraw".to_owned(),
            device_id: "hidraw9".to_owned(),
            path: root.join("sys/class/hidraw/hidraw9").display().to_string(),
            zones: vec![
                legion_common::KeyboardRgbZone {
                    id: "left".to_owned(),
                    label: "Left".to_owned(),
                },
                legion_common::KeyboardRgbZone {
                    id: "right".to_owned(),
                    label: "Right".to_owned(),
                },
            ],
            effects: vec!["static".to_owned(), "breath".to_owned()],
            current_effect: Some("static".to_owned()),
            current_colors: BTreeMap::from([
                ("left".to_owned(), "#111111".to_owned()),
                ("right".to_owned(), "#222222".to_owned()),
            ]),
            current_brightness: Some(40),
            min_brightness: 0,
            max_brightness: 100,
            current_speed: Some(10),
            min_speed: 0,
            max_speed: 100,
        };
        fs::write(
            device.join("ratvantage-keyboard-rgb.json"),
            serde_json::to_string(&capability).unwrap(),
        )
        .unwrap();

        let registry = probe(&ProbeOptions {
            sysfs_root: root.clone(),
        });

        let detected = registry.keyboard_rgb.as_ref().unwrap();
        assert_eq!(detected.device_id, "hidraw9");
        assert_eq!(detected.effects, ["static", "breath"]);
        assert_eq!(detected.current_brightness, Some(40));
        assert_eq!(detected.zones.len(), 2);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detects_keyboard_rgb_openrgb_fixture_metadata() {
        let root = std::env::temp_dir().join(format!(
            "legion-probe-keyboard-rgb-openrgb-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let status = KeyboardRgbOpenRgbStatus {
            installed: true,
            path: Some("/usr/bin/openrgb".to_owned()),
            devices: vec![KeyboardRgbOpenRgbDevice {
                index: 0,
                name: "Lenovo 5 2023".to_owned(),
                device_type: Some("Laptop".to_owned()),
                description: Some("Lenovo 4-Zone device".to_owned()),
                modes: vec!["Direct".to_owned(), "Breathing".to_owned()],
                current_mode: Some("Direct".to_owned()),
                zones: vec!["Keyboard".to_owned()],
                leds: vec![
                    "Left side".to_owned(),
                    "Left center".to_owned(),
                    "Right center".to_owned(),
                    "Right side".to_owned(),
                ],
            }],
            i2c_dev_loaded: true,
            user_in_i2c_group: false,
            has_i2c_rw_access: true,
            has_hidraw_rw_access: true,
            backend_ready: false,
            write_support_claimed: false,
            sdk_helper_installed: false,
            sdk_helper_path: None,
            sdk_server_running: false,
            sdk_snapshot_supported: false,
            sdk_active_mode: None,
            sdk_color_zones: vec![],
            sdk_colors: BTreeMap::new(),
        };
        fs::write(
            root.join("ratvantage-keyboard-rgb-openrgb.json"),
            serde_json::to_string(&status).unwrap(),
        )
        .unwrap();

        let registry = probe(&ProbeOptions {
            sysfs_root: root.clone(),
        });

        let detected = registry.keyboard_rgb_openrgb.as_ref().unwrap();
        assert!(detected.installed);
        assert_eq!(detected.devices[0].name, "Lenovo 5 2023");
        assert_eq!(detected.devices[0].leds.len(), 4);
        assert!(registry.keyboard_rgb.is_none());

        fs::remove_dir_all(root).unwrap();
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
        let spl = registry
            .firmware_attributes
            .iter()
            .find(|attribute| attribute.name == "ppt_pl1_spl")
            .expect("ppt_pl1_spl fixture metadata must be detected");
        assert_eq!(spl.attribute_type.as_deref(), Some("integer"));
        assert_eq!(spl.default_value.as_deref(), Some("70"));
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
