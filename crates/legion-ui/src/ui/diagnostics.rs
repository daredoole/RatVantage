use crate::{render_diagnostics_json, DiagnosticsBundle};
use adw::prelude::*;
use anyhow::Result;
use legion_common::{CurveOptimizerReadbackStatus, RyzenBackendStatus};

use super::shared::{append_error, info_row};

const COMPATIBILITY_BUNDLE_COMMAND: &str =
    "ratvantage-capture-compatibility-bundle --output target/validation/compatibility-bundle";
const AUTOMATION_DIAGNOSTICS_COMMAND: &str = "legion-control-ui --automation-diagnostics";
const RESET_DIAGNOSTICS_COMMAND: &str = "legion-control-ui --reset-diagnostics";
const RYZEN_BACKEND_STATUS_COMMAND: &str = "legion-control-ui --ryzen-backend-status";
const RYZEN_SMU_SETUP_COMMAND: &str = "legion-control-ui --ryzen-smu-setup";

pub fn diagnostics_page(diagnostics: Result<DiagnosticsBundle>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    match diagnostics {
        Ok(bundle) => append_diagnostics(&page, &bundle),
        Err(error) => append_error(&page, &error),
    }

    page
}

fn append_diagnostics(page: &adw::PreferencesPage, bundle: &DiagnosticsBundle) {
    let group = adw::PreferencesGroup::new();
    group.add(&info_row(
        "Vendor",
        bundle.hardware.vendor.as_deref().unwrap_or("unknown"),
    ));
    group.add(&info_row(
        "Product",
        bundle.hardware.product_name.as_deref().unwrap_or("unknown"),
    ));
    group.add(&info_row(
        "Kernel",
        bundle.kernel_version.as_deref().unwrap_or("unknown"),
    ));
    group.add(&info_row(
        "Detected sysfs paths",
        &bundle.detected_sysfs_paths.len().to_string(),
    ));
    group.add(&info_row(
        "Capabilities",
        &format!(
            "{} available, {} missing",
            bundle.summary.available_capability_count, bundle.summary.missing_capability_count
        ),
    ));
    group.add(&info_row(
        "Sensors",
        &bundle.summary.sensor_count.to_string(),
    ));
    group.add(&info_row(
        "Fan curves",
        &bundle.summary.fan_curve_count.to_string(),
    ));
    group.add(&info_row(
        "Daemon log lines",
        &bundle.recent_daemon_logs.len().to_string(),
    ));
    group.add(&info_row(
        "Hardware profile drift",
        &hardware_profile_drift_summary(bundle),
    ));
    group.add(&info_row(
        "Fan curve drift",
        &fan_curve_drift_summary(bundle),
    ));
    page.add(&group);

    let json = render_diagnostics_json(bundle).unwrap_or_else(|error| error.to_string());

    let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let copy = gtk4::Button::with_label("Copy JSON");
    copy.set_tooltip_text(Some("Copy diagnostics JSON"));
    let json_for_clipboard = json.clone();
    copy.connect_clicked(move |_| {
        if let Some(display) = gtk4::gdk::Display::default() {
            display.clipboard().set_text(&json_for_clipboard);
        }
    });
    actions.append(&copy);

    let actions_group = adw::PreferencesGroup::new();
    actions_group.add(&actions);
    page.add(&actions_group);

    let compatibility = adw::PreferencesGroup::new();
    compatibility.set_title("Compatibility Bundle");
    let bundle_row = adw::ActionRow::builder()
        .title("Read-only support bundle")
        .subtitle(format!(
            "{}; captures overview, diagnostics, automation/reset snapshots, probe JSON, OpenRGB readiness, and RGB bridge evidence status",
            COMPATIBILITY_BUNDLE_COMMAND
        ))
        .selectable(false)
        .build();
    let copy_bundle = gtk4::Button::with_label("Copy bundle command");
    copy_bundle.set_tooltip_text(Some(
        "Copy the read-only compatibility bundle command for hardware support reports.",
    ));
    copy_bundle.add_css_class("pill");
    copy_bundle.set_valign(gtk4::Align::Center);
    copy_bundle.connect_clicked(move |_| {
        if let Some(display) = gtk4::gdk::Display::default() {
            display.clipboard().set_text(COMPATIBILITY_BUNDLE_COMMAND);
        }
    });
    bundle_row.add_suffix(&copy_bundle);
    compatibility.add(&bundle_row);
    page.add(&compatibility);

    let automations = adw::PreferencesGroup::new();
    automations.set_title("Automation Diagnostics");
    let automation_row = adw::ActionRow::builder()
        .title("Read-only automation snapshot")
        .subtitle(format!(
            "{}; captures hardware profiles, trigger mappings, automation rules, and last apply runs without executing actions",
            AUTOMATION_DIAGNOSTICS_COMMAND
        ))
        .selectable(false)
        .build();
    let copy_automation = gtk4::Button::with_label("Copy automation command");
    copy_automation.set_tooltip_text(Some(
        "Copy the read-only automation diagnostics command for profile and rule support.",
    ));
    copy_automation.add_css_class("pill");
    copy_automation.set_valign(gtk4::Align::Center);
    copy_automation.connect_clicked(move |_| {
        if let Some(display) = gtk4::gdk::Display::default() {
            display.clipboard().set_text(AUTOMATION_DIAGNOSTICS_COMMAND);
        }
    });
    automation_row.add_suffix(&copy_automation);
    automations.add(&automation_row);
    page.add(&automations);

    let resets = adw::PreferencesGroup::new();
    resets.set_title("Reset Diagnostics");
    let reset_row = adw::ActionRow::builder()
        .title("Read-only reset snapshot")
        .subtitle(format!(
            "{}; plans Curve Optimizer reset, firmware PPT defaults, RGB SDK recovery, GPU switching recovery guidance, restore-auto-fan, and custom-thermal restore-auto-fan without applying changes",
            RESET_DIAGNOSTICS_COMMAND
        ))
        .selectable(false)
        .build();
    let copy_reset = gtk4::Button::with_label("Copy reset command");
    copy_reset.set_tooltip_text(Some(
        "Copy the read-only reset diagnostics command for risky tuning families.",
    ));
    copy_reset.add_css_class("pill");
    copy_reset.set_valign(gtk4::Align::Center);
    copy_reset.connect_clicked(move |_| {
        if let Some(display) = gtk4::gdk::Display::default() {
            display.clipboard().set_text(RESET_DIAGNOSTICS_COMMAND);
        }
    });
    reset_row.add_suffix(&copy_reset);
    resets.add(&reset_row);
    page.add(&resets);

    if let Some(status) = &bundle.ryzen_backend_status {
        append_ryzen_backend_setup(page, status);
    }

    let text = gtk4::TextView::new();
    text.set_editable(false);
    text.set_cursor_visible(false);
    text.set_monospace(true);
    text.set_wrap_mode(gtk4::WrapMode::WordChar);
    text.buffer().set_text(&json);

    let scroller = gtk4::ScrolledWindow::builder()
        .min_content_height(220)
        .vexpand(true)
        .child(&text)
        .build();

    let json_group = adw::PreferencesGroup::new();
    json_group.add(&scroller);
    page.add(&json_group);
}

fn append_ryzen_backend_setup(page: &adw::PreferencesPage, status: &RyzenBackendStatus) {
    let group = adw::PreferencesGroup::new();
    group.set_title("Ryzen Backend Setup");
    group.add(&info_row(
        "Curve Optimizer backend",
        &status.curve_optimizer_backend,
    ));
    group.add(&info_row(
        "Curve Optimizer read-back",
        match status.curve_optimizer_readback_status {
            CurveOptimizerReadbackStatus::WriteOnly => "write-only",
            CurveOptimizerReadbackStatus::Verified => "available",
            CurveOptimizerReadbackStatus::Failed => "failed",
        },
    ));
    group.add(&info_row(
        "ryzen_smu setup",
        if status.setup_assistant.recommended {
            "recommended"
        } else {
            "optional"
        },
    ));

    let status_row = adw::ActionRow::builder()
        .title("Read-only backend status")
        .subtitle(format!(
            "{}; reports RyzenAdj, ryzen_smu, and Curve Optimizer read-back state",
            RYZEN_BACKEND_STATUS_COMMAND
        ))
        .selectable(false)
        .build();
    let copy_status = gtk4::Button::with_label("Copy Ryzen status command");
    copy_status.set_tooltip_text(Some("Copy the read-only Ryzen backend status command."));
    copy_status.add_css_class("pill");
    copy_status.set_valign(gtk4::Align::Center);
    copy_status.connect_clicked(move |_| {
        if let Some(display) = gtk4::gdk::Display::default() {
            display.clipboard().set_text(RYZEN_BACKEND_STATUS_COMMAND);
        }
    });
    status_row.add_suffix(&copy_status);
    group.add(&status_row);

    let setup_row = adw::ActionRow::builder()
        .title("ryzen_smu setup assistant")
        .subtitle(format!(
            "{}; prints review-first setup commands only, without installing or loading modules",
            RYZEN_SMU_SETUP_COMMAND
        ))
        .selectable(false)
        .build();
    let copy_setup = gtk4::Button::with_label("Copy Ryzen setup command");
    copy_setup.set_tooltip_text(Some(
        "Copy the read-only ryzen_smu setup assistant command.",
    ));
    copy_setup.add_css_class("pill");
    copy_setup.set_valign(gtk4::Align::Center);
    copy_setup.connect_clicked(move |_| {
        if let Some(display) = gtk4::gdk::Display::default() {
            display.clipboard().set_text(RYZEN_SMU_SETUP_COMMAND);
        }
    });
    setup_row.add_suffix(&copy_setup);
    group.add(&setup_row);

    group.add(&info_row("Setup reason", &status.setup_assistant.reason));
    page.add(&group);
}

fn hardware_profile_drift_summary(bundle: &DiagnosticsBundle) -> String {
    let drift = &bundle.hardware_profile_drift;
    match drift.status.as_str() {
        "no_last_apply" => "No hardware profile apply recorded".to_owned(),
        "last_apply_incomplete" => format!(
            "Last apply for {} did not complete",
            drift.profile_id.as_deref().unwrap_or("unknown profile")
        ),
        "in_sync" => format!("{} checked, no drift", drift.checked_count),
        "drifted" => format!(
            "{} drifted of {} checked",
            drift.drifted_count, drift.checked_count
        ),
        "no_comparable_actions" => "No comparable applied actions".to_owned(),
        other => other.replace('_', " "),
    }
}

fn fan_curve_drift_summary(bundle: &DiagnosticsBundle) -> String {
    let drift = &bundle.fan_curve_drift;
    match drift.status.as_str() {
        "no_saved_snapshot" => "No saved fan curve snapshot".to_owned(),
        "missing_live_readings" => "Live fan curve readings unavailable".to_owned(),
        "in_sync" => format!("{} checked, no drift", drift.checked_count),
        "drifted" => format!(
            "{} drifted of {} checked",
            drift.drifted_count, drift.checked_count
        ),
        other => other.replace('_', " "),
    }
}
