use crate::{capability_status_label, DiagnosticsBundle};
use adw::prelude::*;
use anyhow::Result;
use legion_common::PlatformProfileCapability;

use super::shared::{
    append_error, build_write_controls, build_write_feedback_group, info_row, make_client,
    request_dashboard_refresh, section_note, spawn_dbus_call,
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
    group.set_title("Power Profiles");
    group.add(&section_note(
        "Platform is firmware. Fedora power is the desktop profile. RatVantage writes one path and reads both back.",
    ));
    if let Some(profile) = &bundle.raw_probe_report.platform_profile {
        let current = info_row(
            "Platform profile",
            profile.current.as_deref().unwrap_or("unknown"),
        );
        group.add(&current);
        if let Some(power_profile) = bundle
            .raw_probe_report
            .power_profiles
            .as_ref()
            .and_then(|profile| profile.active_profile.as_deref())
        {
            group.add(&info_row("Plasma/Fedora power profile", power_profile));
        } else {
            group.add(&info_row("Plasma/Fedora power profile", "unavailable"));
        }
        group.add(&info_row("Choices", &profile.choices.join(", ")));
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

    page.add(&build_write_feedback_group("Platform profile"));

    if let Some(pp) = &bundle.raw_probe_report.power_profiles {
        let desktop = adw::PreferencesGroup::new();
        desktop.set_title("Desktop Profile Detail");
        desktop.add(&info_row("Bus", &pp.bus));
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
        "Platform Control",
        capability.map(|capability| capability.current.as_deref().unwrap_or("unknown")),
        capability.map(|capability| capability.choices.as_slice()),
        "Requested profile",
        "Apply profile",
        "Platform profile",
        |requested| make_client().and_then(|client| client.set_platform_profile(requested)),
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
        || make_client().and_then(|client| client.refresh_runtime_snapshot()),
        move |result| {
            if let Ok(snapshot) = result {
                if let Some(profile) = snapshot.diagnostics.raw_probe_report.platform_profile {
                    row.set_subtitle(profile.current.as_deref().unwrap_or("unknown"));
                }
            }
        },
    );
}
