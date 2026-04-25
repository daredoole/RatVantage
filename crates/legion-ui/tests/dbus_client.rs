use std::process::Command;

use legion_common::{CapabilityStatus, RiskLevel};
use legion_control_daemon::{LegionControl, DBUS_INTERFACE, DBUS_PATH};
use legion_control_ui::LegionControlClient;
use legion_probe::ProbeOptions;
use ratvantage_test_support::{fixture_root, PrivateBus};
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
            "battery_charge_type",
            "fan_curves",
            "firmware_attributes",
            "hwmon",
            "ideapad_toggles",
            "leds",
            "platform_profile"
        ]
    );
    assert!(capabilities.iter().all(|capability| {
        capability.risk == RiskLevel::ReadOnly
            && capability.status == CapabilityStatus::ProbeOnly
            && capability.details.is_null()
    }));

    let telemetry = client.telemetry().unwrap();
    assert!(telemetry
        .sensors
        .iter()
        .any(|sensor| sensor.label.as_deref() == Some("CPU Fan")));

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

    let refreshed = client.refresh_capabilities().unwrap();
    assert_eq!(refreshed, capabilities);
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
            "capability_count=7\n",
            "capabilities=battery_charge_type,fan_curves,firmware_attributes,hwmon,ideapad_toggles,leds,platform_profile\n",
        )
    );
}

fn fixture_service() -> (PrivateBus, zbus::blocking::Connection, String) {
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
    let address = bus.address().to_owned();

    (bus, service_connection, address)
}
