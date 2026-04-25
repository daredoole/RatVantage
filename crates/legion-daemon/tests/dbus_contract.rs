use legion_common::{
    Capability, CapabilityRegistry, HardwareSummary, TelemetrySnapshot, WriteDryRunPlan,
};
use legion_control_daemon::{LegionControl, DBUS_INTERFACE, DBUS_PATH};
use legion_probe::ProbeOptions;
use ratvantage_test_support::{call_json, fixture_root, introspected_methods, PrivateBus};
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
}

#[test]
fn introspection_exposes_only_read_only_legion_methods() {
    let (_bus, _service_connection, proxy) = test_proxy();
    let xml = proxy.introspect().unwrap();
    let mut methods = introspected_methods(&xml, DBUS_INTERFACE);
    methods.sort_unstable();

    assert_eq!(
        methods,
        [
            "GetCapabilities",
            "GetHardwareSummary",
            "GetRawProbeReport",
            "GetTelemetry",
            "PlanBatteryChargeTypeWrite",
            "PlanFanPresetWrite",
            "PlanGpuModeWrite",
            "PlanPlatformProfileWrite",
            "PlanRestoreAutoFanWrite",
            "RefreshCapabilities"
        ]
    );
    assert!(methods
        .iter()
        .all(|method| !method.starts_with("Set") && !method.starts_with("Write")));
}

#[test]
fn daemon_builds_dry_run_plans_without_dbus_write_methods() {
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
fn daemon_builds_fan_preset_plan_from_runtime_fixture() {
    let service = LegionControl::new(ProbeOptions {
        sysfs_root: runtime_fixture_root(),
    });

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
}

fn test_proxy() -> (PrivateBus, zbus::blocking::Connection, Proxy<'static>) {
    let bus = PrivateBus::start();
    let service = LegionControl::new(ProbeOptions {
        sysfs_root: fixture_root(),
    });
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
