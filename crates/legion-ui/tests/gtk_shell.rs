#![cfg(feature = "gtk-ui")]

use adw::prelude::*;
use legion_common::{
    BatteryChargeTypeCapability, BatteryTelemetry, Capability, CapabilityRegistry,
    CapabilityStatus, HardwareSummary, PlatformProfileCapability, RiskLevel,
};
use legion_control_ui::{gtk_shell, DiagnosticsBundle, UiStatus};

#[test]
fn status_and_error_pages_build_under_headless_display() {
    std::env::set_var("GSK_RENDERER", "cairo");
    std::env::set_var("GTK_A11Y", "none");
    adw::init().expect("GTK/libadwaita must initialize under Xvfb");

    let page = gtk_shell::status_page(Ok(sample_status()));
    let page = page
        .downcast::<gtk4::Box>()
        .expect("status page should be a vertical box");

    assert_eq!(page.orientation(), gtk4::Orientation::Vertical);
    assert_eq!(page.spacing(), 12);
    assert_eq!(page.observe_children().n_items(), 3);

    let page = gtk_shell::diagnostics_page(Ok(sample_diagnostics()));
    let page = page
        .downcast::<gtk4::Box>()
        .expect("diagnostics page should be a vertical box");

    assert_eq!(page.orientation(), gtk4::Orientation::Vertical);
    assert_eq!(page.spacing(), 12);
    assert_eq!(page.observe_children().n_items(), 4);

    let page = gtk_shell::profiles_page(Ok(sample_diagnostics()));
    let page = page
        .downcast::<gtk4::Box>()
        .expect("profiles page should be a vertical box");

    assert_eq!(page.orientation(), gtk4::Orientation::Vertical);
    assert_eq!(page.spacing(), 12);
    assert_eq!(page.observe_children().n_items(), 2);

    let page = gtk_shell::battery_page(Ok(sample_diagnostics()));
    let page = page
        .downcast::<gtk4::Box>()
        .expect("battery page should be a vertical box");

    assert_eq!(page.orientation(), gtk4::Orientation::Vertical);
    assert_eq!(page.spacing(), 12);
    assert_eq!(page.observe_children().n_items(), 3);

    let page = gtk_shell::dashboard_page(Ok(sample_status()), Ok(sample_diagnostics()));
    let page = page
        .downcast::<gtk4::Box>()
        .expect("dashboard page should be a vertical box");

    assert_eq!(page.orientation(), gtk4::Orientation::Vertical);
    assert_eq!(page.observe_children().n_items(), 2);

    let page = gtk_shell::status_page(Err(anyhow::anyhow!("daemon unavailable")));
    let page = page
        .downcast::<gtk4::Box>()
        .expect("error page should be a vertical box");

    assert_eq!(page.orientation(), gtk4::Orientation::Vertical);
    assert_eq!(page.observe_children().n_items(), 2);

    let page = gtk_shell::diagnostics_page(Err(anyhow::anyhow!("daemon unavailable")));
    let page = page
        .downcast::<gtk4::Box>()
        .expect("diagnostics error page should be a vertical box");

    assert_eq!(page.orientation(), gtk4::Orientation::Vertical);
    assert_eq!(page.observe_children().n_items(), 2);

    let page = gtk_shell::profiles_page(Err(anyhow::anyhow!("daemon unavailable")));
    let page = page
        .downcast::<gtk4::Box>()
        .expect("profiles error page should be a vertical box");

    assert_eq!(page.orientation(), gtk4::Orientation::Vertical);
    assert_eq!(page.observe_children().n_items(), 2);

    let page = gtk_shell::battery_page(Err(anyhow::anyhow!("daemon unavailable")));
    let page = page
        .downcast::<gtk4::Box>()
        .expect("battery error page should be a vertical box");

    assert_eq!(page.orientation(), gtk4::Orientation::Vertical);
    assert_eq!(page.observe_children().n_items(), 2);
}

fn sample_status() -> UiStatus {
    UiStatus::from_parts(
        HardwareSummary {
            sysfs_root: "/tmp/fixture".to_owned(),
            vendor: Some("LENOVO".to_owned()),
            product_name: Some("82WM".to_owned()),
            product_version: Some("Legion Pro 5 16ARX8".to_owned()),
            product_sku: None,
        },
        vec![
            Capability {
                id: "platform_profile".to_owned(),
                label: "Platform profile".to_owned(),
                status: CapabilityStatus::ProbeOnly,
                risk: RiskLevel::ReadOnly,
                evidence: vec![],
                details: serde_json::Value::Null,
            },
            Capability {
                id: "fan_curves".to_owned(),
                label: "Fan curves".to_owned(),
                status: CapabilityStatus::ProbeOnly,
                risk: RiskLevel::ReadOnly,
                evidence: vec![],
                details: serde_json::Value::Null,
            },
        ],
    )
    .unwrap()
}

fn sample_diagnostics() -> DiagnosticsBundle {
    let mut report = CapabilityRegistry {
        hardware: HardwareSummary {
            sysfs_root: "/tmp/fixture".to_owned(),
            vendor: Some("LENOVO".to_owned()),
            product_name: Some("82WM".to_owned()),
            product_version: Some("Legion Pro 5 16ARX8".to_owned()),
            product_sku: None,
        },
        ..Default::default()
    };
    report.capabilities = vec![Capability {
        id: "platform_profile".to_owned(),
        label: "Platform profile".to_owned(),
        status: CapabilityStatus::ProbeOnly,
        risk: RiskLevel::ReadOnly,
        evidence: vec![],
        details: serde_json::Value::Null,
    }];
    report.platform_profile = Some(PlatformProfileCapability {
        current: Some("balanced".to_owned()),
        choices: vec![
            "low-power".to_owned(),
            "balanced".to_owned(),
            "performance".to_owned(),
        ],
        path: "/tmp/fixture/sys/firmware/acpi/platform_profile".to_owned(),
        choices_path: "/tmp/fixture/sys/firmware/acpi/platform_profile_choices".to_owned(),
    });
    report.battery_charge_type = Some(BatteryChargeTypeCapability {
        current: Some("Standard".to_owned()),
        choices: vec![
            "Fast".to_owned(),
            "Standard".to_owned(),
            "Conservation".to_owned(),
        ],
        path: "/tmp/fixture/sys/class/power_supply/BAT0/charge_control_end_threshold".to_owned(),
        choices_path: "/tmp/fixture/sys/class/power_supply/BAT0/charge_types".to_owned(),
    });
    report.telemetry.battery = Some(BatteryTelemetry {
        name: "BAT0".to_owned(),
        path: "/tmp/fixture/sys/class/power_supply/BAT0".to_owned(),
        capacity_percent: Some(79),
        status: Some("Charging".to_owned()),
        health: Some("Good".to_owned()),
    });

    DiagnosticsBundle::from_report_with_logs(
        report,
        Some("6.17.0-test".to_owned()),
        vec!["2026-04-25T17:44:00 legion-control-daemon started".to_owned()],
    )
}
