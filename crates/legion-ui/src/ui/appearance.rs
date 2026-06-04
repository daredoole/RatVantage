use crate::DiagnosticsBundle;
use adw::prelude::*;
use anyhow::Result;
use legion_common::{
    IdeapadToggleCapability, KeyboardRgbCandidate, KeyboardRgbOpenRgbStatus,
    KeyboardRgbWriteRequest, LedCapability,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::process::Command;
use std::rc::Rc;

use super::shared::{
    append_error, info_row, make_client, render_dry_run_plan_summary, request_dashboard_refresh,
    section_note, selected_dropdown_value, spawn_dbus_call, status_pill,
    store_write_feedback_state, write_feedback_row, write_feedback_subtitle, write_feedback_title,
    PillTone,
};

const OPENRGB_ACCESS_SETUP_COMMAND: &str = "ratvantage-setup-keyboard-rgb-openrgb-access";
const OPENRGB_BRIDGE_DRY_RUN_COMMAND: &str =
    "ratvantage-capture-keyboard-rgb-openrgb-bridge-evidence --output target/validation/keyboard-rgb-openrgb-bridge-dry-run";
const OPENRGB_BRIDGE_EXECUTE_COMMAND: &str =
    "ratvantage-capture-keyboard-rgb-openrgb-bridge-evidence --output target/validation/keyboard-rgb-openrgb-bridge-execute --execute";
const OPENRGB_BRIDGE_REVIEW_COMMAND: &str =
    "ratvantage-review-keyboard-rgb-openrgb-bridge-evidence --require-promotable target/validation/keyboard-rgb-openrgb-bridge-execute";
const OPENRGB_READINESS_OUTPUT: &str = "target/validation/keyboard-rgb-openrgb-readiness";
const OPENRGB_READINESS_COMMAND: &str =
    "ratvantage-check-keyboard-rgb-openrgb --output target/validation/keyboard-rgb-openrgb-readiness";
const OPENRGB_BRIDGE_STATUS_BIN: &str = "ratvantage-keyboard-rgb-openrgb-bridge-status";
const OPENRGB_BRIDGE_STATUS_COMMAND: &str =
    "ratvantage-keyboard-rgb-openrgb-bridge-status --readiness target/validation/keyboard-rgb-openrgb-readiness --sdk target/validation/keyboard-rgb-openrgb-sdk --sdk-write target/validation/keyboard-rgb-openrgb-sdk-write";
const OPENRGB_SDK_OUTPUT: &str = "target/validation/keyboard-rgb-openrgb-sdk";
const OPENRGB_SDK_WRITE_OUTPUT: &str = "target/validation/keyboard-rgb-openrgb-sdk-write";
const OPENRGB_SDK_EVIDENCE_COMMAND: &str =
    "ratvantage-capture-keyboard-rgb-openrgb-sdk-evidence --output target/validation/keyboard-rgb-openrgb-sdk";
const OPENRGB_SDK_WRITE_EVIDENCE_COMMAND: &str =
    "ratvantage-capture-keyboard-rgb-openrgb-sdk-write-evidence --output target/validation/keyboard-rgb-openrgb-sdk-write --execute --mode Breathing";
const OPENRGB_SDK_SERVER_COMMAND: &str = "ratvantage-openrgb-sdk-server start";
const OPENRGB_ACCESS_SETUP_LABEL: &str = "OpenRGB access setup";

pub fn appearance_page(diagnostics: Result<DiagnosticsBundle>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    match diagnostics {
        Ok(bundle) => append_appearance(&page, &bundle),
        Err(error) => append_error(&page, &error),
    }

    page
}

fn append_appearance(page: &adw::PreferencesPage, bundle: &DiagnosticsBundle) {
    page.add(&build_keyboard_rgb_controls(bundle));

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

fn build_keyboard_rgb_controls(bundle: &DiagnosticsBundle) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Keyboard RGB");

    if let Some(rgb) = bundle.raw_probe_report.keyboard_rgb.as_ref() {
        let current_effect = rgb.current_effect.as_deref().unwrap_or("unknown");
        let current_brightness = rgb
            .current_brightness
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_owned());
        let current_speed = rgb
            .current_speed
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_owned());

        let status = adw::ActionRow::builder()
            .title("Backend readiness")
            .subtitle(format!(
                "backend={} device={} zones={} effect={} brightness={} speed={}",
                rgb.backend,
                rgb.device_id,
                rgb.zones.len(),
                current_effect,
                current_brightness,
                current_speed
            ))
            .selectable(false)
            .build();
        status.add_suffix(&status_pill("backend ready", PillTone::Good));
        group.add(&status);
    } else if bundle.raw_probe_report.keyboard_rgb_candidates.is_empty()
        && bundle.raw_probe_report.keyboard_rgb_openrgb.is_none()
    {
        group.add(&info_row(
            "Backend readiness",
            "No keyboard RGB backend or HID research candidates detected.",
        ));
        return group;
    } else if !bundle.raw_probe_report.keyboard_rgb_candidates.is_empty() {
        let candidates = &bundle.raw_probe_report.keyboard_rgb_candidates;
        let status = adw::ActionRow::builder()
            .title("Backend readiness")
            .subtitle(format_keyboard_rgb_candidate_summary(candidates))
            .selectable(false)
            .build();
        status.add_suffix(&status_pill("research", PillTone::Warning));
        group.add(&status);

        group.add(&info_row(
            "Observed firmware state",
            "Fn+Space cycles firmware RGB modes; visible effect breathing. Observation only; not daemon read-back.",
        ));
    }

    if let Some(openrgb) = bundle.raw_probe_report.keyboard_rgb_openrgb.as_ref() {
        let status = adw::ActionRow::builder()
            .title("OpenRGB readiness")
            .subtitle(format_keyboard_rgb_openrgb_summary(openrgb))
            .selectable(false)
            .build();
        status.add_suffix(&status_pill(
            if openrgb.devices.is_empty() {
                "not detected"
            } else {
                "detected"
            },
            if openrgb.devices.is_empty() {
                PillTone::Neutral
            } else {
                PillTone::Warning
            },
        ));
        group.add(&status);

        if openrgb.installed && openrgb_linux_access_needs_setup(openrgb) {
            let setup_row = adw::ActionRow::builder()
                .title("OpenRGB access setup")
                .subtitle(format!(
                    "Daemon setup adds {} to i2c when needed, loads i2c-dev, and installs udev access rules; log out/in after success. Missing: {}",
                    current_setup_user(),
                    openrgb_missing_access_summary(openrgb)
                ))
                .selectable(false)
                .build();
            let setup = gtk4::Button::with_label("Set up");
            setup.set_tooltip_text(Some(
                "Ask the RatVantage daemon to set up OpenRGB i2c access with polkit authorization.",
            ));
            setup.add_css_class("pill");
            setup.set_valign(gtk4::Align::Center);
            let feedback_row = write_feedback_row(OPENRGB_ACCESS_SETUP_LABEL);
            let feedback_row_for_click = feedback_row.clone();
            setup.connect_clicked(move |_| {
                handle_openrgb_access_setup_click(&feedback_row_for_click);
            });
            setup_row.add_suffix(&setup);
            setup_row.add_suffix(&copy_command_button(
                "Copy fallback",
                OPENRGB_ACCESS_SETUP_COMMAND,
                "Copy the fallback setup command if polkit setup is unavailable.",
            ));
            setup_row.add_suffix(&status_pill("setup needed", PillTone::Warning));
            group.add(&setup_row);
            group.add(&feedback_row);
        }

        if openrgb.installed && !openrgb.devices.is_empty() {
            let status_row = adw::ActionRow::builder()
                .title("OpenRGB bridge evidence status")
                .subtitle(format!(
                    "{}; then {}",
                    OPENRGB_READINESS_COMMAND, OPENRGB_BRIDGE_STATUS_COMMAND
                ))
                .selectable(false)
                .build();
            let check_status = gtk4::Button::with_label("Check status");
            check_status.set_tooltip_text(Some(
                "Run the read-only OpenRGB bridge evidence status helper and show the result here.",
            ));
            check_status.add_css_class("pill");
            check_status.set_valign(gtk4::Align::Center);
            let bridge_status_feedback = write_feedback_row("OpenRGB bridge status");
            let bridge_status_feedback_for_click = bridge_status_feedback.clone();
            check_status.connect_clicked(move |_| {
                handle_openrgb_bridge_status_click(&bridge_status_feedback_for_click);
            });
            status_row.add_suffix(&check_status);
            status_row.add_suffix(&copy_command_button(
                "Copy status",
                OPENRGB_BRIDGE_STATUS_COMMAND,
                "Copy the command that summarizes OpenRGB readiness, bridge evidence, and SDK read-back evidence.",
            ));
            status_row.add_suffix(&status_pill("read-only", PillTone::Neutral));
            group.add(&status_row);
            group.add(&bridge_status_feedback);

            let sdk_row = adw::ActionRow::builder()
                .title("OpenRGB SDK read-back evidence")
                .subtitle(OPENRGB_SDK_EVIDENCE_COMMAND)
                .selectable(false)
                .build();
            let check_sdk = gtk4::Button::with_label("Check SDK");
            check_sdk.set_tooltip_text(Some(
                "Run the read-only OpenRGB SDK controller probe and show whether active mode/color read-back is available.",
            ));
            check_sdk.add_css_class("pill");
            check_sdk.set_valign(gtk4::Align::Center);
            let sdk_feedback = write_feedback_row("OpenRGB SDK evidence");
            let sdk_feedback_for_click = sdk_feedback.clone();
            check_sdk.connect_clicked(move |_| {
                handle_openrgb_sdk_evidence_click(&sdk_feedback_for_click);
            });
            sdk_row.add_suffix(&check_sdk);
            sdk_row.add_suffix(&copy_command_button(
                "Copy SDK",
                OPENRGB_SDK_EVIDENCE_COMMAND,
                "Copy the read-only OpenRGB SDK controller evidence command.",
            ));
            sdk_row.add_suffix(&status_pill("read-only", PillTone::Neutral));
            group.add(&sdk_row);
            group.add(&sdk_feedback);

            let sdk_server_row = adw::ActionRow::builder()
                .title("OpenRGB SDK server")
                .subtitle(OPENRGB_SDK_SERVER_COMMAND)
                .selectable(false)
                .build();
            let start_sdk_server = gtk4::Button::with_label("Start server");
            start_sdk_server.set_tooltip_text(Some(
                "Start a user-session OpenRGB SDK server for RatVantage helper connections.",
            ));
            start_sdk_server.add_css_class("pill");
            start_sdk_server.set_valign(gtk4::Align::Center);
            let sdk_server_feedback = write_feedback_row("OpenRGB SDK server");
            let sdk_server_feedback_for_click = sdk_server_feedback.clone();
            start_sdk_server.connect_clicked(move |_| {
                handle_openrgb_sdk_server_click(&sdk_server_feedback_for_click);
            });
            sdk_server_row.add_suffix(&start_sdk_server);
            sdk_server_row.add_suffix(&copy_command_button(
                "Copy server",
                OPENRGB_SDK_SERVER_COMMAND,
                "Copy the user-session OpenRGB SDK server start command.",
            ));
            sdk_server_row.add_suffix(&status_pill("user", PillTone::Neutral));
            group.add(&sdk_server_row);
            group.add(&sdk_server_feedback);

            let sdk_write_row = adw::ActionRow::builder()
                .title("OpenRGB SDK write evidence")
                .subtitle(OPENRGB_SDK_WRITE_EVIDENCE_COMMAND)
                .selectable(false)
                .build();
            sdk_write_row.add_suffix(&copy_command_button(
                "Copy SDK write",
                OPENRGB_SDK_WRITE_EVIDENCE_COMMAND,
                "Copy the operator-triggered OpenRGB SDK write/read-back/restore evidence command.",
            ));
            sdk_write_row.add_suffix(&status_pill("operator", PillTone::Warning));
            group.add(&sdk_write_row);

            let dry_run_row = adw::ActionRow::builder()
                .title("OpenRGB bridge dry-run evidence")
                .subtitle(OPENRGB_BRIDGE_DRY_RUN_COMMAND)
                .selectable(false)
                .build();
            let capture_dry_run = gtk4::Button::with_label("Capture dry-run");
            capture_dry_run.set_tooltip_text(Some(
                "Run the non-mutating OpenRGB bridge evidence capture and refresh status.",
            ));
            capture_dry_run.add_css_class("pill");
            capture_dry_run.set_valign(gtk4::Align::Center);
            let dry_run_feedback = write_feedback_row("OpenRGB bridge dry-run");
            let dry_run_feedback_for_click = dry_run_feedback.clone();
            capture_dry_run.connect_clicked(move |_| {
                handle_openrgb_bridge_dry_run_click(&dry_run_feedback_for_click);
            });
            dry_run_row.add_suffix(&capture_dry_run);
            dry_run_row.add_suffix(&copy_command_button(
                "Copy dry-run",
                OPENRGB_BRIDGE_DRY_RUN_COMMAND,
                "Copy the non-mutating OpenRGB bridge evidence command.",
            ));
            dry_run_row.add_suffix(&status_pill("safe", PillTone::Neutral));
            group.add(&dry_run_row);
            group.add(&dry_run_feedback);

            let execute_row = adw::ActionRow::builder()
                .title("OpenRGB bridge execute evidence")
                .subtitle(format!(
                    "{}; then {}",
                    OPENRGB_BRIDGE_EXECUTE_COMMAND, OPENRGB_BRIDGE_REVIEW_COMMAND
                ))
                .selectable(false)
                .build();
            let review_execute = gtk4::Button::with_label("Review execute");
            review_execute.set_tooltip_text(Some(
                "Run the read-only promotion reviewer for the execute evidence bundle.",
            ));
            review_execute.add_css_class("pill");
            review_execute.set_valign(gtk4::Align::Center);
            let review_feedback = write_feedback_row("OpenRGB bridge review");
            let review_feedback_for_click = review_feedback.clone();
            review_execute.connect_clicked(move |_| {
                handle_openrgb_bridge_review_click(&review_feedback_for_click);
            });
            execute_row.add_suffix(&review_execute);
            execute_row.add_suffix(&copy_command_button(
                "Copy execute",
                OPENRGB_BRIDGE_EXECUTE_COMMAND,
                "Copy the operator-triggered command that briefly changes RGB and restores it.",
            ));
            execute_row.add_suffix(&copy_command_button(
                "Copy review",
                OPENRGB_BRIDGE_REVIEW_COMMAND,
                "Copy the promotion review command for the execute evidence bundle.",
            ));
            execute_row.add_suffix(&status_pill("operator", PillTone::Warning));
            group.add(&execute_row);
            group.add(&review_feedback);
        }
    }

    let rgb_mode_choices = keyboard_rgb_mode_choices(bundle);
    let rgb_mode_refs = rgb_mode_choices
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let selected_mode = selected_keyboard_rgb_mode(bundle, &rgb_mode_choices);
    let selected_index = rgb_mode_choices
        .iter()
        .position(|mode| mode == &selected_mode)
        .unwrap_or(0) as u32;
    let effects = gtk4::StringList::new(&rgb_mode_refs);
    let effect = gtk4::DropDown::builder()
        .model(&effects)
        .selected(selected_index)
        .sensitive(true)
        .build();
    effect.set_valign(gtk4::Align::Center);
    let effect_row = adw::ActionRow::builder()
        .title("Effect")
        .subtitle("Staged request mode; preview is read-only.")
        .selectable(false)
        .build();
    effect_row.add_suffix(&effect);
    group.add(&effect_row);

    let mut color_entry_widgets = Vec::new();
    for (zone_id, default_color) in keyboard_rgb_color_entries(bundle) {
        let entry = gtk4::Entry::new();
        entry.set_text(&default_color);
        entry.set_width_chars(9);
        entry.set_max_width_chars(9);
        entry.set_valign(gtk4::Align::Center);
        let row = adw::ActionRow::builder()
            .title(format!("Zone color: {zone_id}"))
            .subtitle("Use #RRGGBB. OpenRGB preview sends all listed zones together.")
            .selectable(false)
            .build();
        row.add_suffix(&entry);
        group.add(&row);
        color_entry_widgets.push((zone_id, entry));
    }
    let color_entries = Rc::new(color_entry_widgets);

    let brightness = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 0.0, 100.0, 1.0);
    brightness.set_value(50.0);
    brightness.set_draw_value(true);
    brightness.set_sensitive(true);
    brightness.set_hexpand(true);
    let brightness_row = adw::ActionRow::builder()
        .title("Brightness")
        .subtitle("Staged request brightness; preview is read-only.")
        .selectable(false)
        .build();
    brightness_row.add_suffix(&brightness);
    group.add(&brightness_row);

    let speed = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 0.0, 100.0, 1.0);
    speed.set_value(50.0);
    speed.set_draw_value(true);
    speed.set_sensitive(true);
    speed.set_hexpand(true);
    let speed_row = adw::ActionRow::builder()
        .title("Speed")
        .subtitle("Staged request speed; preview is read-only.")
        .selectable(false)
        .build();
    speed_row.add_suffix(&speed);
    group.add(&speed_row);

    let preview_row = write_feedback_row("Keyboard RGB preview");
    let controls = KeyboardRgbControlWidgets {
        effect: effect.clone(),
        colors: Rc::clone(&color_entries),
        brightness: brightness.clone(),
        speed: speed.clone(),
    };
    let preview = gtk4::Button::with_label("Preview plan");
    preview.add_css_class("pill");
    preview.set_valign(gtk4::Align::Center);
    let preview_row_for_click = preview_row.clone();
    let preview_backend = bundle
        .raw_probe_report
        .keyboard_rgb_openrgb
        .as_ref()
        .map(|openrgb| {
            if openrgb.backend_ready {
                KeyboardRgbPreviewBackend::OpenRgbSdk
            } else if openrgb_plan_available(openrgb) {
                KeyboardRgbPreviewBackend::OpenRgbBridge
            } else {
                KeyboardRgbPreviewBackend::Native
            }
        })
        .unwrap_or(KeyboardRgbPreviewBackend::Native);
    let controls_for_preview = controls.clone();
    preview.connect_clicked(move |_| {
        handle_keyboard_rgb_preview_click(
            &preview_row_for_click,
            controls_for_preview.clone(),
            preview_backend,
        );
    });
    let copy_json = keyboard_rgb_copy_json_button("Copy request JSON", controls.clone());
    let preview_action_row = adw::ActionRow::builder()
        .title("RGB request preview")
        .subtitle("Builds a validated D-Bus/OpenRGB dry-run plan; no RGB write is sent.")
        .selectable(false)
        .build();
    preview_action_row.add_suffix(&preview);
    preview_action_row.add_suffix(&copy_json);
    preview_action_row.add_suffix(&status_pill("read-only", PillTone::Neutral));
    group.add(&preview_action_row);
    group.add(&preview_row);

    let apply_row = adw::ActionRow::builder()
        .title("Apply RGB")
        .subtitle("Disabled until live backend evidence proves requested mode/color read-back and restore.")
        .selectable(false)
        .build();
    let apply = gtk4::Button::with_label("Apply");
    let apply_enabled = bundle.raw_probe_report.keyboard_rgb.is_some()
        || bundle
            .raw_probe_report
            .keyboard_rgb_openrgb
            .as_ref()
            .is_some_and(|openrgb| openrgb.backend_ready);
    apply.set_sensitive(apply_enabled);
    apply.add_css_class("pill");
    apply.set_valign(gtk4::Align::Center);
    let apply_feedback_row = write_feedback_row("Keyboard RGB");
    let apply_controls = KeyboardRgbControlWidgets {
        effect,
        colors: Rc::clone(&color_entries),
        brightness,
        speed,
    };
    let apply_feedback_for_click = apply_feedback_row.clone();
    apply.connect_clicked(move |_| {
        handle_keyboard_rgb_apply_click(&apply_feedback_for_click, apply_controls.clone());
    });
    apply_row.add_suffix(&apply);
    apply_row.add_suffix(&status_pill(
        if apply_enabled {
            "guarded"
        } else {
            "evidence gate"
        },
        if apply_enabled {
            PillTone::Warning
        } else {
            PillTone::Neutral
        },
    ));
    group.add(&apply_row);
    group.add(&apply_feedback_row);

    group
}

#[derive(Clone)]
struct KeyboardRgbControlWidgets {
    effect: gtk4::DropDown,
    colors: Rc<Vec<(String, gtk4::Entry)>>,
    brightness: gtk4::Scale,
    speed: gtk4::Scale,
}

#[derive(Clone, Copy)]
enum KeyboardRgbPreviewBackend {
    Native,
    OpenRgbBridge,
    OpenRgbSdk,
}

fn keyboard_rgb_mode_choices(bundle: &DiagnosticsBundle) -> Vec<String> {
    if let Some(openrgb) = bundle.raw_probe_report.keyboard_rgb_openrgb.as_ref() {
        if let Some(device) = openrgb.devices.first() {
            if !device.modes.is_empty() {
                return device.modes.clone();
            }
        }
    }
    if let Some(rgb) = bundle.raw_probe_report.keyboard_rgb.as_ref() {
        if !rgb.effects.is_empty() {
            return rgb.effects.clone();
        }
    }
    vec![
        "Direct".to_owned(),
        "Breathing".to_owned(),
        "Rainbow Wave".to_owned(),
        "Spectrum Cycle".to_owned(),
    ]
}

fn selected_keyboard_rgb_mode(bundle: &DiagnosticsBundle, choices: &[String]) -> String {
    let selected = bundle
        .raw_probe_report
        .keyboard_rgb_openrgb
        .as_ref()
        .and_then(|openrgb| openrgb.devices.first())
        .and_then(|device| device.current_mode.as_deref())
        .or_else(|| {
            bundle
                .raw_probe_report
                .keyboard_rgb
                .as_ref()
                .and_then(|rgb| rgb.current_effect.as_deref())
        });
    selected
        .and_then(|selected| {
            choices
                .iter()
                .find(|choice| choice.eq_ignore_ascii_case(selected))
                .cloned()
        })
        .or_else(|| choices.first().cloned())
        .unwrap_or_else(|| "Direct".to_owned())
}

fn keyboard_rgb_color_entries(bundle: &DiagnosticsBundle) -> Vec<(String, String)> {
    if let Some(openrgb) = bundle.raw_probe_report.keyboard_rgb_openrgb.as_ref() {
        if let Some(device) = openrgb.devices.first() {
            if !device.leds.is_empty() {
                return device
                    .leds
                    .iter()
                    .map(|label| (label.clone(), "#00A8FF".to_owned()))
                    .collect();
            }
        }
    }
    if let Some(rgb) = bundle.raw_probe_report.keyboard_rgb.as_ref() {
        if !rgb.zones.is_empty() {
            return rgb
                .zones
                .iter()
                .map(|zone| {
                    let color = rgb
                        .current_colors
                        .get(&zone.id)
                        .cloned()
                        .unwrap_or_else(|| "#00A8FF".to_owned());
                    (zone.id.clone(), color)
                })
                .collect();
        }
    }
    ["Left side", "Left center", "Right center", "Right side"]
        .into_iter()
        .map(|label| (label.to_owned(), "#00A8FF".to_owned()))
        .collect()
}

fn openrgb_plan_available(openrgb: &KeyboardRgbOpenRgbStatus) -> bool {
    openrgb.installed && !openrgb.devices.is_empty()
}

fn openrgb_linux_access_needs_setup(openrgb: &KeyboardRgbOpenRgbStatus) -> bool {
    !openrgb.i2c_dev_loaded
        || !openrgb.user_in_i2c_group
        || !openrgb.has_i2c_rw_access
        || !openrgb.has_hidraw_rw_access
}

fn openrgb_missing_access_summary(openrgb: &KeyboardRgbOpenRgbStatus) -> String {
    let mut missing = Vec::new();
    if !openrgb.i2c_dev_loaded {
        missing.push("i2c-dev");
    }
    if !openrgb.user_in_i2c_group {
        missing.push("i2c group");
    }
    if !openrgb.has_i2c_rw_access {
        missing.push("i2c rw");
    }
    if !openrgb.has_hidraw_rw_access {
        missing.push("hidraw rw");
    }
    if missing.is_empty() {
        "none".to_owned()
    } else {
        missing.join(", ")
    }
}

fn staged_keyboard_rgb_request(
    controls: &KeyboardRgbControlWidgets,
) -> Result<KeyboardRgbWriteRequest> {
    let effect = selected_dropdown_value(&controls.effect).unwrap_or_else(|| "Direct".to_owned());
    let mut colors = BTreeMap::new();
    for (zone_id, entry) in controls.colors.iter() {
        let color = entry.text().trim().to_owned();
        if !is_hex_color(&color) {
            anyhow::bail!("{zone_id} must be a #RRGGBB color");
        }
        colors.insert(zone_id.clone(), color);
    }
    Ok(KeyboardRgbWriteRequest {
        effect,
        colors,
        brightness: scale_u8(&controls.brightness),
        speed: Some(scale_u8(&controls.speed)),
    })
}

fn scale_u8(scale: &gtk4::Scale) -> u8 {
    scale.value().round().clamp(0.0, 100.0) as u8
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..].chars().all(|ch| ch.is_ascii_hexdigit())
}

fn keyboard_rgb_copy_json_button(label: &str, controls: KeyboardRgbControlWidgets) -> gtk4::Button {
    let copy = gtk4::Button::with_label(label);
    copy.set_tooltip_text(Some("Copy the staged keyboard RGB request JSON."));
    copy.add_css_class("pill");
    copy.set_valign(gtk4::Align::Center);
    copy.connect_clicked(move |_| {
        let text = match staged_keyboard_rgb_request(&controls)
            .and_then(|request| Ok(serde_json::to_string_pretty(&request)?))
        {
            Ok(json) => json,
            Err(error) => format!("invalid keyboard RGB request: {error}"),
        };
        if let Some(display) = gtk4::gdk::Display::default() {
            display.clipboard().set_text(&text);
        }
    });
    copy
}

fn handle_keyboard_rgb_preview_click(
    feedback_row: &adw::ActionRow,
    controls: KeyboardRgbControlWidgets,
    preview_backend: KeyboardRgbPreviewBackend,
) {
    let request = match staged_keyboard_rgb_request(&controls) {
        Ok(request) => request,
        Err(error) => {
            feedback_row.set_title("Preview invalid");
            feedback_row.set_subtitle(&error.to_string());
            return;
        }
    };
    let feedback_row = feedback_row.clone();
    feedback_row.set_title("Preview in progress");
    feedback_row.set_subtitle("Request sent to daemon planning method; no RGB write is sent.");

    spawn_dbus_call(
        move || {
            make_client().and_then(|client| match preview_backend {
                KeyboardRgbPreviewBackend::Native => client.plan_keyboard_rgb_write(&request),
                KeyboardRgbPreviewBackend::OpenRgbBridge => {
                    client.plan_openrgb_keyboard_rgb_bridge(&request)
                }
                KeyboardRgbPreviewBackend::OpenRgbSdk => {
                    client.plan_openrgb_keyboard_rgb_sdk_write(&request)
                }
            })
        },
        move |result| match result {
            Ok(plan) => {
                feedback_row.set_title("Preview plan");
                feedback_row.set_subtitle(&render_dry_run_plan_summary(&plan));
            }
            Err(error) => {
                feedback_row.set_title("Preview error");
                feedback_row.set_subtitle(&format!("Failed to build RGB plan: {error}"));
            }
        },
    );
}

fn handle_keyboard_rgb_apply_click(
    feedback_row: &adw::ActionRow,
    controls: KeyboardRgbControlWidgets,
) {
    let request = match staged_keyboard_rgb_request(&controls) {
        Ok(request) => request,
        Err(error) => {
            feedback_row.set_title("Apply invalid");
            feedback_row.set_subtitle(&error.to_string());
            return;
        }
    };
    let feedback_row = feedback_row.clone();
    feedback_row.set_title("Apply in progress");
    feedback_row
        .set_subtitle("Request sent to the daemon; waiting for policy/auth/write result...");

    spawn_dbus_call(
        move || make_client().and_then(|client| client.set_keyboard_rgb(&request)),
        move |result| match result {
            Ok(result) => {
                let title = write_feedback_title(Some(&result));
                let subtitle = write_feedback_subtitle(Some(&result));
                feedback_row.set_title(title);
                feedback_row.set_subtitle(&subtitle);
                store_write_feedback_state("Keyboard RGB", title, &subtitle);
                let _ = request_dashboard_refresh();
            }
            Err(error) => {
                feedback_row.set_title("Apply error");
                let subtitle = format!("Failed - daemon call could not be completed: {error}");
                feedback_row.set_subtitle(&subtitle);
                store_write_feedback_state("Keyboard RGB", "Apply error", &subtitle);
                let _ = request_dashboard_refresh();
            }
        },
    );
}

fn handle_openrgb_bridge_status_click(feedback_row: &adw::ActionRow) {
    let feedback_row = feedback_row.clone();
    feedback_row.set_title("Status check in progress");
    feedback_row.set_subtitle(
        "Capturing OpenRGB readiness, then running the read-only bridge evidence status helper...",
    );

    spawn_dbus_call(run_openrgb_bridge_status, move |result| match result {
        Ok(summary) => {
            feedback_row.set_title("OpenRGB bridge status");
            feedback_row.set_subtitle(&summary);
            store_write_feedback_state("OpenRGB bridge status", "OpenRGB bridge status", &summary);
        }
        Err(error) => {
            feedback_row.set_title("Status check error");
            let subtitle = format!("Failed to run status helper: {error}");
            feedback_row.set_subtitle(&subtitle);
            store_write_feedback_state("OpenRGB bridge status", "Status check error", &subtitle);
        }
    });
}

fn handle_openrgb_bridge_dry_run_click(feedback_row: &adw::ActionRow) {
    let feedback_row = feedback_row.clone();
    feedback_row.set_title("Dry-run capture in progress");
    feedback_row.set_subtitle(
        "Running OpenRGB bridge dry-run evidence capture; no RGB apply command is sent.",
    );

    spawn_dbus_call(
        run_openrgb_bridge_dry_run_capture,
        move |result| match result {
            Ok(summary) => {
                feedback_row.set_title("Dry-run evidence captured");
                feedback_row.set_subtitle(&summary);
                store_write_feedback_state(
                    "OpenRGB bridge dry-run",
                    "Dry-run evidence captured",
                    &summary,
                );
            }
            Err(error) => {
                feedback_row.set_title("Dry-run capture error");
                let subtitle = format!("Failed to capture dry-run evidence: {error}");
                feedback_row.set_subtitle(&subtitle);
                store_write_feedback_state(
                    "OpenRGB bridge dry-run",
                    "Dry-run capture error",
                    &subtitle,
                );
            }
        },
    );
}

fn handle_openrgb_bridge_review_click(feedback_row: &adw::ActionRow) {
    let feedback_row = feedback_row.clone();
    feedback_row.set_title("Review in progress");
    feedback_row.set_subtitle("Running the read-only OpenRGB bridge promotion reviewer...");

    spawn_dbus_call(run_openrgb_bridge_review, move |result| match result {
        Ok(summary) => {
            feedback_row.set_title("Execute evidence review passed");
            feedback_row.set_subtitle(&summary);
            store_write_feedback_state(
                "OpenRGB bridge review",
                "Execute evidence review passed",
                &summary,
            );
        }
        Err(error) => {
            feedback_row.set_title("Execute evidence not promotable");
            let subtitle = format!("{error}");
            feedback_row.set_subtitle(&subtitle);
            store_write_feedback_state(
                "OpenRGB bridge review",
                "Execute evidence not promotable",
                &subtitle,
            );
        }
    });
}

fn handle_openrgb_sdk_evidence_click(feedback_row: &adw::ActionRow) {
    let feedback_row = feedback_row.clone();
    feedback_row.set_title("SDK check in progress");
    feedback_row.set_subtitle(
        "Running the read-only OpenRGB SDK controller evidence probe; no RGB write is sent.",
    );

    spawn_dbus_call(
        run_openrgb_sdk_evidence_capture,
        move |result| match result {
            Ok(summary) => {
                feedback_row.set_title("OpenRGB SDK evidence");
                feedback_row.set_subtitle(&summary);
                store_write_feedback_state(
                    "OpenRGB SDK evidence",
                    "OpenRGB SDK evidence",
                    &summary,
                );
            }
            Err(error) => {
                feedback_row.set_title("SDK check error");
                let subtitle = format!("Failed to capture SDK evidence: {error}");
                feedback_row.set_subtitle(&subtitle);
                store_write_feedback_state("OpenRGB SDK evidence", "SDK check error", &subtitle);
            }
        },
    );
}

fn handle_openrgb_sdk_server_click(feedback_row: &adw::ActionRow) {
    let feedback_row = feedback_row.clone();
    feedback_row.set_title("SDK server start in progress");
    feedback_row
        .set_subtitle("Starting the user-session OpenRGB SDK server; no RGB write is sent.");

    spawn_dbus_call(run_openrgb_sdk_server_start, move |result| match result {
        Ok(summary) => {
            feedback_row.set_title("OpenRGB SDK server");
            feedback_row.set_subtitle(&summary);
            store_write_feedback_state("OpenRGB SDK server", "OpenRGB SDK server", &summary);
        }
        Err(error) => {
            feedback_row.set_title("SDK server error");
            let subtitle = format!("Failed to start OpenRGB SDK server: {error}");
            feedback_row.set_subtitle(&subtitle);
            store_write_feedback_state("OpenRGB SDK server", "SDK server error", &subtitle);
        }
    });
}

fn run_openrgb_bridge_dry_run_capture() -> Result<String> {
    let output = Command::new("ratvantage-capture-keyboard-rgb-openrgb-bridge-evidence")
        .args([
            "--output",
            "target/validation/keyboard-rgb-openrgb-bridge-dry-run",
        ])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        let stderr = stderr.trim();
        anyhow::bail!(
            "exit status {}; {}",
            output.status,
            if stderr.is_empty() {
                "no stderr"
            } else {
                stderr
            }
        );
    }

    let capture = render_bridge_status_output(&stdout);
    let status = run_openrgb_bridge_status()?;
    if capture.is_empty() {
        Ok(status)
    } else {
        Ok(format!("{capture}  {status}"))
    }
}

fn run_openrgb_sdk_server_start() -> Result<String> {
    let output = Command::new("ratvantage-openrgb-sdk-server")
        .arg("start")
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        let stderr = stderr.trim();
        anyhow::bail!(
            "exit status {}; {}",
            output.status,
            if stderr.is_empty() {
                "no stderr"
            } else {
                stderr
            }
        );
    }
    let server = stdout
        .lines()
        .find(|line| line.starts_with("openrgb_sdk_server="))
        .unwrap_or_else(|| stdout.trim());
    if let Ok(client) = make_client() {
        let _ = client.refresh_capabilities();
    }
    let _ = request_dashboard_refresh();
    let status = run_openrgb_bridge_status()
        .unwrap_or_else(|error| format!("status refresh failed after server start: {error}"));
    if status.is_empty() {
        Ok(server.to_owned())
    } else {
        Ok(format!("{server}  {status}"))
    }
}

fn run_openrgb_sdk_evidence_capture() -> Result<String> {
    let output = Command::new("ratvantage-capture-keyboard-rgb-openrgb-sdk-evidence")
        .args(["--output", OPENRGB_SDK_OUTPUT])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        let stderr = stderr.trim();
        anyhow::bail!(
            "exit status {}; {}",
            output.status,
            if stderr.is_empty() {
                "no stderr"
            } else {
                stderr
            }
        );
    }
    let json_path = format!("{OPENRGB_SDK_OUTPUT}/openrgb-keyboard-rgb-sdk-evidence.json");
    let json = std::fs::read_to_string(&json_path)?;
    let report: serde_json::Value = serde_json::from_str(&json)?;
    let result = &report["result"];
    let sdk = &report["sdk"];
    let keyboard = &report["keyboard"];
    let controller = &keyboard["controller"];
    let status = result["status"].as_str().unwrap_or("unknown");
    let connected = sdk["connected"].as_bool().unwrap_or(false);
    let protocol = sdk["protocol_version"]
        .as_i64()
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_owned());
    let keyboard_detected = keyboard["detected"].as_bool().unwrap_or(false);
    let active_mode = controller["active_mode"].as_str().unwrap_or("unknown");
    let color_count = controller["colors"]
        .as_array()
        .map(Vec::len)
        .unwrap_or_default();
    let read_back_supported = result["read_back_supported"].as_bool().unwrap_or(false);
    let blockers = result["promotion_blockers"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .take(2)
                .collect::<Vec<_>>()
                .join("; ")
        })
        .filter(|summary| !summary.is_empty())
        .unwrap_or_else(|| "none".to_owned());
    let capture = render_bridge_status_output(&stdout);
    let summary = format!(
        "status={status} connected={connected} protocol={protocol} keyboard_detected={keyboard_detected} active_mode={active_mode} colors={color_count} read_back_supported={read_back_supported} blockers={blockers}"
    );
    if capture.is_empty() {
        Ok(summary)
    } else {
        Ok(format!("{capture}  {summary}"))
    }
}

fn run_openrgb_bridge_review() -> Result<String> {
    let output = Command::new("ratvantage-review-keyboard-rgb-openrgb-bridge-evidence")
        .args([
            "--require-promotable",
            "target/validation/keyboard-rgb-openrgb-bridge-execute",
        ])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let summary = render_bridge_status_output(&stdout);
    if output.status.success() {
        if summary.is_empty() {
            Ok("Execute evidence is promotable; add production backend policy gates before enabling Apply RGB.".to_owned())
        } else {
            Ok(summary)
        }
    } else {
        let stderr = stderr.trim();
        let detail = if !summary.is_empty() {
            summary
        } else if !stderr.is_empty() {
            stderr.to_owned()
        } else {
            "reviewer returned no detail".to_owned()
        };
        anyhow::bail!("{detail}")
    }
}

fn run_openrgb_bridge_status() -> Result<String> {
    let readiness = run_openrgb_readiness_capture()?;
    let output = Command::new(OPENRGB_BRIDGE_STATUS_BIN)
        .args([
            "--readiness",
            OPENRGB_READINESS_OUTPUT,
            "--sdk",
            OPENRGB_SDK_OUTPUT,
            "--sdk-write",
            OPENRGB_SDK_WRITE_OUTPUT,
        ])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        let stderr = stderr.trim();
        anyhow::bail!(
            "exit status {}; {}",
            output.status,
            if stderr.is_empty() {
                "no stderr"
            } else {
                stderr
            }
        );
    }
    let summary = render_bridge_status_output(&stdout);
    if summary.is_empty() {
        anyhow::bail!("status helper returned no output");
    }
    if readiness.is_empty() {
        Ok(summary)
    } else {
        Ok(format!("{readiness}  {summary}"))
    }
}

fn run_openrgb_readiness_capture() -> Result<String> {
    let output = Command::new("ratvantage-check-keyboard-rgb-openrgb")
        .args(["--output", OPENRGB_READINESS_OUTPUT])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        let stderr = stderr.trim();
        anyhow::bail!(
            "readiness exit status {}; {}",
            output.status,
            if stderr.is_empty() {
                "no stderr"
            } else {
                stderr
            }
        );
    }
    Ok(render_bridge_status_output(&stdout))
}

fn render_bridge_status_output(stdout: &str) -> String {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("  ")
}

fn copy_command_button(label: &str, command: &'static str, tooltip: &str) -> gtk4::Button {
    let copy = gtk4::Button::with_label(label);
    copy.set_tooltip_text(Some(tooltip));
    copy.add_css_class("pill");
    copy.set_valign(gtk4::Align::Center);
    copy.connect_clicked(move |_| {
        if let Some(display) = gtk4::gdk::Display::default() {
            display.clipboard().set_text(command);
        }
    });
    copy
}

fn current_setup_user() -> String {
    env::var("USER")
        .or_else(|_| env::var("LOGNAME"))
        .unwrap_or_else(|_| "current user".to_owned())
}

fn handle_openrgb_access_setup_click(feedback_row: &adw::ActionRow) {
    let feedback_row = feedback_row.clone();
    let target_user = current_setup_user();

    feedback_row.set_title("Setup in progress");
    feedback_row
        .set_subtitle("Request sent to the daemon; waiting for policy/auth/setup result...");

    spawn_dbus_call(
        move || make_client().and_then(|client| client.setup_openrgb_access(&target_user)),
        move |result| match result {
            Ok(result) => {
                let title = write_feedback_title(Some(&result));
                let subtitle = write_feedback_subtitle(Some(&result));
                feedback_row.set_title(title);
                feedback_row.set_subtitle(&subtitle);
                store_write_feedback_state(OPENRGB_ACCESS_SETUP_LABEL, title, &subtitle);
                let _ = request_dashboard_refresh();
            }
            Err(error) => {
                feedback_row.set_title("Setup error");
                let subtitle = format!("Failed - daemon call could not be completed: {error}");
                feedback_row.set_subtitle(&subtitle);
                store_write_feedback_state(OPENRGB_ACCESS_SETUP_LABEL, "Setup error", &subtitle);
                let _ = request_dashboard_refresh();
            }
        },
    );
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

fn format_keyboard_rgb_candidate_summary(candidates: &[KeyboardRgbCandidate]) -> String {
    let devices = candidates
        .iter()
        .map(|candidate| candidate.device_id.to_ascii_lowercase())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut parts = vec![format!("{} HID research candidates", candidates.len())];
    if !devices.is_empty() {
        parts.push(format!("devices={}", devices.join(", ")));
    }
    parts.push("backend_ready=false".to_owned());
    parts.join("; ")
}

fn format_keyboard_rgb_openrgb_summary(
    openrgb: &legion_common::KeyboardRgbOpenRgbStatus,
) -> String {
    let device = openrgb
        .devices
        .first()
        .map(|device| {
            format!(
                "{} ({})",
                device.description.as_deref().unwrap_or(&device.name),
                if device.modes.is_empty() {
                    "modes unknown".to_owned()
                } else {
                    format!("modes: {}", device.modes.join(", "))
                }
            )
        })
        .unwrap_or_else(|| "no Lenovo keyboard RGB device detected by OpenRGB".to_owned());
    format!(
        "{device}; i2c_dev_loaded={} user_in_i2c_group={} i2c_rw={} hidraw_rw={} sdk_helper={} sdk_server={} sdk_snapshot={} backend_ready={}",
        openrgb.i2c_dev_loaded,
        openrgb.user_in_i2c_group,
        openrgb.has_i2c_rw_access,
        openrgb.has_hidraw_rw_access,
        openrgb.sdk_helper_installed,
        openrgb.sdk_server_running,
        openrgb.sdk_snapshot_supported,
        openrgb.backend_ready
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_status_renderer_keeps_sdk_and_next_action_lines() {
        let rendered = render_bridge_status_output(
            "\
dry_run=present status=dry_run
execute=present status=executed
readiness=present ready_for_execute=true
sdk=present status=keyboard_not_found connected=true
next_action=review execute bundle and SDK read-back failures before promotion
",
        );

        assert!(rendered.contains("sdk=present status=keyboard_not_found"));
        assert!(rendered.contains(
            "next_action=review execute bundle and SDK read-back failures before promotion"
        ));
    }
}
