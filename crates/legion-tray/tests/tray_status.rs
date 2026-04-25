use std::process::Command;

use legion_control_daemon::{LegionControl, DBUS_INTERFACE, DBUS_PATH};
use legion_probe::ProbeOptions;
use ratvantage_test_support::{fixture_root, PrivateBus};
use zbus::blocking::ConnectionBuilder;

#[test]
fn status_cli_prints_tray_summary_over_private_bus() {
    let (_bus, _service_connection, address) = fixture_service();
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-tray"))
        .args(["--status", "--bus-address", &address])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Legion Control tray status"));
    assert!(stdout.contains("tooltip=82WM Legion Pro 5 16ARX8: 8 read-only capabilities"));
    assert!(stdout.contains(
        "capabilities=battery_charge_type,fan_curves,firmware_attributes,gpu,hwmon,ideapad_toggles,leds,platform_profile"
    ));
}

#[test]
fn tooltip_cli_prints_single_line_over_private_bus() {
    let (_bus, _service_connection, address) = fixture_service();
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-tray"))
        .args(["--tooltip", "--bus-address", &address])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "82WM Legion Pro 5 16ARX8: 8 read-only capabilities\n"
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
