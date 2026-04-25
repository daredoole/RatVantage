use legion_common::{Capability, CapabilityRegistry, HardwareSummary, TelemetrySnapshot};
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
            "RefreshCapabilities"
        ]
    );
    assert!(methods
        .iter()
        .all(|method| !method.starts_with("Set") && !method.starts_with("Write")));
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
