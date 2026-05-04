use crate::DiagnosticsBundle;
use adw::prelude::*;
use anyhow::Result;
use legion_common::BatteryChargeTypeCapability;

use super::shared::{
    append_error, build_write_controls, build_write_feedback_group, info_row, make_client,
    request_dashboard_refresh, section_note, spawn_dbus_call,
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

    page.add(&build_write_feedback_group("Battery charge type"));

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
    } else {
        telemetry.add(&info_row("Battery telemetry", "unavailable"));
    }
    page.add(&telemetry);
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
