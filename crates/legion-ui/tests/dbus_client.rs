use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use legion_control_daemon::{LegionControl, DBUS_INTERFACE, DBUS_PATH};
use legion_control_ui::LegionControlClient;
use legion_probe::ProbeOptions;
use zbus::blocking::ConnectionBuilder;

#[test]
fn client_reads_daemon_contract_over_private_bus() {
    let bus = TestBus::start();
    let service = LegionControl::new(ProbeOptions {
        sysfs_root: fixture_root(),
    });
    let _service_connection = ConnectionBuilder::address(bus.address.as_str())
        .unwrap()
        .name(DBUS_INTERFACE)
        .unwrap()
        .serve_at(DBUS_PATH, service)
        .unwrap()
        .build()
        .unwrap();
    let client = LegionControlClient::address(&bus.address).unwrap();

    let hardware = client.hardware_summary().unwrap();
    assert_eq!(hardware.vendor.as_deref(), Some("LENOVO"));
    assert_eq!(hardware.product_name.as_deref(), Some("82WM"));

    let capabilities = client.capabilities().unwrap();
    assert!(capabilities
        .iter()
        .any(|capability| capability.id == "platform_profile"));
    assert!(capabilities
        .iter()
        .any(|capability| capability.id == "ideapad_toggles"));

    let telemetry = client.telemetry().unwrap();
    assert!(telemetry
        .sensors
        .iter()
        .any(|sensor| sensor.label.as_deref() == Some("CPU Fan")));

    let raw = client.raw_probe_report().unwrap();
    assert_eq!(raw.hardware, hardware);
    assert_eq!(raw.telemetry, telemetry);
    assert!(raw.leds.iter().any(|led| led.name == "platform::ylogo"));

    let refreshed = client.refresh_capabilities().unwrap();
    assert_eq!(refreshed, capabilities);
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
