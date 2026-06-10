use crate::DiagnosticsBundle;
use adw::prelude::*;
use anyhow::Result;
use legion_common::{BatteryChargeTypeCapability, IdeapadToggleCapability};

use super::shared::{
    append_error, build_write_controls, info_row, make_client, request_dashboard_refresh,
    section_note, spawn_dbus_call,
};

pub fn battery_page(diagnostics: Result<DiagnosticsBundle>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    match diagnostics {
        Ok(bundle) => append_battery(&page, &bundle),
        Err(error) => append_error(&page, &error),
    }

    page
}

fn append_battery(page: &adw::PreferencesPage, bundle: &DiagnosticsBundle) {
    let charge_type = adw::PreferencesGroup::new();
    charge_type.set_title("Battery Charging");
    charge_type.add(&section_note(
        "Charge mode changes are applied through the daemon and verified by read-back.",
    ));
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
            "Modes",
            &charge_type_capability.choices.join(", "),
        ));
        page.add(&charge_type);

        let controls = build_battery_charge_type_controls(
            bundle.raw_probe_report.battery_charge_type.as_ref(),
            Some(current),
        );
        page.add(&controls);
    } else {
        charge_type.add(&info_row("Charge type", "unavailable"));
        page.add(&charge_type);

        let controls = build_battery_charge_type_controls(None, None);
        page.add(&controls);
    }

    let conservation = bundle
        .raw_probe_report
        .ideapad_toggles
        .iter()
        .find(|toggle| toggle.name == "conservation_mode");
    page.add(&build_conservation_mode_controls(conservation));

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
        if let Some(uw) = battery.power_now_uw {
            telemetry.add(&info_row(
                "Power draw",
                &format!("{:.1} W", uw as f64 / 1_000_000.0),
            ));
        }
        if let Some(cycles) = battery.cycle_count {
            telemetry.add(&info_row("Cycle count", &cycles.to_string()));
        }
        if let (Some(now), Some(full)) = (battery.energy_now_uwh, battery.energy_full_uwh) {
            telemetry.add(&info_row(
                "Energy",
                &format!(
                    "{:.1} / {:.1} Wh",
                    now as f64 / 1_000_000.0,
                    full as f64 / 1_000_000.0
                ),
            ));
        }
        if let (Some(full), Some(design)) =
            (battery.energy_full_uwh, battery.energy_full_design_uwh)
        {
            if design > 0 {
                telemetry.add(&info_row(
                    "Wear level",
                    &format!(
                        "{:.0}% of design ({:.1} Wh)",
                        full as f64 / design as f64 * 100.0,
                        design as f64 / 1_000_000.0
                    ),
                ));
            }
        }
        if let Some(uv) = battery.voltage_now_uv {
            telemetry.add(&info_row(
                "Voltage",
                &format!("{:.2} V", uv as f64 / 1_000_000.0),
            ));
        }
        if let Some(level) = &battery.capacity_level {
            telemetry.add(&info_row("Capacity level", level));
        }
        if let Some(tech) = &battery.technology {
            telemetry.add(&info_row("Technology", tech));
        }
        if let Some(model) = &battery.model_name {
            telemetry.add(&info_row("Model", model));
        }
        if let Some(mfr) = &battery.manufacturer {
            telemetry.add(&info_row("Manufacturer", mfr));
        }
    } else {
        telemetry.add(&info_row("Battery telemetry", "unavailable"));
    }
    page.add(&telemetry);

    let ac_adapters = &bundle.raw_probe_report.telemetry.ac_adapters;
    if !ac_adapters.is_empty() {
        let ac_group = adw::PreferencesGroup::new();
        ac_group.set_title("AC Adapter");
        for adapter in ac_adapters {
            let state = match adapter.online {
                Some(true) => "connected",
                Some(false) => "disconnected",
                None => "unknown",
            };
            ac_group.add(&info_row(&adapter.name, state));
        }
        page.add(&ac_group);
    }
}

fn build_conservation_mode_controls(
    toggle: Option<&IdeapadToggleCapability>,
) -> adw::PreferencesGroup {
    let choices = vec!["Off (0)".to_owned(), "On (1)".to_owned()];
    let current_label = toggle
        .and_then(|toggle| toggle.current_value.as_deref())
        .map(|value| match value {
            "0" => "Off (0)",
            "1" => "On (1)",
            other => other,
        });
    let group = build_write_controls(
        "Conservation Mode Control",
        current_label,
        toggle.map(|_| choices.as_slice()),
        "Requested conservation mode",
        "Apply conservation mode",
        "Conservation mode",
        |requested| {
            let value = if requested.starts_with("On") {
                "1"
            } else {
                "0"
            };
            make_client().and_then(|client| client.set_conservation_mode(value))
        },
        move |_| {
            request_dashboard_refresh();
        },
    );
    group.add(&section_note(
        "This is the ideapad_acpi conservation_mode toggle. On some firmware it mirrors the battery charge-type Long_Life/Conservation mode; verify both read-backs after changing either control.",
    ));
    group
}

fn build_battery_charge_type_controls(
    capability: Option<&BatteryChargeTypeCapability>,
    current_row: Option<adw::ActionRow>,
) -> adw::PreferencesGroup {
    build_write_controls(
        "Charge Control",
        capability.map(|capability| capability.current.as_deref().unwrap_or("unknown")),
        capability.map(|capability| capability.choices.as_slice()),
        "Requested charge type",
        "Apply charge type",
        "Battery charge type",
        |requested| make_client().and_then(|client| client.set_battery_charge_type(requested)),
        move |_| {
            if !request_dashboard_refresh() {
                if let Some(row) = &current_row {
                    refresh_battery_charge_type_row(row);
                }
            }
        },
    )
}

fn refresh_battery_charge_type_row(row: &adw::ActionRow) {
    let row = row.clone();
    spawn_dbus_call(
        || make_client().and_then(|client| client.refresh_runtime_snapshot()),
        move |result| {
            if let Ok(snapshot) = result {
                if let Some(charge_type) = snapshot.diagnostics.raw_probe_report.battery_charge_type
                {
                    row.set_subtitle(charge_type.current.as_deref().unwrap_or("unknown"));
                }
            }
        },
    );
}
