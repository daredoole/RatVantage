use crate::{
    capability_status_label, render_diagnostics_json, risk_level_label, DiagnosticsBundle,
    LegionControlClient, RuntimeSnapshot, UiStatus,
};
use adw::prelude::*;
use anyhow::{anyhow, Result};
use legion_common::{
    BatteryChargeTypeCapability, FanCurveSnapshot, GpuModePending, LedCapability,
    PlatformProfileCapability, WriteExecutionResult, WriteExecutionStatus,
};

pub fn run() -> Result<()> {
    let app = adw::Application::builder()
        .application_id("org.ratvantage.LegionControl")
        .build();

    app.connect_activate(|app| {
        let snapshot =
            LegionControlClient::system().and_then(|client| client.refresh_runtime_snapshot());
        let status = snapshot
            .as_ref()
            .map(|snapshot| snapshot.status.clone())
            .map_err(clone_error);
        let diagnostics = snapshot
            .as_ref()
            .map(|snapshot| snapshot.diagnostics.clone())
            .map_err(clone_error);
        let gpu_pending = runtime_snapshot_gpu_pending(&snapshot);
        let fan_snapshot = runtime_snapshot_fan_snapshot(&snapshot);
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Legion Control")
            .default_width(720)
            .default_height(480)
            .build();

        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.append(&adw::HeaderBar::new());
        root.append(&dashboard_page(
            status,
            diagnostics,
            gpu_pending,
            fan_snapshot,
        ));
        window.set_content(Some(&root));
        window.present();
    });

    app.run();
    Ok(())
}

pub fn dashboard_page(
    status: Result<UiStatus>,
    diagnostics: Result<DiagnosticsBundle>,
    gpu_pending: Result<Option<GpuModePending>>,
    fan_snapshot: Result<Option<FanCurveSnapshot>>,
) -> gtk4::Widget {
    let stack = gtk4::Stack::new();
    stack.set_vexpand(true);
    stack.add_titled(&status_page(status, gpu_pending), Some("status"), "Status");
    stack.add_titled(
        &profiles_page(clone_result(&diagnostics)),
        Some("profiles"),
        "Profiles",
    );
    stack.add_titled(
        &battery_page(clone_result(&diagnostics)),
        Some("battery"),
        "Battery",
    );
    stack.add_titled(
        &fans_page(clone_result(&diagnostics), fan_snapshot),
        Some("fans"),
        "Fans",
    );
    stack.add_titled(
        &appearance_page(clone_result(&diagnostics)),
        Some("appearance"),
        "Appearance",
    );
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

pub fn status_page(
    status: Result<UiStatus>,
    gpu_pending: Result<Option<GpuModePending>>,
) -> gtk4::Widget {
    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(24);
    page.set_margin_end(24);

    match status {
        Ok(status) => append_status(&page, &status, gpu_pending),
        Err(error) => append_error(&page, &error),
    }

    page.upcast()
}

pub fn profiles_page(diagnostics: Result<DiagnosticsBundle>) -> gtk4::Widget {
    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(24);
    page.set_margin_end(24);

    match diagnostics {
        Ok(bundle) => append_profiles(&page, &bundle),
        Err(error) => append_error(&page, &error),
    }

    page.upcast()
}

pub fn battery_page(diagnostics: Result<DiagnosticsBundle>) -> gtk4::Widget {
    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(24);
    page.set_margin_end(24);

    match diagnostics {
        Ok(bundle) => append_battery(&page, &bundle),
        Err(error) => append_error(&page, &error),
    }

    page.upcast()
}

pub fn fans_page(
    diagnostics: Result<DiagnosticsBundle>,
    fan_snapshot: Result<Option<FanCurveSnapshot>>,
) -> gtk4::Widget {
    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(24);
    page.set_margin_end(24);

    match diagnostics {
        Ok(bundle) => append_fans(&page, &bundle, fan_snapshot),
        Err(error) => append_error(&page, &error),
    }

    page.upcast()
}

pub fn appearance_page(diagnostics: Result<DiagnosticsBundle>) -> gtk4::Widget {
    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(24);
    page.set_margin_end(24);

    match diagnostics {
        Ok(bundle) => append_appearance(&page, &bundle),
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

fn append_status(page: &gtk4::Box, status: &UiStatus, gpu_pending: Result<Option<GpuModePending>>) {
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
    group.add(&info_row(
        "GPU pending reboot",
        &render_gpu_pending_row(gpu_pending),
    ));
    page.append(&group);

    let capabilities = adw::PreferencesGroup::new();
    capabilities.set_title("Read-only Capabilities");
    for capability in &status.capabilities {
        capabilities.add(&info_row(
            &capability.label,
            &format!(
                "{} - {} - {}",
                capability.id,
                capability_status_label(capability.status),
                risk_level_label(capability.risk)
            ),
        ));
    }
    page.append(&capabilities);
}

fn append_profiles(page: &gtk4::Box, bundle: &DiagnosticsBundle) {
    let title = gtk4::Label::new(Some("Profiles"));
    title.add_css_class("title-2");
    title.set_xalign(0.0);
    page.append(&title);

    let group = adw::PreferencesGroup::new();
    group.set_title("Platform Profile");
    if let Some(profile) = &bundle.raw_probe_report.platform_profile {
        let current = info_row("Current", profile.current.as_deref().unwrap_or("unknown"));
        group.add(&current);
        group.add(&info_row("Choices", &profile.choices.join(", ")));
        group.add(&info_row("Profile path", &profile.path));
        group.add(&info_row("Choices path", &profile.choices_path));
        page.append(&group);

        let controls = build_platform_profile_controls(
            bundle.raw_probe_report.platform_profile.as_ref(),
            Some(current),
        );
        page.append(&controls);
    } else {
        group.add(&info_row("Platform profile", "unavailable"));
        page.append(&group);

        let controls = build_platform_profile_controls(None, None);
        page.append(&controls);
    }

    let feedback = build_write_feedback_group("Platform profile");
    page.append(&feedback);
}

fn append_battery(page: &gtk4::Box, bundle: &DiagnosticsBundle) {
    let title = gtk4::Label::new(Some("Battery"));
    title.add_css_class("title-2");
    title.set_xalign(0.0);
    page.append(&title);

    let charge_type = adw::PreferencesGroup::new();
    charge_type.set_title("Charge Type");
    if let Some(charge_type_capability) = &bundle.raw_probe_report.battery_charge_type {
        let current = info_row(
            "Current",
            charge_type_capability
                .current
                .as_deref()
                .unwrap_or("unknown"),
        );
        charge_type.add(&current);
        charge_type.add(&info_row(
            "Choices",
            &charge_type_capability.choices.join(", "),
        ));
        charge_type.add(&info_row("Status path", &charge_type_capability.path));
        charge_type.add(&info_row(
            "Choices path",
            &charge_type_capability.choices_path,
        ));
        page.append(&charge_type);

        let controls = build_battery_charge_type_controls(
            bundle.raw_probe_report.battery_charge_type.as_ref(),
            Some(current),
        );
        page.append(&controls);
    } else {
        charge_type.add(&info_row("Charge type", "unavailable"));
        page.append(&charge_type);

        let controls = build_battery_charge_type_controls(None, None);
        page.append(&controls);
    }

    let feedback = build_write_feedback_group("Battery charge type");
    page.append(&feedback);

    let telemetry = adw::PreferencesGroup::new();
    telemetry.set_title("Telemetry");
    if let Some(battery) = &bundle.raw_probe_report.telemetry.battery {
        telemetry.add(&info_row("Name", &battery.name));
        telemetry.add(&info_row(
            "Capacity",
            &battery
                .capacity_percent
                .map(|capacity| format!("{capacity}%"))
                .unwrap_or_else(|| "unknown".to_owned()),
        ));
        telemetry.add(&info_row(
            "Status",
            battery.status.as_deref().unwrap_or("unknown"),
        ));
        telemetry.add(&info_row(
            "Health",
            battery.health.as_deref().unwrap_or("unknown"),
        ));
        telemetry.add(&info_row("Path", &battery.path));
    } else {
        telemetry.add(&info_row("Battery telemetry", "unavailable"));
    }
    page.append(&telemetry);
}

fn append_fans(
    page: &gtk4::Box,
    bundle: &DiagnosticsBundle,
    fan_snapshot: Result<Option<FanCurveSnapshot>>,
) {
    let title = gtk4::Label::new(Some("Fans"));
    title.add_css_class("title-2");
    title.set_xalign(0.0);
    page.append(&title);

    let telemetry = adw::PreferencesGroup::new();
    telemetry.set_title("Telemetry");
    let fan_sensors = bundle
        .raw_probe_report
        .telemetry
        .sensors
        .iter()
        .filter(|sensor| sensor.kind == "fan")
        .collect::<Vec<_>>();
    if fan_sensors.is_empty() {
        telemetry.add(&info_row("Fan telemetry", "unavailable"));
    } else {
        for sensor in fan_sensors {
            let title = sensor.label.as_deref().unwrap_or("Fan");
            let value = sensor
                .value
                .map(|value| format!("{value} RPM"))
                .unwrap_or_else(|| "unknown".to_owned());
            telemetry.add(&info_row(title, &value));
        }
    }
    page.append(&telemetry);

    let curves = adw::PreferencesGroup::new();
    curves.set_title("Fan Curves");
    if bundle.raw_probe_report.fan_curves.is_empty() {
        curves.add(&info_row("Fan curves", "unavailable"));
    } else {
        for curve in &bundle.raw_probe_report.fan_curves {
            let path = curve.path.as_deref().unwrap_or("unknown");
            curves.add(&info_row(
                &curve.id,
                &format!("{} point files - {path}", curve.point_paths.len()),
            ));
        }
    }
    curves.add(&info_row(
        "Last known good",
        &render_fan_snapshot_row(fan_snapshot),
    ));
    page.append(&curves);

    let presets = adw::PreferencesGroup::new();
    presets.set_title("Packaged Presets");
    for preset in [
        ("quiet-office", "Quiet office"),
        ("balanced-daily", "Balanced daily"),
        ("gaming", "Gaming"),
        ("max-safe", "Max safe"),
    ] {
        presets.add(&info_row(preset.1, preset.0));
    }
    page.append(&presets);
}

fn append_appearance(page: &gtk4::Box, bundle: &DiagnosticsBundle) {
    let title = gtk4::Label::new(Some("Appearance"));
    title.add_css_class("title-2");
    title.set_xalign(0.0);
    page.append(&title);

    let leds = adw::PreferencesGroup::new();
    leds.set_title("LEDs");
    if bundle.raw_probe_report.leds.is_empty() {
        leds.add(&info_row("LEDs", "unavailable"));
        page.append(&leds);
        page.append(&build_led_state_controls(None, None));
    } else {
        let mut ylogo_row = None;
        for led in &bundle.raw_probe_report.leds {
            let row = info_row(&led.name, &render_led_row(led));
            if led.name == "platform::ylogo" {
                ylogo_row = Some(row.clone());
            }
            leds.add(&row);
        }
        page.append(&leds);
        page.append(&build_led_state_controls(
            writable_ylogo(bundle.raw_probe_report.leds.as_slice()),
            ylogo_row,
        ));
    }
    page.append(&build_write_feedback_group("Y-logo LED"));

    let toggles = adw::PreferencesGroup::new();
    toggles.set_title("Firmware Toggles");
    if bundle.raw_probe_report.ideapad_toggles.is_empty() {
        toggles.add(&info_row("Firmware toggles", "unavailable"));
    } else {
        for toggle in &bundle.raw_probe_report.ideapad_toggles {
            let value = toggle.current_value.as_deref().unwrap_or("unknown");
            let path = toggle.path.as_deref().unwrap_or("unknown");
            toggles.add(&info_row(&toggle.name, &format!("{value} - {path}")));
        }
    }
    page.append(&toggles);
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

fn clone_result<T: Clone>(result: &Result<T>) -> Result<T> {
    match result {
        Ok(value) => Ok(value.clone()),
        Err(error) => Err(anyhow!(error.to_string())),
    }
}

fn clone_error(error: &anyhow::Error) -> anyhow::Error {
    anyhow!(error.to_string())
}

fn runtime_snapshot_gpu_pending(
    snapshot: &Result<RuntimeSnapshot>,
) -> Result<Option<GpuModePending>> {
    snapshot
        .as_ref()
        .map(|snapshot| snapshot.diagnostics.gpu_mode_pending.clone())
        .map_err(clone_error)
}

fn runtime_snapshot_fan_snapshot(
    snapshot: &Result<RuntimeSnapshot>,
) -> Result<Option<FanCurveSnapshot>> {
    snapshot
        .as_ref()
        .map(|snapshot| snapshot.diagnostics.last_known_good_fan_curve.clone())
        .map_err(clone_error)
}

fn render_gpu_pending_row(pending: Result<Option<GpuModePending>>) -> String {
    match pending {
        Ok(Some(pending)) => {
            let previous = pending.previous_mode.as_deref().unwrap_or("unknown");
            format!(
                "{} - previous {} - reboot required {}",
                pending.requested_mode, previous, pending.reboot_required
            )
        }
        Ok(None) => "none".to_owned(),
        Err(error) => format!("state unavailable - {error}"),
    }
}

fn render_fan_snapshot_row(snapshot: Result<Option<FanCurveSnapshot>>) -> String {
    match snapshot {
        Ok(Some(snapshot)) => {
            let path = snapshot.path.as_deref().unwrap_or("unknown");
            format!("{path} - {} captured values", snapshot.points.len())
        }
        Ok(None) => "none captured".to_owned(),
        Err(error) => format!("state unavailable - {error}"),
    }
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

fn build_platform_profile_controls(
    capability: Option<&PlatformProfileCapability>,
    current_row: Option<adw::ActionRow>,
) -> adw::PreferencesGroup {
    build_write_controls(
        "Platform profile quick apply",
        capability.map(|capability| capability.current.as_deref().unwrap_or("unknown")),
        capability.map(|capability| capability.choices.as_slice()),
        "Requested profile",
        "Apply profile",
        "Platform profile",
        |requested| {
            LegionControlClient::system().and_then(|client| client.set_platform_profile(requested))
        },
        move |_| {
            if let Some(row) = &current_row {
                refresh_platform_profile_row(row);
            }
        },
    )
}

fn build_battery_charge_type_controls(
    capability: Option<&BatteryChargeTypeCapability>,
    current_row: Option<adw::ActionRow>,
) -> adw::PreferencesGroup {
    build_write_controls(
        "Battery charge type quick apply",
        capability.map(|capability| capability.current.as_deref().unwrap_or("unknown")),
        capability.map(|capability| capability.choices.as_slice()),
        "Requested charge type",
        "Apply charge type",
        "Battery charge type",
        |requested| {
            LegionControlClient::system()
                .and_then(|client| client.set_battery_charge_type(requested))
        },
        move |_| {
            if let Some(row) = &current_row {
                refresh_battery_charge_type_row(row);
            }
        },
    )
}

fn build_led_state_controls(
    led: Option<&LedCapability>,
    current_row: Option<adw::ActionRow>,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("LED quick apply");

    let Some(led) = led else {
        group.add(&info_row(
            "Y-logo LED",
            "unavailable - quick apply disabled",
        ));
        return group;
    };

    let row = adw::ActionRow::builder()
        .title("Y-logo LED")
        .subtitle("Apply a reversible on/off change to the detected ylogo LED.")
        .selectable(false)
        .build();

    let off = gtk4::Button::with_label("Turn off");
    let on = gtk4::Button::with_label("Turn on");
    off.set_sensitive(led.brightness != Some(0));
    on.set_sensitive(led.brightness != Some(1));
    row.add_suffix(&off);
    row.add_suffix(&on);
    group.add(&row);

    let feedback_row = write_feedback_row(None);
    group.add(&feedback_row);

    let led_id = led.name.clone();
    let path = led.path.clone();
    let max_brightness = led.max_brightness.unwrap_or(1);
    let feedback_row_for_off = feedback_row.clone();
    let current_row_for_off = current_row.clone();
    off.connect_clicked(move |_| {
        handle_led_button_click(
            &feedback_row_for_off,
            current_row_for_off.as_ref(),
            &led_id,
            &path,
            max_brightness,
            false,
        );
    });

    let led_id = led.name.clone();
    let path = led.path.clone();
    let feedback_row_for_on = feedback_row.clone();
    let current_row_for_on = current_row.clone();
    on.connect_clicked(move |_| {
        handle_led_button_click(
            &feedback_row_for_on,
            current_row_for_on.as_ref(),
            &led_id,
            &path,
            max_brightness,
            true,
        );
    });

    group
}

#[allow(clippy::too_many_arguments)]
fn build_write_controls<F, G>(
    title: &str,
    current: Option<&str>,
    choices: Option<&[String]>,
    chooser_title: &str,
    button_label: &str,
    capability_label: &str,
    execute: F,
    on_result: G,
) -> adw::PreferencesGroup
where
    F: Fn(&str) -> Result<WriteExecutionResult> + 'static,
    G: Fn(&WriteExecutionResult) + 'static,
{
    let group = adw::PreferencesGroup::new();
    group.set_title(title);

    let Some(choices) = choices else {
        group.add(&info_row(
            capability_label,
            "unavailable - quick apply disabled",
        ));
        return group;
    };

    let can_apply = !choices.is_empty();
    let choice_refs = choices.iter().map(String::as_str).collect::<Vec<_>>();
    let model = gtk4::StringList::new(&choice_refs);
    let selected_index = current
        .and_then(|current| choices.iter().position(|choice| choice == current))
        .unwrap_or(0) as u32;

    let chooser = gtk4::DropDown::builder().model(&model).build();
    chooser.set_hexpand(true);
    chooser.set_selected(selected_index);
    chooser.set_sensitive(can_apply);

    let apply = gtk4::Button::with_label(button_label);
    apply.set_sensitive(can_apply);

    let row = adw::ActionRow::builder()
        .title(chooser_title)
        .subtitle(if can_apply {
            "Choose a detected runtime value to request from the daemon."
        } else {
            "No detected runtime values are available."
        })
        .selectable(false)
        .build();
    row.add_suffix(&chooser);
    row.add_suffix(&apply);
    group.add(&row);

    let feedback_row = write_feedback_row(None);
    group.add(&feedback_row);

    let feedback_row_for_click = feedback_row.clone();
    apply.connect_clicked(move |_| {
        let Some(selected) = chooser
            .selected_item()
            .and_then(|item| item.downcast::<gtk4::StringObject>().ok())
        else {
            feedback_row_for_click.set_title("Apply result");
            feedback_row_for_click.set_subtitle("Failed - no selected value was available.");
            return;
        };

        let requested = selected.string().to_string();
        feedback_row_for_click.set_title("Apply result");
        feedback_row_for_click.set_subtitle("Applying write request...");

        match execute(&requested) {
            Ok(result) => {
                feedback_row_for_click.set_title(write_feedback_title(Some(&result)));
                feedback_row_for_click.set_subtitle(&write_feedback_subtitle(Some(&result)));
                on_result(&result);
            }
            Err(error) => {
                feedback_row_for_click.set_title("Apply error");
                feedback_row_for_click.set_subtitle(&format!(
                    "Failed - daemon call could not be completed: {error}"
                ));
            }
        }
    });

    group
}

fn build_write_feedback_group(capability_label: &str) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Write feedback");
    group.add(&info_row(
        capability_label,
        "Quick apply uses polkit-gated daemon writes and reports the last result inline.",
    ));
    group
}

fn write_feedback_row(result: Option<&WriteExecutionResult>) -> adw::ActionRow {
    adw::ActionRow::builder()
        .title(write_feedback_title(result))
        .subtitle(write_feedback_subtitle(result))
        .selectable(false)
        .build()
}

pub fn write_feedback_title(result: Option<&WriteExecutionResult>) -> &'static str {
    match result.map(|result| result.status) {
        None => "Apply result",
        Some(WriteExecutionStatus::Applied) => "Apply succeeded",
        Some(WriteExecutionStatus::BlockedByPolicy) => "Apply blocked by policy",
        Some(WriteExecutionStatus::BlockedByAuthorization) => "Apply blocked by authorization",
        Some(WriteExecutionStatus::Failed) => "Apply failed",
    }
}

pub fn write_feedback_subtitle(result: Option<&WriteExecutionResult>) -> String {
    match result {
        None => "No write attempted yet.".to_owned(),
        Some(result) => {
            let readback = result
                .readback_value
                .as_deref()
                .map(|value| format!(" Read-back: {value}."))
                .unwrap_or_default();
            format!("{}{}", result.message, readback)
        }
    }
}

fn refresh_platform_profile_row(row: &adw::ActionRow) {
    if let Ok(snapshot) =
        LegionControlClient::system().and_then(|client| client.refresh_runtime_snapshot())
    {
        if let Some(profile) = snapshot.diagnostics.raw_probe_report.platform_profile {
            row.set_subtitle(profile.current.as_deref().unwrap_or("unknown"));
        }
    }
}

fn refresh_battery_charge_type_row(row: &adw::ActionRow) {
    if let Ok(snapshot) =
        LegionControlClient::system().and_then(|client| client.refresh_runtime_snapshot())
    {
        if let Some(charge_type) = snapshot.diagnostics.raw_probe_report.battery_charge_type {
            row.set_subtitle(charge_type.current.as_deref().unwrap_or("unknown"));
        }
    }
}

fn handle_led_button_click(
    feedback_row: &adw::ActionRow,
    current_row: Option<&adw::ActionRow>,
    led_id: &str,
    path: &str,
    max_brightness: i64,
    enabled: bool,
) {
    feedback_row.set_title("Apply result");
    feedback_row.set_subtitle("Applying write request...");

    match LegionControlClient::system().and_then(|client| client.set_led_state(led_id, enabled)) {
        Ok(result) => {
            feedback_row.set_title(write_feedback_title(Some(&result)));
            feedback_row.set_subtitle(&write_feedback_subtitle(Some(&result)));
            if let Some(row) = current_row {
                refresh_led_row(row, led_id, path, max_brightness, &result);
            }
        }
        Err(error) => {
            feedback_row.set_title("Apply error");
            feedback_row.set_subtitle(&format!(
                "Failed - daemon call could not be completed: {error}"
            ));
        }
    }
}

fn refresh_led_row(
    row: &adw::ActionRow,
    led_id: &str,
    path: &str,
    max_brightness: i64,
    result: &WriteExecutionResult,
) {
    if let Ok(snapshot) =
        LegionControlClient::system().and_then(|client| client.refresh_runtime_snapshot())
    {
        if let Some(led) = snapshot
            .diagnostics
            .raw_probe_report
            .leds
            .into_iter()
            .find(|led| led.name == led_id)
        {
            row.set_subtitle(&render_led_row(&led));
            return;
        }
    }

    if let Some(readback) = result.readback_value.as_deref() {
        row.set_subtitle(&format!(
            "brightness {} / max {} - {}",
            readback, max_brightness, path
        ));
    }
}

fn writable_ylogo(leds: &[LedCapability]) -> Option<&LedCapability> {
    leds.iter().find(|led| {
        led.name == "platform::ylogo"
            && led.max_brightness == Some(1)
            && matches!(led.brightness, Some(0 | 1))
    })
}

fn render_led_row(led: &LedCapability) -> String {
    let brightness = led
        .brightness
        .map(|brightness| brightness.to_string())
        .unwrap_or_else(|| "unknown".to_owned());
    let max = led
        .max_brightness
        .map(|max| max.to_string())
        .unwrap_or_else(|| "unknown".to_owned());
    format!("brightness {brightness} / max {max} - {}", led.path)
}
