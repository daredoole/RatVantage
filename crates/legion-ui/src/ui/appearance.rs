use crate::{DiagnosticsBundle, LegionControlClient};
use adw::prelude::*;
use anyhow::Result;
use legion_common::{IdeapadToggleCapability, LedCapability};
use std::cell::RefCell;
use std::rc::Rc;

use super::shared::{
    append_error, build_write_feedback_group, info_row, request_dashboard_refresh, spawn_dbus_call,
    store_write_feedback_state, write_feedback_row, write_feedback_subtitle, write_feedback_title,
};

pub fn appearance_page(diagnostics: Result<DiagnosticsBundle>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    match diagnostics {
        Ok(bundle) => append_appearance(&page, &bundle),
        Err(error) => append_error(&page, &error),
    }

    page
}

fn append_appearance(page: &adw::PreferencesPage, bundle: &DiagnosticsBundle) {
    let leds = adw::PreferencesGroup::new();
    leds.set_title("LEDs");
    if bundle.raw_probe_report.leds.is_empty() {
        leds.add(&info_row("LEDs", "unavailable"));
        page.add(&leds);
        page.add(&build_led_state_controls(None, None));
    } else {
        let mut ylogo_row = None;
        for led in &bundle.raw_probe_report.leds {
            let row = info_row(&led.name, &render_led_row(led));
            if led.name == "platform::ylogo" {
                ylogo_row = Some(row.clone());
            }
            leds.add(&row);
        }
        page.add(&leds);
        page.add(&build_led_state_controls(
            writable_ylogo(bundle.raw_probe_report.leds.as_slice()),
            ylogo_row,
        ));
    }
    page.add(&build_write_feedback_group("Y-logo LED"));

    let toggles = adw::PreferencesGroup::new();
    toggles.set_title("Firmware Toggles");
    if bundle.raw_probe_report.ideapad_toggles.is_empty() {
        toggles.add(&info_row("Firmware toggles", "unavailable"));
        page.add(&toggles);
        page.add(&build_ideapad_toggle_controls(None, None));
        page.add(&build_camera_power_controls(None, None));
        page.add(&build_usb_charging_controls(None, None));
    } else {
        let mut fn_lock_row = None;
        let mut camera_power_row = None;
        let mut usb_charging_row = None;
        for toggle in &bundle.raw_probe_report.ideapad_toggles {
            let row = info_row(&toggle.name, &render_ideapad_toggle_row(toggle));
            if toggle.name == "fn_lock" {
                fn_lock_row = Some(row.clone());
            }
            if toggle.name == "camera_power" {
                camera_power_row = Some(row.clone());
            }
            if toggle.name == "usb_charging" {
                usb_charging_row = Some(row.clone());
            }
            toggles.add(&row);
        }
        page.add(&toggles);
        page.add(&build_ideapad_toggle_controls(
            writable_fn_lock_toggle(
                bundle.raw_probe_report.ideapad_toggles.as_slice(),
                bundle.raw_probe_report.leds.as_slice(),
            ),
            fn_lock_row,
        ));
        page.add(&build_camera_power_controls(
            writable_camera_power_toggle(bundle.raw_probe_report.ideapad_toggles.as_slice()),
            camera_power_row,
        ));
        page.add(&build_usb_charging_controls(
            writable_usb_charging_toggle(bundle.raw_probe_report.ideapad_toggles.as_slice()),
            usb_charging_row,
        ));
    }
    page.add(&build_write_feedback_group("Fn-lock"));
    page.add(&build_write_feedback_group("Camera power"));
    page.add(&build_write_feedback_group("USB charging"));
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

    let feedback_row = write_feedback_row("Y-logo LED");
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

fn build_ideapad_toggle_controls(
    toggle: Option<&IdeapadToggleCapability>,
    current_row: Option<adw::ActionRow>,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Fn-lock quick apply");

    let Some(toggle) = toggle else {
        group.add(&info_row(
            "Functional Fn-lock",
            "unavailable - quick apply disabled",
        ));
        return group;
    };

    let row = adw::ActionRow::builder()
        .title("Functional Fn-lock")
        .subtitle("Apply a reversible on/off change to the detected fn_lock ideapad toggle.")
        .selectable(false)
        .build();

    let off = gtk4::Button::with_label("Turn off");
    let on = gtk4::Button::with_label("Turn on");
    off.set_sensitive(toggle.current_value.as_deref() != Some("0"));
    on.set_sensitive(toggle.current_value.as_deref() != Some("1"));
    row.add_suffix(&off);
    row.add_suffix(&on);
    group.add(&row);

    let feedback_row = write_feedback_row("Fn-lock");
    group.add(&feedback_row);

    let toggle_id = toggle.name.clone();
    let path = toggle.path.clone().unwrap_or_else(|| "unknown".to_owned());
    let feedback_row_for_off = feedback_row.clone();
    let current_row_for_off = current_row.clone();
    off.connect_clicked(move |_| {
        handle_ideapad_toggle_button_click(
            &feedback_row_for_off,
            current_row_for_off.as_ref(),
            "Fn-lock",
            &toggle_id,
            &path,
            false,
        );
    });

    let toggle_id = toggle.name.clone();
    let path = toggle.path.clone().unwrap_or_else(|| "unknown".to_owned());
    let feedback_row_for_on = feedback_row.clone();
    let current_row_for_on = current_row.clone();
    on.connect_clicked(move |_| {
        handle_ideapad_toggle_button_click(
            &feedback_row_for_on,
            current_row_for_on.as_ref(),
            "Fn-lock",
            &toggle_id,
            &path,
            true,
        );
    });

    group
}

fn build_camera_power_controls(
    toggle: Option<&IdeapadToggleCapability>,
    current_row: Option<adw::ActionRow>,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Camera privacy quick apply");

    let Some(toggle) = toggle else {
        group.add(&info_row(
            "Camera power",
            "unavailable - quick apply disabled",
        ));
        return group;
    };

    let row = adw::ActionRow::builder()
        .title("Camera power")
        .subtitle("Changes require confirmation because active camera apps can lose the device.")
        .selectable(false)
        .build();

    let request_off = gtk4::Button::with_label("Request off");
    let request_on = gtk4::Button::with_label("Request on");
    request_off.set_sensitive(toggle.current_value.as_deref() != Some("0"));
    request_on.set_sensitive(toggle.current_value.as_deref() != Some("1"));
    row.add_suffix(&request_off);
    row.add_suffix(&request_on);
    group.add(&row);

    let confirm_row = adw::ActionRow::builder()
        .title("Confirmation required")
        .subtitle("Choose Request on or Request off first. If apps lose the camera, re-enable it here and restart them.")
        .selectable(false)
        .build();
    let confirm = gtk4::Button::with_label("Confirm");
    let cancel = gtk4::Button::with_label("Cancel");
    confirm.set_sensitive(false);
    cancel.set_sensitive(false);
    confirm_row.add_suffix(&confirm);
    confirm_row.add_suffix(&cancel);
    group.add(&confirm_row);

    let feedback_row = write_feedback_row("Camera power");
    group.add(&feedback_row);

    let pending_enabled = Rc::new(RefCell::new(None::<bool>));

    let confirm_row_for_off = confirm_row.clone();
    let confirm_for_off = confirm.clone();
    let cancel_for_off = cancel.clone();
    let pending_for_off = Rc::clone(&pending_enabled);
    request_off.connect_clicked(move |_| {
        *pending_for_off.borrow_mut() = Some(false);
        confirm_row_for_off.set_title("Confirm camera power change");
        confirm_row_for_off.set_subtitle(
            "Turning camera power off can interrupt active camera apps. Confirm to continue or Cancel to leave the device alone.",
        );
        confirm_for_off.set_label("Confirm off");
        confirm_for_off.set_sensitive(true);
        cancel_for_off.set_sensitive(true);
    });

    let confirm_row_for_on = confirm_row.clone();
    let confirm_for_on = confirm.clone();
    let cancel_for_on = cancel.clone();
    let pending_for_on = Rc::clone(&pending_enabled);
    request_on.connect_clicked(move |_| {
        *pending_for_on.borrow_mut() = Some(true);
        confirm_row_for_on.set_title("Confirm camera power change");
        confirm_row_for_on.set_subtitle(
            "Turning camera power on should restore the device, but camera apps may still need to be restarted. Confirm to continue or Cancel to leave the device alone.",
        );
        confirm_for_on.set_label("Confirm on");
        confirm_for_on.set_sensitive(true);
        cancel_for_on.set_sensitive(true);
    });

    let confirm_row_for_cancel = confirm_row.clone();
    let confirm_for_cancel = confirm.clone();
    let cancel_for_cancel = cancel.clone();
    let pending_for_cancel = Rc::clone(&pending_enabled);
    cancel.connect_clicked(move |_| {
        *pending_for_cancel.borrow_mut() = None;
        confirm_row_for_cancel.set_title("Confirmation required");
        confirm_row_for_cancel.set_subtitle("Choose Request on or Request off first. If apps lose the camera, re-enable it here and restart them.");
        confirm_for_cancel.set_label("Confirm");
        confirm_for_cancel.set_sensitive(false);
        cancel_for_cancel.set_sensitive(false);
    });

    let feedback_row_for_confirm = feedback_row.clone();
    let confirm_row_for_confirm = confirm_row.clone();
    let confirm_for_confirm = confirm.clone();
    let cancel_for_confirm = cancel.clone();
    let current_row_for_confirm = current_row.clone();
    let toggle_id = toggle.name.clone();
    let path = toggle.path.clone().unwrap_or_else(|| "unknown".to_owned());
    let pending_for_confirm = Rc::clone(&pending_enabled);
    confirm.connect_clicked(move |_| {
        let Some(enabled) = *pending_for_confirm.borrow() else {
            return;
        };
        handle_ideapad_toggle_button_click(
            &feedback_row_for_confirm,
            current_row_for_confirm.as_ref(),
            "Camera power",
            &toggle_id,
            &path,
            enabled,
        );
        *pending_for_confirm.borrow_mut() = None;
        confirm_row_for_confirm.set_title("Confirmation required");
        confirm_row_for_confirm.set_subtitle("Choose Request on or Request off first. If apps lose the camera, re-enable it here and restart them.");
        confirm_for_confirm.set_label("Confirm");
        confirm_for_confirm.set_sensitive(false);
        cancel_for_confirm.set_sensitive(false);
    });

    group
}

fn build_usb_charging_controls(
    toggle: Option<&IdeapadToggleCapability>,
    current_row: Option<adw::ActionRow>,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("USB charging quick apply");

    let Some(toggle) = toggle else {
        group.add(&info_row(
            "USB charging",
            "unavailable - quick apply disabled",
        ));
        return group;
    };

    let row = adw::ActionRow::builder()
        .title("USB charging")
        .subtitle(
            "Changes require confirmation because enabling always-on USB charging can drain the laptop battery while it is shut down.",
        )
        .selectable(false)
        .build();

    let request_off = gtk4::Button::with_label("Request off");
    let request_on = gtk4::Button::with_label("Request on");
    request_off.set_sensitive(toggle.current_value.as_deref() != Some("0"));
    request_on.set_sensitive(toggle.current_value.as_deref() != Some("1"));
    row.add_suffix(&request_off);
    row.add_suffix(&request_on);
    group.add(&row);

    let confirm_row = adw::ActionRow::builder()
        .title("Confirmation required")
        .subtitle("Choose Request on or Request off first. Enabling USB charging may increase battery drain while the laptop is powered off, and the write alone does not verify real-world charging behavior.")
        .selectable(false)
        .build();
    let confirm = gtk4::Button::with_label("Confirm");
    let cancel = gtk4::Button::with_label("Cancel");
    confirm.set_sensitive(false);
    cancel.set_sensitive(false);
    confirm_row.add_suffix(&confirm);
    confirm_row.add_suffix(&cancel);
    group.add(&confirm_row);

    let feedback_row = write_feedback_row("USB charging");
    group.add(&feedback_row);

    let pending_enabled = Rc::new(RefCell::new(None::<bool>));

    let confirm_row_for_off = confirm_row.clone();
    let confirm_for_off = confirm.clone();
    let cancel_for_off = cancel.clone();
    let pending_for_off = Rc::clone(&pending_enabled);
    request_off.connect_clicked(move |_| {
        *pending_for_off.borrow_mut() = Some(false);
        confirm_row_for_off.set_title("Confirm USB charging change");
        confirm_row_for_off.set_subtitle(
            "Turning USB charging off should stop always-on charging behavior. Confirm to continue or Cancel to leave the current state in place.",
        );
        confirm_for_off.set_label("Confirm off");
        confirm_for_off.set_sensitive(true);
        cancel_for_off.set_sensitive(true);
    });

    let confirm_row_for_on = confirm_row.clone();
    let confirm_for_on = confirm.clone();
    let cancel_for_on = cancel.clone();
    let pending_for_on = Rc::clone(&pending_enabled);
    request_on.connect_clicked(move |_| {
        *pending_for_on.borrow_mut() = Some(true);
        confirm_row_for_on.set_title("Confirm USB charging change");
        confirm_row_for_on.set_subtitle(
            "Turning USB charging on can drain the battery while the laptop is shut down. Confirm to continue or Cancel to leave the current state in place.",
        );
        confirm_for_on.set_label("Confirm on");
        confirm_for_on.set_sensitive(true);
        cancel_for_on.set_sensitive(true);
    });

    let confirm_row_for_cancel = confirm_row.clone();
    let confirm_for_cancel = confirm.clone();
    let cancel_for_cancel = cancel.clone();
    let pending_for_cancel = Rc::clone(&pending_enabled);
    cancel.connect_clicked(move |_| {
        *pending_for_cancel.borrow_mut() = None;
        confirm_row_for_cancel.set_title("Confirmation required");
        confirm_row_for_cancel.set_subtitle("Choose Request on or Request off first. Enabling USB charging may increase battery drain while the laptop is powered off, and the write alone does not verify real-world charging behavior.");
        confirm_for_cancel.set_label("Confirm");
        confirm_for_cancel.set_sensitive(false);
        cancel_for_cancel.set_sensitive(false);
    });

    let feedback_row_for_confirm = feedback_row.clone();
    let confirm_row_for_confirm = confirm_row.clone();
    let confirm_for_confirm = confirm.clone();
    let cancel_for_confirm = cancel.clone();
    let current_row_for_confirm = current_row.clone();
    let toggle_id = toggle.name.clone();
    let path = toggle.path.clone().unwrap_or_else(|| "unknown".to_owned());
    let pending_for_confirm = Rc::clone(&pending_enabled);
    confirm.connect_clicked(move |_| {
        let Some(enabled) = *pending_for_confirm.borrow() else {
            return;
        };
        handle_ideapad_toggle_button_click(
            &feedback_row_for_confirm,
            current_row_for_confirm.as_ref(),
            "USB charging",
            &toggle_id,
            &path,
            enabled,
        );
        *pending_for_confirm.borrow_mut() = None;
        confirm_row_for_confirm.set_title("Confirmation required");
        confirm_row_for_confirm.set_subtitle("Choose Request on or Request off first. Enabling USB charging may increase battery drain while the laptop is powered off, and the write alone does not verify real-world charging behavior.");
        confirm_for_confirm.set_label("Confirm");
        confirm_for_confirm.set_sensitive(false);
        cancel_for_confirm.set_sensitive(false);
    });

    group
}

fn handle_led_button_click(
    feedback_row: &adw::ActionRow,
    current_row: Option<&adw::ActionRow>,
    led_id: &str,
    path: &str,
    max_brightness: i64,
    enabled: bool,
) {
    let feedback_row = feedback_row.clone();
    let current_row = current_row.cloned();
    let led_id = led_id.to_owned();
    let path = path.to_owned();

    feedback_row.set_title("Apply result");
    feedback_row.set_subtitle("Applying write request...");

    let led_id_for_call = led_id.clone();
    spawn_dbus_call(
        move || {
            LegionControlClient::system()
                .and_then(|client| client.set_led_state(&led_id_for_call, enabled))
        },
        move |result| match result {
            Ok(result) => {
                let title = write_feedback_title(Some(&result));
                let subtitle = write_feedback_subtitle(Some(&result));
                feedback_row.set_title(title);
                feedback_row.set_subtitle(&subtitle);
                store_write_feedback_state("Y-logo LED", title, &subtitle);
                if !request_dashboard_refresh() {
                    if let Some(row) = current_row {
                        refresh_led_row(&row, &led_id, &path, max_brightness, &result);
                    }
                }
            }
            Err(error) => {
                feedback_row.set_title("Apply error");
                let subtitle = format!("Failed - daemon call could not be completed: {error}");
                feedback_row.set_subtitle(&subtitle);
                store_write_feedback_state("Y-logo LED", "Apply error", &subtitle);
                let _ = request_dashboard_refresh();
            }
        },
    );
}

fn handle_ideapad_toggle_button_click(
    feedback_row: &adw::ActionRow,
    current_row: Option<&adw::ActionRow>,
    capability_label: &'static str,
    toggle_id: &str,
    path: &str,
    enabled: bool,
) {
    let feedback_row = feedback_row.clone();
    let current_row = current_row.cloned();
    let toggle_id = toggle_id.to_owned();
    let path = path.to_owned();

    feedback_row.set_title("Apply result");
    feedback_row.set_subtitle("Applying write request...");

    let toggle_id_for_call = toggle_id.clone();
    spawn_dbus_call(
        move || {
            LegionControlClient::system()
                .and_then(|client| client.set_ideapad_toggle(&toggle_id_for_call, enabled))
        },
        move |result| match result {
            Ok(result) => {
                let title = write_feedback_title(Some(&result));
                let subtitle = write_feedback_subtitle(Some(&result));
                feedback_row.set_title(title);
                feedback_row.set_subtitle(&subtitle);
                store_write_feedback_state(capability_label, title, &subtitle);
                if !request_dashboard_refresh() {
                    if let Some(row) = current_row {
                        refresh_ideapad_toggle_row(&row, &toggle_id, &path, &result);
                    }
                }
            }
            Err(error) => {
                feedback_row.set_title("Apply error");
                let subtitle = format!("Failed - daemon call could not be completed: {error}");
                feedback_row.set_subtitle(&subtitle);
                store_write_feedback_state(capability_label, "Apply error", &subtitle);
                let _ = request_dashboard_refresh();
            }
        },
    );
}

fn refresh_led_row(
    row: &adw::ActionRow,
    led_id: &str,
    path: &str,
    max_brightness: i64,
    result: &legion_common::WriteExecutionResult,
) {
    let row = row.clone();
    let led_id = led_id.to_owned();
    let path = path.to_owned();
    let result = result.clone();

    spawn_dbus_call(
        || LegionControlClient::system().and_then(|client| client.refresh_runtime_snapshot()),
        move |res| {
            if let Ok(snapshot) = res {
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
        },
    );
}

fn refresh_ideapad_toggle_row(
    row: &adw::ActionRow,
    toggle_id: &str,
    path: &str,
    result: &legion_common::WriteExecutionResult,
) {
    let row = row.clone();
    let toggle_id = toggle_id.to_owned();
    let path = path.to_owned();
    let result = result.clone();

    spawn_dbus_call(
        || LegionControlClient::system().and_then(|client| client.refresh_runtime_snapshot()),
        move |res| {
            if let Ok(snapshot) = res {
                if let Some(toggle) = snapshot
                    .diagnostics
                    .raw_probe_report
                    .ideapad_toggles
                    .into_iter()
                    .find(|toggle| toggle.name == toggle_id)
                {
                    row.set_subtitle(&render_ideapad_toggle_row(&toggle));
                    return;
                }
            }

            if let Some(readback) = result.readback_value.as_deref() {
                row.set_subtitle(&format!("{readback} - {path}"));
            }
        },
    );
}

fn writable_ylogo(leds: &[LedCapability]) -> Option<&LedCapability> {
    leds.iter().find(|led| {
        led.name == "platform::ylogo"
            && led.max_brightness == Some(1)
            && matches!(led.brightness, Some(0 | 1))
    })
}

fn writable_fn_lock_toggle<'a>(
    toggles: &'a [IdeapadToggleCapability],
    leds: &[LedCapability],
) -> Option<&'a IdeapadToggleCapability> {
    toggles.iter().find(|toggle| {
        if toggle.name != "fn_lock" || !matches!(toggle.current_value.as_deref(), Some("0" | "1")) {
            return false;
        }
        let Some(path) = toggle.path.as_deref() else {
            return false;
        };
        if path.is_empty() {
            return false;
        }
        leds.iter().any(|led| {
            led.name == "platform::fnlock"
                && led.max_brightness == Some(1)
                && matches!(led.brightness, Some(0 | 1))
                && toggle.current_value.as_deref()
                    == led
                        .brightness
                        .map(|brightness| if brightness == 0 { "0" } else { "1" })
        })
    })
}

fn writable_camera_power_toggle(
    toggles: &[IdeapadToggleCapability],
) -> Option<&IdeapadToggleCapability> {
    toggles.iter().find(|toggle| {
        toggle.name == "camera_power"
            && matches!(toggle.current_value.as_deref(), Some("0" | "1"))
            && toggle.path.as_deref().is_some_and(|path| !path.is_empty())
    })
}

fn writable_usb_charging_toggle(
    toggles: &[IdeapadToggleCapability],
) -> Option<&IdeapadToggleCapability> {
    toggles.iter().find(|toggle| {
        toggle.name == "usb_charging"
            && matches!(toggle.current_value.as_deref(), Some("0" | "1"))
            && toggle.path.as_deref().is_some_and(|path| !path.is_empty())
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

fn render_ideapad_toggle_row(toggle: &IdeapadToggleCapability) -> String {
    let value = toggle.current_value.as_deref().unwrap_or("unknown");
    let path = toggle.path.as_deref().unwrap_or("unknown");
    format!("{value} - {path}")
}
