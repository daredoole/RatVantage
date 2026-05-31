use crate::DiagnosticsBundle;
use adw::prelude::*;
use anyhow::Result;
use legion_common::{IdeapadToggleCapability, LedCapability};
use std::cell::RefCell;
use std::rc::Rc;

use super::shared::{
    append_error, info_row, make_client, request_dashboard_refresh, section_note, spawn_dbus_call,
    status_pill, store_write_feedback_state, write_feedback_row, write_feedback_subtitle,
    write_feedback_title, PillTone,
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

    let toggles = adw::PreferencesGroup::new();
    toggles.set_title("Firmware Toggles");
    if bundle.raw_probe_report.ideapad_toggles.is_empty() {
        toggles.add(&info_row("Firmware toggles", "unavailable"));
        page.add(&toggles);
        page.add(&build_ideapad_toggle_controls(None, None));
        page.add(&build_camera_power_controls(None, None));
        page.add(&build_usb_charging_controls(None, None));
        page.add(&build_fan_mode_controls(None, None));
    } else {
        let mut fn_lock_row = None;
        let mut camera_power_row = None;
        let mut usb_charging_row = None;
        let mut fan_mode_row = None;
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
            if toggle.name == "fan_mode" {
                fan_mode_row = Some(row.clone());
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
        page.add(&build_fan_mode_controls(
            bundle
                .raw_probe_report
                .ideapad_toggles
                .iter()
                .find(|t| t.name == "fan_mode"),
            fan_mode_row,
        ));
    }
}

fn build_led_state_controls(
    led: Option<&LedCapability>,
    current_row: Option<adw::ActionRow>,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Logo LED");

    let Some(led) = led else {
        group.add(&info_row(
            "Y-logo LED",
            "Unavailable: writable platform::ylogo LED was not detected, so quick apply is disabled.",
        ));
        return group;
    };

    let row = adw::ActionRow::builder()
        .title("Y-logo LED")
        .subtitle("Guarded write; rollback if read-back disagrees.")
        .selectable(false)
        .build();
    row.add_suffix(&status_pill("guarded", PillTone::Warning));

    let off = gtk4::Button::with_label("Turn off");
    let on = gtk4::Button::with_label("Turn on");
    off.set_sensitive(led.brightness != Some(0));
    on.set_sensitive(led.brightness != Some(1));
    off.add_css_class("pill");
    off.set_valign(gtk4::Align::Center);
    on.add_css_class("suggested-action");
    on.add_css_class("pill");
    on.set_valign(gtk4::Align::Center);
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
    group.set_title("Keyboard");

    let Some(toggle) = toggle else {
        group.add(&info_row(
            "Functional Fn-lock",
            "Unavailable: writable fn_lock toggle was not detected, so quick apply is disabled.",
        ));
        return group;
    };

    let row = adw::ActionRow::builder()
        .title("Functional Fn-lock")
        .subtitle("Verified with the Fn-lock indicator LED.")
        .selectable(false)
        .build();
    row.add_suffix(&status_pill("verified", PillTone::Good));

    let off = gtk4::Button::with_label("Turn off");
    let on = gtk4::Button::with_label("Turn on");
    off.set_sensitive(toggle.current_value.as_deref() != Some("0"));
    on.set_sensitive(toggle.current_value.as_deref() != Some("1"));
    off.add_css_class("pill");
    off.set_valign(gtk4::Align::Center);
    on.add_css_class("suggested-action");
    on.add_css_class("pill");
    on.set_valign(gtk4::Align::Center);
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
    group.set_title("Camera");

    let Some(toggle) = toggle else {
        group.add(&info_row(
            "Camera power",
            "Unavailable: writable camera_power toggle was not detected, so quick apply is disabled.",
        ));
        return group;
    };

    let row = adw::ActionRow::builder()
        .title("Camera power")
        .subtitle("Guarded write; rollback if read-back disagrees.")
        .selectable(false)
        .build();
    row.add_suffix(&status_pill("guarded", PillTone::Warning));

    let request_off = gtk4::Button::with_label("Request off");
    let request_on = gtk4::Button::with_label("Request on");
    request_off.set_sensitive(toggle.current_value.as_deref() != Some("0"));
    request_on.set_sensitive(toggle.current_value.as_deref() != Some("1"));
    request_off.add_css_class("pill");
    request_off.set_valign(gtk4::Align::Center);
    request_on.add_css_class("pill");
    request_on.set_valign(gtk4::Align::Center);
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
    confirm.add_css_class("suggested-action");
    confirm.add_css_class("pill");
    confirm.set_valign(gtk4::Align::Center);
    cancel.add_css_class("pill");
    cancel.set_valign(gtk4::Align::Center);
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
    let feedback_for_off = feedback_row.clone();
    request_off.connect_clicked(move |_| {
        *pending_for_off.borrow_mut() = Some(false);
        confirm_row_for_off.set_title("Confirm camera power change");
        confirm_row_for_off.set_subtitle(
            "Turning camera power off can interrupt active camera apps. Confirm to continue or Cancel to leave the device alone.",
        );
        confirm_for_off.set_label("Confirm off");
        confirm_for_off.set_sensitive(true);
        cancel_for_off.set_sensitive(true);
        feedback_for_off.set_title("Confirmation pending");
        feedback_for_off.set_subtitle(
            "Camera power off requested locally. Click Confirm off to send the daemon write.",
        );
    });

    let confirm_row_for_on = confirm_row.clone();
    let confirm_for_on = confirm.clone();
    let cancel_for_on = cancel.clone();
    let pending_for_on = Rc::clone(&pending_enabled);
    let feedback_for_on = feedback_row.clone();
    request_on.connect_clicked(move |_| {
        *pending_for_on.borrow_mut() = Some(true);
        confirm_row_for_on.set_title("Confirm camera power change");
        confirm_row_for_on.set_subtitle(
            "Turning camera power on should restore the device, but camera apps may still need to be restarted. Confirm to continue or Cancel to leave the device alone.",
        );
        confirm_for_on.set_label("Confirm on");
        confirm_for_on.set_sensitive(true);
        cancel_for_on.set_sensitive(true);
        feedback_for_on.set_title("Confirmation pending");
        feedback_for_on.set_subtitle(
            "Camera power on requested locally. Click Confirm on to send the daemon write.",
        );
    });

    let confirm_row_for_cancel = confirm_row.clone();
    let confirm_for_cancel = confirm.clone();
    let cancel_for_cancel = cancel.clone();
    let pending_for_cancel = Rc::clone(&pending_enabled);
    let feedback_for_cancel = feedback_row.clone();
    cancel.connect_clicked(move |_| {
        *pending_for_cancel.borrow_mut() = None;
        confirm_row_for_cancel.set_title("Confirmation required");
        confirm_row_for_cancel.set_subtitle("Choose Request on or Request off first. If apps lose the camera, re-enable it here and restart them.");
        confirm_for_cancel.set_label("Confirm");
        confirm_for_cancel.set_sensitive(false);
        cancel_for_cancel.set_sensitive(false);
        feedback_for_cancel.set_title("Apply result");
        feedback_for_cancel
            .set_subtitle("Camera power request cancelled locally; no daemon write was sent.");
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
    group.set_title("USB Power");

    let Some(toggle) = toggle else {
        group.add(&info_row(
            "USB charging",
            "Unavailable: writable usb_charging toggle was not detected, so quick apply is disabled.",
        ));
        return group;
    };

    let row = adw::ActionRow::builder()
        .title("USB charging")
        .subtitle("Always-on USB charging; confirmation required.")
        .selectable(false)
        .build();
    row.add_suffix(&status_pill("verified", PillTone::Good));

    let request_off = gtk4::Button::with_label("Request off");
    let request_on = gtk4::Button::with_label("Request on");
    request_off.set_sensitive(toggle.current_value.as_deref() != Some("0"));
    request_on.set_sensitive(toggle.current_value.as_deref() != Some("1"));
    request_off.add_css_class("pill");
    request_off.set_valign(gtk4::Align::Center);
    request_on.add_css_class("pill");
    request_on.set_valign(gtk4::Align::Center);
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
    confirm.add_css_class("suggested-action");
    confirm.add_css_class("pill");
    confirm.set_valign(gtk4::Align::Center);
    cancel.add_css_class("pill");
    cancel.set_valign(gtk4::Align::Center);
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
    let feedback_for_off = feedback_row.clone();
    request_off.connect_clicked(move |_| {
        *pending_for_off.borrow_mut() = Some(false);
        confirm_row_for_off.set_title("Confirm USB charging change");
        confirm_row_for_off.set_subtitle(
            "Turning USB charging off should stop always-on charging behavior. Confirm to continue or Cancel to leave the current state in place.",
        );
        confirm_for_off.set_label("Confirm off");
        confirm_for_off.set_sensitive(true);
        cancel_for_off.set_sensitive(true);
        feedback_for_off.set_title("Confirmation pending");
        feedback_for_off.set_subtitle(
            "USB charging off requested locally. Click Confirm off to send the daemon write.",
        );
    });

    let confirm_row_for_on = confirm_row.clone();
    let confirm_for_on = confirm.clone();
    let cancel_for_on = cancel.clone();
    let pending_for_on = Rc::clone(&pending_enabled);
    let feedback_for_on = feedback_row.clone();
    request_on.connect_clicked(move |_| {
        *pending_for_on.borrow_mut() = Some(true);
        confirm_row_for_on.set_title("Confirm USB charging change");
        confirm_row_for_on.set_subtitle(
            "Turning USB charging on can drain the battery while the laptop is shut down. Confirm to continue or Cancel to leave the current state in place.",
        );
        confirm_for_on.set_label("Confirm on");
        confirm_for_on.set_sensitive(true);
        cancel_for_on.set_sensitive(true);
        feedback_for_on.set_title("Confirmation pending");
        feedback_for_on.set_subtitle(
            "USB charging on requested locally. Click Confirm on to send the daemon write.",
        );
    });

    let confirm_row_for_cancel = confirm_row.clone();
    let confirm_for_cancel = confirm.clone();
    let cancel_for_cancel = cancel.clone();
    let pending_for_cancel = Rc::clone(&pending_enabled);
    let feedback_for_cancel = feedback_row.clone();
    cancel.connect_clicked(move |_| {
        *pending_for_cancel.borrow_mut() = None;
        confirm_row_for_cancel.set_title("Confirmation required");
        confirm_row_for_cancel.set_subtitle("Choose Request on or Request off first. Enabling USB charging may increase battery drain while the laptop is powered off, and the write alone does not verify real-world charging behavior.");
        confirm_for_cancel.set_label("Confirm");
        confirm_for_cancel.set_sensitive(false);
        cancel_for_cancel.set_sensitive(false);
        feedback_for_cancel.set_title("Apply result");
        feedback_for_cancel
            .set_subtitle("USB charging request cancelled locally; no daemon write was sent.");
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

    feedback_row.set_title("Apply in progress");
    feedback_row
        .set_subtitle("Request sent to the daemon; waiting for policy/auth/write result...");

    let led_id_for_call = led_id.clone();
    spawn_dbus_call(
        move || make_client().and_then(|client| client.set_led_state(&led_id_for_call, enabled)),
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

    feedback_row.set_title("Apply in progress");
    feedback_row
        .set_subtitle("Request sent to the daemon; waiting for policy/auth/write result...");

    let toggle_id_for_call = toggle_id.clone();
    spawn_dbus_call(
        move || {
            make_client().and_then(|client| client.set_ideapad_toggle(&toggle_id_for_call, enabled))
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
        || make_client().and_then(|client| client.refresh_runtime_snapshot()),
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
        || make_client().and_then(|client| client.refresh_runtime_snapshot()),
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

fn build_fan_mode_controls(
    toggle: Option<&IdeapadToggleCapability>,
    current_row: Option<adw::ActionRow>,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Fan Mode");
    group.add(&section_note(
        "0 = Auto (firmware-controlled). 1 = Full speed (dust cleaning / max cooling). Requires --enable-fan-mode-write daemon flag.",
    ));

    let Some(toggle) = toggle else {
        group.add(&info_row(
            "fan_mode",
            "Unavailable — ideapad_acpi not bound or fan_mode not exposed.",
        ));
        return group;
    };

    let current_val = toggle.current_value.as_deref().unwrap_or("0");
    let current_label = render_ideapad_toggle_value(&toggle.name, current_val);
    let is_full = current_val == "1";

    let toggle_id = toggle.name.clone();
    let path = toggle.path.clone().unwrap_or_else(|| "unknown".to_owned());

    let auto_btn = gtk4::Button::with_label("Auto (0)");
    auto_btn.set_sensitive(is_full); // only enabled if currently full
    auto_btn.add_css_class("pill");
    auto_btn.set_valign(gtk4::Align::Center);

    let full_btn = gtk4::Button::with_label("Full speed (1)");
    full_btn.set_sensitive(!is_full); // only enabled if currently auto
    full_btn.add_css_class("destructive-action");
    full_btn.add_css_class("pill");
    full_btn.set_valign(gtk4::Align::Center);

    let row = adw::ActionRow::builder()
        .title("Fan mode")
        .subtitle(format!("Current: {current_label} — {path}"))
        .selectable(false)
        .build();
    row.add_suffix(&auto_btn);
    row.add_suffix(&full_btn);
    group.add(&row);

    let feedback_row = write_feedback_row("Fan mode");
    group.add(&feedback_row);

    let feedback_for_auto = feedback_row.clone();
    let current_row_for_auto = current_row.clone();
    let toggle_id_for_auto = toggle_id.clone();
    let path_for_auto = path.clone();
    auto_btn.connect_clicked(move |_| {
        handle_ideapad_toggle_button_click(
            &feedback_for_auto,
            current_row_for_auto.as_ref(),
            "Fan mode",
            &toggle_id_for_auto,
            &path_for_auto,
            false,
        );
    });

    let feedback_for_full = feedback_row.clone();
    let current_row_for_full = current_row.clone();
    full_btn.connect_clicked(move |_| {
        handle_ideapad_toggle_button_click(
            &feedback_for_full,
            current_row_for_full.as_ref(),
            "Fan mode",
            &toggle_id,
            &path,
            true,
        );
    });

    group
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
    let value = render_ideapad_toggle_value(&toggle.name, value);
    format!("{value} - {path}")
}

fn render_ideapad_toggle_value(toggle_name: &str, value: &str) -> String {
    match (toggle_name, value) {
        ("fan_mode", "0") => "Auto (0)".to_owned(),
        ("fan_mode", "1") => "Full speed (1)".to_owned(),
        _ => value.to_owned(),
    }
}
