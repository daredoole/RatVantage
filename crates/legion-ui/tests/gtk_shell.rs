#![cfg(feature = "gtk-ui")]

use adw::prelude::*;
use std::{
    collections::BTreeMap,
    sync::Once,
    time::{Duration, Instant},
};

use legion_common::{
    BatteryChargeTypeCapability, BatteryTelemetry, Capability, CapabilityRegistry,
    CapabilityStatus, FanCurveCapability, FanCurvePointSnapshot, FanCurveSnapshot, GpuCapability,
    GpuModePending, HardwareSummary, HwmonSensor, IdeapadToggleCapability, LedCapability,
    PlatformProfileCapability, RiskLevel, WriteDryRunPlan, WriteExecutionResult,
    WriteExecutionStatus,
};
use legion_control_ui::{gtk_shell, DiagnosticsBundle, UiStatus};

static GTK_INIT: Once = Once::new();

#[test]
fn runtime_refresh_policy_triggers_for_periodic_and_resume_gaps() {
    let now = Instant::now();
    assert!(!gtk_shell::should_auto_refresh(
        now,
        now - Duration::from_secs(10),
        now - Duration::from_secs(10),
    ));
    assert!(gtk_shell::should_auto_refresh(
        now,
        now - Duration::from_secs(31),
        now - Duration::from_secs(10),
    ));
    assert!(gtk_shell::should_auto_refresh(
        now,
        now - Duration::from_secs(5),
        now - Duration::from_secs(91),
    ));
}

#[gtk4::test]
fn status_and_error_pages_build_under_headless_display() {
    init_gtk();

    let page = gtk_shell::status_page(Ok(sample_status()), Ok(None));
    let page = page
        .downcast::<gtk4::Box>()
        .expect("status page should be a vertical box");

    assert_eq!(page.orientation(), gtk4::Orientation::Vertical);
    assert_eq!(page.spacing(), 12);
    assert_eq!(page.observe_children().n_items(), 3);

    let page = gtk_shell::status_page(Ok(sample_status()), Ok(Some(sample_gpu_pending())));
    let page = page
        .downcast::<gtk4::Box>()
        .expect("status page with runtime state should be a vertical box");

    assert_eq!(page.orientation(), gtk4::Orientation::Vertical);
    assert_eq!(page.spacing(), 12);
    assert_eq!(page.observe_children().n_items(), 3);

    let page = gtk_shell::fans_page(Ok(sample_diagnostics()), Ok(None));
    let page = page
        .downcast::<gtk4::Box>()
        .expect("fans page should be a vertical box");

    assert_eq!(page.orientation(), gtk4::Orientation::Vertical);
    assert_eq!(page.spacing(), 12);
    assert_eq!(page.observe_children().n_items(), 36);

    let page = gtk_shell::fans_page(Ok(sample_diagnostics()), Ok(Some(sample_fan_snapshot())));
    let page = page
        .downcast::<gtk4::Box>()
        .expect("fans page with runtime state should be a vertical box");

    assert_eq!(page.orientation(), gtk4::Orientation::Vertical);
    assert_eq!(page.spacing(), 12);
    assert_eq!(page.observe_children().n_items(), 37);

    let page = gtk_shell::appearance_page(Ok(sample_diagnostics()));
    let page = page
        .downcast::<gtk4::Box>()
        .expect("appearance page should be a vertical box");

    assert_eq!(page.orientation(), gtk4::Orientation::Vertical);
    assert_eq!(page.spacing(), 12);
    assert_eq!(page.observe_children().n_items(), 11);

    let page = gtk_shell::diagnostics_page(Ok(sample_diagnostics()));
    let page = page
        .downcast::<gtk4::Box>()
        .expect("diagnostics page should be a vertical box");

    assert_eq!(page.orientation(), gtk4::Orientation::Vertical);
    assert_eq!(page.spacing(), 12);
    assert_eq!(page.observe_children().n_items(), 4);
    let scroller = page
        .last_child()
        .expect("diagnostics page should end with a scroller")
        .downcast::<gtk4::ScrolledWindow>()
        .expect("last child should be a scrolled window");
    let text = scroller
        .child()
        .expect("scroller should wrap the diagnostics text")
        .downcast::<gtk4::TextView>()
        .expect("scroller child should be a text view");
    let buffer = text.buffer();
    let json = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
    assert!(json.contains("\"gpu_mode_pending\""));
    assert!(json.contains("\"requested_mode\": \"hybrid\""));
    assert!(json.contains("\"last_known_good_fan_curve\""));
    assert!(json.contains("\"curve_id\": \"legion_hwmon\""));

    let page = gtk_shell::profiles_page(Ok(sample_diagnostics()));
    let page = page
        .downcast::<gtk4::Box>()
        .expect("profiles page should be a vertical box");

    assert_eq!(page.orientation(), gtk4::Orientation::Vertical);
    assert_eq!(page.spacing(), 12);
    assert_eq!(page.observe_children().n_items(), 4);

    let page = gtk_shell::battery_page(Ok(sample_diagnostics()));
    let page = page
        .downcast::<gtk4::Box>()
        .expect("battery page should be a vertical box");

    assert_eq!(page.orientation(), gtk4::Orientation::Vertical);
    assert_eq!(page.spacing(), 12);
    assert_eq!(page.observe_children().n_items(), 5);

    let page = gtk_shell::gpu_page(Ok(sample_diagnostics()), Ok(Some(sample_gpu_pending())));
    let page = page
        .downcast::<gtk4::Box>()
        .expect("gpu page should be a vertical box");

    assert_eq!(page.orientation(), gtk4::Orientation::Vertical);
    assert_eq!(page.spacing(), 12);
    assert_eq!(page.observe_children().n_items(), 3);

    let page = gtk_shell::dashboard_page(
        Ok(sample_status()),
        Ok(sample_diagnostics()),
        Ok(None),
        Ok(None),
        None,
    );
    let page = page
        .downcast::<gtk4::Box>()
        .expect("dashboard page should be a vertical box");

    assert_eq!(page.orientation(), gtk4::Orientation::Vertical);
    assert_eq!(page.observe_children().n_items(), 2);

    let stack = page
        .last_child()
        .expect("dashboard should contain a stack")
        .downcast::<gtk4::Stack>()
        .expect("dashboard content should be a stack");
    let visible_child = stack
        .visible_child()
        .expect("dashboard stack should have a visible child");
    visible_child
        .downcast::<gtk4::ScrolledWindow>()
        .expect("dashboard stack pages should be scrollable");

    let page = gtk_shell::status_page(Err(anyhow::anyhow!("daemon unavailable")), Ok(None));
    let page = page
        .downcast::<gtk4::Box>()
        .expect("error page should be a vertical box");

    assert_eq!(page.orientation(), gtk4::Orientation::Vertical);
    assert_eq!(page.observe_children().n_items(), 2);

    let page = gtk_shell::fans_page(Err(anyhow::anyhow!("daemon unavailable")), Ok(None));
    let page = page
        .downcast::<gtk4::Box>()
        .expect("fans error page should be a vertical box");

    assert_eq!(page.orientation(), gtk4::Orientation::Vertical);
    assert_eq!(page.observe_children().n_items(), 2);

    let page = gtk_shell::appearance_page(Err(anyhow::anyhow!("daemon unavailable")));
    let page = page
        .downcast::<gtk4::Box>()
        .expect("appearance error page should be a vertical box");

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

    let page = gtk_shell::gpu_page(Err(anyhow::anyhow!("daemon unavailable")), Ok(None));
    let page = page
        .downcast::<gtk4::Box>()
        .expect("gpu error page should be a vertical box");

    assert_eq!(page.orientation(), gtk4::Orientation::Vertical);
    assert_eq!(page.observe_children().n_items(), 2);
}

#[test]
fn dashboard_page_name_normalization_accepts_known_pages_only() {
    assert_eq!(
        gtk_shell::normalize_dashboard_page_name(Some("battery")),
        "battery"
    );
    assert_eq!(gtk_shell::normalize_dashboard_page_name(Some("gpu")), "gpu");
    assert_eq!(
        gtk_shell::normalize_dashboard_page_name(Some("fans")),
        "fans"
    );
    assert_eq!(
        gtk_shell::normalize_dashboard_page_name(Some("not-a-page")),
        "status"
    );
    assert_eq!(gtk_shell::normalize_dashboard_page_name(None), "status");
}

#[gtk4::test]
fn dashboard_pages_render_quick_apply_and_gpu_controls() {
    init_gtk();

    let profiles = gtk_shell::profiles_page(Ok(sample_diagnostics()))
        .downcast::<gtk4::Box>()
        .expect("profiles page should be a vertical box");
    let battery = gtk_shell::battery_page(Ok(sample_diagnostics()))
        .downcast::<gtk4::Box>()
        .expect("battery page should be a vertical box");
    let gpu = gtk_shell::gpu_page(Ok(sample_diagnostics()), Ok(Some(sample_gpu_pending())))
        .downcast::<gtk4::Box>()
        .expect("gpu page should be a vertical box");

    let profile_text = collect_widget_text(&profiles.clone().upcast());
    assert!(profile_text
        .iter()
        .any(|text| text == "Platform profile quick apply"));
    assert!(profile_text.iter().any(|text| text == "Requested profile"));
    assert!(profile_text.iter().any(|text| text == "Apply profile"));
    assert!(profile_text.iter().any(|text| text == "Apply result"));
    assert!(profile_text
        .iter()
        .any(|text| text == "No write attempted yet."));

    let battery_text = collect_widget_text(&battery.clone().upcast());
    assert!(battery_text
        .iter()
        .any(|text| text == "Battery charge type quick apply"));
    assert!(battery_text
        .iter()
        .any(|text| text == "Requested charge type"));
    assert!(battery_text.iter().any(|text| text == "Apply charge type"));
    assert!(battery_text.iter().any(|text| text == "Apply result"));
    assert!(battery_text
        .iter()
        .any(|text| text == "No write attempted yet."));

    let fans = gtk_shell::fans_page(Ok(sample_diagnostics()), Ok(Some(sample_fan_snapshot())))
        .downcast::<gtk4::Box>()
        .expect("fans page should be a vertical box");
    let fans_text = collect_widget_text(&fans.clone().upcast());
    assert!(fans_text.iter().any(|text| text == "Guided fan planning"));
    assert!(fans_text.iter().any(|text| text == "Packaged preset"));
    assert!(fans_text.iter().any(|text| text == "Preview plan"));
    assert!(fans_text.iter().any(|text| text == "Preview restore plan"));
    assert!(fans_text.iter().any(|text| text == "Capture snapshot"));
    assert!(fans_text.iter().any(|text| text == "Preset plan preview"));
    assert!(fans_text.iter().any(|text| text == "Restore plan preview"));
    assert!(fans_text.iter().any(|text| text == "Live curve readings"));
    assert!(fans_text
        .iter()
        .any(|text| text == "Live sysfs points (read-only)"));
    assert!(fans_text.iter().any(|text| text == "Refresh live readings"));
    assert!(fans_text
        .iter()
        .any(|text| text == "Saved last-known-good detail"));
    assert!(fans_text
        .iter()
        .any(|text| text == "Saved curve points (read-only)"));
    assert!(fans_text
        .iter()
        .any(|text| text == "Refresh saved snapshot"));
    assert!(fans_text
        .iter()
        .any(|text| text == "Manual curve scratchpad"));
    assert!(fans_text.iter().any(|text| {
        text.contains("Click or drag")
            && text.contains("arrow keys")
            && text.contains("Shift")
            && text.contains("Focusing a row's temp or pwm field syncs the chart highlight")
            && text.contains("PWM 0–255 vertically")
    }));
    assert!(fans_text.iter().any(|text| text == "Load from live"));
    assert!(fans_text.iter().any(|text| text == "Validate pairs"));
    assert!(fans_text.iter().any(|text| text == "Preview sysfs targets"));
    assert!(fans_text
        .iter()
        .any(|text| text == "Sysfs target preview (scratchpad)"));
    assert!(fans_text.iter().any(|text| text == "Copy JSON"));
    assert!(fans_text.iter().any(|text| text == "Copy scratchpad TOML"));
    assert!(fans_text.iter().any(|text| text == "TOML exchange"));
    assert!(fans_text
        .iter()
        .any(|text| text == "Import TOML from editor"));
    assert!(fans_text
        .iter()
        .any(|text| text == "Live vs saved comparison"));
    assert!(fans_text.iter().any(|text| text == "Compare live to saved"));
    assert!(fans_text
        .iter()
        .any(|text| text == "Curve shape (read-only preview)"));
    assert!(fans_text
        .iter()
        .any(|text| { text.contains("Temperature vs PWM") && text.contains("read-only chart") }));
    assert!(fans_text
        .iter()
        .any(|text| text == "Fan preset per platform profile"));
    assert!(fans_text
        .iter()
        .any(|text| text == "Clear all profile mappings"));
    assert!(fans_text
        .iter()
        .any(|text| text == "Re-apply mapped fan preset after resume"));

    let gpu_text = collect_widget_text(&gpu.clone().upcast());
    assert!(gpu_text.iter().any(|text| text == "GPU Mode"));
    assert!(gpu_text
        .iter()
        .any(|text| text == "Guided GPU switch planning"));
    assert!(gpu_text.iter().any(|text| text == "Current mode"));
    assert!(gpu_text.iter().any(|text| text == "Pending reboot"));
    assert!(gpu_text.iter().any(|text| text == "Target mode"));
    assert!(gpu_text.iter().any(|text| text == "Preview plan"));
    assert!(gpu_text.iter().any(|text| text == "Record pending"));
    assert!(gpu_text.iter().any(|text| text == "Clear pending"));
    assert!(gpu_text.iter().any(|text| text == "Plan preview"));
    assert!(gpu_text.iter().any(|text| text == "Recovery guidance"));

    let appearance = gtk_shell::appearance_page(Ok(sample_diagnostics()))
        .downcast::<gtk4::Box>()
        .expect("appearance page should be a vertical box");
    let appearance_text = collect_widget_text(&appearance.clone().upcast());
    assert!(appearance_text.iter().any(|text| text == "LED quick apply"));
    assert!(appearance_text.iter().any(|text| text == "Y-logo LED"));
    assert!(appearance_text.iter().any(|text| text == "Turn off"));
    assert!(appearance_text.iter().any(|text| text == "Turn on"));
    assert!(appearance_text
        .iter()
        .any(|text| text == "Fn-lock quick apply"));
    assert!(appearance_text
        .iter()
        .any(|text| text == "Functional Fn-lock"));
    assert!(appearance_text
        .iter()
        .any(|text| text == "Camera privacy quick apply"));
    assert!(appearance_text.iter().any(|text| text == "Camera power"));
    assert!(appearance_text
        .iter()
        .any(|text| text == "Confirmation required"));
    assert!(appearance_text.iter().any(|text| text == "Request off"));
    assert!(appearance_text.iter().any(|text| text == "Request on"));
    assert!(appearance_text.iter().any(|text| text == "Confirm"));
    assert!(appearance_text.iter().any(|text| text == "Cancel"));
    assert!(appearance_text
        .iter()
        .any(|text| text == "USB charging quick apply"));
    assert!(appearance_text.iter().any(|text| text == "USB charging"));
}

#[gtk4::test]
fn dashboard_pages_disable_quick_apply_when_capabilities_are_unavailable() {
    init_gtk();

    let mut diagnostics = sample_diagnostics();
    diagnostics.raw_probe_report.platform_profile = None;
    diagnostics.raw_probe_report.battery_charge_type = None;
    diagnostics.raw_probe_report.gpu = None;
    diagnostics.raw_probe_report.fan_curves.clear();
    diagnostics.raw_probe_report.leds.clear();
    diagnostics.raw_probe_report.ideapad_toggles.clear();

    let profiles = gtk_shell::profiles_page(Ok(diagnostics.clone()))
        .downcast::<gtk4::Box>()
        .expect("profiles page should be a vertical box");
    let battery = gtk_shell::battery_page(Ok(diagnostics.clone()))
        .downcast::<gtk4::Box>()
        .expect("battery page should be a vertical box");
    let gpu = gtk_shell::gpu_page(Ok(diagnostics.clone()), Ok(None))
        .downcast::<gtk4::Box>()
        .expect("gpu page should be a vertical box");

    let profile_text = collect_widget_text(&profiles.clone().upcast());
    assert!(profile_text
        .iter()
        .any(|text| text == "Platform profile quick apply"));
    assert!(profile_text
        .iter()
        .any(|text| text == "unavailable - quick apply disabled"));
    assert!(!profile_text.iter().any(|text| text == "Apply profile"));

    let battery_text = collect_widget_text(&battery.clone().upcast());
    assert!(battery_text
        .iter()
        .any(|text| text == "Battery charge type quick apply"));
    assert!(battery_text
        .iter()
        .any(|text| text == "unavailable - quick apply disabled"));
    assert!(!battery_text.iter().any(|text| text == "Apply charge type"));

    let gpu_text = collect_widget_text(&gpu.clone().upcast());
    assert!(gpu_text
        .iter()
        .any(|text| text == "Guided GPU switch planning"));
    assert!(gpu_text
        .iter()
        .any(|text| text == "unavailable - envycontrol was not detected"));
    assert!(!gpu_text.iter().any(|text| text == "Preview plan"));

    let fans = gtk_shell::fans_page(Ok(diagnostics.clone()), Ok(None))
        .downcast::<gtk4::Box>()
        .expect("fans page should be a vertical box");
    let fans_text = collect_widget_text(&fans.clone().upcast());
    assert!(fans_text.iter().any(|text| text == "Fan preset planning"));
    assert!(fans_text
        .iter()
        .any(|text| { text.contains("no fan curve capability was detected") }));
    assert!(!fans_text.iter().any(|text| text == "Preview restore plan"));

    let appearance = gtk_shell::appearance_page(Ok(diagnostics))
        .downcast::<gtk4::Box>()
        .expect("appearance page should be a vertical box");
    let appearance_text = collect_widget_text(&appearance.clone().upcast());
    assert!(appearance_text.iter().any(|text| text == "LED quick apply"));
    assert!(appearance_text
        .iter()
        .any(|text| text == "Fn-lock quick apply"));
    assert!(appearance_text
        .iter()
        .any(|text| text == "Camera privacy quick apply"));
    assert!(appearance_text
        .iter()
        .any(|text| text == "unavailable - quick apply disabled"));
    assert!(!appearance_text.iter().any(|text| text == "Turn on"));
    assert!(!appearance_text.iter().any(|text| text == "Request off"));
}

#[test]
fn write_feedback_helpers_render_idle_success_blocked_and_failure_states() {
    let plan = WriteDryRunPlan {
        method: "SetPlatformProfile".to_owned(),
        capability_id: "platform_profile".to_owned(),
        polkit_action: "org.ratvantage.LegionControl1.set-platform-profile".to_owned(),
        path: "/tmp/platform_profile".to_owned(),
        previous_value: "balanced".to_owned(),
        requested_value: "performance".to_owned(),
        readback_required: true,
        rollback_value: "balanced".to_owned(),
        rollback_instructions: Vec::new(),
        reboot_required: false,
        safety_notes: Vec::new(),
        steps: Vec::new(),
    };
    let applied = WriteExecutionResult::applied(
        plan.clone(),
        "platform profile write applied and read back successfully",
        Some("performance".to_owned()),
    );
    let blocked =
        WriteExecutionResult::blocked_by_authorization(plan.clone(), "polkit authorization failed");
    let failed = WriteExecutionResult {
        status: WriteExecutionStatus::Failed,
        applied: false,
        message: "platform profile read-back mismatch after write".to_owned(),
        readback_value: Some("balanced".to_owned()),
        plan,
    };

    assert_eq!(gtk_shell::write_feedback_title(None), "Apply result");
    assert_eq!(
        gtk_shell::write_feedback_subtitle(None),
        "No write attempted yet."
    );
    assert_eq!(
        gtk_shell::write_feedback_title(Some(&applied)),
        "Apply succeeded"
    );
    assert!(gtk_shell::write_feedback_subtitle(Some(&applied)).contains("read back successfully"));
    assert_eq!(
        gtk_shell::write_feedback_title(Some(&blocked)),
        "Apply blocked by authorization"
    );
    assert!(
        gtk_shell::write_feedback_subtitle(Some(&blocked)).contains("polkit authorization failed")
    );
    assert_eq!(
        gtk_shell::write_feedback_title(Some(&failed)),
        "Apply failed"
    );
    assert!(gtk_shell::write_feedback_subtitle(Some(&failed)).contains("Read-back: balanced."));
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
    report.capabilities = vec![
        Capability {
            id: "gpu".to_owned(),
            label: "GPU mode".to_owned(),
            status: CapabilityStatus::ProbeOnly,
            risk: RiskLevel::ReadOnly,
            evidence: vec![],
            details: serde_json::Value::Null,
        },
        Capability {
            id: "platform_profile".to_owned(),
            label: "Platform profile".to_owned(),
            status: CapabilityStatus::ProbeOnly,
            risk: RiskLevel::ReadOnly,
            evidence: vec![],
            details: serde_json::Value::Null,
        },
    ];
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
    report.gpu = Some(GpuCapability {
        provider: "envycontrol".to_owned(),
        status: CapabilityStatus::ProbeOnly,
        mode: Some("hybrid".to_owned()),
    });
    report.telemetry.battery = Some(BatteryTelemetry {
        name: "BAT0".to_owned(),
        path: "/tmp/fixture/sys/class/power_supply/BAT0".to_owned(),
        capacity_percent: Some(79),
        status: Some("Charging".to_owned()),
        health: Some("Good".to_owned()),
    });
    report.telemetry.sensors = vec![HwmonSensor {
        hwmon_name: Some("legion".to_owned()),
        label: Some("CPU Fan".to_owned()),
        kind: "fan".to_owned(),
        input_path: "/tmp/fixture/sys/class/hwmon/hwmon7/fan1_input".to_owned(),
        value: Some(2840),
    }];
    report.fan_curves = vec![FanCurveCapability {
        id: "legion-hwmon".to_owned(),
        status: CapabilityStatus::ProbeOnly,
        path: Some("/tmp/fixture/sys/class/hwmon/hwmon7".to_owned()),
        point_paths: vec![
            "/tmp/fixture/sys/class/hwmon/hwmon7/pwm1_auto_point1_temp".to_owned(),
            "/tmp/fixture/sys/class/hwmon/hwmon7/pwm1_auto_point1_pwm".to_owned(),
        ],
    }];
    report.leds = vec![
        LedCapability {
            name: "platform::fnlock".to_owned(),
            path: "/tmp/fixture/sys/class/leds/platform::fnlock/brightness".to_owned(),
            brightness: Some(0),
            max_brightness: Some(1),
        },
        LedCapability {
            name: "platform::ylogo".to_owned(),
            path: "/tmp/fixture/sys/class/leds/platform::ylogo/brightness".to_owned(),
            brightness: Some(0),
            max_brightness: Some(1),
        },
    ];
    report.ideapad_toggles = vec![
        IdeapadToggleCapability {
            name: "fn_lock".to_owned(),
            status: CapabilityStatus::ProbeOnly,
            path: Some(
                "/tmp/fixture/sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/fn_lock".to_owned(),
            ),
            current_value: Some("0".to_owned()),
        },
        IdeapadToggleCapability {
            name: "camera_power".to_owned(),
            status: CapabilityStatus::ProbeOnly,
            path: Some(
                "/tmp/fixture/sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/camera_power"
                    .to_owned(),
            ),
            current_value: Some("1".to_owned()),
        },
        IdeapadToggleCapability {
            name: "usb_charging".to_owned(),
            status: CapabilityStatus::ProbeOnly,
            path: Some(
                "/tmp/fixture/sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/usb_charging"
                    .to_owned(),
            ),
            current_value: Some("0".to_owned()),
        },
    ];

    DiagnosticsBundle::from_report_with_logs(
        report,
        Some("6.17.0-test".to_owned()),
        vec!["2026-04-25T17:44:00 legion-control-daemon started".to_owned()],
    )
    .with_runtime_state(
        Some(sample_gpu_pending()),
        Some(sample_fan_snapshot()),
        BTreeMap::new(),
        false,
    )
}

fn sample_gpu_pending() -> GpuModePending {
    GpuModePending {
        requested_mode: "hybrid".to_owned(),
        previous_mode: Some("nvidia".to_owned()),
        reboot_required: true,
    }
}

fn sample_fan_snapshot() -> FanCurveSnapshot {
    FanCurveSnapshot {
        curve_id: "legion_hwmon".to_owned(),
        path: Some("/tmp/fixture/sys/class/hwmon/hwmon7".to_owned()),
        points: vec![
            FanCurvePointSnapshot {
                path: "/tmp/fixture/sys/class/hwmon/hwmon7/pwm1_auto_point1_temp".to_owned(),
                value: "42000".to_owned(),
            },
            FanCurvePointSnapshot {
                path: "/tmp/fixture/sys/class/hwmon/hwmon7/pwm1_auto_point1_pwm".to_owned(),
                value: "88".to_owned(),
            },
        ],
    }
}

fn init_gtk() {
    GTK_INIT.call_once(|| {
        std::env::set_var("GSK_RENDERER", "cairo");
        std::env::set_var("GTK_A11Y", "none");
        adw::init().expect("GTK/libadwaita must initialize under Xvfb");
    });
}

fn collect_widget_text(root: &gtk4::Widget) -> Vec<String> {
    let mut text = Vec::new();
    collect_widget_text_recursive(root, &mut text);
    text
}

fn collect_widget_text_recursive(widget: &gtk4::Widget, text: &mut Vec<String>) {
    if let Ok(label) = widget.clone().downcast::<gtk4::Label>() {
        let value = label.label().to_string();
        if !value.is_empty() {
            text.push(value);
        }
    }
    if let Ok(button) = widget.clone().downcast::<gtk4::Button>() {
        if let Some(label) = button.label() {
            text.push(label.to_string());
        }
    }
    if let Ok(row) = widget.clone().downcast::<adw::ActionRow>() {
        let title = row.title().to_string();
        if !title.is_empty() {
            text.push(title);
        }
        if let Some(subtitle) = row.subtitle() {
            let subtitle = subtitle.to_string();
            if !subtitle.is_empty() {
                text.push(subtitle);
            }
        }
    }
    if let Ok(group) = widget.clone().downcast::<adw::PreferencesGroup>() {
        let title = group.title().to_string();
        if !title.is_empty() {
            text.push(title);
        }
    }

    let mut child = widget.first_child();
    while let Some(current) = child {
        collect_widget_text_recursive(&current, text);
        child = current.next_sibling();
    }
}
