use std::{fs, sync::Arc};

use legion_common::{
    Capability, CapabilityRegistry, GpuModePending, HardwareSummary, TelemetrySnapshot,
    WriteDryRunPlan, WriteExecutionResult, WriteExecutionStatus,
};
use legion_control_daemon::{
    LegionControl, PlatformProfileWriter, WriteAccessPolicy, WriteAuthorizer, DBUS_INTERFACE,
    DBUS_PATH,
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

    let payload: String = proxy.call("SetPlatformProfile", &("performance",)).unwrap();
    let execution: WriteExecutionResult = serde_json::from_str(&payload).unwrap();
    assert_eq!(execution.status, WriteExecutionStatus::BlockedByPolicy);
    assert!(!execution.applied);
    assert_eq!(execution.plan.method, "SetPlatformProfile");
    assert_eq!(execution.plan.requested_value, "performance");
}

#[test]
fn introspection_exposes_gated_platform_profile_write_only() {
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
            "GetRawProbeReport",
            "GetTelemetry",
            "PlanBatteryChargeTypeWrite",
            "PlanFanPresetWrite",
            "PlanGpuModeWrite",
            "PlanPlatformProfileWrite",
            "PlanRestoreAutoFanWrite",
            "RefreshCapabilities",
            "SetGpuModePending",
            "SetPlatformProfile",
        ]
    );
    assert!(!methods.iter().any(|method| matches!(
        method.as_str(),
        "SetBatteryChargeType" | "SetGpuMode" | "ApplyFanPreset" | "RestoreAutoFan"
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

    assert!(service.plan_platform_profile_write("custom").is_err());
    assert!(service.plan_battery_charge_type_write("Invalid").is_err());
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
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
    );

    let result = service.set_platform_profile("performance").unwrap();
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
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
    );

    assert!(service.set_platform_profile("custom").is_err());
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
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(FailingPlatformProfileWriter),
    );

    let result = service.set_platform_profile("performance").unwrap();
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
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(MismatchingPlatformProfileWriter),
    );

    let result = service.set_platform_profile("performance").unwrap();
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

fn unique_state_path(label: &str) -> std::path::PathBuf {
    std::path::PathBuf::from("/tmp").join(format!(
        "ratvantage-{label}-{}-{}.toml",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ))
}

struct AllowAllAuthorizer;

impl WriteAuthorizer for AllowAllAuthorizer {
    fn authorize(&self, _action: &str) -> std::result::Result<(), String> {
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
