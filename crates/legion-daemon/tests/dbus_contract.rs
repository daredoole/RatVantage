use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use legion_common::{Capability, CapabilityRegistry, HardwareSummary, TelemetrySnapshot};
use legion_control_daemon::{LegionControl, DBUS_INTERFACE, DBUS_PATH};
use legion_probe::ProbeOptions;
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

fn test_proxy() -> (TestBus, zbus::blocking::Connection, Proxy<'static>) {
    let bus = TestBus::start();
    let service = LegionControl::new(ProbeOptions {
        sysfs_root: fixture_root(),
    });
    let service_connection = ConnectionBuilder::address(bus.address.as_str())
        .unwrap()
        .name(DBUS_INTERFACE)
        .unwrap()
        .serve_at(DBUS_PATH, service)
        .unwrap()
        .build()
        .unwrap();
    let client_connection = ConnectionBuilder::address(bus.address.as_str())
        .unwrap()
        .build()
        .unwrap();
    let proxy =
        Proxy::new_owned(client_connection, DBUS_INTERFACE, DBUS_PATH, DBUS_INTERFACE).unwrap();

    (bus, service_connection, proxy)
}

fn introspected_methods(xml: &str, interface: &str) -> Vec<String> {
    let Some(interface_start) = xml.find(&format!("<interface name=\"{interface}\">")) else {
        return Vec::new();
    };
    let interface_xml = &xml[interface_start..];
    let Some(interface_end) = interface_xml.find("</interface>") else {
        return Vec::new();
    };

    interface_xml[..interface_end]
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("<method name=\"")?
                .split_once('"')
                .map(|(name, _)| name.to_owned())
        })
        .collect()
}

fn call_json<T>(proxy: &Proxy<'_>, method: &str) -> T
where
    T: serde::de::DeserializeOwned,
{
    let payload: String = proxy.call(method, &()).unwrap();
    serde_json::from_str(&payload).unwrap()
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("tests/fixtures/sysfs-82wm-confirmed")
}

struct TestBus {
    address: String,
    child: Child,
}

impl TestBus {
    fn start() -> Self {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--print-address=1", "--nofork"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("dbus-daemon must be available for D-Bus integration tests");
        let stdout = child.stdout.take().unwrap();
        let mut lines = BufReader::new(stdout).lines();
        let address = lines.next().unwrap().unwrap();

        Self { address, child }
    }
}

impl Drop for TestBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
