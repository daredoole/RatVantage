use crate::DiagnosticsBundle;
use adw::prelude::*;
use anyhow::Result;
use legion_common::CapabilityStatus;

use super::shared::{append_error, info_row, request_dashboard_refresh};

pub fn settings_page(diagnostics: Result<DiagnosticsBundle>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    // ── Appearance ───────────────────────────────────────────────────────────
    let appearance_group = adw::PreferencesGroup::new();
    appearance_group.set_title("Appearance");
    appearance_group.set_description(Some(
        "Theme follows the system Adwaita preference. \
         Accent color and row density persistence are planned for a future release.",
    ));

    let scheme_row = adw::ActionRow::builder()
        .title("Colour scheme")
        .subtitle("Follows the system GNOME/Adwaita dark/light preference via GTK portal")
        .selectable(false)
        .build();
    let style_manager = adw::StyleManager::default();
    let scheme_text = match style_manager.color_scheme() {
        adw::ColorScheme::ForceDark | adw::ColorScheme::PreferDark => "Dark",
        adw::ColorScheme::ForceLight | adw::ColorScheme::PreferLight => "Light",
        _ => "System default",
    };
    let scheme_label = gtk4::Label::new(Some(scheme_text));
    scheme_label.add_css_class("dim-label");
    scheme_label.add_css_class("caption");
    scheme_row.add_suffix(&scheme_label);
    appearance_group.add(&scheme_row);

    page.add(&appearance_group);

    // ── Notifications ────────────────────────────────────────────────────────
    let notif_group = adw::PreferencesGroup::new();
    notif_group.set_title("Notifications");
    notif_group.set_description(Some(
        "Desktop notifications via the system notification daemon. \
         Requires an active daemon D-Bus connection.",
    ));

    let notif_rows: &[(&str, &str, bool)] = &[
        (
            "Notify on successful write",
            "Desktop notification when a hardware change is applied and read-back confirms",
            true,
        ),
        (
            "Notify on external profile drift",
            "When another app changes the platform profile and RatVantage detects it",
            true,
        ),
        (
            "Notify on write rollback",
            "When a write fails read-back validation and the daemon rolls back the value",
            true,
        ),
    ];

    for (title, subtitle, default_on) in notif_rows {
        let row = adw::ActionRow::builder()
            .title(*title)
            .subtitle(*subtitle)
            .selectable(false)
            .build();
        let switch = gtk4::Switch::new();
        switch.set_active(*default_on);
        switch.set_valign(gtk4::Align::Center);
        row.add_suffix(&switch);
        notif_group.add(&row);
    }

    page.add(&notif_group);

    // ── Telemetry refresh ────────────────────────────────────────────────────
    let telemetry_group = adw::PreferencesGroup::new();
    telemetry_group.set_title("Telemetry Refresh");

    let auto_row = adw::ActionRow::builder()
        .title("Auto-refresh while window is visible")
        .subtitle("Re-polls fan RPM, temperature, and battery telemetry on a timer")
        .selectable(false)
        .build();
    let auto_switch = gtk4::Switch::new();
    auto_switch.set_active(true);
    auto_switch.set_valign(gtk4::Align::Center);
    auto_row.add_suffix(&auto_switch);
    telemetry_group.add(&auto_row);

    let interval_row = adw::ActionRow::builder()
        .title("Refresh interval")
        .subtitle(
            "Active window: 30 s · Background tick: 90 s · \
             Configurable interval ships in a future release",
        )
        .selectable(false)
        .build();
    let interval_label = gtk4::Label::new(Some("30 s"));
    interval_label.add_css_class("dim-label");
    interval_label.add_css_class("caption");
    interval_row.add_suffix(&interval_label);
    telemetry_group.add(&interval_row);

    page.add(&telemetry_group);

    // ── Startup & tray ───────────────────────────────────────────────────────
    let startup_group = adw::PreferencesGroup::new();
    startup_group.set_title("Startup and Tray");
    startup_group.set_description(Some(
        "Autostart and tray behavior. Packaging and .desktop integration are planned.",
    ));

    let startup_rows: &[(&str, &str, bool)] = &[
        (
            "Start to tray on login (autostart)",
            "Requires the autostart .desktop entry to be installed — packaging placeholder",
            false,
        ),
        (
            "Minimise to tray on window close",
            "Keeps the daemon connection alive and the tray icon visible",
            true,
        ),
        (
            "Start minimized",
            "Open to tray instead of showing the dashboard window on login",
            false,
        ),
    ];

    for (title, subtitle, default_on) in startup_rows {
        let row = adw::ActionRow::builder()
            .title(*title)
            .subtitle(*subtitle)
            .selectable(false)
            .build();
        let switch = gtk4::Switch::new();
        switch.set_active(*default_on);
        switch.set_valign(gtk4::Align::Center);
        row.add_suffix(&switch);
        startup_group.add(&row);
    }

    page.add(&startup_group);

    // ── Write surfaces + daemon connection ───────────────────────────────────
    match diagnostics {
        Ok(bundle) => {
            append_write_surfaces(&page, &bundle);
            append_daemon_connection(&page, &bundle);
        }
        Err(error) => append_error(&page, &error),
    }

    page
}

fn append_write_surfaces(page: &adw::PreferencesPage, bundle: &DiagnosticsBundle) {
    let group = adw::PreferencesGroup::new();
    group.set_title("Enabled Write Surfaces");
    group.set_description(Some(
        "Which hardware controls are exposed by the running daemon. \
         To change this, restart the daemon with the appropriate --enable-*-write flags.",
    ));

    let surfaces: &[(&str, &str, &str)] = &[
        (
            "platform_profile",
            "Platform profile",
            "Quiet / Balanced / Performance — polkit-gated, read-back validated",
        ),
        (
            "battery_charge_type",
            "Battery charge type",
            "Standard / Conservation / Express — polkit-gated, read-back validated",
        ),
        (
            "ylogo_led",
            "Y-logo LED",
            "On/off + brightness — polkit-gated, read-back validated",
        ),
        (
            "fn_lock",
            "Fn lock",
            "Requires platform::fnlock LED corroboration — off by default",
        ),
        (
            "camera_power",
            "Camera power",
            "Dashboard-confirmed only — not exposed as one-click tray action",
        ),
        (
            "usb_charging",
            "USB charging",
            "Dashboard-confirmed only — charges peripherals with lid closed",
        ),
        (
            "gpu_mode",
            "GPU mode",
            "Probe-only; execution disabled pending recovery-guide gate",
        ),
        (
            "fan_curves",
            "Fan curves",
            "Probe-only PWM snapshot; write planning only",
        ),
    ];

    for (cap_id, cap_label, note) in surfaces {
        let detected = bundle
            .raw_probe_report
            .capabilities
            .iter()
            .any(|c| c.id == *cap_id && c.status != CapabilityStatus::Missing);

        let status_label = gtk4::Label::new(Some(if detected { "detected" } else { "missing" }));
        if detected {
            status_label.add_css_class("success");
        } else {
            status_label.add_css_class("dim-label");
        }
        status_label.add_css_class("caption");

        let row = adw::ActionRow::builder()
            .title(*cap_label)
            .subtitle(*note)
            .selectable(false)
            .build();
        row.add_suffix(&status_label);
        group.add(&row);
    }

    page.add(&group);
}

fn append_daemon_connection(page: &adw::PreferencesPage, bundle: &DiagnosticsBundle) {
    let group = adw::PreferencesGroup::new();
    group.set_title("Daemon Connection");

    group.add(&info_row("D-Bus name", "org.ratvantage.LegionControl1"));
    group.add(&info_row("Bus", "system"));
    group.add(&info_row("polkit", "active"));

    if let Some(kernel) = &bundle.kernel_version {
        group.add(&info_row("Kernel", kernel));
    }

    group.add(&info_row(
        "Capabilities detected",
        &format!(
            "{} available · {} probe-only · {} missing",
            bundle.summary.available_capability_count,
            bundle
                .summary
                .capability_status_counts
                .get("probe_only")
                .copied()
                .unwrap_or(0),
            bundle.summary.missing_capability_count,
        ),
    ));

    let reconnect_row = adw::ActionRow::builder()
        .title("Reconnect")
        .subtitle("Trigger a fresh D-Bus connection and capability re-probe")
        .selectable(false)
        .build();
    let reconnect_btn = gtk4::Button::builder()
        .label("Reconnect")
        .css_classes(["pill"])
        .valign(gtk4::Align::Center)
        .build();
    reconnect_btn.connect_clicked(|_| {
        let _ = request_dashboard_refresh();
    });
    reconnect_row.add_suffix(&reconnect_btn);
    group.add(&reconnect_row);

    page.add(&group);
}
