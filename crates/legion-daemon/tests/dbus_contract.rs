use std::{fs, sync::Arc};

use legion_common::{
    Capability, CapabilityRegistry, FanCurveSnapshot, GpuModePending, HardwareSummary,
    TelemetrySnapshot, WriteDryRunPlan, WriteExecutionResult, WriteExecutionStatus,
};
use legion_control_daemon::{
    BatteryChargeTypeWriter, IdeapadToggleWriter, LedStateWriter, LegionControl,
    PlatformProfileWriter, WriteAccessPolicy, WriteAuthorizer, DBUS_INTERFACE, DBUS_PATH,
};
use legion_probe::ProbeOptions;
use ratvantage_test_support::{
    call_json, copied_fixture_root, fixture_root, introspected_methods, PrivateBus,
};
use zbus::blocking::{ConnectionBuilder, Proxy};

#[test]
fn read_only_methods_return_expected_json_contracts() {
    let (_bus, _service_connection, proxy) = test_proxy();

    let hardware: HardwareSummary = call_json(&proxy, "GetHardwareSummary");
    assert_eq!(hardware.vendor.as_deref(), Some("LENOVO"));
    assert_eq!(hardware.product_name.as_deref(), Some("82WM"));
    assert_eq!(
        hardware.product_version.as_deref(),
        Some("Legion Pro 5 16ARX8")
    );

    let capabilities: Vec<Capability> = call_json(&proxy, "GetCapabilities");
    assert!(capabilities
        .iter()
        .any(|capability| capability.id == "platform_profile"));
    assert!(capabilities
        .iter()
        .any(|capability| capability.id == "leds"));

    let telemetry: TelemetrySnapshot = call_json(&proxy, "GetTelemetry");
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
    assert_eq!(
        telemetry
            .battery
            .as_ref()
            .and_then(|battery| battery.status.as_deref()),
        Some("Charging")
    );
    assert_eq!(
        telemetry
            .battery
            .as_ref()
            .and_then(|battery| battery.health.as_deref()),
        Some("Good")
    );

    let raw: CapabilityRegistry = call_json(&proxy, "GetRawProbeReport");
    assert_eq!(raw.hardware, hardware);
    assert_eq!(raw.capabilities, capabilities);
    assert_eq!(raw.telemetry, telemetry);
    assert!(raw
        .fan_curves
        .iter()
        .any(|curve| curve.id == "legion-hwmon"));

    let refreshed: Vec<Capability> = call_json(&proxy, "RefreshCapabilities");
    assert_eq!(refreshed, capabilities);

    let live: FanCurveSnapshot = call_json(&proxy, "GetLiveFanCurveReadings");
    assert_eq!(live.curve_id, "legion-hwmon");
    assert!(
        live.points
            .iter()
            .any(|point| point.path.contains("pwm1_auto")),
        "expected pwm auto point paths in live readings: {:?}",
        live.points
    );

    let payload: String = proxy
        .call("PlanPlatformProfileWrite", &("performance",))
        .unwrap();
    let platform_plan: WriteDryRunPlan = serde_json::from_str(&payload).unwrap();
    assert_eq!(platform_plan.method, "SetPlatformProfile");
    assert_eq!(platform_plan.requested_value, "performance");
    assert_eq!(platform_plan.previous_value, "balanced");

    let payload: String = proxy
        .call("PlanBatteryChargeTypeWrite", &("Conservation",))
        .unwrap();
    let battery_plan: WriteDryRunPlan = serde_json::from_str(&payload).unwrap();
    assert_eq!(battery_plan.method, "SetBatteryChargeType");
    assert_eq!(battery_plan.requested_value, "Conservation");
    assert_eq!(battery_plan.previous_value, "Standard");

    let payload: String = proxy
        .call("PlanLedStateWrite", &("platform::ylogo", false))
        .unwrap();
    let led_plan: WriteDryRunPlan = serde_json::from_str(&payload).unwrap();
    assert_eq!(led_plan.method, "SetLedState");
    assert_eq!(led_plan.requested_value, "0");
    assert_eq!(led_plan.previous_value, "1");

    let payload: String = proxy
        .call("PlanIdeapadToggleWrite", &("fn_lock", true))
        .unwrap();
    let toggle_plan: WriteDryRunPlan = serde_json::from_str(&payload).unwrap();
    assert_eq!(toggle_plan.method, "SetIdeapadToggle");
    assert_eq!(toggle_plan.requested_value, "1");
    assert_eq!(toggle_plan.previous_value, "0");

    let payload: String = proxy
        .call("PlanIdeapadToggleWrite", &("camera_power", false))
        .unwrap();
    let toggle_plan: WriteDryRunPlan = serde_json::from_str(&payload).unwrap();
    assert_eq!(toggle_plan.method, "SetIdeapadToggle");
    assert_eq!(toggle_plan.requested_value, "0");
    assert_eq!(toggle_plan.previous_value, "1");

    let payload: String = proxy.call("SetPlatformProfile", &("performance",)).unwrap();
    let execution: WriteExecutionResult = serde_json::from_str(&payload).unwrap();
    assert_eq!(execution.status, WriteExecutionStatus::BlockedByPolicy);
    assert!(!execution.applied);
    assert_eq!(execution.plan.method, "SetPlatformProfile");
    assert_eq!(execution.plan.requested_value, "performance");

    let payload: String = proxy
        .call("SetBatteryChargeType", &("Conservation",))
        .unwrap();
    let execution: WriteExecutionResult = serde_json::from_str(&payload).unwrap();
    assert_eq!(execution.status, WriteExecutionStatus::BlockedByPolicy);
    assert!(!execution.applied);
    assert_eq!(execution.plan.method, "SetBatteryChargeType");
    assert_eq!(execution.plan.requested_value, "Conservation");

    let payload: String = proxy
        .call("SetLedState", &("platform::ylogo", false))
        .unwrap();
    let execution: WriteExecutionResult = serde_json::from_str(&payload).unwrap();
    assert_eq!(execution.status, WriteExecutionStatus::BlockedByPolicy);
    assert!(!execution.applied);
    assert_eq!(execution.plan.method, "SetLedState");
    assert_eq!(execution.plan.requested_value, "0");

    let payload: String = proxy.call("SetIdeapadToggle", &("fn_lock", true)).unwrap();
    let execution: WriteExecutionResult = serde_json::from_str(&payload).unwrap();
    assert_eq!(execution.status, WriteExecutionStatus::BlockedByPolicy);
    assert!(!execution.applied);
    assert_eq!(execution.plan.method, "SetIdeapadToggle");
    assert_eq!(execution.plan.requested_value, "1");
}

#[test]
fn introspection_exposes_gated_reversible_write_methods_only() {
    let (_bus, _service_connection, proxy) = test_proxy();
    let xml = proxy.introspect().unwrap();
    let mut methods = introspected_methods(&xml, DBUS_INTERFACE);
    methods.sort_unstable();

    assert_eq!(
        methods,
        [
            "CaptureLastKnownGoodFanCurve",
            "ClearGpuModePending",
            "GetCapabilities",
            "GetGpuModePending",
            "GetHardwareSummary",
            "GetLastKnownGoodFanCurve",
            "GetLiveFanCurveReadings",
            "GetRawProbeReport",
            "GetTelemetry",
            "PlanBatteryChargeTypeWrite",
            "PlanFanPresetWrite",
            "PlanGpuModeWrite",
            "PlanIdeapadToggleWrite",
            "PlanLedStateWrite",
            "PlanPlatformProfileWrite",
            "PlanRestoreAutoFanWrite",
            "RefreshCapabilities",
            "SetBatteryChargeType",
            "SetGpuModePending",
            "SetIdeapadToggle",
            "SetLedState",
            "SetPlatformProfile",
        ]
    );
    assert!(!methods.iter().any(|method| matches!(
        method.as_str(),
        "SetGpuMode" | "ApplyFanPreset" | "RestoreAutoFan"
    )));
}

#[test]
fn daemon_builds_dry_run_plans_without_other_dbus_write_methods() {
    let service = LegionControl::new(ProbeOptions {
        sysfs_root: fixture_root(),
    });

    let platform_plan = service.plan_platform_profile_write("performance").unwrap();
    assert_eq!(platform_plan.method, "SetPlatformProfile");
    assert_eq!(platform_plan.capability_id, "platform_profile");
    assert_eq!(platform_plan.previous_value, "balanced");
    assert_eq!(platform_plan.requested_value, "performance");
    assert_eq!(platform_plan.rollback_value, "balanced");
    assert!(platform_plan.readback_required);

    let battery_plan = service
        .plan_battery_charge_type_write("Conservation")
        .unwrap();
    assert_eq!(battery_plan.method, "SetBatteryChargeType");
    assert_eq!(battery_plan.capability_id, "battery_charge_type");
    assert_eq!(battery_plan.previous_value, "Standard");
    assert_eq!(battery_plan.requested_value, "Conservation");
    assert_eq!(battery_plan.rollback_value, "Standard");
    assert!(battery_plan.readback_required);

    let led_plan = service
        .plan_led_state_write("platform::ylogo", false)
        .unwrap();
    assert_eq!(led_plan.method, "SetLedState");
    assert_eq!(led_plan.capability_id, "leds");
    assert_eq!(led_plan.previous_value, "1");
    assert_eq!(led_plan.requested_value, "0");
    assert_eq!(led_plan.rollback_value, "1");
    assert!(led_plan.readback_required);

    let toggle_plan = service.plan_ideapad_toggle_write("fn_lock", true).unwrap();
    assert_eq!(toggle_plan.method, "SetIdeapadToggle");
    assert_eq!(toggle_plan.capability_id, "ideapad_toggles");
    assert_eq!(toggle_plan.previous_value, "0");
    assert_eq!(toggle_plan.requested_value, "1");
    assert_eq!(toggle_plan.rollback_value, "0");
    assert!(toggle_plan.readback_required);

    let camera_plan = service
        .plan_ideapad_toggle_write("camera_power", false)
        .unwrap();
    assert_eq!(camera_plan.previous_value, "1");
    assert_eq!(camera_plan.requested_value, "0");

    assert!(service.plan_platform_profile_write("custom").is_err());
    assert!(service.plan_battery_charge_type_write("Invalid").is_err());
    assert!(service
        .plan_led_state_write("platform::fnlock", true)
        .is_err());
    assert!(service
        .plan_ideapad_toggle_write("touchpad", false)
        .is_err());
    assert!(service
        .plan_ideapad_toggle_write("conservation_mode", false)
        .is_err());
    assert!(service
        .plan_ideapad_toggle_write("fan_mode", false)
        .is_err());
    assert!(service.plan_gpu_mode_write("hybrid").is_err());
    assert!(service.plan_fan_preset_write("balanced-daily").is_err());

    let restore_plan = service.plan_restore_auto_fan_write().unwrap();
    assert_eq!(restore_plan.method, "RestoreAutoFan");
    assert_eq!(restore_plan.capability_id, "fan_curves");
    assert_eq!(restore_plan.requested_value, "auto/default fan control");
}

#[test]
fn platform_profile_write_applies_when_policy_and_authorizer_allow_it() {
    let fixture = copied_fixture_root("platform-profile-write-success");
    let state_path = unique_state_path("platform-profile-write-success");
    let service = LegionControl::new_with_runtime(
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
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
    );

    let result = service
        .set_platform_profile("performance", ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::Applied);
    assert!(result.applied);
    assert_eq!(result.readback_value.as_deref(), Some("performance"));
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
fn platform_profile_write_rejects_invalid_choice_before_write() {
    let fixture = copied_fixture_root("platform-profile-write-invalid");
    let state_path = unique_state_path("platform-profile-write-invalid");
    let service = LegionControl::new_with_runtime(
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
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
    );

    assert!(service.set_platform_profile("custom", ":1.99").is_err());
    assert_eq!(
        fs::read_to_string(fixture.join("sys/firmware/acpi/platform_profile"))
            .unwrap()
            .trim(),
        "balanced"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn platform_profile_write_reports_write_failure_without_changing_value() {
    let fixture = copied_fixture_root("platform-profile-write-failure");
    let state_path = unique_state_path("platform-profile-write-failure");
    let service = LegionControl::new_with_runtime(
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
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(FailingPlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
    );

    let result = service
        .set_platform_profile("performance", ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::Failed);
    assert!(!result.applied);
    assert!(result.message.contains("failed to write platform profile"));
    assert_eq!(
        fs::read_to_string(fixture.join("sys/firmware/acpi/platform_profile"))
            .unwrap()
            .trim(),
        "balanced"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn platform_profile_write_rolls_back_after_readback_mismatch() {
    let fixture = copied_fixture_root("platform-profile-write-rollback");
    let state_path = unique_state_path("platform-profile-write-rollback");
    let service = LegionControl::new_with_runtime(
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
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(MismatchingPlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
    );

    let result = service
        .set_platform_profile("performance", ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::Failed);
    assert!(!result.applied);
    assert!(result
        .message
        .contains("restored previous value `balanced`"));
    assert_eq!(result.readback_value.as_deref(), Some("balanced"));
    assert_eq!(
        fs::read_to_string(fixture.join("sys/firmware/acpi/platform_profile"))
            .unwrap()
            .trim(),
        "balanced"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn battery_charge_type_write_applies_when_policy_and_authorizer_allow_it() {
    let fixture = copied_fixture_root("battery-charge-type-write-success");
    let state_path = unique_state_path("battery-charge-type-write-success");
    let service = LegionControl::new_with_runtime(
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
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
    );

    let result = service
        .set_battery_charge_type("Conservation", ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::Applied);
    assert!(result.applied);
    assert_eq!(result.readback_value.as_deref(), Some("Conservation"));
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
fn battery_charge_type_write_rolls_back_after_readback_mismatch() {
    let fixture = copied_fixture_root("battery-charge-type-write-rollback");
    let state_path = unique_state_path("battery-charge-type-write-rollback");
    let service = LegionControl::new_with_runtime(
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
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(MismatchingBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
    );

    let result = service
        .set_battery_charge_type("Conservation", ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::Failed);
    assert!(!result.applied);
    assert!(result
        .message
        .contains("restored previous value `Standard`"));
    assert_eq!(result.readback_value.as_deref(), Some("Standard"));
    assert_eq!(
        fs::read_to_string(fixture.join("sys/class/power_supply/BAT0/charge_type"))
            .unwrap()
            .trim(),
        "Standard"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn led_state_write_applies_when_policy_and_authorizer_allow_it() {
    let fixture = copied_fixture_root("led-state-write-success");
    let state_path = unique_state_path("led-state-write-success");
    let service = LegionControl::new_with_runtime(
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
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
    );

    let result = service
        .set_led_state("platform::ylogo", false, ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::Applied);
    assert!(result.applied);
    assert_eq!(result.readback_value.as_deref(), Some("0"));
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
fn led_state_write_rolls_back_after_readback_mismatch() {
    let fixture = copied_fixture_root("led-state-write-rollback");
    let state_path = unique_state_path("led-state-write-rollback");
    let service = LegionControl::new_with_runtime(
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
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(MismatchingLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
    );

    let result = service
        .set_led_state("platform::ylogo", false, ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::Failed);
    assert!(!result.applied);
    assert!(result.message.contains("restored previous value `1`"));
    assert_eq!(result.readback_value.as_deref(), Some("1"));
    assert_eq!(
        fs::read_to_string(fixture.join("sys/class/leds/platform::ylogo/brightness"))
            .unwrap()
            .trim(),
        "1"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn ideapad_toggle_write_applies_when_policy_and_authorizer_allow_it() {
    let fixture = copied_fixture_root("ideapad-toggle-write-success");
    let state_path = unique_state_path("ideapad-toggle-write-success");
    let service = LegionControl::new_with_runtime(
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
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
    );

    let result = service
        .set_ideapad_toggle("fn_lock", true, ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::Applied);
    assert!(result.applied);
    assert_eq!(result.readback_value.as_deref(), Some("1"));
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
fn ideapad_toggle_write_rolls_back_after_readback_mismatch() {
    let fixture = copied_fixture_root("ideapad-toggle-write-rollback");
    let state_path = unique_state_path("ideapad-toggle-write-rollback");
    let service = LegionControl::new_with_runtime(
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
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(MismatchingIdeapadToggleWriter),
    );

    let result = service
        .set_ideapad_toggle("fn_lock", true, ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::Failed);
    assert!(!result.applied);
    assert!(result.message.contains("restored previous value `0`"));
    assert_eq!(result.readback_value.as_deref(), Some("0"));
    assert_eq!(
        fs::read_to_string(
            fixture.join("sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/fn_lock")
        )
        .unwrap()
        .trim(),
        "0"
    );
    assert_eq!(
        fs::read_to_string(fixture.join("sys/class/leds/platform::fnlock/brightness"))
            .unwrap()
            .trim(),
        "0"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn camera_power_write_applies_when_policy_and_authorizer_allow_it() {
    let fixture = copied_fixture_root("camera-power-write-success");
    let state_path = unique_state_path("camera-power-write-success");
    let service = LegionControl::new_with_runtime(
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
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
    );

    let result = service
        .set_ideapad_toggle("camera_power", false, ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::Applied);
    assert!(result.applied);
    assert_eq!(result.readback_value.as_deref(), Some("0"));
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
fn camera_power_write_rolls_back_after_readback_mismatch() {
    let fixture = copied_fixture_root("camera-power-write-rollback");
    let state_path = unique_state_path("camera-power-write-rollback");
    let service = LegionControl::new_with_runtime(
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
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(MismatchingCameraPowerWriter),
    );

    let result = service
        .set_ideapad_toggle("camera_power", false, ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::Failed);
    assert!(!result.applied);
    assert!(result.message.contains("restored previous value `1`"));
    assert_eq!(result.readback_value.as_deref(), Some("1"));
    assert_eq!(
        fs::read_to_string(
            fixture.join("sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/camera_power")
        )
        .unwrap()
        .trim(),
        "1"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn usb_charging_write_reports_policy_block_when_write_is_disabled() {
    let fixture = copied_runtime_fixture_root("usb-charging-write-blocked");
    seed_usb_charging_toggle(&fixture, "1");
    let state_path = unique_state_path("usb-charging-write-blocked");
    let service = LegionControl::new_with_runtime(
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
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
    );

    let plan = service
        .plan_ideapad_toggle_write("usb_charging", false)
        .unwrap();
    assert_eq!(plan.previous_value, "1");
    assert_eq!(plan.requested_value, "0");

    let result = service
        .set_ideapad_toggle("usb_charging", false, ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::BlockedByPolicy);
    assert!(!result.applied);
    assert!(result.message.contains("usb charging writes are disabled"));
    assert_eq!(
        fs::read_to_string(
            fixture.join("sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/usb_charging")
        )
        .unwrap()
        .trim(),
        "1"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn usb_charging_write_applies_when_policy_and_authorizer_allow_it() {
    let fixture = copied_runtime_fixture_root("usb-charging-write-success");
    seed_usb_charging_toggle(&fixture, "1");
    let state_path = unique_state_path("usb-charging-write-success");
    let service = LegionControl::new_with_runtime(
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
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
    );

    let result = service
        .set_ideapad_toggle("usb_charging", false, ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::Applied);
    assert!(result.applied);
    assert_eq!(result.readback_value.as_deref(), Some("0"));
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
fn usb_charging_write_rolls_back_after_readback_mismatch() {
    let fixture = copied_runtime_fixture_root("usb-charging-write-rollback");
    seed_usb_charging_toggle(&fixture, "1");
    let state_path = unique_state_path("usb-charging-write-rollback");
    let service = LegionControl::new_with_runtime(
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
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(MismatchingUsbChargingWriter),
    );

    let result = service
        .set_ideapad_toggle("usb_charging", false, ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::Failed);
    assert!(!result.applied);
    assert!(result.message.contains("restored previous value `1`"));
    assert_eq!(result.readback_value.as_deref(), Some("1"));
    assert_eq!(
        fs::read_to_string(
            fixture.join("sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/usb_charging")
        )
        .unwrap()
        .trim(),
        "1"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn daemon_builds_fan_preset_plan_from_runtime_fixture() {
    let state_path = unique_state_path("runtime-fan-curve");
    let service = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: runtime_fixture_root(),
        },
        &state_path,
    );

    let plan = service.plan_fan_preset_write("balanced-daily").unwrap();
    assert_eq!(plan.method, "ApplyFanPreset");
    assert_eq!(plan.capability_id, "fan_curves");
    assert_eq!(plan.requested_value, "balanced-daily");
    assert_eq!(plan.previous_value, "current fan curve snapshot");
    assert!(plan.readback_required);
    assert!(!plan.reboot_required);
    assert!(plan
        .safety_notes
        .iter()
        .any(|note| note.contains("Middle-ground fan ramp")));

    let restore_plan = service.plan_restore_auto_fan_write().unwrap();
    assert_eq!(restore_plan.method, "RestoreAutoFan");
    assert_eq!(restore_plan.capability_id, "fan_curves");
    assert_eq!(restore_plan.requested_value, "auto/default fan control");
    assert!(restore_plan.readback_required);
    assert!(!restore_plan.reboot_required);

    assert_eq!(service.last_known_good_fan_curve().unwrap(), None);
    let captured = service.capture_last_known_good_fan_curve().unwrap();
    assert_eq!(captured.curve_id, "legion_hwmon");
    assert!(captured.points.len() >= 20);
    assert!(captured
        .points
        .iter()
        .any(|point| point.path.ends_with("pwm1_auto_point1_temp") && !point.value.is_empty()));
    assert_eq!(service.last_known_good_fan_curve().unwrap(), Some(captured));
    let _ = fs::remove_file(state_path);
}

#[test]
fn daemon_loads_clears_and_ignores_invalid_state_files() {
    let state_path = unique_state_path("pending-gpu");
    fs::write(
        &state_path,
        r#"schema_version = 1

[gpu_mode_pending]
requested_mode = "hybrid"
previous_mode = "nvidia"
reboot_required = true
"#,
    )
    .unwrap();

    let service = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: fixture_root(),
        },
        &state_path,
    );
    let pending = service.gpu_mode_pending().unwrap().unwrap();
    assert_eq!(
        pending,
        GpuModePending {
            requested_mode: "hybrid".to_owned(),
            previous_mode: Some("nvidia".to_owned()),
            reboot_required: true,
        }
    );

    let cleared = service.clear_gpu_mode_pending().unwrap().unwrap();
    assert_eq!(cleared.requested_mode, "hybrid");
    let reloaded = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: fixture_root(),
        },
        &state_path,
    );
    assert_eq!(reloaded.gpu_mode_pending().unwrap(), None);

    fs::write(&state_path, "not valid toml =").unwrap();
    let corrupt = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: fixture_root(),
        },
        &state_path,
    );
    assert_eq!(corrupt.gpu_mode_pending().unwrap(), None);
    let _ = fs::remove_file(state_path);
}

fn test_proxy() -> (PrivateBus, zbus::blocking::Connection, Proxy<'static>) {
    test_proxy_with_service(LegionControl::new(ProbeOptions {
        sysfs_root: fixture_root(),
    }))
}

fn test_proxy_with_service(
    service: LegionControl,
) -> (PrivateBus, zbus::blocking::Connection, Proxy<'static>) {
    let bus = PrivateBus::start();
    let service_connection = ConnectionBuilder::address(bus.address())
        .unwrap()
        .name(DBUS_INTERFACE)
        .unwrap()
        .serve_at(DBUS_PATH, service)
        .unwrap()
        .build()
        .unwrap();
    let client_connection = ConnectionBuilder::address(bus.address())
        .unwrap()
        .build()
        .unwrap();
    let proxy =
        Proxy::new_owned(client_connection, DBUS_INTERFACE, DBUS_PATH, DBUS_INTERFACE).unwrap();

    (bus, service_connection, proxy)
}

fn runtime_fixture_root() -> std::path::PathBuf {
    fixture_root()
        .parent()
        .expect("fixture root must have parent")
        .join("sysfs-82wm-runtime-capture")
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
            runtime_fixture_root().to_str().unwrap(),
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

struct FailingPlatformProfileWriter;

impl PlatformProfileWriter for FailingPlatformProfileWriter {
    fn write_platform_profile(
        &self,
        _path: &str,
        _requested: &str,
    ) -> std::result::Result<(), String> {
        Err("injected write failure".to_owned())
    }
}

struct MismatchingPlatformProfileWriter;

impl PlatformProfileWriter for MismatchingPlatformProfileWriter {
    fn write_platform_profile(
        &self,
        path: &str,
        requested: &str,
    ) -> std::result::Result<(), String> {
        let value = if requested == "performance" {
            "balanced"
        } else {
            requested
        };
        fs::write(path, value).map_err(|error| error.to_string())
    }
}

struct MismatchingBatteryChargeTypeWriter;

impl BatteryChargeTypeWriter for MismatchingBatteryChargeTypeWriter {
    fn write_battery_charge_type(
        &self,
        path: &str,
        requested: &str,
    ) -> std::result::Result<(), String> {
        let value = if requested == "Conservation" {
            "Standard"
        } else {
            requested
        };
        fs::write(path, value).map_err(|error| error.to_string())
    }
}

struct MismatchingLedStateWriter;

impl LedStateWriter for MismatchingLedStateWriter {
    fn write_led_state(&self, path: &str, _enabled: bool) -> std::result::Result<(), String> {
        fs::write(path, "1").map_err(|error| error.to_string())
    }
}

struct MismatchingIdeapadToggleWriter;

impl IdeapadToggleWriter for MismatchingIdeapadToggleWriter {
    fn write_ideapad_toggle(&self, path: &str, _enabled: bool) -> std::result::Result<(), String> {
        fs::write(path, "0").map_err(|error| error.to_string())?;
        if path.ends_with("/fn_lock") {
            let indicator = path.replace(
                "sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/fn_lock",
                "sys/class/leds/platform::fnlock/brightness",
            );
            fs::write(indicator, "0").map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

struct MismatchingCameraPowerWriter;

impl IdeapadToggleWriter for MismatchingCameraPowerWriter {
    fn write_ideapad_toggle(&self, path: &str, _enabled: bool) -> std::result::Result<(), String> {
        fs::write(path, "1").map_err(|error| error.to_string())
    }
}

struct MismatchingUsbChargingWriter;

impl IdeapadToggleWriter for MismatchingUsbChargingWriter {
    fn write_ideapad_toggle(&self, path: &str, _enabled: bool) -> std::result::Result<(), String> {
        fs::write(path, "1").map_err(|error| error.to_string())
    }
}
