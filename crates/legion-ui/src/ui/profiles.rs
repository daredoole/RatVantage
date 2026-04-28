use crate::{capability_status_label, DiagnosticsBundle, LegionControlClient};
use adw::prelude::*;
use anyhow::Result;
use legion_common::PlatformProfileCapability;

use super::shared::{
    append_error, build_write_controls, build_write_feedback_group, info_row,
    request_dashboard_refresh, spawn_dbus_call,
};

pub fn profiles_page(diagnostics: Result<DiagnosticsBundle>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    match diagnostics {
        Ok(bundle) => append_profiles(&page, &bundle),
        Err(error) => append_error(&page, &error),
    }

    page
}

fn append_profiles(page: &adw::PreferencesPage, bundle: &DiagnosticsBundle) {
    let group = adw::PreferencesGroup::new();
    group.set_title("Platform Profile");
    if let Some(profile) = &bundle.raw_probe_report.platform_profile {
        let current = info_row("Current", profile.current.as_deref().unwrap_or("unknown"));
        group.add(&current);
        group.add(&info_row("Choices", &profile.choices.join(", ")));
        group.add(&info_row("Profile path", &profile.path));
        group.add(&info_row("Choices path", &profile.choices_path));
        page.add(&group);

        let controls = build_platform_profile_controls(
            bundle.raw_probe_report.platform_profile.as_ref(),
            Some(current),
        );
        page.add(&controls);
    } else {
        group.add(&info_row("Platform profile", "unavailable"));
        page.add(&group);

        let controls = build_platform_profile_controls(None, None);
        page.add(&controls);
    }

    let feedback = build_write_feedback_group("Platform profile");
    page.add(&feedback);

    if let Some(pp) = &bundle.raw_probe_report.power_profiles {
        let desktop = adw::PreferencesGroup::new();
        desktop.set_title("Desktop PowerProfiles");
        desktop.set_description(Some(
            "Session D-Bus `org.freedesktop.UPower.PowerProfiles` (often power-profiles-daemon).",
        ));
        desktop.add(&info_row("Bus", &pp.bus));
        desktop.add(&info_row("Well-known name", &pp.well_known_name));
        if let Some(owner) = &pp.unique_owner {
            desktop.add(&info_row("Unique owner", owner));
            desktop.add(&info_row(
                "Active profile",
                pp.active_profile.as_deref().unwrap_or("unknown"),
            ));
        } else if let Some(detail) = &pp.detail {
            desktop.add(&info_row("Unavailable", detail));
        } else {
            desktop.add(&info_row("Unavailable", "no D-Bus owner"));
        }
        desktop.add(&info_row(
            "Probe status",
            capability_status_label(pp.status),
        ));
        page.add(&desktop);
    }
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
            if !request_dashboard_refresh() {
                if let Some(row) = &current_row {
                    refresh_platform_profile_row(row);
                }
            }
        },
    )
}

fn refresh_platform_profile_row(row: &adw::ActionRow) {
    let row = row.clone();
    spawn_dbus_call(
        || LegionControlClient::system().and_then(|client| client.refresh_runtime_snapshot()),
        move |result| {
            if let Ok(snapshot) = result {
                if let Some(profile) = snapshot.diagnostics.raw_probe_report.platform_profile {
                    row.set_subtitle(profile.current.as_deref().unwrap_or("unknown"));
                }
            }
        },
    );
}
