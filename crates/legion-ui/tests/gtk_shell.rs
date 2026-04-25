#![cfg(feature = "gtk-ui")]

use adw::prelude::*;
use legion_common::{Capability, CapabilityStatus, HardwareSummary, RiskLevel};
use legion_control_ui::{gtk_shell, UiStatus};

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

    let page = gtk_shell::status_page(Err(anyhow::anyhow!("daemon unavailable")));
    let page = page
        .downcast::<gtk4::Box>()
        .expect("error page should be a vertical box");

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
