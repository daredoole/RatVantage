use std::{fs, process::Command, sync::Arc};

use legion_common::{Capability, CapabilityStatus, HardwareSummary, RiskLevel};
use legion_control_daemon::{
    BatteryChargeTypeWriter, CpuEppWriter, CpuGovernorWriter, IdeapadToggleWriter, LedStateWriter,
    LegionControl, PlatformProfileWriter, WriteAccessPolicy, WriteAuthorizer, DBUS_INTERFACE,
    DBUS_PATH,
};
use legion_control_ui::{
    render_diagnostics_json, render_overview_lines, DiagnosticsBundle, LegionControlClient,
    UiStatus,
};
use legion_probe::ProbeOptions;
use ratvantage_test_support::{copied_fixture_root, fixture_root, PrivateBus};
use zbus::blocking::ConnectionBuilder;

#[test]
fn client_reads_daemon_contract_over_private_bus() {
    let (_bus, _service_connection, address) = fixture_service();
    let client = LegionControlClient::address(&address).unwrap();

    let hardware = client.hardware_summary().unwrap();
    assert_eq!(hardware.vendor.as_deref(), Some("LENOVO"));
    assert_eq!(hardware.product_name.as_deref(), Some("82WM"));

    let capabilities = client.capabilities().unwrap();
    let mut capability_ids = capabilities
        .iter()
        .map(|capability| capability.id.as_str())
        .collect::<Vec<_>>();
    capability_ids.sort_unstable();
    assert_eq!(
        capability_ids,
        [
            "amd_gpu_power_dpm",
            "battery_charge_type",
            "cpu_power",
            "fan_curves",
            "firmware_attributes",
            "gpu",
            "hwmon",
            "ideapad_toggles",
            "leds",
            "platform_profile",
            "power_profiles",
            "thermal_zones"
        ]
    );
    assert!(capabilities.iter().all(|capability| {
        capability.risk == RiskLevel::ReadOnly
            && (capability.status == CapabilityStatus::ProbeOnly
                || capability.id == "amd_gpu_power_dpm"
                || capability.id == "gpu"
                || capability.id == "power_profiles")
            && capability.details.is_null()
    }));
    assert!(
        capabilities
            .iter()
            .any(|capability| capability.id == "gpu"
                && capability.status == CapabilityStatus::Missing)
    );

    let telemetry = client.telemetry().unwrap();
    assert!(telemetry
        .sensors
        .iter()
        .any(|sensor| sensor.label.as_deref() == Some("CPU Fan")));
    assert_eq!(
        telemetry
            .battery
            .as_ref()
            .and_then(|battery| battery.capacity_percent),
        Some(79)
    );

    let raw = client.raw_probe_report().unwrap();
    assert_eq!(raw.hardware, hardware);
    assert_eq!(raw.telemetry, telemetry);
    assert!(raw.leds.iter().any(|led| led.name == "platform::ylogo"));
    assert_eq!(
        raw.platform_profile
            .as_ref()
            .and_then(|profile| profile.current.as_deref()),
        Some("balanced")
    );
    assert_eq!(
        raw.battery_charge_type
            .as_ref()
            .and_then(|charge_type| charge_type.current.as_deref()),
        Some("Standard")
    );
    assert_eq!(
        render_overview_lines(&raw),
        [
            "Legion Control overview",
            "platform_profile=balanced",
            "battery_charge_type=Standard",
            "fan_rpm=CPU Fan:2410",
            "temperatures=CPU Temp:52000",
            "gpu_mode=unknown",
            "desktop_power_profiles=not_applicable",
            "gpu_pending_reboot=none",
            "last_known_good_fan_curve=none",
            "fan_preset_by_platform_profile=none",
            "fan_preset_reapply_after_resume=false",
            "battery_capacity_percent=79",
            "battery_status=Charging",
            "battery_health=Good",
            "battery_power_now_w=30.4",
            "battery_cycle_count=219",
            "leds=platform::fnlock:0,platform::ylogo:1",
            "firmware_toggles=camera_power:1,conservation_mode:1,fan_mode:0,fn_lock:0",
        ]
    );

    let bundle = DiagnosticsBundle::from_report(raw.clone(), Some("test-kernel".to_owned()));
    assert_eq!(bundle.hardware, hardware);
    assert_eq!(bundle.kernel_version.as_deref(), Some("test-kernel"));
    assert_eq!(bundle.gpu_mode_pending, None);
    assert_eq!(bundle.last_known_good_fan_curve, None);
    assert!(bundle.recent_daemon_logs.is_empty());
    assert!(bundle
        .detected_sysfs_paths
        .iter()
        .any(|path| path == &raw.hardware.sysfs_root));
    assert!(bundle
        .detected_sysfs_paths
        .iter()
        .any(|path| path.ends_with("sys/firmware/acpi/platform_profile")));
    assert!(bundle
        .detected_sysfs_paths
        .iter()
        .any(|path| path.ends_with("sys/firmware/acpi/platform_profile_choices")));
    assert!(bundle
        .detected_sysfs_paths
        .iter()
        .any(|path| path.ends_with("sys/class/power_supply/BAT0/charge_types")));
    assert!(bundle
        .detected_sysfs_paths
        .iter()
        .any(|path| path.ends_with("sys/class/power_supply/BAT0")));

    let json: serde_json::Value =
        serde_json::from_str(&render_diagnostics_json(&bundle).unwrap()).unwrap();
    assert_eq!(json["kernel_version"], "test-kernel");
    assert_eq!(json["gpu_mode_pending"], serde_json::Value::Null);
    assert_eq!(json["last_known_good_fan_curve"], serde_json::Value::Null);
    assert_eq!(
        json["fan_preset_by_platform_profile"],
        serde_json::json!({})
    );
    assert_eq!(json["fan_preset_reapply_after_resume"], false);
    assert_eq!(json["recent_daemon_logs"], serde_json::json!([]));
    assert_eq!(json["hardware"]["product_name"], "82WM");
    assert_eq!(json["summary"]["capability_count"], 12);
    assert_eq!(json["summary"]["available_capability_count"], 10);
    assert_eq!(json["summary"]["missing_capability_count"], 2);
    assert_eq!(
        json["summary"]["capability_status_counts"]["probe_only"],
        10
    );
    assert_eq!(json["summary"]["capability_status_counts"]["missing"], 2);
    assert_eq!(json["summary"]["sensor_count"], 2);
    assert_eq!(json["summary"]["fan_curve_count"], 1);
    assert_eq!(
        json["summary"]["detected_sysfs_path_count"],
        bundle.detected_sysfs_paths.len()
    );
    assert_eq!(
        json["raw_probe_report"]["power_profiles"],
        serde_json::Value::Null
    );
    assert_eq!(
        json["raw_probe_report"]["platform_profile"]["current"],
        "balanced"
    );
    assert!(json["raw_probe_report"]["platform_profile"]["choices_path"]
        .as_str()
        .unwrap()
        .ends_with("sys/firmware/acpi/platform_profile_choices"));
    assert!(
        json["raw_probe_report"]["battery_charge_type"]["choices_path"]
            .as_str()
            .unwrap()
            .ends_with("sys/class/power_supply/BAT0/charge_types")
    );

    let bundle = DiagnosticsBundle::from_report_with_logs(
        raw.clone(),
        Some("test-kernel".to_owned()),
        vec!["2026-04-25 daemon started".to_owned()],
    );
    let json: serde_json::Value =
        serde_json::from_str(&render_diagnostics_json(&bundle).unwrap()).unwrap();
    assert_eq!(json["recent_daemon_logs"][0], "2026-04-25 daemon started");
    assert_eq!(json["fan_preset_reapply_after_resume"], false);

    let refreshed = client.refresh_capabilities().unwrap();
    assert_eq!(refreshed, capabilities);

    let platform_plan = client.plan_platform_profile_write("performance").unwrap();
    assert_eq!(platform_plan.method, "SetPlatformProfile");
    assert_eq!(platform_plan.previous_value, "balanced");
    assert_eq!(platform_plan.requested_value, "performance");

    let battery_plan = client
        .plan_battery_charge_type_write("Conservation")
        .unwrap();
    assert_eq!(battery_plan.method, "SetBatteryChargeType");
    assert_eq!(battery_plan.previous_value, "Standard");
    assert_eq!(battery_plan.requested_value, "Conservation");

    let led_plan = client
        .plan_led_state_write("platform::ylogo", false)
        .unwrap();
    assert_eq!(led_plan.method, "SetLedState");
    assert_eq!(led_plan.previous_value, "1");
    assert_eq!(led_plan.requested_value, "0");

    let toggle_plan = client.plan_ideapad_toggle_write("fn_lock", true).unwrap();
    assert_eq!(toggle_plan.method, "SetIdeapadToggle");
    assert_eq!(toggle_plan.previous_value, "0");
    assert_eq!(toggle_plan.requested_value, "1");

    let toggle_plan = client
        .plan_ideapad_toggle_write("camera_power", false)
        .unwrap();
    assert_eq!(toggle_plan.method, "SetIdeapadToggle");
    assert_eq!(toggle_plan.previous_value, "1");
    assert_eq!(toggle_plan.requested_value, "0");
}

#[test]
fn refresh_runtime_snapshot_reprobes_before_reading_runtime_state() {
    let state_path = unique_state_path("refresh-runtime-snapshot");
    fs::write(
        &state_path,
        r#"schema_version = 1

[gpu_mode_pending]
requested_mode = "hybrid"
previous_mode = "nvidia"
reboot_required = true

[last_known_good_fan_curve]
curve_id = "legion_hwmon"
path = "/tmp/fixture/sys/class/hwmon/hwmon7"

[[last_known_good_fan_curve.points]]
path = "/tmp/fixture/sys/class/hwmon/hwmon7/pwm1_auto_point1_temp"
value = "42000"
"#,
    )
    .unwrap();
    let (_bus, _service_connection, address) = fixture_service_with_state(&state_path);
    let client = LegionControlClient::address(&address).unwrap();

    let snapshot = client.refresh_runtime_snapshot().unwrap();

    assert_eq!(snapshot.status.hardware.product_name, "82WM");
    assert_eq!(
        snapshot
            .diagnostics
            .raw_probe_report
            .platform_profile
            .as_ref()
            .and_then(|profile| profile.current.as_deref()),
        Some("balanced")
    );
    assert_eq!(
        snapshot
            .diagnostics
            .gpu_mode_pending
            .as_ref()
            .map(|pending| pending.requested_mode.as_str()),
        Some("hybrid")
    );
    assert_eq!(
        snapshot
            .diagnostics
            .last_known_good_fan_curve
            .as_ref()
            .map(|snapshot| snapshot.curve_id.as_str()),
        Some("legion_hwmon")
    );

    let _ = fs::remove_file(state_path);
}

#[test]
fn status_model_normalizes_daemon_data_for_ui() {
    let (_bus, _service_connection, address) = fixture_service();
    let client = LegionControlClient::address(&address).unwrap();
    let status = client.status().unwrap();

    assert_eq!(status.hardware.vendor, "LENOVO");
    assert_eq!(status.hardware.product_name, "82WM");
    assert_eq!(status.hardware.product_version, "Legion Pro 5 16ARX8");
    assert_eq!(
        status.hardware.product_sku.as_deref(),
        Some("LENOVO_MT_82WM_BU_idea_FM_Legion Pro 5 16ARX8")
    );
    assert_eq!(status.capability_count(), 12);
    assert_eq!(
        status.capability_ids(),
        [
            "amd_gpu_power_dpm",
            "battery_charge_type",
            "cpu_power",
            "fan_curves",
            "firmware_attributes",
            "gpu",
            "hwmon",
            "ideapad_toggles",
            "leds",
            "platform_profile",
            "power_profiles",
            "thermal_zones"
        ]
    );
    assert!(status.capabilities.iter().all(|capability| {
        !capability.label.is_empty()
            && capability.risk == RiskLevel::ReadOnly
            && (capability.status == CapabilityStatus::ProbeOnly
                || capability.id == "amd_gpu_power_dpm"
                || capability.id == "gpu"
                || capability.id == "power_profiles")
    }));
    assert!(
        status
            .capabilities
            .iter()
            .any(|capability| capability.id == "gpu"
                && capability.status == CapabilityStatus::Missing)
    );
    assert!(status
        .capabilities
        .iter()
        .any(|capability| capability.id == "power_profiles"
            && capability.status == CapabilityStatus::Missing));
    assert_eq!(
        status.render_lines(),
        [
                "Legion Control status",
                "vendor=LENOVO",
                "product_name=82WM",
                "product_version=Legion Pro 5 16ARX8",
                "capability_count=12",
                "capabilities=amd_gpu_power_dpm,battery_charge_type,cpu_power,fan_curves,firmware_attributes,gpu,hwmon,ideapad_toggles,leds,platform_profile,power_profiles,thermal_zones",
                "capability_statuses=amd_gpu_power_dpm:probe_only:read_only,battery_charge_type:probe_only:read_only,cpu_power:probe_only:read_only,fan_curves:probe_only:read_only,firmware_attributes:probe_only:read_only,gpu:missing:read_only,hwmon:probe_only:read_only,ideapad_toggles:probe_only:read_only,leds:probe_only:read_only,platform_profile:probe_only:read_only,power_profiles:missing:read_only,thermal_zones:probe_only:read_only",
            ]
    );
}

#[test]
fn status_cli_prints_hardware_and_capability_summary() {
    let (_bus, _service_connection, address) = fixture_service();
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args(["--status", "--bus-address", &address])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        stdout,
        concat!(
            "Legion Control status\n",
            "vendor=LENOVO\n",
            "product_name=82WM\n",
            "product_version=Legion Pro 5 16ARX8\n",
            "capability_count=12\n",
            "capabilities=amd_gpu_power_dpm,battery_charge_type,cpu_power,fan_curves,firmware_attributes,gpu,hwmon,ideapad_toggles,leds,platform_profile,power_profiles,thermal_zones\n",
            "capability_statuses=amd_gpu_power_dpm:probe_only:read_only,battery_charge_type:probe_only:read_only,cpu_power:probe_only:read_only,fan_curves:probe_only:read_only,firmware_attributes:probe_only:read_only,gpu:missing:read_only,hwmon:probe_only:read_only,ideapad_toggles:probe_only:read_only,leds:probe_only:read_only,platform_profile:probe_only:read_only,power_profiles:missing:read_only,thermal_zones:probe_only:read_only\n",
        )
    );
}

#[test]
fn overview_cli_prints_read_only_mvp_summary() {
    let (_bus, _service_connection, address) = fixture_service();
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args(["--overview", "--bus-address", &address])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        concat!(
            "Legion Control overview\n",
            "platform_profile=balanced\n",
            "battery_charge_type=Standard\n",
            "fan_rpm=CPU Fan:2410\n",
            "temperatures=CPU Temp:52000\n",
            "gpu_mode=unknown\n",
            "desktop_power_profiles=not_applicable\n",
            "gpu_pending_reboot=none\n",
            "last_known_good_fan_curve=none\n",
            "fan_preset_by_platform_profile=none\n",
            "fan_preset_reapply_after_resume=false\n",
            "battery_capacity_percent=79\n",
            "battery_status=Charging\n",
            "battery_health=Good\n",
            "battery_power_now_w=30.4\n",
            "battery_cycle_count=219\n",
            "leds=platform::fnlock:0,platform::ylogo:1\n",
            "firmware_toggles=camera_power:1,conservation_mode:1,fan_mode:0,fn_lock:0\n",
        )
    );
}

#[test]
fn overview_cli_surfaces_saved_fan_curve_state() {
    let state_path = unique_state_path("ui-overview-state");
    std::fs::write(
        &state_path,
        r#"schema_version = 1
fan_preset_reapply_after_resume = true

[gpu_mode_pending]
requested_mode = "hybrid"
previous_mode = "nvidia"
reboot_required = true

[last_known_good_fan_curve]
curve_id = "legion_hwmon"
path = "/tmp/fixture/sys/class/hwmon/hwmon7"

[[last_known_good_fan_curve.points]]
path = "/tmp/fixture/sys/class/hwmon/hwmon7/pwm1_auto_point1_temp"
value = "42000"

[fan_preset_by_platform_profile]
balanced = "quiet-office"
"#,
    )
    .unwrap();

    let (_bus, _service_connection, address) = fixture_service_with_state(&state_path);
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args(["--overview", "--bus-address", &address])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("gpu_pending_reboot=hybrid pending (was nvidia); reboot required"));
    assert!(stdout.contains("last_known_good_fan_curve=1 point on legion_hwmon"));
    assert!(stdout.contains("fan_preset_by_platform_profile=balanced=quiet-office"));
    assert!(stdout.contains("fan_preset_reapply_after_resume=true"));
    let _ = std::fs::remove_file(state_path);
}

#[test]
fn diagnostics_cli_prints_read_only_debug_bundle_json() {
    let (_bus, _service_connection, address) = fixture_service();
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args(["--diagnostics", "--bus-address", &address])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["hardware"]["vendor"], "LENOVO");
    assert_eq!(json["hardware"]["product_name"], "82WM");
    assert_eq!(
        json["raw_probe_report"]["power_profiles"],
        serde_json::Value::Null
    );
    assert_eq!(
        json["raw_probe_report"]["battery_charge_type"]["current"],
        "Standard"
    );
    assert!(json["detected_sysfs_paths"]
        .as_array()
        .unwrap()
        .iter()
        .any(|path| path
            .as_str()
            .unwrap()
            .ends_with("sys/firmware/acpi/platform_profile")));
    assert_eq!(json["gpu_mode_pending"], serde_json::Value::Null);
    assert_eq!(json["last_known_good_fan_curve"], serde_json::Value::Null);
    assert_eq!(
        json["fan_preset_by_platform_profile"],
        serde_json::json!({})
    );
    assert_eq!(json["fan_preset_reapply_after_resume"], false);
}

#[test]
fn diagnostics_cli_surfaces_durable_runtime_state_in_json() {
    let state_path = unique_state_path("ui-diagnostics-state");
    std::fs::write(
        &state_path,
        r#"schema_version = 1

[gpu_mode_pending]
requested_mode = "hybrid"
previous_mode = "nvidia"
reboot_required = true

[last_known_good_fan_curve]
curve_id = "legion_hwmon"
path = "/tmp/fixture/sys/class/hwmon/hwmon7"

[[last_known_good_fan_curve.points]]
path = "/tmp/fixture/sys/class/hwmon/hwmon7/pwm1_auto_point1_temp"
value = "42000"
"#,
    )
    .unwrap();

    let (_bus, _service_connection, address) = fixture_service_with_state(&state_path);
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args(["--diagnostics", "--bus-address", &address])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["gpu_mode_pending"]["requested_mode"], "hybrid");
    assert_eq!(json["gpu_mode_pending"]["previous_mode"], "nvidia");
    assert_eq!(json["gpu_mode_pending"]["reboot_required"], true);
    assert_eq!(
        json["last_known_good_fan_curve"]["curve_id"],
        "legion_hwmon"
    );
    assert_eq!(
        json["last_known_good_fan_curve"]["points"][0]["value"],
        "42000"
    );
    assert_eq!(
        json["fan_preset_by_platform_profile"],
        serde_json::json!({})
    );
    assert_eq!(json["fan_preset_reapply_after_resume"], false);
    let _ = std::fs::remove_file(state_path);
}

#[test]
fn plan_cli_prints_read_only_write_preview_json() {
    let (_bus, _service_connection, address) = fixture_service();
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args([
            "--plan-platform-profile",
            "performance",
            "--bus-address",
            &address,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["method"], "SetPlatformProfile");
    assert_eq!(json["previous_value"], "balanced");
    assert_eq!(json["requested_value"], "performance");
    assert_eq!(json["readback_required"], true);

    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args([
            "--plan-battery-charge-type",
            "Conservation",
            "--bus-address",
            &address,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["method"], "SetBatteryChargeType");
    assert_eq!(json["previous_value"], "Standard");
    assert_eq!(json["requested_value"], "Conservation");

    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args([
            "--plan-firmware-attribute",
            "ppt_pl1_spl=75",
            "--bus-address",
            &address,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["method"], "SetFirmwareAttribute");
    assert_eq!(json["previous_value"], "70");
    assert_eq!(json["requested_value"], "75");
}

#[test]
fn set_platform_profile_cli_reports_policy_block_when_write_is_disabled() {
    let (_bus, _service_connection, address) = fixture_service();
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args([
            "--set-platform-profile",
            "performance",
            "--bus-address",
            &address,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "blocked_by_policy");
    assert_eq!(json["applied"], false);
    assert_eq!(json["plan"]["method"], "SetPlatformProfile");
    assert_eq!(json["plan"]["requested_value"], "performance");
}

#[test]
fn set_platform_profile_cli_executes_gated_write_and_prints_result() {
    let fixture = copied_fixture_root("ui-platform-profile-write");
    let state_path = unique_state_path("ui-platform-profile-write");
    let (_bus, _service_connection, address) =
        fixture_service_with_runtime(LegionControl::new_with_runtime(
            ProbeOptions {
                sysfs_root: fixture.clone(),
            },
            &state_path,
            WriteAccessPolicy {
                platform_profile_enabled: true,
                battery_charge_type_enabled: false,
                led_state_enabled: false,
                ideapad_toggle_enabled: false,
                camera_power_enabled: false,
                usb_charging_enabled: false,
                fan_mode_enabled: false,
                gpu_mode_enabled: false,
                cpu_governor_enabled: false,
                cpu_epp_enabled: false,
                firmware_attribute_enabled: false,
                cpu_boost_enabled: false,
                conservation_mode_enabled: false,
                amd_gpu_dpm_enabled: false,
                curve_optimizer_enabled: false,
                hardware_profile_apply_enabled: false,
            },
            Arc::new(AllowAllAuthorizer),
            Arc::new(RealFixturePlatformProfileWriter),
            Arc::new(RealFixtureBatteryChargeTypeWriter),
            Arc::new(RealFixtureLedStateWriter),
            Arc::new(RealFixtureIdeapadToggleWriter),
            Arc::new(NoOpCpuGovernorWriter),
            Arc::new(NoOpCpuEppWriter),
        ));
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args([
            "--set-platform-profile",
            "performance",
            "--bus-address",
            &address,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "applied");
    assert_eq!(json["applied"], true);
    assert_eq!(json["readback_value"], "performance");
    assert_eq!(json["plan"]["method"], "SetPlatformProfile");
    assert_eq!(json["plan"]["requested_value"], "performance");
    assert_eq!(
        fs::read_to_string(fixture.join("sys/firmware/acpi/platform_profile"))
            .unwrap()
            .trim(),
        "performance"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn set_battery_charge_type_cli_reports_policy_block_when_write_is_disabled() {
    let (_bus, _service_connection, address) = fixture_service();
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args([
            "--set-battery-charge-type",
            "Conservation",
            "--bus-address",
            &address,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "blocked_by_policy");
    assert_eq!(json["applied"], false);
    assert_eq!(json["plan"]["method"], "SetBatteryChargeType");
    assert_eq!(json["plan"]["requested_value"], "Conservation");
}

#[test]
fn set_firmware_attribute_cli_reports_policy_block_when_write_is_disabled() {
    let (_bus, _service_connection, address) = fixture_service();
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args([
            "--set-firmware-attribute",
            "ppt_pl1_spl=75",
            "--bus-address",
            &address,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "blocked_by_policy");
    assert_eq!(json["applied"], false);
    assert_eq!(json["plan"]["method"], "SetFirmwareAttribute");
    assert_eq!(json["plan"]["requested_value"], "75");
}

#[test]
fn set_firmware_attribute_cli_executes_gated_write_and_prints_result() {
    let fixture = copied_fixture_root("ui-firmware-attribute-write");
    let state_path = unique_state_path("ui-firmware-attribute-write");
    let (_bus, _service_connection, address) =
        fixture_service_with_runtime(LegionControl::new_with_runtime(
            ProbeOptions {
                sysfs_root: fixture.clone(),
            },
            &state_path,
            WriteAccessPolicy {
                platform_profile_enabled: false,
                battery_charge_type_enabled: false,
                led_state_enabled: false,
                ideapad_toggle_enabled: false,
                camera_power_enabled: false,
                usb_charging_enabled: false,
                fan_mode_enabled: false,
                gpu_mode_enabled: false,
                cpu_governor_enabled: false,
                cpu_epp_enabled: false,
                firmware_attribute_enabled: true,
                cpu_boost_enabled: false,
                conservation_mode_enabled: false,
                amd_gpu_dpm_enabled: false,
                curve_optimizer_enabled: false,
                hardware_profile_apply_enabled: false,
            },
            Arc::new(AllowAllAuthorizer),
            Arc::new(RealFixturePlatformProfileWriter),
            Arc::new(RealFixtureBatteryChargeTypeWriter),
            Arc::new(RealFixtureLedStateWriter),
            Arc::new(RealFixtureIdeapadToggleWriter),
            Arc::new(NoOpCpuGovernorWriter),
            Arc::new(NoOpCpuEppWriter),
        ));
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args([
            "--set-firmware-attribute",
            "ppt_pl1_spl=75",
            "--bus-address",
            &address,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "applied");
    assert_eq!(json["applied"], true);
    assert_eq!(json["readback_value"], "75");
    assert_eq!(json["plan"]["method"], "SetFirmwareAttribute");
    assert_eq!(json["plan"]["requested_value"], "75");
    assert_eq!(
        fs::read_to_string(
            fixture.join(
                "sys/class/firmware-attributes/thinklmi/attributes/ppt_pl1_spl/current_value"
            )
        )
        .unwrap()
        .trim(),
        "75"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn set_battery_charge_type_cli_executes_gated_write_and_prints_result() {
    let fixture = copied_fixture_root("ui-battery-charge-type-write");
    let state_path = unique_state_path("ui-battery-charge-type-write");
    let (_bus, _service_connection, address) =
        fixture_service_with_runtime(LegionControl::new_with_runtime(
            ProbeOptions {
                sysfs_root: fixture.clone(),
            },
            &state_path,
            WriteAccessPolicy {
                platform_profile_enabled: false,
                battery_charge_type_enabled: true,
                led_state_enabled: false,
                ideapad_toggle_enabled: false,
                camera_power_enabled: false,
                usb_charging_enabled: false,
                fan_mode_enabled: false,
                gpu_mode_enabled: false,
                cpu_governor_enabled: false,
                cpu_epp_enabled: false,
                firmware_attribute_enabled: false,
                cpu_boost_enabled: false,
                conservation_mode_enabled: false,
                amd_gpu_dpm_enabled: false,
                curve_optimizer_enabled: false,
                hardware_profile_apply_enabled: false,
            },
            Arc::new(AllowAllAuthorizer),
            Arc::new(RealFixturePlatformProfileWriter),
            Arc::new(RealFixtureBatteryChargeTypeWriter),
            Arc::new(RealFixtureLedStateWriter),
            Arc::new(RealFixtureIdeapadToggleWriter),
            Arc::new(NoOpCpuGovernorWriter),
            Arc::new(NoOpCpuEppWriter),
        ));
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args([
            "--set-battery-charge-type",
            "Conservation",
            "--bus-address",
            &address,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "applied");
    assert_eq!(json["applied"], true);
    assert_eq!(json["readback_value"], "Conservation");
    assert_eq!(json["plan"]["method"], "SetBatteryChargeType");
    assert_eq!(json["plan"]["requested_value"], "Conservation");
    assert_eq!(
        fs::read_to_string(fixture.join("sys/class/power_supply/BAT0/charge_type"))
            .unwrap()
            .trim(),
        "Conservation"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn set_led_state_cli_reports_policy_block_when_write_is_disabled() {
    let (_bus, _service_connection, address) = fixture_service();
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args([
            "--set-led-state",
            "platform::ylogo=off",
            "--bus-address",
            &address,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "blocked_by_policy");
    assert_eq!(json["applied"], false);
    assert_eq!(json["plan"]["method"], "SetLedState");
    assert_eq!(json["plan"]["requested_value"], "0");
}

#[test]
fn set_led_state_cli_executes_gated_write_and_prints_result() {
    let fixture = copied_fixture_root("ui-led-state-write");
    let state_path = unique_state_path("ui-led-state-write");
    let (_bus, _service_connection, address) =
        fixture_service_with_runtime(LegionControl::new_with_runtime(
            ProbeOptions {
                sysfs_root: fixture.clone(),
            },
            &state_path,
            WriteAccessPolicy {
                platform_profile_enabled: false,
                battery_charge_type_enabled: false,
                led_state_enabled: true,
                ideapad_toggle_enabled: false,
                camera_power_enabled: false,
                usb_charging_enabled: false,
                fan_mode_enabled: false,
                gpu_mode_enabled: false,
                cpu_governor_enabled: false,
                cpu_epp_enabled: false,
                firmware_attribute_enabled: false,
                cpu_boost_enabled: false,
                conservation_mode_enabled: false,
                amd_gpu_dpm_enabled: false,
                curve_optimizer_enabled: false,
                hardware_profile_apply_enabled: false,
            },
            Arc::new(AllowAllAuthorizer),
            Arc::new(RealFixturePlatformProfileWriter),
            Arc::new(RealFixtureBatteryChargeTypeWriter),
            Arc::new(RealFixtureLedStateWriter),
            Arc::new(RealFixtureIdeapadToggleWriter),
            Arc::new(NoOpCpuGovernorWriter),
            Arc::new(NoOpCpuEppWriter),
        ));
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args([
            "--set-led-state",
            "platform::ylogo=off",
            "--bus-address",
            &address,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "applied");
    assert_eq!(json["applied"], true);
    assert_eq!(json["readback_value"], "0");
    assert_eq!(json["plan"]["method"], "SetLedState");
    assert_eq!(
        fs::read_to_string(fixture.join("sys/class/leds/platform::ylogo/brightness"))
            .unwrap()
            .trim(),
        "0"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn set_ideapad_toggle_cli_reports_policy_block_when_write_is_disabled() {
    let (_bus, _service_connection, address) = fixture_service();
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args([
            "--set-ideapad-toggle",
            "fn_lock=on",
            "--bus-address",
            &address,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "blocked_by_policy");
    assert_eq!(json["applied"], false);
    assert_eq!(json["plan"]["method"], "SetIdeapadToggle");
    assert_eq!(json["plan"]["requested_value"], "1");
}

#[test]
fn set_ideapad_toggle_cli_executes_gated_write_and_prints_result() {
    let fixture = copied_fixture_root("ui-ideapad-toggle-write");
    let state_path = unique_state_path("ui-ideapad-toggle-write");
    let (_bus, _service_connection, address) =
        fixture_service_with_runtime(LegionControl::new_with_runtime(
            ProbeOptions {
                sysfs_root: fixture.clone(),
            },
            &state_path,
            WriteAccessPolicy {
                platform_profile_enabled: false,
                battery_charge_type_enabled: false,
                led_state_enabled: false,
                ideapad_toggle_enabled: true,
                camera_power_enabled: false,
                usb_charging_enabled: false,
                fan_mode_enabled: false,
                gpu_mode_enabled: false,
                cpu_governor_enabled: false,
                cpu_epp_enabled: false,
                firmware_attribute_enabled: false,
                cpu_boost_enabled: false,
                conservation_mode_enabled: false,
                amd_gpu_dpm_enabled: false,
                curve_optimizer_enabled: false,
                hardware_profile_apply_enabled: false,
            },
            Arc::new(AllowAllAuthorizer),
            Arc::new(RealFixturePlatformProfileWriter),
            Arc::new(RealFixtureBatteryChargeTypeWriter),
            Arc::new(RealFixtureLedStateWriter),
            Arc::new(RealFixtureIdeapadToggleWriter),
            Arc::new(NoOpCpuGovernorWriter),
            Arc::new(NoOpCpuEppWriter),
        ));
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args([
            "--set-ideapad-toggle",
            "fn_lock=on",
            "--bus-address",
            &address,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "applied");
    assert_eq!(json["applied"], true);
    assert_eq!(json["readback_value"], "1");
    assert_eq!(json["plan"]["method"], "SetIdeapadToggle");
    assert_eq!(
        fs::read_to_string(
            fixture.join("sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/fn_lock")
        )
        .unwrap()
        .trim(),
        "1"
    );
    assert_eq!(
        fs::read_to_string(fixture.join("sys/class/leds/platform::fnlock/brightness"))
            .unwrap()
            .trim(),
        "1"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn set_cpu_governor_cli_reports_policy_block_when_write_is_disabled() {
    let (_bus, _service_connection, address) = fixture_service();
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args([
            "--set-cpu-governor",
            "performance",
            "--bus-address",
            &address,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "blocked_by_policy");
    assert_eq!(json["applied"], false);
    assert_eq!(json["plan"]["method"], "SetCpuGovernor");
    assert_eq!(json["plan"]["requested_value"], "performance");
}

#[test]
fn set_cpu_governor_cli_executes_gated_write_and_prints_result() {
    let fixture = copied_fixture_root("ui-cpu-governor-write");
    let state_path = unique_state_path("ui-cpu-governor-write");
    let (_bus, _service_connection, address) =
        fixture_service_with_runtime(LegionControl::new_with_runtime(
            ProbeOptions {
                sysfs_root: fixture.clone(),
            },
            &state_path,
            WriteAccessPolicy {
                platform_profile_enabled: false,
                battery_charge_type_enabled: false,
                led_state_enabled: false,
                ideapad_toggle_enabled: false,
                camera_power_enabled: false,
                usb_charging_enabled: false,
                fan_mode_enabled: false,
                gpu_mode_enabled: false,
                cpu_governor_enabled: true,
                cpu_epp_enabled: false,
                firmware_attribute_enabled: false,
                cpu_boost_enabled: false,
                conservation_mode_enabled: false,
                amd_gpu_dpm_enabled: false,
                curve_optimizer_enabled: false,
                hardware_profile_apply_enabled: false,
            },
            Arc::new(AllowAllAuthorizer),
            Arc::new(RealFixturePlatformProfileWriter),
            Arc::new(RealFixtureBatteryChargeTypeWriter),
            Arc::new(RealFixtureLedStateWriter),
            Arc::new(RealFixtureIdeapadToggleWriter),
            Arc::new(RealFixtureCpuGovernorWriter),
            Arc::new(NoOpCpuEppWriter),
        ));
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args([
            "--set-cpu-governor",
            "performance",
            "--bus-address",
            &address,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "applied");
    assert_eq!(json["applied"], true);
    assert_eq!(json["readback_value"], "performance");
    assert_eq!(json["plan"]["method"], "SetCpuGovernor");
    assert_eq!(
        fs::read_to_string(fixture.join("sys/devices/system/cpu/cpufreq/policy0/scaling_governor"))
            .unwrap()
            .trim(),
        "performance"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn set_cpu_epp_cli_reports_policy_block_when_write_is_disabled() {
    let (_bus, _service_connection, address) = fixture_service();
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args([
            "--set-cpu-epp",
            "balance_performance",
            "--bus-address",
            &address,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "blocked_by_policy");
    assert_eq!(json["applied"], false);
    assert_eq!(json["plan"]["method"], "SetCpuEpp");
    assert_eq!(json["plan"]["requested_value"], "balance_performance");
}

#[test]
fn set_cpu_epp_cli_executes_gated_write_and_prints_result() {
    let fixture = copied_fixture_root("ui-cpu-epp-write");
    let state_path = unique_state_path("ui-cpu-epp-write");
    let (_bus, _service_connection, address) =
        fixture_service_with_runtime(LegionControl::new_with_runtime(
            ProbeOptions {
                sysfs_root: fixture.clone(),
            },
            &state_path,
            WriteAccessPolicy {
                platform_profile_enabled: false,
                battery_charge_type_enabled: false,
                led_state_enabled: false,
                ideapad_toggle_enabled: false,
                camera_power_enabled: false,
                usb_charging_enabled: false,
                fan_mode_enabled: false,
                gpu_mode_enabled: false,
                cpu_governor_enabled: false,
                cpu_epp_enabled: true,
                firmware_attribute_enabled: false,
                cpu_boost_enabled: false,
                conservation_mode_enabled: false,
                amd_gpu_dpm_enabled: false,
                curve_optimizer_enabled: false,
                hardware_profile_apply_enabled: false,
            },
            Arc::new(AllowAllAuthorizer),
            Arc::new(RealFixturePlatformProfileWriter),
            Arc::new(RealFixtureBatteryChargeTypeWriter),
            Arc::new(RealFixtureLedStateWriter),
            Arc::new(RealFixtureIdeapadToggleWriter),
            Arc::new(NoOpCpuGovernorWriter),
            Arc::new(RealFixtureCpuEppWriter),
        ));
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args([
            "--set-cpu-epp",
            "balance_performance",
            "--bus-address",
            &address,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "applied");
    assert_eq!(json["applied"], true);
    assert_eq!(json["readback_value"], "balance_performance");
    assert_eq!(json["plan"]["method"], "SetCpuEpp");
    assert_eq!(
        fs::read_to_string(
            fixture.join("sys/devices/system/cpu/cpufreq/policy0/energy_performance_preference")
        )
        .unwrap()
        .trim(),
        "balance_performance"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn set_camera_power_cli_reports_policy_block_when_write_is_disabled() {
    let (_bus, _service_connection, address) = fixture_service();
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args([
            "--set-ideapad-toggle",
            "camera_power=off",
            "--bus-address",
            &address,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "blocked_by_policy");
    assert_eq!(json["applied"], false);
    assert_eq!(json["plan"]["method"], "SetIdeapadToggle");
    assert_eq!(json["plan"]["requested_value"], "0");
}

#[test]
fn set_camera_power_cli_executes_gated_write_and_prints_result() {
    let fixture = copied_fixture_root("ui-camera-power-write");
    let state_path = unique_state_path("ui-camera-power-write");
    let (_bus, _service_connection, address) =
        fixture_service_with_runtime(LegionControl::new_with_runtime(
            ProbeOptions {
                sysfs_root: fixture.clone(),
            },
            &state_path,
            WriteAccessPolicy {
                platform_profile_enabled: false,
                battery_charge_type_enabled: false,
                led_state_enabled: false,
                ideapad_toggle_enabled: false,
                camera_power_enabled: true,
                usb_charging_enabled: false,
                fan_mode_enabled: false,
                gpu_mode_enabled: false,
                cpu_governor_enabled: false,
                cpu_epp_enabled: false,
                firmware_attribute_enabled: false,
                cpu_boost_enabled: false,
                conservation_mode_enabled: false,
                amd_gpu_dpm_enabled: false,
                curve_optimizer_enabled: false,
                hardware_profile_apply_enabled: false,
            },
            Arc::new(AllowAllAuthorizer),
            Arc::new(RealFixturePlatformProfileWriter),
            Arc::new(RealFixtureBatteryChargeTypeWriter),
            Arc::new(RealFixtureLedStateWriter),
            Arc::new(RealFixtureIdeapadToggleWriter),
            Arc::new(NoOpCpuGovernorWriter),
            Arc::new(NoOpCpuEppWriter),
        ));
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args([
            "--set-ideapad-toggle",
            "camera_power=off",
            "--bus-address",
            &address,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "applied");
    assert_eq!(json["applied"], true);
    assert_eq!(json["readback_value"], "0");
    assert_eq!(json["plan"]["method"], "SetIdeapadToggle");
    assert_eq!(
        fs::read_to_string(
            fixture.join("sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/camera_power")
        )
        .unwrap()
        .trim(),
        "0"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn set_usb_charging_cli_reports_policy_block_when_write_is_disabled() {
    let fixture = copied_runtime_fixture_root("ui-usb-charging-write-blocked");
    seed_usb_charging_toggle(&fixture, "1");
    let state_path = unique_state_path("ui-usb-charging-write-blocked");
    let (_bus, _service_connection, address) =
        fixture_service_with_runtime(LegionControl::new_with_runtime(
            ProbeOptions {
                sysfs_root: fixture.clone(),
            },
            &state_path,
            WriteAccessPolicy {
                platform_profile_enabled: false,
                battery_charge_type_enabled: false,
                led_state_enabled: false,
                ideapad_toggle_enabled: false,
                camera_power_enabled: false,
                usb_charging_enabled: false,
                fan_mode_enabled: false,
                gpu_mode_enabled: false,
                cpu_governor_enabled: false,
                cpu_epp_enabled: false,
                firmware_attribute_enabled: false,
                cpu_boost_enabled: false,
                conservation_mode_enabled: false,
                amd_gpu_dpm_enabled: false,
                curve_optimizer_enabled: false,
                hardware_profile_apply_enabled: false,
            },
            Arc::new(AllowAllAuthorizer),
            Arc::new(RealFixturePlatformProfileWriter),
            Arc::new(RealFixtureBatteryChargeTypeWriter),
            Arc::new(RealFixtureLedStateWriter),
            Arc::new(RealFixtureIdeapadToggleWriter),
            Arc::new(NoOpCpuGovernorWriter),
            Arc::new(NoOpCpuEppWriter),
        ));
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args([
            "--set-ideapad-toggle",
            "usb_charging=off",
            "--bus-address",
            &address,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "blocked_by_policy");
    assert_eq!(json["applied"], false);
    assert_eq!(json["plan"]["method"], "SetIdeapadToggle");
    assert_eq!(json["plan"]["requested_value"], "0");

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn set_usb_charging_cli_executes_gated_write_and_prints_result() {
    let fixture = copied_runtime_fixture_root("ui-usb-charging-write");
    seed_usb_charging_toggle(&fixture, "1");
    let state_path = unique_state_path("ui-usb-charging-write");
    let (_bus, _service_connection, address) =
        fixture_service_with_runtime(LegionControl::new_with_runtime(
            ProbeOptions {
                sysfs_root: fixture.clone(),
            },
            &state_path,
            WriteAccessPolicy {
                platform_profile_enabled: false,
                battery_charge_type_enabled: false,
                led_state_enabled: false,
                ideapad_toggle_enabled: false,
                camera_power_enabled: false,
                usb_charging_enabled: true,
                fan_mode_enabled: false,
                gpu_mode_enabled: false,
                cpu_governor_enabled: false,
                cpu_epp_enabled: false,
                firmware_attribute_enabled: false,
                cpu_boost_enabled: false,
                conservation_mode_enabled: false,
                amd_gpu_dpm_enabled: false,
                curve_optimizer_enabled: false,
                hardware_profile_apply_enabled: false,
            },
            Arc::new(AllowAllAuthorizer),
            Arc::new(RealFixturePlatformProfileWriter),
            Arc::new(RealFixtureBatteryChargeTypeWriter),
            Arc::new(RealFixtureLedStateWriter),
            Arc::new(RealFixtureIdeapadToggleWriter),
            Arc::new(NoOpCpuGovernorWriter),
            Arc::new(NoOpCpuEppWriter),
        ));
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args([
            "--set-ideapad-toggle",
            "usb_charging=off",
            "--bus-address",
            &address,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "applied");
    assert_eq!(json["applied"], true);
    assert_eq!(json["readback_value"], "0");
    assert_eq!(json["plan"]["method"], "SetIdeapadToggle");
    assert_eq!(
        fs::read_to_string(
            fixture.join("sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/usb_charging")
        )
        .unwrap()
        .trim(),
        "0"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn fan_preset_plan_cli_prints_read_only_write_preview_json() {
    let (_bus, _service_connection, address) = runtime_fixture_service();
    let client = LegionControlClient::address(&address).unwrap();
    let plan = client.plan_fan_preset_write("balanced-daily").unwrap();
    assert_eq!(plan.method, "ApplyFanPreset");
    assert_eq!(plan.previous_value, "current fan curve snapshot");
    assert_eq!(plan.requested_value, "balanced-daily");
    assert!(!plan.reboot_required);

    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args([
            "--plan-fan-preset",
            "balanced-daily",
            "--bus-address",
            &address,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["method"], "ApplyFanPreset");
    assert_eq!(json["capability_id"], "fan_curves");
    assert_eq!(json["requested_value"], "balanced-daily");
    assert_eq!(json["readback_required"], true);
}

#[test]
fn restore_auto_fan_plan_cli_prints_read_only_write_preview_json() {
    let (_bus, _service_connection, address) = runtime_fixture_service();
    let client = LegionControlClient::address(&address).unwrap();
    let plan = client.plan_restore_auto_fan_write().unwrap();
    assert_eq!(plan.method, "RestoreAutoFan");
    assert_eq!(plan.previous_value, "current fan-control state");
    assert_eq!(plan.requested_value, "auto/default fan control");
    assert!(!plan.reboot_required);

    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args(["--plan-restore-auto-fan", "--bus-address", &address])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["method"], "RestoreAutoFan");
    assert_eq!(json["capability_id"], "fan_curves");
    assert_eq!(json["requested_value"], "auto/default fan control");
    assert_eq!(json["readback_required"], true);
}

#[test]
fn fan_curve_live_cli_prints_read_only_sysfs_snapshot_json() {
    let (_bus, _service_connection, address) = runtime_fixture_service();
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args(["--fan-curve-live", "--bus-address", &address])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["curve_id"], "legion_hwmon");
    assert!(json["points"].as_array().unwrap().len() >= 20);
}

#[test]
fn gpu_pending_cli_prints_and_clears_empty_state() {
    let state_path = unique_state_path("ui-pending-empty");
    let (_bus, _service_connection, address) = fixture_service_with_state(&state_path);
    let client = LegionControlClient::address(&address).unwrap();
    assert_eq!(client.gpu_mode_pending().unwrap(), None);
    assert_eq!(client.clear_gpu_mode_pending().unwrap(), None);

    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args(["--gpu-mode-pending", "--bus-address", &address])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "null\n");

    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args(["--clear-gpu-mode-pending", "--bus-address", &address])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "null\n");
    let _ = std::fs::remove_file(state_path);
}

#[test]
fn last_curve_optimizer_cli_prints_empty_state() {
    let state_path = unique_state_path("ui-curve-optimizer-empty");
    let (_bus, _service_connection, address) = fixture_service_with_state(&state_path);
    let client = LegionControlClient::address(&address).unwrap();
    assert_eq!(client.last_curve_optimizer_all_core().unwrap(), None);

    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args(["--last-curve-optimizer-all-core", "--bus-address", &address])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "null\n");
    let _ = std::fs::remove_file(state_path);
}

#[test]
fn last_known_good_fan_curve_cli_captures_state_snapshot() {
    let state_path = unique_state_path("ui-fan-curve");
    let (_bus, _service_connection, address) = runtime_fixture_service_with_state(&state_path);
    let client = LegionControlClient::address(&address).unwrap();
    assert_eq!(client.last_known_good_fan_curve().unwrap(), None);

    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args(["--last-known-good-fan-curve", "--bus-address", &address])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "null\n");

    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-ui"))
        .args([
            "--capture-last-known-good-fan-curve",
            "--bus-address",
            &address,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["curve_id"], "legion_hwmon");
    let point_count = json["points"].as_array().unwrap().len();
    assert!(point_count >= 20);
    assert!(json["points"].as_array().unwrap().iter().any(|point| {
        point["path"]
            .as_str()
            .unwrap()
            .ends_with("pwm1_auto_point1_temp")
            && point["value"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
    }));
    assert_eq!(
        client
            .last_known_good_fan_curve()
            .unwrap()
            .unwrap()
            .points
            .len(),
        point_count
    );
    let _ = std::fs::remove_file(state_path);
}

#[test]
fn status_model_uses_unknown_for_missing_hardware_fields() {
    let status = UiStatus::from_parts(Default::default(), Vec::new()).unwrap();

    assert_eq!(status.hardware.vendor, "unknown");
    assert_eq!(status.hardware.product_name, "unknown");
    assert_eq!(status.hardware.product_version, "unknown");
    assert!(status.hardware.product_sku.is_none());
    assert_eq!(status.capability_count(), 0);
    assert!(status.capability_ids().is_empty());
}

#[test]
fn status_model_preserves_capability_badge_fields() {
    let status = UiStatus::from_parts(
        HardwareSummary {
            vendor: Some("LENOVO".to_owned()),
            product_name: Some("82WM".to_owned()),
            product_version: Some("Legion Pro 5 16ARX8".to_owned()),
            product_sku: Some("SKU".to_owned()),
            sysfs_root: "/fixture".to_owned(),
        },
        vec![
            capability(
                "z_last",
                "Last",
                CapabilityStatus::Missing,
                RiskLevel::Unsupported,
            ),
            capability(
                "a_first",
                "First",
                CapabilityStatus::Detected,
                RiskLevel::ReadOnly,
            ),
        ],
    )
    .unwrap();

    assert_eq!(status.hardware.product_sku.as_deref(), Some("SKU"));
    assert_eq!(status.capability_ids(), ["a_first", "z_last"]);
    assert_eq!(status.capabilities[0].label, "First");
    assert_eq!(status.capabilities[0].status, CapabilityStatus::Detected);
    assert_eq!(status.capabilities[0].risk, RiskLevel::ReadOnly);
    assert_eq!(status.capabilities[1].label, "Last");
    assert_eq!(status.capabilities[1].status, CapabilityStatus::Missing);
    assert_eq!(status.capabilities[1].risk, RiskLevel::Unsupported);
}

fn capability(id: &str, label: &str, status: CapabilityStatus, risk: RiskLevel) -> Capability {
    Capability {
        id: id.to_owned(),
        label: label.to_owned(),
        status,
        risk,
        evidence: Vec::new(),
        details: serde_json::Value::Null,
    }
}

fn fixture_service() -> (PrivateBus, zbus::blocking::Connection, String) {
    let state_path = unique_state_path("ui-empty-state");
    let _ = fs::remove_file(&state_path);
    fixture_service_with_state(&state_path)
}

fn fixture_service_with_runtime(
    service: LegionControl,
) -> (PrivateBus, zbus::blocking::Connection, String) {
    let bus = PrivateBus::start();
    let service_connection = ConnectionBuilder::address(bus.address())
        .unwrap()
        .name(DBUS_INTERFACE)
        .unwrap()
        .serve_at(DBUS_PATH, service)
        .unwrap()
        .build()
        .unwrap();
    let address = bus.address().to_owned();

    (bus, service_connection, address)
}

fn fixture_service_with_state(
    state_path: &std::path::Path,
) -> (PrivateBus, zbus::blocking::Connection, String) {
    let bus = PrivateBus::start();
    let service = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: fixture_root(),
        },
        state_path,
    );
    let service_connection = ConnectionBuilder::address(bus.address())
        .unwrap()
        .name(DBUS_INTERFACE)
        .unwrap()
        .serve_at(DBUS_PATH, service)
        .unwrap()
        .build()
        .unwrap();
    let address = bus.address().to_owned();

    (bus, service_connection, address)
}

fn runtime_fixture_service() -> (PrivateBus, zbus::blocking::Connection, String) {
    let state_path = unique_state_path("ui-runtime-empty-state");
    let _ = fs::remove_file(&state_path);
    runtime_fixture_service_with_state(&state_path)
}

fn runtime_fixture_service_with_state(
    state_path: &std::path::Path,
) -> (PrivateBus, zbus::blocking::Connection, String) {
    let bus = PrivateBus::start();
    let service = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: fixture_root()
                .parent()
                .expect("fixture root must have parent")
                .join("sysfs-82wm-runtime-capture"),
        },
        state_path,
    );
    let service_connection = ConnectionBuilder::address(bus.address())
        .unwrap()
        .name(DBUS_INTERFACE)
        .unwrap()
        .serve_at(DBUS_PATH, service)
        .unwrap()
        .build()
        .unwrap();
    let address = bus.address().to_owned();

    (bus, service_connection, address)
}

fn copied_runtime_fixture_root(label: &str) -> std::path::PathBuf {
    let destination = std::path::PathBuf::from("/tmp").join(format!(
        "ratvantage-{label}-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let status = std::process::Command::new("cp")
        .args([
            "-a",
            fixture_root()
                .parent()
                .expect("fixture root must have parent")
                .join("sysfs-82wm-runtime-capture")
                .to_str()
                .unwrap(),
            destination.to_str().unwrap(),
        ])
        .status()
        .expect("cp must be available for runtime fixture copy tests");
    assert!(status.success(), "cp -a runtime fixture copy must succeed");
    destination
}

fn seed_usb_charging_toggle(root: &std::path::Path, value: &str) {
    let path = root.join("sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/usb_charging");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, value).unwrap();
}

fn unique_state_path(label: &str) -> std::path::PathBuf {
    std::path::PathBuf::from("/tmp").join(format!(
        "ratvantage-{label}-{}-{}.toml",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ))
}

struct AllowAllAuthorizer;

impl WriteAuthorizer for AllowAllAuthorizer {
    fn authorize(&self, _action: &str, _sender: &str) -> std::result::Result<(), String> {
        Ok(())
    }
}

struct RealFixturePlatformProfileWriter;

impl PlatformProfileWriter for RealFixturePlatformProfileWriter {
    fn write_platform_profile(
        &self,
        path: &str,
        requested: &str,
    ) -> std::result::Result<(), String> {
        fs::write(path, requested).map_err(|error| error.to_string())
    }
}

struct RealFixtureBatteryChargeTypeWriter;

impl BatteryChargeTypeWriter for RealFixtureBatteryChargeTypeWriter {
    fn write_battery_charge_type(
        &self,
        path: &str,
        requested: &str,
    ) -> std::result::Result<(), String> {
        fs::write(path, requested).map_err(|error| error.to_string())
    }
}

struct RealFixtureLedStateWriter;

impl LedStateWriter for RealFixtureLedStateWriter {
    fn write_led_state(&self, path: &str, enabled: bool) -> std::result::Result<(), String> {
        fs::write(path, if enabled { "1" } else { "0" }).map_err(|error| error.to_string())
    }
}

struct NoOpCpuGovernorWriter;

impl CpuGovernorWriter for NoOpCpuGovernorWriter {
    fn write_cpu_governor(&self, _path: &str, _requested: &str) -> std::result::Result<(), String> {
        Ok(())
    }
}

struct RealFixtureCpuGovernorWriter;

impl CpuGovernorWriter for RealFixtureCpuGovernorWriter {
    fn write_cpu_governor(&self, path: &str, requested: &str) -> std::result::Result<(), String> {
        fs::write(path, requested).map_err(|error| error.to_string())
    }
}

struct NoOpCpuEppWriter;

impl CpuEppWriter for NoOpCpuEppWriter {
    fn write_cpu_epp(&self, _path: &str, _requested: &str) -> std::result::Result<(), String> {
        Ok(())
    }
}

struct RealFixtureCpuEppWriter;

impl CpuEppWriter for RealFixtureCpuEppWriter {
    fn write_cpu_epp(&self, path: &str, requested: &str) -> std::result::Result<(), String> {
        fs::write(path, requested).map_err(|error| error.to_string())
    }
}

struct RealFixtureIdeapadToggleWriter;

impl IdeapadToggleWriter for RealFixtureIdeapadToggleWriter {
    fn write_ideapad_toggle(&self, path: &str, enabled: bool) -> std::result::Result<(), String> {
        fs::write(path, if enabled { "1" } else { "0" }).map_err(|error| error.to_string())?;
        if path.ends_with("/fn_lock") {
            let indicator = path.replace(
                "sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/fn_lock",
                "sys/class/leds/platform::fnlock/brightness",
            );
            fs::write(indicator, if enabled { "1" } else { "0" })
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}
