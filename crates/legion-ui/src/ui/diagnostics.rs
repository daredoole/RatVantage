use crate::{render_diagnostics_json, DiagnosticsBundle};
use adw::prelude::*;
use anyhow::Result;

use super::shared::{append_error, info_row};

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
