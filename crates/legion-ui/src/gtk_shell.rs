use crate::{render_diagnostics_json, DiagnosticsBundle, LegionControlClient, UiStatus};
use adw::prelude::*;
use anyhow::Result;

pub fn run() -> Result<()> {
    let app = adw::Application::builder()
        .application_id("org.ratvantage.LegionControl")
        .build();

    app.connect_activate(|app| {
        let status = LegionControlClient::system().and_then(|client| client.status());
        let diagnostics =
            LegionControlClient::system().and_then(|client| client.diagnostics_bundle());
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Legion Control")
            .default_width(720)
            .default_height(480)
            .build();

        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.append(&adw::HeaderBar::new());
        root.append(&dashboard_page(status, diagnostics));
        window.set_content(Some(&root));
        window.present();
    });

    app.run();
    Ok(())
}

pub fn dashboard_page(
    status: Result<UiStatus>,
    diagnostics: Result<DiagnosticsBundle>,
) -> gtk4::Widget {
    let stack = gtk4::Stack::new();
    stack.set_vexpand(true);
    stack.add_titled(&status_page(status), Some("status"), "Status");
    stack.add_titled(
        &diagnostics_page(diagnostics),
        Some("diagnostics"),
        "Diagnostics",
    );

    let switcher = gtk4::StackSwitcher::new();
    switcher.set_stack(Some(&stack));
    switcher.set_halign(gtk4::Align::Start);
    switcher.set_margin_top(12);
    switcher.set_margin_start(24);
    switcher.set_margin_end(24);

    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    page.append(&switcher);
    page.append(&stack);
    page.upcast()
}

pub fn status_page(status: Result<UiStatus>) -> gtk4::Widget {
    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(24);
    page.set_margin_end(24);

    match status {
        Ok(status) => append_status(&page, &status),
        Err(error) => append_error(&page, &error),
    }

    page.upcast()
}

pub fn diagnostics_page(diagnostics: Result<DiagnosticsBundle>) -> gtk4::Widget {
    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(24);
    page.set_margin_end(24);

    match diagnostics {
        Ok(bundle) => append_diagnostics(&page, &bundle),
        Err(error) => append_error(&page, &error),
    }

    page.upcast()
}

fn append_status(page: &gtk4::Box, status: &UiStatus) {
    let title = gtk4::Label::new(Some("Detected Hardware"));
    title.add_css_class("title-2");
    title.set_xalign(0.0);
    page.append(&title);

    let group = adw::PreferencesGroup::new();
    group.add(&info_row("Vendor", &status.hardware.vendor));
    group.add(&info_row("Product", &status.hardware.product_name));
    group.add(&info_row("Version", &status.hardware.product_version));
    if let Some(sku) = &status.hardware.product_sku {
        group.add(&info_row("SKU", sku));
    }
    group.add(&info_row(
        "Capabilities",
        &status.capability_count().to_string(),
    ));
    page.append(&group);

    let capabilities = adw::PreferencesGroup::new();
    capabilities.set_title("Read-only Capabilities");
    for capability in &status.capabilities {
        capabilities.add(&info_row(&capability.label, &capability.id));
    }
    page.append(&capabilities);
}

fn append_diagnostics(page: &gtk4::Box, bundle: &DiagnosticsBundle) {
    let title = gtk4::Label::new(Some("Diagnostics"));
    title.add_css_class("title-2");
    title.set_xalign(0.0);
    page.append(&title);

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
        "Raw capabilities",
        &bundle.raw_probe_report.capabilities.len().to_string(),
    ));
    group.add(&info_row(
        "Daemon log lines",
        &bundle.recent_daemon_logs.len().to_string(),
    ));
    page.append(&group);

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
    page.append(&actions);

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
    page.append(&scroller);
}

fn append_error(page: &gtk4::Box, error: &anyhow::Error) {
    let title = gtk4::Label::new(Some("Daemon unavailable"));
    title.add_css_class("title-2");
    title.set_xalign(0.0);
    page.append(&title);

    let message = gtk4::Label::new(Some(&error.to_string()));
    message.set_wrap(true);
    message.set_xalign(0.0);
    page.append(&message);
}

fn info_row(title: &str, value: &str) -> adw::ActionRow {
    adw::ActionRow::builder()
        .title(title)
        .subtitle(value)
        .selectable(false)
        .build()
}
