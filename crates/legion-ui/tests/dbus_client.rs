use std::process::Command;

use legion_common::{Capability, CapabilityStatus, HardwareSummary, RiskLevel};
use legion_control_daemon::{LegionControl, DBUS_INTERFACE, DBUS_PATH};
use legion_control_ui::{render_overview_lines, LegionControlClient, UiStatus};
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
            "gpu",
            "hwmon",
            "ideapad_toggles",
            "leds",
            "platform_profile"
        ]
    );
    assert!(capabilities.iter().all(|capability| {
        capability.risk == RiskLevel::ReadOnly
            && (capability.status == CapabilityStatus::ProbeOnly || capability.id == "gpu")
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
            "battery_capacity_percent=79",
            "battery_status=Charging",
            "battery_health=Good",
        ]
    );

    let refreshed = client.refresh_capabilities().unwrap();
    assert_eq!(refreshed, capabilities);
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
    assert_eq!(status.capability_count(), 8);
    assert_eq!(
        status.capability_ids(),
        [
            "battery_charge_type",
            "fan_curves",
            "firmware_attributes",
            "gpu",
            "hwmon",
            "ideapad_toggles",
            "leds",
            "platform_profile"
        ]
    );
    assert!(status.capabilities.iter().all(|capability| {
        !capability.label.is_empty()
            && capability.risk == RiskLevel::ReadOnly
            && (capability.status == CapabilityStatus::ProbeOnly || capability.id == "gpu")
    }));
    assert!(
        status
            .capabilities
            .iter()
            .any(|capability| capability.id == "gpu"
                && capability.status == CapabilityStatus::Missing)
    );
    assert_eq!(
        status.render_lines(),
        [
            "Legion Control status",
            "vendor=LENOVO",
            "product_name=82WM",
            "product_version=Legion Pro 5 16ARX8",
            "capability_count=8",
            "capabilities=battery_charge_type,fan_curves,firmware_attributes,gpu,hwmon,ideapad_toggles,leds,platform_profile",
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
            "capability_count=8\n",
            "capabilities=battery_charge_type,fan_curves,firmware_attributes,gpu,hwmon,ideapad_toggles,leds,platform_profile\n",
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
            "battery_capacity_percent=79\n",
            "battery_status=Charging\n",
            "battery_health=Good\n",
        )
    );
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
