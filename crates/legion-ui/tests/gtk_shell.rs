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
    PlatformProfileCapability, PowerProfilesCapability, RiskLevel, WriteDryRunPlan,
    WriteExecutionResult, WriteExecutionStatus,
};
use legion_control_ui::{gtk_shell, ui, DiagnosticsBundle, UiStatus};

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

    let page = ui::status::status_page(Ok(sample_status()), Ok(sample_diagnostics()), Ok(None));
    assert!(page.observe_children().n_items() >= 1);

    let page = ui::status::status_page(
        Ok(sample_status()),
        Ok(sample_diagnostics()),
        Ok(Some(sample_gpu_pending())),
    );
    assert!(page.observe_children().n_items() >= 1);

    let page = ui::fans::fans_page(Ok(sample_diagnostics()), Ok(None));
    assert!(page.observe_children().n_items() >= 1);

    let page = ui::fans::fans_page(Ok(sample_diagnostics()), Ok(Some(sample_fan_snapshot())));
    assert!(page.observe_children().n_items() >= 1);

    let page = ui::appearance::appearance_page(Ok(sample_diagnostics()));
    assert!(page.observe_children().n_items() >= 1);

    let page = ui::diagnostics::diagnostics_page(Ok(sample_diagnostics()));
    assert!(page.observe_children().n_items() >= 1);

    let page = ui::profiles::profiles_page(Ok(sample_diagnostics()));
    assert!(page.observe_children().n_items() >= 1);

    let page = ui::battery::battery_page(Ok(sample_diagnostics()));
    assert!(page.observe_children().n_items() >= 1);

    let page = ui::gpu::gpu_page(Ok(sample_diagnostics()), Ok(Some(sample_gpu_pending())));
    assert!(page.observe_children().n_items() >= 1);

    let page = gtk_shell::dashboard_page(
        Ok(sample_status()),
        Ok(sample_diagnostics()),
        Ok(None),
        Ok(None),
        std::rc::Rc::new(std::cell::RefCell::new("status".to_owned())),
    );
    let body = page
        .downcast::<gtk4::Box>()
        .expect("dashboard page should be a horizontal box");

    assert_eq!(body.orientation(), gtk4::Orientation::Horizontal);

    // Last child is the adw::ViewStack (sidebar | separator | stack)
    let stack = body
        .last_child()
        .expect("body should have children")
        .downcast::<adw::ViewStack>()
        .expect("last child should be adw::ViewStack");

    let visible_child = stack
        .visible_child()
        .expect("dashboard stack should have a visible child");
    visible_child
        .downcast::<adw::PreferencesPage>()
        .expect("dashboard stack pages should be PreferencesPage");

    let page = ui::status::status_page(
        Err(anyhow::anyhow!("daemon unavailable")),
        Ok(sample_diagnostics()),
        Ok(None),
    );
    assert!(page.observe_children().n_items() >= 1);
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

    let profiles = ui::profiles::profiles_page(Ok(sample_diagnostics()));
    let battery = ui::battery::battery_page(Ok(sample_diagnostics()));
    let gpu = ui::gpu::gpu_page(Ok(sample_diagnostics()), Ok(Some(sample_gpu_pending())));
    let status = ui::status::status_page(Ok(sample_status()), Ok(sample_diagnostics()), Ok(None));

    let status_text = collect_widget_text(&status.clone().upcast());
    assert!(status_text.iter().any(|text| text == "Control Center"));
    assert!(status_text
        .iter()
        .any(|text| text
            == "Current daemon readback. Writes count only after kernel read-back agrees."));
    assert!(status_text.iter().any(|text| text == "Fedora power"));
    assert!(status_text.iter().any(|text| text == "Devices"));

    let profile_text = collect_widget_text(&profiles.clone().upcast());
    assert!(profile_text.iter().any(|text| text == "Power Profiles"));
    assert!(profile_text.iter().any(|text| text == "Platform Control"));
    assert!(profile_text.iter().any(|text| text == "Requested profile"));
    assert!(profile_text.iter().any(|text| text == "Apply profile"));
    assert!(profile_text.iter().any(|text| text == "Apply result"));
    assert!(profile_text.iter().any(|text| {
        text == "No write attempted yet. If a request is blocked, the daemon will report why here."
    }));
    assert!(!profile_text.iter().any(|text| text == "INFO"));
    assert!(
        find_action_row_by_title(&profiles.clone().upcast(), "Requested profile")
            .and_then(|row| row.activatable_widget())
            .is_some()
    );

    let battery_text = collect_widget_text(&battery.clone().upcast());
    assert!(battery_text.iter().any(|text| text == "Charge Control"));
    assert!(battery_text
        .iter()
        .any(|text| text == "Requested charge type"));
    assert!(battery_text.iter().any(|text| text == "Apply charge type"));
    assert!(battery_text.iter().any(|text| text == "Apply result"));
    assert!(battery_text.iter().any(|text| {
        text == "No write attempted yet. If a request is blocked, the daemon will report why here."
    }));
    assert!(
        find_action_row_by_title(&battery.clone().upcast(), "Requested charge type")
            .and_then(|row| row.activatable_widget())
            .is_some()
    );

    let fans = ui::fans::fans_page(Ok(sample_diagnostics()), Ok(Some(sample_fan_snapshot())));
    let fans_text = collect_widget_text(&fans.clone().upcast());
    assert!(fans_text.iter().any(|text| text == "Guided fan planning"));
    assert!(fans_text.iter().any(|text| text == "Packaged preset"));
    assert!(
        find_action_row_by_title(&fans.clone().upcast(), "Packaged preset")
            .and_then(|row| row.activatable_widget())
            .is_some()
    );
    assert!(fans_text.iter().any(|text| text == "Preview dry-run plan"));
    assert!(fans_text
        .iter()
        .any(|text| text == "Preview restore dry-run"));
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
    assert!(fans_text.iter().any(|text| text == "Load live readings"));
    assert!(fans_text.iter().any(|text| text == "Validate pairs"));
    assert!(fans_text.iter().any(|text| text == "Preview sysfs text"));
    assert!(fans_text.iter().any(|text| text == "Copy sysfs preview"));
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
        .any(|text| text == "Save app-state mapping"));
    assert!(fans_text
        .iter()
        .any(|text| text == "Clear all profile mappings"));
    assert!(fans_text
        .iter()
        .any(|text| text == "Re-apply mapped fan preset after resume"));

    let gpu_text = collect_widget_text(&gpu.clone().upcast());
    assert!(gpu_text.iter().any(|text| text == "GPU"));
    assert!(gpu_text.iter().any(|text| text == "Switch Planning"));
    assert!(gpu_text.iter().any(|text| text == "Current mode"));
    assert!(gpu_text.iter().any(|text| text == "Pending reboot"));
    assert!(gpu_text.iter().any(|text| text == "Target mode"));
    assert!(
        find_action_row_by_title(&gpu.clone().upcast(), "Target mode")
            .and_then(|row| row.activatable_widget())
            .is_some()
    );
    assert!(gpu_text.iter().any(|text| text == "Preview plan"));
    assert!(gpu_text.iter().any(|text| text == "Record pending"));
    assert!(gpu_text.iter().any(|text| text == "Clear pending"));
    assert!(gpu_text.iter().any(|text| text == "Plan preview"));
    assert!(gpu_text.iter().any(|text| text == "Recovery guidance"));

    let appearance = ui::appearance::appearance_page(Ok(sample_diagnostics()));
    let appearance_text = collect_widget_text(&appearance.clone().upcast());
    assert!(appearance_text.iter().any(|text| text == "Logo LED"));
    assert!(appearance_text.iter().any(|text| text == "Y-logo LED"));
    assert!(appearance_text.iter().any(|text| text == "Turn off"));
    assert!(appearance_text.iter().any(|text| text == "Turn on"));
    assert!(appearance_text.iter().any(|text| text == "Keyboard"));
    assert!(appearance_text
        .iter()
        .any(|text| text == "Functional Fn-lock"));
    assert!(appearance_text.iter().any(|text| text == "Camera"));
    assert!(appearance_text.iter().any(|text| text == "Camera power"));
    assert!(appearance_text
        .iter()
        .any(|text| text == "Confirmation required"));
    assert!(appearance_text.iter().any(|text| text == "Request off"));
    assert!(appearance_text.iter().any(|text| text == "Request on"));
    assert!(appearance_text.iter().any(|text| text == "Confirm"));
    assert!(appearance_text.iter().any(|text| text == "Cancel"));
    assert!(appearance_text.iter().any(|text| text == "USB Power"));
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

    let profiles = ui::profiles::profiles_page(Ok(diagnostics.clone()));
    let battery = ui::battery::battery_page(Ok(diagnostics.clone()));
    let gpu = ui::gpu::gpu_page(Ok(diagnostics.clone()), Ok(None));

    let profile_text = collect_widget_text(&profiles.clone().upcast());
    assert!(profile_text.iter().any(|text| text == "Platform Control"));
    assert!(profile_text
        .iter()
        .any(|text| text.contains("this hardware capability was not detected")));
    assert!(!profile_text.iter().any(|text| text == "Apply profile"));

    let battery_text = collect_widget_text(&battery.clone().upcast());
    assert!(battery_text.iter().any(|text| text == "Charge Control"));
    assert!(battery_text
        .iter()
        .any(|text| text.contains("this hardware capability was not detected")));
    assert!(!battery_text.iter().any(|text| text == "Apply charge type"));

    let gpu_text = collect_widget_text(&gpu.clone().upcast());
    assert!(gpu_text.iter().any(|text| text == "Switch Planning"));
    assert!(gpu_text
        .iter()
        .any(|text| text.contains("envycontrol was not detected")));
    assert!(!gpu_text.iter().any(|text| text == "Preview plan"));

    let fans = ui::fans::fans_page(Ok(diagnostics.clone()), Ok(None));
    let fans_text = collect_widget_text(&fans.clone().upcast());
    assert!(fans_text.iter().any(|text| text == "Fan preset planning"));
    assert!(fans_text
        .iter()
        .any(|text| { text.contains("no fan curve capability was detected") }));
    assert!(!fans_text
        .iter()
        .any(|text| text == "Preview restore dry-run"));

    let appearance = ui::appearance::appearance_page(Ok(diagnostics));
    let appearance_text = collect_widget_text(&appearance.clone().upcast());
    assert!(appearance_text.iter().any(|text| text == "Logo LED"));
    assert!(appearance_text.iter().any(|text| text == "Keyboard"));
    assert!(appearance_text.iter().any(|text| text == "Camera"));
    assert!(appearance_text
        .iter()
        .any(|text| text.contains("quick apply is disabled")));
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
    let policy_blocked =
        WriteExecutionResult::blocked_by_policy(plan.clone(), "writes disabled by daemon policy");
    let blocked =
        WriteExecutionResult::blocked_by_authorization(plan.clone(), "polkit authorization failed");
    let failed = WriteExecutionResult {
        status: WriteExecutionStatus::Failed,
        applied: false,
        message: "platform profile read-back mismatch after write".to_owned(),
        readback_value: Some("balanced".to_owned()),
        plan,
    };

    assert_eq!(ui::shared::write_feedback_title(None), "Apply result");
    assert_eq!(
        ui::shared::write_feedback_subtitle(None),
        "No write attempted yet. If a request is blocked, the daemon will report why here."
    );
    assert_eq!(
        ui::shared::write_feedback_title(Some(&applied)),
        "Applied and verified"
    );
    assert!(ui::shared::write_feedback_subtitle(Some(&applied)).contains("read back successfully"));
    assert_eq!(
        ui::shared::write_feedback_title(Some(&blocked)),
        "Apply blocked by authorization"
    );
    assert!(
        ui::shared::write_feedback_subtitle(Some(&blocked)).contains("polkit authorization failed")
    );
    assert!(ui::shared::write_feedback_subtitle(Some(&blocked)).contains("Polkit denied"));
    assert_eq!(
        ui::shared::write_feedback_title(Some(&policy_blocked)),
        "Apply blocked by policy"
    );
    assert!(ui::shared::write_feedback_subtitle(Some(&policy_blocked))
        .contains("matching --enable-*-write flag"));
    assert_eq!(
        ui::shared::write_feedback_title(Some(&failed)),
        "Apply failed readback"
    );
    assert!(ui::shared::write_feedback_subtitle(Some(&failed)).contains("Read-back: balanced."));
}

#[gtk4::test]
fn button_sensitivity_reflects_current_led_and_toggle_state() {
    init_gtk();

    let appearance = ui::appearance::appearance_page(Ok(sample_diagnostics()));
    let root = appearance.clone().upcast::<gtk4::Widget>();

    // platform::ylogo brightness=Some(0) → "Turn off" disabled, "Turn on" enabled
    let turn_off = find_button_by_label(&root, "Turn off");
    let turn_on = find_button_by_label(&root, "Turn on");
    assert!(
        turn_off.is_some(),
        "Turn off button should exist when ylogo is detected"
    );
    assert!(
        turn_on.is_some(),
        "Turn on button should exist when ylogo is detected"
    );
    assert!(
        !turn_off.unwrap().is_sensitive(),
        "Turn off should be insensitive when LED is already off (brightness=0)"
    );
    assert!(
        turn_on.unwrap().is_sensitive(),
        "Turn on should be sensitive when LED is off"
    );

    // fn_lock current_value=Some("0") → "Turn off" disabled, "Turn on" enabled
    let fn_buttons = find_all_buttons_by_label(&root, "Turn off");
    assert!(
        !fn_buttons.is_empty(),
        "At least one Turn off button (fn-lock or ylogo)"
    );
}

#[gtk4::test]
fn dropdown_preselects_current_platform_profile() {
    init_gtk();

    // sample_diagnostics has platform_profile.current = "balanced"
    // choices = ["low-power", "balanced", "performance"]  → selected index 1
    let profiles = ui::profiles::profiles_page(Ok(sample_diagnostics()));
    let root = profiles.clone().upcast::<gtk4::Widget>();

    let dropdown = find_dropdown(&root);
    assert!(dropdown.is_some(), "Profiles page should have a DropDown");
    let dropdown = dropdown.unwrap();
    assert!(
        dropdown.is_sensitive(),
        "DropDown should be sensitive when choices are available"
    );
    assert_eq!(
        dropdown.selected(),
        1,
        "Should pre-select index 1 (balanced is second choice)"
    );
}

#[gtk4::test]
fn dropdown_preselects_current_battery_charge_type() {
    init_gtk();

    // sample_diagnostics has battery_charge_type.current = "Standard"
    // choices = ["Fast", "Standard", "Conservation"] → selected index 1
    let battery = ui::battery::battery_page(Ok(sample_diagnostics()));
    let root = battery.clone().upcast::<gtk4::Widget>();

    let dropdown = find_dropdown(&root);
    assert!(dropdown.is_some(), "Battery page should have a DropDown");
    let dropdown = dropdown.unwrap();
    assert!(
        dropdown.is_sensitive(),
        "DropDown should be sensitive when choices are available"
    );
    assert_eq!(
        dropdown.selected(),
        1,
        "Should pre-select index 1 (Standard is second choice)"
    );
}

#[gtk4::test]
fn fan_chart_and_scratchpad_exist_with_snapshot() {
    init_gtk();

    let fans = ui::fans::fans_page(Ok(sample_diagnostics()), Ok(Some(sample_fan_snapshot())));
    let root = fans.clone().upcast::<gtk4::Widget>();
    let fans_text = collect_widget_text(&root);

    // Chart section present
    assert!(
        fans_text
            .iter()
            .any(|t| t.contains("Temperature vs PWM") || t.contains("Curve shape")),
        "Fan chart caption should appear when snapshot is present"
    );

    // Scratchpad controls exist
    assert!(fans_text.iter().any(|t| t == "Load live readings"));
    assert!(fans_text.iter().any(|t| t == "Load saved snapshot"));
    assert!(fans_text.iter().any(|t| t == "Validate pairs"));
    assert!(fans_text.iter().any(|t| t == "Preview sysfs text"));
    assert!(fans_text.iter().any(|t| t == "Copy JSON"));
    assert!(fans_text.iter().any(|t| t == "Import TOML from editor"));

    // Drawing area (chart) is present in widget tree
    assert!(
        find_drawing_area(&root).is_some(),
        "Fan page should contain a DrawingArea (chart)"
    );
}

#[gtk4::test]
fn gpu_dropdown_preselects_current_mode() {
    init_gtk();

    // sample_diagnostics has gpu.mode = "hybrid"
    // GPU_MODE_CHOICES = ["integrated", "hybrid", "nvidia"] → selected index 1
    let gpu = ui::gpu::gpu_page(Ok(sample_diagnostics()), Ok(None));
    let root = gpu.clone().upcast::<gtk4::Widget>();

    let dropdown = find_dropdown(&root);
    assert!(dropdown.is_some(), "GPU page should have a DropDown");
    assert_eq!(
        dropdown.unwrap().selected(),
        1,
        "Should pre-select index 1 (hybrid)"
    );
}

#[gtk4::test]
fn camera_confirm_flow_buttons_start_insensitive() {
    init_gtk();

    let appearance = ui::appearance::appearance_page(Ok(sample_diagnostics()));
    let root = appearance.clone().upcast::<gtk4::Widget>();

    // Confirm and Cancel should start insensitive (nothing requested yet)
    let confirm = find_button_by_label(&root, "Confirm");
    let cancel = find_button_by_label(&root, "Cancel");
    assert!(
        confirm.is_some() && cancel.is_some(),
        "Confirm/Cancel buttons should exist for camera power flow"
    );
    assert!(
        !confirm.unwrap().is_sensitive(),
        "Confirm should start insensitive before request"
    );
    assert!(
        !cancel.unwrap().is_sensitive(),
        "Cancel should start insensitive before request"
    );
}

#[gtk4::test]
fn dashboard_stack_has_all_nine_pages() {
    init_gtk();

    let page = gtk_shell::dashboard_page(
        Ok(sample_status()),
        Ok(sample_diagnostics()),
        Ok(None),
        Ok(None),
        std::rc::Rc::new(std::cell::RefCell::new("status".to_owned())),
    );
    let body = page.downcast::<gtk4::Box>().unwrap();
    // Layout: sidebar | separator | stack
    let stack = body
        .last_child()
        .unwrap()
        .downcast::<adw::ViewStack>()
        .unwrap();

    let expected = [
        "status",
        "profiles",
        "battery",
        "gpu",
        "fans",
        "appearance",
        "automations",
        "settings",
        "diagnostics",
    ];
    for name in expected {
        assert!(
            stack.child_by_name(name).is_some(),
            "ViewStack should have page named '{name}'"
        );
    }
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
    report.power_profiles = Some(PowerProfilesCapability {
        bus: "system".to_owned(),
        well_known_name: "org.freedesktop.UPower.PowerProfiles".to_owned(),
        unique_owner: Some(":1.53".to_owned()),
        active_profile: Some("balanced".to_owned()),
        status: CapabilityStatus::ProbeOnly,
        detail: None,
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

fn find_action_row_by_title(root: &gtk4::Widget, title: &str) -> Option<adw::ActionRow> {
    if let Ok(row) = root.clone().downcast::<adw::ActionRow>() {
        if row.title() == title {
            return Some(row);
        }
    }

    let mut child = root.first_child();
    while let Some(current) = child {
        if let Some(row) = find_action_row_by_title(&current, title) {
            return Some(row);
        }
        child = current.next_sibling();
    }

    None
}

fn find_button_by_label(root: &gtk4::Widget, label: &str) -> Option<gtk4::Button> {
    if let Ok(button) = root.clone().downcast::<gtk4::Button>() {
        if button.label().as_deref() == Some(label) {
            return Some(button);
        }
    }
    let mut child = root.first_child();
    while let Some(current) = child {
        if let Some(found) = find_button_by_label(&current, label) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

fn find_all_buttons_by_label(root: &gtk4::Widget, label: &str) -> Vec<gtk4::Button> {
    let mut results = Vec::new();
    find_all_buttons_by_label_recursive(root, label, &mut results);
    results
}

fn find_all_buttons_by_label_recursive(
    widget: &gtk4::Widget,
    label: &str,
    results: &mut Vec<gtk4::Button>,
) {
    if let Ok(button) = widget.clone().downcast::<gtk4::Button>() {
        if button.label().as_deref() == Some(label) {
            results.push(button);
        }
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        find_all_buttons_by_label_recursive(&current, label, results);
        child = current.next_sibling();
    }
}

fn find_dropdown(root: &gtk4::Widget) -> Option<gtk4::DropDown> {
    if let Ok(dropdown) = root.clone().downcast::<gtk4::DropDown>() {
        return Some(dropdown);
    }
    let mut child = root.first_child();
    while let Some(current) = child {
        if let Some(found) = find_dropdown(&current) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

fn find_drawing_area(root: &gtk4::Widget) -> Option<gtk4::DrawingArea> {
    if let Ok(area) = root.clone().downcast::<gtk4::DrawingArea>() {
        return Some(area);
    }
    let mut child = root.first_child();
    while let Some(current) = child {
        if let Some(found) = find_drawing_area(&current) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}
