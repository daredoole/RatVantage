#![cfg(feature = "gtk-ui")]

use adw::prelude::*;
use std::{
    sync::Once,
    time::{Duration, Instant},
};

use legion_common::{
    AmdGpuPowerDpmCapability, AutomationRule, AutomationRuleKind, BatteryChargeTypeCapability,
    BatteryTelemetry, Capability, CapabilityRegistry, CapabilityStatus, CpuPowerCapability,
    CurveOptimizerReadbackStatus, DesktopPowerProfileChangeEvent, FanCurveCapability,
    FanCurvePointSnapshot, FanCurveSnapshot, FirmwareAttributeCapability, GpuCapability,
    GpuModePending, GpuSwitchType, HardwareProfile, HardwareProfileActions,
    HardwareProfileApplyActionResult, HardwareProfileApplyRun, HardwareSummary, HwmonSensor,
    IdeapadToggleCapability, KeyboardRgbCandidate, KeyboardRgbHidReport, KeyboardRgbOpenRgbDevice,
    KeyboardRgbOpenRgbStatus, KeyboardRgbWriteRequest, LedCapability, PlatformProfileCapability,
    PlatformProfileChangeEvent, PowerProfilesCapability, RiskLevel, RyzenAdjBackendStatus,
    RyzenBackendStatus, RyzenSmuBackendStatus, RyzenSmuSetupAssistant, WriteDryRunPlan,
    WriteExecutionResult, WriteExecutionStatus,
};
use legion_control_ui::{gtk_shell, ui, DiagnosticsBundle, DiagnosticsRuntimeState, UiStatus};

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
fn diagnostics_page_surfaces_compatibility_bundle_command() {
    init_gtk();

    let base = sample_diagnostics();
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
    let diagnostics = DiagnosticsBundle::from_report_with_logs(
        base.raw_probe_report.clone(),
        base.kernel_version.clone(),
        base.recent_daemon_logs.clone(),
    )
    .with_runtime_state(DiagnosticsRuntimeState {
        last_hardware_profile_apply: Some(HardwareProfileApplyRun {
            profile_id: "gaming".to_owned(),
            profile_label: "Gaming".to_owned(),
            timestamp_unix_secs: 1_770_000_000,
            completed: true,
            message: "hardware profile applied".to_owned(),
            results: vec![HardwareProfileApplyActionResult {
                action_id: "platform_profile".to_owned(),
                result: WriteExecutionResult::applied(
                    plan,
                    "platform profile write applied and read back successfully",
                    Some("performance".to_owned()),
                ),
            }],
        }),
        ryzen_backend_status: Some(sample_ryzen_backend_status()),
        ..Default::default()
    });
    let page = ui::diagnostics::diagnostics_page(Ok(diagnostics));
    let root = page.clone().upcast::<gtk4::Widget>();
    let text = collect_widget_text(&root);

    assert!(text.iter().any(|value| value == "Hardware profile drift"));
    assert!(text.iter().any(|value| value == "1 drifted of 1 checked"));
    assert!(text.iter().any(|value| value == "Fan curve drift"));
    assert!(text
        .iter()
        .any(|value| value == "No saved fan curve snapshot"));
    assert!(text.iter().any(|value| value == "Compatibility Bundle"));
    assert!(text.iter().any(|value| value == "Read-only support bundle"));
    assert!(text.iter().any(|value| {
        value.contains("ratvantage-capture-compatibility-bundle")
            && value.contains("automation/reset snapshots")
            && value.contains("RGB bridge evidence status")
    }));
    assert!(find_button_by_label(&root, "Copy bundle command").is_some());
    assert!(text.iter().any(|value| value == "Automation Diagnostics"));
    assert!(text
        .iter()
        .any(|value| value == "Read-only automation snapshot"));
    assert!(text.iter().any(|value| {
        value.contains("legion-control-ui --automation-diagnostics")
            && value.contains("trigger mappings")
    }));
    assert!(find_button_by_label(&root, "Copy automation command").is_some());
    assert!(text.iter().any(|value| value == "Reset Diagnostics"));
    assert!(text.iter().any(|value| value == "Read-only reset snapshot"));
    assert!(text.iter().any(|value| {
        value.contains("legion-control-ui --reset-diagnostics")
            && value.contains("restore-auto-fan")
    }));
    assert!(find_button_by_label(&root, "Copy reset command").is_some());
    assert!(text
        .iter()
        .any(|value| value == "Recovery command reference"));
    assert!(text.iter().any(|value| {
        value.contains("Curve Optimizer")
            && value.contains("firmware PPT defaults")
            && value.contains("GPU pending-marker cleanup")
    }));
    assert!(find_button_by_label(&root, "Copy recovery commands").is_some());
    assert!(text.iter().any(|value| value == "Ryzen Backend Setup"));
    assert!(text.iter().any(|value| value == "Curve Optimizer backend"));
    assert!(text.iter().any(|value| value == "ryzenadj_write_only"));
    assert!(text.iter().any(|value| value == "write-only"));
    assert!(text.iter().any(|value| value == "recommended"));
    assert!(text.iter().any(|value| value == "Read-only backend status"));
    assert!(text.iter().any(|value| {
        value.contains("legion-control-ui --ryzen-backend-status")
            && value.contains("Curve Optimizer read-back")
    }));
    assert!(find_button_by_label(&root, "Copy Ryzen status command").is_some());
    assert!(text
        .iter()
        .any(|value| value == "ryzen_smu setup assistant"));
    assert!(text.iter().any(|value| {
        value.contains("legion-control-ui --ryzen-smu-setup")
            && value.contains("without installing or loading modules")
    }));
    assert!(find_button_by_label(&root, "Copy Ryzen setup command").is_some());
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
    assert!(profile_text
        .iter()
        .any(|text| text == "Advanced CPU Tuning"));
    assert!(profile_text
        .iter()
        .any(|text| text == "Show Curve Optimizer controls"));
    assert!(profile_text
        .iter()
        .any(|text| text == "Firmware Power Limits (TDP)"));
    assert_eq!(
        count_action_rows_by_title(&profiles.clone().upcast(), "SPL"),
        1,
        "PPT firmware row should render once"
    );
    let spl_row = find_action_row_by_title(&profiles.clone().upcast(), "SPL")
        .expect("SPL firmware row should exist");
    assert_eq!(
        spl_row.subtitle().as_deref(),
        Some("ppt_pl1_spl · range 50..85 step 1")
    );
    let spl_spin = find_spin_button(&spl_row.upcast()).expect("SPL row should have a SpinButton");
    assert_eq!(spl_spin.value_as_int(), 70);
    assert!(profile_text.iter().any(|text| {
        text == "No write attempted yet. If a request is blocked, the daemon will report why here."
    }));
    assert!(!profile_text.iter().any(|text| text == "INFO"));
    assert!(find_expander_row_by_title(&profiles.clone().upcast(), "Requested profile").is_some());

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
    assert!(battery_text.iter().any(|text| {
        text.contains("ideapad_acpi conservation_mode") && text.contains("Long_Life/Conservation")
    }));
    assert!(
        find_expander_row_by_title(&battery.clone().upcast(), "Requested charge type").is_some()
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
    assert!(gpu_text.iter().any(|text| text == "AMD GPU DPM"));
    assert!(gpu_text.iter().any(|text| {
        text.contains("Manual SCLK/MCLK clock states are read-only")
            && text.contains("not exposed as write controls")
    }));
    assert!(gpu_text.iter().any(|text| text == "AMD GPU DPM Control"));
    assert!(gpu_text.iter().any(|text| {
        text.contains("DPM force-level writes may affect display/GPU stability")
            && text.contains("use auto to restore driver control")
    }));
    assert!(gpu_text
        .iter()
        .any(|text| text == "Confirm DPM force-level write"));
    let dpm_apply =
        find_button_by_label(&gpu.clone().upcast(), "Apply force level").expect("DPM apply button");
    assert!(
        !dpm_apply.is_sensitive(),
        "DPM force-level execution should start disabled until confirmed"
    );
    let dpm_confirm = find_check_button_in_action_row_by_title(
        &gpu.clone().upcast(),
        "Confirm DPM force-level write",
    )
    .expect("DPM confirmation checkbox");
    dpm_confirm.set_active(true);
    assert!(
        dpm_apply.is_sensitive(),
        "DPM force-level execution should be enabled after confirmation"
    );
    assert!(gpu_text.iter().any(|text| text == "Switch Planning"));
    assert!(gpu_text.iter().any(|text| text == "Current mode"));
    assert!(gpu_text.iter().any(|text| text == "GPU switching status"));
    assert!(gpu_text
        .iter()
        .any(|text| text == "reboot_required_baseline"));
    assert!(gpu_text.iter().any(|text| text == "Execution model"));
    assert!(gpu_text.iter().any(|text| text == "Runtime plan"));
    assert!(gpu_text.iter().any(|text| text == "Pending reboot"));
    assert!(gpu_text.iter().any(|text| text == "Target mode"));
    assert!(gpu_text.iter().any(|text| text == "Confirm GPU switch"));
    assert!(
        find_action_row_by_title(&gpu.clone().upcast(), "Target mode")
            .and_then(|row| row.activatable_widget())
            .is_some()
    );
    let switch_mode =
        find_button_by_label(&gpu.clone().upcast(), "Switch mode").expect("Switch mode button");
    assert!(
        !switch_mode.is_sensitive(),
        "GPU mode execution should start disabled until confirmed"
    );
    let confirm =
        find_check_button_in_action_row_by_title(&gpu.clone().upcast(), "Confirm GPU switch")
            .expect("GPU mode confirmation checkbox");
    confirm.set_active(true);
    assert!(
        switch_mode.is_sensitive(),
        "GPU mode execution should be enabled after confirmation"
    );
    assert!(gpu_text.iter().any(|text| text == "Preview plan"));
    assert!(gpu_text.iter().any(|text| text == "Record pending"));
    assert!(gpu_text.iter().any(|text| text == "Clear pending"));
    assert!(gpu_text.iter().any(|text| text == "Plan preview"));
    assert!(gpu_text.iter().any(|text| text == "Recovery guidance"));

    let appearance = ui::appearance::appearance_page(Ok(sample_diagnostics()));
    let appearance_text = collect_widget_text(&appearance.clone().upcast());
    assert!(appearance_text.iter().any(|text| text == "Keyboard RGB"));
    assert!(appearance_text
        .iter()
        .any(|text| text == "OpenRGB readiness"));
    assert!(appearance_text
        .iter()
        .any(|text| text == "OpenRGB access setup"));
    assert!(appearance_text
        .iter()
        .any(|text| text == "OpenRGB SDK read-back evidence"));
    assert!(appearance_text.iter().any(|text| text == "Check SDK"));
    assert!(appearance_text
        .iter()
        .any(|text| text == "RGB request preview"));
    assert!(appearance_text
        .iter()
        .any(|text| text == "Zone color: Left side"));
    assert!(appearance_text
        .iter()
        .any(|text| text == "Zone color: Right side"));
    assert!(appearance_text.iter().any(|text| text == "Preview plan"));
    assert!(appearance_text
        .iter()
        .any(|text| text == "Copy request JSON"));
    assert!(appearance_text.iter().any(|text| text == "Apply RGB"));
    let rgb_apply =
        find_button_by_label(&appearance.clone().upcast(), "Apply").expect("RGB apply button");
    assert!(
        !rgb_apply.is_sensitive(),
        "RGB execution should stay disabled until backend_ready evidence is promoted"
    );
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
    assert!(appearance_text.iter().any(|text| text == "Fan Mode"));
    assert!(appearance_text
        .iter()
        .any(|text| { text.contains("0 = Auto") && text.contains("1 = Full speed") }));
    assert!(appearance_text.iter().any(|text| text == "Auto (0)"));
    assert!(appearance_text.iter().any(|text| text == "Full speed (1)"));
    let fan_mode_row = find_action_row_by_title(&appearance.clone().upcast(), "Fan mode")
        .expect("fan mode control row should render when fan_mode exists");
    assert!(fan_mode_row
        .subtitle()
        .as_deref()
        .is_some_and(|subtitle| subtitle.contains("Current: Auto (0)")));
}

#[gtk4::test]
fn automations_page_renders_ac_profile_router_rules() {
    init_gtk();

    let mut diagnostics = sample_diagnostics();
    diagnostics.hardware_profiles.insert(
        "plugged_in".to_owned(),
        HardwareProfile {
            schema_version: 1,
            label: "Plugged in".to_owned(),
            actions: HardwareProfileActions {
                platform_profile: Some("performance".to_owned()),
                ..Default::default()
            },
        },
    );
    diagnostics.hardware_profiles.insert(
        "on_battery".to_owned(),
        HardwareProfile {
            schema_version: 1,
            label: "On battery".to_owned(),
            actions: HardwareProfileActions {
                platform_profile: Some("low-power".to_owned()),
                ..Default::default()
            },
        },
    );
    diagnostics.hardware_profiles.insert(
        "integrated_gpu_on_battery".to_owned(),
        HardwareProfile {
            schema_version: 1,
            label: "Integrated GPU on battery".to_owned(),
            actions: HardwareProfileActions {
                gpu_mode: Some("integrated".to_owned()),
                keyboard_rgb: Some(KeyboardRgbWriteRequest {
                    effect: "Breathing".to_owned(),
                    colors: [("Left side".to_owned(), "#333333".to_owned())]
                        .into_iter()
                        .collect(),
                    brightness: 40,
                    speed: Some(30),
                }),
                ..Default::default()
            },
        },
    );
    diagnostics.automation_rules.insert(
        "ac_router".to_owned(),
        AutomationRule {
            schema_version: 1,
            label: "AC profile router".to_owned(),
            enabled: true,
            kind: AutomationRuleKind::AcProfileRouter {
                ac_profile_id: "plugged_in".to_owned(),
                battery_profile_id: "on_battery".to_owned(),
                cooldown_secs: 120,
            },
        },
    );
    diagnostics.automation_rules.insert(
        "quiet_below_80".to_owned(),
        AutomationRule {
            schema_version: 1,
            label: "Quiet below 80%".to_owned(),
            enabled: true,
            kind: AutomationRuleKind::BatteryProfileThreshold {
                threshold_percent: 80,
                profile_id: "on_battery".to_owned(),
                when_below_or_equal: true,
                require_ac: Some(false),
                cooldown_secs: 60,
            },
        },
    );
    diagnostics.automation_rules.insert(
        "periodic_idle_correction".to_owned(),
        AutomationRule {
            schema_version: 1,
            label: "Periodic idle correction".to_owned(),
            enabled: true,
            kind: AutomationRuleKind::PeriodicIdle {
                profile_id: "on_battery".to_owned(),
                cooldown_secs: 1800,
            },
        },
    );
    diagnostics
        .recent_platform_profile_changes
        .push(PlatformProfileChangeEvent {
            timestamp_unix_secs: 1_770_000_000,
            previous_profile: "balanced".to_owned(),
            current_profile: "performance".to_owned(),
            source: "platform_profile_observer".to_owned(),
        });
    diagnostics
        .recent_desktop_power_profile_changes
        .push(DesktopPowerProfileChangeEvent {
            timestamp_unix_secs: 1_770_000_001,
            previous_profile: "balanced".to_owned(),
            current_profile: "power-saver".to_owned(),
            source: "desktop_power_profile_observer".to_owned(),
        });

    let automations = ui::automations::automations_page(Ok(diagnostics));
    let text = collect_widget_text(&automations.clone().upcast());
    assert!(text.iter().any(|value| {
        value.contains("Resume and GPU reboot-completion triggers run from the daemon")
    }));
    assert!(text.iter().any(|value| value == "GPU reboot completed"));
    assert!(text
        .iter()
        .any(|value| value == "Desktop power profile changed"));
    assert!(text
        .iter()
        .any(|value| value == "Periodic idle correction starter"));
    assert!(text.iter().any(|value| value == "Periodic idle correction"));
    assert!(text
        .iter()
        .any(|value| value == "Recent Platform Profile Changes"));
    assert!(text.iter().any(|value| value == "balanced -> performance"));
    assert!(text.iter().any(|value| {
        value.contains("platform_profile_observer") && value.contains("1770000000")
    }));
    assert!(text
        .iter()
        .any(|value| value == "Recent Desktop Power Profile Changes"));
    assert!(text.iter().any(|value| value == "balanced -> power-saver"));
    assert!(text.iter().any(|value| {
        value.contains("desktop_power_profile_observer") && value.contains("1770000001")
    }));
    assert!(text.iter().any(|value| value == "Saved Automation Rules"));
    assert!(text.iter().any(|value| value == "AC profile router"));
    assert!(text.iter().any(|value| value == "Quiet below 80%"));
    assert!(text.iter().any(|value| value == "Create Automation Rule"));
    assert!(text
        .iter()
        .any(|value| value == "Create battery threshold rule"));
    assert!(text.iter().any(|value| value == "Target profile"));
    assert!(text.iter().any(|value| {
        value.contains("Saves a daemon-owned rule") && value.contains("normal validation")
    }));
    assert!(text.iter().any(|value| value == "Create AC Router Rule"));
    assert!(text
        .iter()
        .any(|value| value == "Create AC profile router rule"));
    assert!(text.iter().any(|value| value == "AC online profile"));
    assert!(text.iter().any(|value| value == "Battery power profile"));
    assert!(text.iter().any(|value| {
        value.contains("Saves a daemon-owned AC router rule") && value.contains("normal validation")
    }));
    assert!(text.iter().any(|value| value == "Create Fast Charge Rule"));
    assert!(text
        .iter()
        .any(|value| value == "Create fast-charge threshold rule"));
    assert!(text.iter().any(|value| value == "Fast-charge profile"));
    assert!(text.iter().any(|value| value == "Protect profile"));
    assert!(text.iter().any(|value| value == "Require AC online"));
    assert!(text.iter().any(|value| {
        value.contains("Saves a daemon-owned fast-charge rule")
            && value.contains("normal validation")
    }));
    assert!(text
        .iter()
        .any(|value| value == "Battery profile threshold"));
    assert!(text.iter().any(|value| value == "Threshold direction"));
    assert!(text.iter().any(|value| value == "At or below"));
    assert!(text.iter().any(|value| value == "Require battery power"));
    assert!(text.iter().any(|value| value == "AC profile"));
    assert!(text.iter().any(|value| value == "plugged_in"));
    assert!(text.iter().any(|value| value == "Battery profile"));
    assert!(text.iter().any(|value| value == "on_battery"));
    assert!(text
        .iter()
        .any(|value| value.contains("integrated_gpu_on_battery")));
    assert!(text.iter().any(|value| value.contains("gpu=integrated")));
    assert!(text
        .iter()
        .any(|value| value.contains("rgb=Breathing #333333")));
    assert!(text.iter().any(|value| value == "Cooldown seconds"));
    assert!(text
        .iter()
        .any(|value| value.contains("save edits") && value.contains("AC router")));
    assert!(text
        .iter()
        .any(|value| { value.contains("save edits") && value.contains("battery threshold rule") }));
    assert!(find_button_by_label(&automations.clone().upcast(), "Preview").is_some());
    assert!(find_button_by_label(&automations.clone().upcast(), "Test run").is_some());
    assert!(find_button_by_label(&automations.clone().upcast(), "Save").is_some());
    assert!(find_button_by_label(&automations.clone().upcast(), "Delete").is_some());
    assert!(find_button_by_label(&automations.clone().upcast(), "Create rule").is_some());
    assert!(find_button_by_label(&automations.clone().upcast(), "Create AC router").is_some());
    assert!(
        find_button_by_label(&automations.clone().upcast(), "Create fast-charge rule").is_some()
    );
}

#[gtk4::test]
fn automations_page_renders_ac_profile_router_starter() {
    init_gtk();

    let automations = ui::automations::automations_page(Ok(sample_diagnostics()));
    let text = collect_widget_text(&automations.clone().upcast());
    assert!(text.iter().any(|value| value == "Quick Templates"));
    assert!(text
        .iter()
        .any(|value| value == "Balanced daily mixed profile starter"));
    assert!(text.iter().any(|value| {
        value.contains("platform, charge, CPU, AMD GPU DPM, staged RGB")
            && value.contains("balanced fan-preset mapping")
    }));
    assert!(find_button_by_label(&automations.clone().upcast(), "Create mixed").is_some());
    assert!(text
        .iter()
        .any(|value| value == "Quiet battery mixed profile starter"));
    assert!(text.iter().any(|value| {
        value.contains("low-power platform")
            && value.contains("conservation charge")
            && value.contains("quiet fan-preset mapping")
    }));
    assert!(find_button_by_label(&automations.clone().upcast(), "Create quiet mixed").is_some());
    assert!(text
        .iter()
        .any(|value| value == "Performance AC mixed profile starter"));
    assert!(text.iter().any(|value| {
        value.contains("performance platform")
            && value.contains("AC trigger")
            && value.contains("performance fan-preset mapping")
    }));
    assert!(
        find_button_by_label(&automations.clone().upcast(), "Create performance mixed").is_some()
    );
    assert!(text
        .iter()
        .any(|value| value == "AC profile router starter"));
    assert!(text.iter().any(|value| {
        value.contains("Creates plugged-in and battery hardware profiles")
            && value.contains("routes AC state")
    }));
    assert!(find_button_by_label(&automations.clone().upcast(), "Create router").is_some());
    assert!(text
        .iter()
        .any(|value| value == "AC CPU performance router starter"));
    assert!(text.iter().any(|value| {
        value.contains("Creates AC performance and battery efficiency CPU profiles")
            && value.contains("governor")
            && value.contains("boost")
    }));
    assert!(find_button_by_label(&automations.clone().upcast(), "Create CPU router").is_some());
    assert!(text.iter().any(|value| value == "Quiet on battery starter"));
    assert!(text.iter().any(|value| {
        value.contains("Creates a low-power battery profile")
            && value.contains("battery 30% or lower")
            && value.contains("quiet-office")
    }));
    assert!(find_button_by_label(&automations.clone().upcast(), "Create quiet").is_some());
    assert!(text
        .iter()
        .any(|value| value == "Battery threshold rule starter"));
    assert!(text.iter().any(|value| {
        value.contains("Creates a balanced recovery profile")
            && value.contains("battery 80% or higher")
            && value.contains("battery-threshold rule")
    }));
    assert!(find_button_by_label(&automations.clone().upcast(), "Create threshold").is_some());
    assert!(text
        .iter()
        .any(|value| value == "Integrated GPU on battery starter"));
    assert!(text.iter().any(|value| {
        value.contains("Creates an integrated-GPU battery profile")
            && value.contains("reboot-gated")
    }));
    assert!(find_button_by_label(&automations.clone().upcast(), "Create iGPU").is_some());
    assert!(text
        .iter()
        .any(|value| value == "Resume balanced profile starter"));
    assert!(text.iter().any(|value| {
        value.contains("Creates a balanced resume profile")
            && value.contains("post-sleep state repair")
    }));
    assert!(find_button_by_label(&automations.clone().upcast(), "Create resume").is_some());
    assert!(text
        .iter()
        .any(|value| value == "GPU reboot repair starter"));
    assert!(text.iter().any(|value| {
        value.contains("Creates a balanced post-GPU-switch repair profile")
            && value.contains("GPU reboot completion")
    }));
    assert!(find_button_by_label(&automations.clone().upcast(), "Create GPU repair").is_some());
    assert!(text
        .iter()
        .any(|value| value == "Fn+Q tuning repair starter"));
    assert!(text.iter().any(|value| {
        value.contains("Creates a profile-change repair profile")
            && value.contains("external platform-profile changes")
    }));
    assert!(find_button_by_label(&automations.clone().upcast(), "Create repair").is_some());
    assert!(text
        .iter()
        .any(|value| value == "Desktop power repair starter"));
    assert!(text.iter().any(|value| {
        value.contains("Creates a balanced profile for desktop power mode changes")
            && value.contains("PowerProfiles changes")
    }));
    assert!(find_button_by_label(&automations.clone().upcast(), "Create power repair").is_some());
    assert!(text
        .iter()
        .any(|value| value == "RGB breathing profile starter"));
    assert!(text.iter().any(|value| {
        value.contains("Creates an RGB breathing hardware profile")
            && value.contains("backend evidence")
    }));
    assert!(find_button_by_label(&automations.clone().upcast(), "Create RGB").is_some());
    assert!(text
        .iter()
        .any(|value| value == "CO experimental profile starter"));
    assert!(text.iter().any(|value| {
        value.contains("Creates a Curve Optimizer -10 hardware profile")
            && value.contains("write-only")
    }));
    assert!(find_button_by_label(&automations.clone().upcast(), "Create CO").is_some());
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
    assert!(appearance_text
        .iter()
        .any(|text| text.contains("fan_mode not exposed")));
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

    let fan_auto = find_button_by_label(&root, "Auto (0)").expect("fan auto button");
    let fan_full = find_button_by_label(&root, "Full speed (1)").expect("fan full-speed button");
    assert!(
        !fan_auto.is_sensitive(),
        "Auto should be insensitive when fan_mode is already 0"
    );
    assert!(
        fan_full.is_sensitive(),
        "Full speed should be sensitive when fan_mode is currently 0"
    );
}

#[gtk4::test]
fn appearance_keyboard_rgb_research_controls_stage_requests_without_apply() {
    init_gtk();

    let appearance = ui::appearance::appearance_page(Ok(sample_diagnostics()));
    let root = appearance.clone().upcast::<gtk4::Widget>();
    let group = find_preferences_group_by_title(&root, "Keyboard RGB")
        .expect("Appearance page should include Keyboard RGB controls");
    let group_root = group.clone().upcast::<gtk4::Widget>();
    let text = collect_widget_text(&group_root);

    assert!(text.iter().any(|value| value == "Backend readiness"));
    assert!(text.iter().any(|value| {
        value.contains("2 HID research candidates")
            && value.contains("devices=048d:c103, 048d:c985")
            && value.contains("backend_ready=false")
    }));
    assert!(text.iter().any(|value| value.contains("Fn+Space cycles")));
    assert!(text.iter().any(|value| value == "OpenRGB readiness"));
    assert!(text.iter().any(|value| {
        value.contains("Lenovo 4-Zone device")
            && value.contains("Breathing")
            && value.contains("i2c_rw=false")
            && value.contains("backend_ready=false")
    }));
    assert!(text.iter().any(|value| value == "OpenRGB access setup"));
    assert!(text.iter().any(|value| {
        value.contains("Daemon setup adds")
            && value.contains("log out/in")
            && value.contains("Missing: i2c group, i2c rw")
    }));
    assert!(find_button_by_label(&group_root, "Set up").is_some());
    assert!(find_button_by_label(&group_root, "Copy fallback").is_some());
    assert!(text
        .iter()
        .any(|value| value == "OpenRGB bridge evidence status"));
    assert!(text.iter().any(|value| {
        value.contains("ratvantage-check-keyboard-rgb-openrgb")
            && value.contains("target/validation/keyboard-rgb-openrgb-readiness")
            && value.contains("ratvantage-keyboard-rgb-openrgb-bridge-status --readiness")
            && value.contains("--sdk target/validation/keyboard-rgb-openrgb-sdk")
            && value.contains("--sdk-write target/validation/keyboard-rgb-openrgb-sdk-write")
    }));
    assert!(text
        .iter()
        .any(|value| value == "OpenRGB bridge dry-run evidence"));
    assert!(text
        .iter()
        .any(|value| value == "OpenRGB SDK read-back evidence"));
    assert!(text.iter().any(|value| value == "OpenRGB SDK server"));
    assert!(text
        .iter()
        .any(|value| value == "OpenRGB SDK write evidence"));
    assert!(text.iter().any(|value| {
        value.contains("ratvantage-capture-keyboard-rgb-openrgb-sdk-evidence")
            && value.contains("keyboard-rgb-openrgb-sdk")
    }));
    assert!(text.iter().any(|value| {
        value.contains("ratvantage-capture-keyboard-rgb-openrgb-sdk-write-evidence")
            && value.contains("keyboard-rgb-openrgb-sdk-write")
            && value.contains("--execute")
            && value.contains("--mode Breathing")
    }));
    assert!(text
        .iter()
        .any(|value| value.contains("ratvantage-openrgb-sdk-server start")));
    assert!(text.iter().any(|value| {
        value.contains("ratvantage-capture-keyboard-rgb-openrgb-bridge-evidence")
            && value.contains("keyboard-rgb-openrgb-bridge-dry-run")
    }));
    assert!(text
        .iter()
        .any(|value| value == "OpenRGB bridge execute evidence"));
    assert!(text.iter().any(|value| {
        value.contains("keyboard-rgb-openrgb-bridge-execute")
            && value.contains("ratvantage-review-keyboard-rgb-openrgb-bridge-evidence")
    }));
    assert!(find_button_by_label(&group_root, "Copy status").is_some());
    assert!(find_button_by_label(&group_root, "Check status").is_some());
    assert!(find_button_by_label(&group_root, "Check SDK").is_some());
    assert!(find_button_by_label(&group_root, "Copy SDK").is_some());
    assert!(find_button_by_label(&group_root, "Start server").is_some());
    assert!(find_button_by_label(&group_root, "Copy server").is_some());
    assert!(find_button_by_label(&group_root, "Copy SDK write").is_some());
    assert!(find_button_by_label(&group_root, "Capture dry-run").is_some());
    assert!(find_button_by_label(&group_root, "Copy dry-run").is_some());
    assert!(find_button_by_label(&group_root, "Review execute").is_some());
    assert!(find_button_by_label(&group_root, "Copy execute").is_some());
    assert!(find_button_by_label(&group_root, "Copy review").is_some());
    assert!(text.iter().any(|value| value == "Effect"));
    assert!(text.iter().any(|value| value == "Zone color: Left side"));
    assert!(text.iter().any(|value| value == "Zone color: Right side"));
    assert!(text.iter().any(|value| value == "Brightness"));
    assert!(text.iter().any(|value| value == "Speed"));
    assert!(text.iter().any(|value| value == "RGB request preview"));
    assert!(find_button_by_label(&group_root, "Preview plan").is_some());
    assert!(find_button_by_label(&group_root, "Copy request JSON").is_some());
    assert!(text.iter().any(|value| value == "Apply RGB"));

    let effect = find_dropdown(&group_root).expect("RGB effect dropdown should render");
    assert!(
        effect.is_sensitive(),
        "RGB effect staging should be editable even while apply is gated"
    );
    assert_eq!(effect.selected(), 0);

    let apply = find_button_by_label(&group_root, "Apply").expect("RGB apply button should render");
    assert!(!apply.is_sensitive());
}

#[gtk4::test]
fn advanced_cpu_tuning_controls_start_hidden_until_enabled() {
    init_gtk();

    let profiles = ui::profiles::profiles_page(Ok(sample_diagnostics()));
    let root = profiles.clone().upcast::<gtk4::Widget>();

    let controls = find_preferences_group_by_title(&root, "Advanced CPU Tuning - Curve Optimizer")
        .expect("Curve Optimizer controls should exist in the page tree");
    assert!(
        find_button_by_label(&controls.clone().upcast(), "Reset to 0").is_some(),
        "Curve Optimizer controls should include an explicit reset-to-zero button"
    );
    assert!(
        !controls.is_visible(),
        "Curve Optimizer controls should be hidden until the advanced gate is enabled"
    );

    let gate = find_switch_in_action_row_by_title(&root, "Show Curve Optimizer controls")
        .expect("Advanced CPU tuning gate should have a switch");
    assert!(
        !gate.is_active(),
        "Advanced CPU tuning gate should default to disabled"
    );

    gate.set_active(true);
    assert!(
        controls.is_visible(),
        "Curve Optimizer controls should become visible when the advanced gate is enabled"
    );
}

#[gtk4::test]
fn expander_preselects_current_platform_profile() {
    init_gtk();

    // sample_diagnostics has platform_profile.current = "balanced"
    let profiles = ui::profiles::profiles_page(Ok(sample_diagnostics()));
    let root = profiles.clone().upcast::<gtk4::Widget>();

    let expander = find_expander_row_by_title(&root, "Requested profile");
    assert!(
        expander.is_some(),
        "Profiles page should have a Requested profile ExpanderRow"
    );
    let expander = expander.unwrap();
    assert!(
        expander.is_sensitive(),
        "ExpanderRow should be sensitive when choices are available"
    );
    assert_eq!(
        expander.subtitle().as_str(),
        "balanced",
        "Should pre-select balanced"
    );
}

#[gtk4::test]
fn expander_preselects_current_battery_charge_type() {
    init_gtk();

    // sample_diagnostics has battery_charge_type.current = "Standard"
    let battery = ui::battery::battery_page(Ok(sample_diagnostics()));
    let root = battery.clone().upcast::<gtk4::Widget>();

    let expander = find_expander_row_by_title(&root, "Requested charge type");
    assert!(
        expander.is_some(),
        "Battery page should have a Requested charge type ExpanderRow"
    );
    let expander = expander.unwrap();
    assert!(
        expander.is_sensitive(),
        "ExpanderRow should be sensitive when choices are available"
    );
    assert_eq!(
        expander.subtitle().as_str(),
        "Standard",
        "Should pre-select Standard"
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
    let gpu_text = collect_widget_text(&root);
    assert!(gpu_text.iter().any(|text| text == "Switch type"));
    assert!(gpu_text.iter().any(|text| text == "reboot-required"));
    assert!(gpu_text.iter().any(|text| text == "GPU switching status"));
    assert!(gpu_text
        .iter()
        .any(|text| text == "reboot_required_baseline"));
    assert!(gpu_text.iter().any(|text| text == "Runtime plan"));
    assert!(gpu_text.iter().any(|text| text == "blocked"));
    assert!(gpu_text
        .iter()
        .any(|text| text == "EnvyControl requires reboot on this fixture"));

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
        Capability {
            id: "cpu_power".to_owned(),
            label: "CPU power".to_owned(),
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
    report.cpu_power = Some(CpuPowerCapability {
        status: CapabilityStatus::ProbeOnly,
        scaling_driver: Some("amd-pstate-epp".to_owned()),
        amd_pstate_status: Some("active".to_owned()),
        governor: Some("powersave".to_owned()),
        available_governors: vec!["performance".to_owned(), "powersave".to_owned()],
        epp: Some("balance_performance".to_owned()),
        available_epp: vec![
            "default".to_owned(),
            "performance".to_owned(),
            "balance_performance".to_owned(),
            "balance_power".to_owned(),
            "power".to_owned(),
        ],
        boost: Some(true),
        scaling_min_khz: Some(400_000),
        scaling_max_khz: Some(5_150_000),
        scaling_cur_khz: Some(2_200_000),
        cpuinfo_min_khz: Some(400_000),
        cpuinfo_max_khz: Some(5_150_000),
        governor_path: "/tmp/fixture/sys/devices/system/cpu/cpufreq/policy0/scaling_governor"
            .to_owned(),
        epp_path:
            "/tmp/fixture/sys/devices/system/cpu/cpufreq/policy0/energy_performance_preference"
                .to_owned(),
        boost_path: "/tmp/fixture/sys/devices/system/cpu/cpufreq/boost".to_owned(),
    });
    report.firmware_attributes = vec![
        FirmwareAttributeCapability {
            name: "ppt_pl1_spl".to_owned(),
            current_value: Some("70".to_owned()),
            display_name: Some("SPL".to_owned()),
            path: "/tmp/fixture/sys/class/firmware-attributes/thinklmi/attributes/ppt_pl1_spl/current_value".to_owned(),
            attribute_type: Some("integer".to_owned()),
            default_value: Some("70".to_owned()),
            min_value: Some("50".to_owned()),
            max_value: Some("85".to_owned()),
            scalar_increment: Some("1".to_owned()),
        },
        FirmwareAttributeCapability {
            name: "ppt_pl2_sppt".to_owned(),
            current_value: Some("85".to_owned()),
            display_name: Some("SPPT".to_owned()),
            path: "/tmp/fixture/sys/class/firmware-attributes/thinklmi/attributes/ppt_pl2_sppt/current_value".to_owned(),
            attribute_type: Some("integer".to_owned()),
            default_value: Some("85".to_owned()),
            min_value: Some("60".to_owned()),
            max_value: Some("130".to_owned()),
            scalar_increment: Some("1".to_owned()),
        },
        FirmwareAttributeCapability {
            name: "ppt_pl3_fppt".to_owned(),
            current_value: Some("102".to_owned()),
            display_name: Some("FPPT".to_owned()),
            path: "/tmp/fixture/sys/class/firmware-attributes/thinklmi/attributes/ppt_pl3_fppt/current_value".to_owned(),
            attribute_type: Some("integer".to_owned()),
            default_value: Some("102".to_owned()),
            min_value: Some("70".to_owned()),
            max_value: Some("150".to_owned()),
            scalar_increment: Some("1".to_owned()),
        },
    ];
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
        switch_type: GpuSwitchType::RebootRequired,
        switch_notes: vec!["EnvyControl requires reboot on this fixture".to_owned()],
    });
    report.amd_gpu_power_dpm = Some(AmdGpuPowerDpmCapability {
        card: "card1".to_owned(),
        status: CapabilityStatus::ProbeOnly,
        vendor: "1002".to_owned(),
        force_performance_level_path:
            "/tmp/fixture/sys/class/drm/card1/device/power_dpm_force_performance_level".to_owned(),
        current_force_performance_level: Some("auto".to_owned()),
        power_dpm_state: Some("balanced".to_owned()),
        current_sclk: Some("0: 500Mhz *".to_owned()),
        current_mclk: Some("0: 96Mhz *".to_owned()),
        choices: vec!["auto".to_owned(), "low".to_owned()],
    });
    report.telemetry.battery = Some(BatteryTelemetry {
        name: "BAT0".to_owned(),
        path: "/tmp/fixture/sys/class/power_supply/BAT0".to_owned(),
        capacity_percent: Some(79),
        status: Some("Charging".to_owned()),
        health: Some("Good".to_owned()),
        power_now_uw: Some(30_405_000),
        cycle_count: Some(219),
        energy_full_uwh: None,
        energy_full_design_uwh: None,
        energy_now_uwh: None,
        voltage_now_uv: None,
        capacity_level: None,
        technology: None,
        model_name: None,
        manufacturer: None,
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
        IdeapadToggleCapability {
            name: "fan_mode".to_owned(),
            status: CapabilityStatus::ProbeOnly,
            path: Some(
                "/tmp/fixture/sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/fan_mode".to_owned(),
            ),
            current_value: Some("0".to_owned()),
        },
    ];
    report.keyboard_rgb_candidates = vec![
        KeyboardRgbCandidate {
            backend: "hidraw-research".to_owned(),
            device_id: "048D:C103".to_owned(),
            path: "/tmp/fixture/sys/class/hidraw/hidraw1".to_owned(),
            vendor_id: Some("048D".to_owned()),
            product_id: Some("C103".to_owned()),
            name: Some("ITE Device".to_owned()),
            modalias: None,
            report_descriptor_bytes: Some(156),
            report_ids: vec![1, 2, 3, 90],
            hid_reports: vec![KeyboardRgbHidReport {
                report_id: Some(90),
                kind: "feature".to_owned(),
                report_size_bits: 8,
                report_count: 16,
                bit_length: 128,
                byte_length: 16,
            }],
            evidence: vec![],
        },
        KeyboardRgbCandidate {
            backend: "hidraw-research".to_owned(),
            device_id: "048D:C985".to_owned(),
            path: "/tmp/fixture/sys/class/hidraw/hidraw2".to_owned(),
            vendor_id: Some("048D".to_owned()),
            product_id: Some("C985".to_owned()),
            name: Some("ITE Device".to_owned()),
            modalias: None,
            report_descriptor_bytes: Some(179),
            report_ids: vec![1, 5, 7, 90, 204],
            hid_reports: vec![KeyboardRgbHidReport {
                report_id: Some(90),
                kind: "feature".to_owned(),
                report_size_bits: 8,
                report_count: 16,
                bit_length: 128,
                byte_length: 16,
            }],
            evidence: vec![],
        },
    ];
    report.keyboard_rgb_openrgb = Some(KeyboardRgbOpenRgbStatus {
        installed: true,
        path: Some("/usr/bin/openrgb".to_owned()),
        devices: vec![KeyboardRgbOpenRgbDevice {
            index: 0,
            name: "Lenovo 5 2023".to_owned(),
            device_type: Some("Laptop".to_owned()),
            description: Some("Lenovo 4-Zone device".to_owned()),
            modes: vec![
                "Direct".to_owned(),
                "Breathing".to_owned(),
                "Rainbow Wave".to_owned(),
                "Spectrum Cycle".to_owned(),
            ],
            current_mode: Some("Direct".to_owned()),
            zones: vec!["Keyboard".to_owned()],
            leds: vec![
                "Left side".to_owned(),
                "Left center".to_owned(),
                "Right center".to_owned(),
                "Right side".to_owned(),
            ],
        }],
        i2c_dev_loaded: true,
        user_in_i2c_group: false,
        has_i2c_rw_access: false,
        has_hidraw_rw_access: true,
        backend_ready: false,
        write_support_claimed: false,
        sdk_helper_installed: true,
        sdk_helper_path: Some(
            "/home/test/.local/bin/ratvantage-openrgb-keyboard-rgb-sdk-helper".to_owned(),
        ),
        sdk_server_running: false,
        sdk_snapshot_supported: false,
        sdk_active_mode: None,
        sdk_color_zones: vec![],
        sdk_colors: std::collections::BTreeMap::new(),
    });

    DiagnosticsBundle::from_report_with_logs(
        report,
        Some("6.17.0-test".to_owned()),
        vec!["2026-04-25T17:44:00 legion-control-daemon started".to_owned()],
    )
    .with_runtime_state(DiagnosticsRuntimeState {
        gpu_mode_pending: Some(sample_gpu_pending()),
        last_known_good_fan_curve: Some(sample_fan_snapshot()),
        ..Default::default()
    })
}

fn sample_ryzen_backend_status() -> RyzenBackendStatus {
    RyzenBackendStatus {
        ryzenadj: RyzenAdjBackendStatus {
            path: "/usr/local/bin/ryzenadj".to_owned(),
            available: true,
            executable: true,
            supports_curve_optimizer: true,
            detail: "RyzenAdj command backend is available.".to_owned(),
        },
        ryzen_smu: RyzenSmuBackendStatus {
            module_loaded: false,
            sysfs_path: "/sys/kernel/ryzen_smu_drv".to_owned(),
            sysfs_available: false,
            pm_table_available: false,
            readback_available: false,
            detail: "ryzen_smu backend is not loaded.".to_owned(),
        },
        curve_optimizer_backend: "ryzenadj_write_only".to_owned(),
        curve_optimizer_readback_status: CurveOptimizerReadbackStatus::WriteOnly,
        setup_assistant: RyzenSmuSetupAssistant {
            recommended: true,
            reason:
                "Curve Optimizer writes are available through RyzenAdj, but read-back is missing."
                    .to_owned(),
            commands: vec![
                "git clone https://github.com/amkillam/ryzen_smu.git".to_owned(),
                "sudo modprobe ryzen_smu".to_owned(),
            ],
            notes: vec![
                "RatVantage does not install or load kernel modules automatically.".to_owned(),
            ],
        },
    }
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
    if let Ok(row) = widget.clone().downcast::<adw::ExpanderRow>() {
        let title = row.title().to_string();
        if !title.is_empty() {
            text.push(title);
        }
        let subtitle = row.subtitle().to_string();
        if !subtitle.is_empty() {
            text.push(subtitle);
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

fn count_action_rows_by_title(root: &gtk4::Widget, title: &str) -> usize {
    let current_match = root
        .clone()
        .downcast::<adw::ActionRow>()
        .map(|row| usize::from(row.title() == title))
        .unwrap_or(0);

    let mut count = current_match;
    let mut child = root.first_child();
    while let Some(current) = child {
        count += count_action_rows_by_title(&current, title);
        child = current.next_sibling();
    }
    count
}

fn find_expander_row_by_title(root: &gtk4::Widget, title: &str) -> Option<adw::ExpanderRow> {
    if let Ok(row) = root.clone().downcast::<adw::ExpanderRow>() {
        if row.title() == title {
            return Some(row);
        }
    }

    let mut child = root.first_child();
    while let Some(current) = child {
        if let Some(row) = find_expander_row_by_title(&current, title) {
            return Some(row);
        }
        child = current.next_sibling();
    }

    None
}

fn find_spin_button(root: &gtk4::Widget) -> Option<gtk4::SpinButton> {
    if let Ok(spin) = root.clone().downcast::<gtk4::SpinButton>() {
        return Some(spin);
    }

    let mut child = root.first_child();
    while let Some(current) = child {
        if let Some(spin) = find_spin_button(&current) {
            return Some(spin);
        }
        child = current.next_sibling();
    }

    None
}

fn find_preferences_group_by_title(
    root: &gtk4::Widget,
    title: &str,
) -> Option<adw::PreferencesGroup> {
    if let Ok(group) = root.clone().downcast::<adw::PreferencesGroup>() {
        if group.title() == title {
            return Some(group);
        }
    }

    let mut child = root.first_child();
    while let Some(current) = child {
        if let Some(group) = find_preferences_group_by_title(&current, title) {
            return Some(group);
        }
        child = current.next_sibling();
    }

    None
}

fn find_switch_in_action_row_by_title(root: &gtk4::Widget, title: &str) -> Option<gtk4::Switch> {
    find_action_row_by_title(root, title).and_then(|row| find_switch(&row.upcast()))
}

fn find_check_button_in_action_row_by_title(
    root: &gtk4::Widget,
    title: &str,
) -> Option<gtk4::CheckButton> {
    find_action_row_by_title(root, title).and_then(|row| find_check_button(&row.upcast()))
}

fn find_check_button(root: &gtk4::Widget) -> Option<gtk4::CheckButton> {
    if let Ok(check) = root.clone().downcast::<gtk4::CheckButton>() {
        return Some(check);
    }

    let mut child = root.first_child();
    while let Some(current) = child {
        if let Some(check) = find_check_button(&current) {
            return Some(check);
        }
        child = current.next_sibling();
    }

    None
}

fn find_switch(root: &gtk4::Widget) -> Option<gtk4::Switch> {
    if let Ok(switch) = root.clone().downcast::<gtk4::Switch>() {
        return Some(switch);
    }

    let mut child = root.first_child();
    while let Some(current) = child {
        if let Some(switch) = find_switch(&current) {
            return Some(switch);
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
