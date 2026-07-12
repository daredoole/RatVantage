use crate::{DiagnosticsBundle, GpuModePending, UiStatus};
use adw::prelude::*;
use anyhow::Result;
use legion_common::CapabilityRegistry;

use super::shared::{append_error, info_row, section_note, state_tile, PillTone};

pub fn status_page(
    status: Result<UiStatus>,
    diagnostics: Result<DiagnosticsBundle>,
    gpu_pending: Result<Option<GpuModePending>>,
) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    match status {
        Ok(status) => append_status(&page, &status, diagnostics, gpu_pending),
        Err(error) => append_error(&page, &error),
    }

    page
}

fn append_status(
    page: &adw::PreferencesPage,
    status: &UiStatus,
    diagnostics: Result<DiagnosticsBundle>,
    gpu_pending: Result<Option<GpuModePending>>,
) {
    append_runtime_overview(page, diagnostics, &gpu_pending);

    let group = adw::PreferencesGroup::new();
    group.set_title("This PC");
    group.add(&info_row("Product", &status.hardware.product_name));
    if !status.hardware.product_version.trim().is_empty() {
        group.add(&info_row("Model", &status.hardware.product_version));
    }
    if gpu_pending
        .as_ref()
        .ok()
        .and_then(|pending| pending.as_ref())
        .is_some()
    {
        group.add(&info_row(
            "Graphics change",
            &render_gpu_pending_row(gpu_pending),
        ));
    }
    page.add(&group);
}

fn append_runtime_overview(
    page: &adw::PreferencesPage,
    diagnostics: Result<DiagnosticsBundle>,
    gpu_pending: &Result<Option<GpuModePending>>,
) {
    let group = adw::PreferencesGroup::new();
    group.set_title("At a glance");

    match diagnostics {
        Ok(bundle) => {
            let report = &bundle.raw_probe_report;
            group.add(&section_note(
                "Live hardware state. Changes are shown here only after the system confirms them.",
            ));
            let grid = gtk4::Grid::new();
            grid.set_column_spacing(12);
            grid.set_row_spacing(12);
            grid.set_margin_top(6);
            grid.set_margin_bottom(6);
            grid.set_margin_start(6);
            grid.set_margin_end(6);
            grid.set_hexpand(true);

            let tiles = [
                (
                    "Power mode",
                    platform_profile(report),
                    format!("Fedora: {}", desktop_power_profile(report)),
                    profile_tone(
                        report
                            .platform_profile
                            .as_ref()
                            .and_then(|p| p.current.as_deref()),
                    ),
                ),
                (
                    "Battery",
                    battery_summary(report),
                    battery_charge_type(report),
                    PillTone::Good,
                ),
                (
                    "Cooling",
                    fan_summary(report),
                    "Live fan speed".to_owned(),
                    PillTone::Neutral,
                ),
                (
                    "Graphics",
                    gpu_summary(report, gpu_pending),
                    gpu_detail(gpu_pending),
                    PillTone::Neutral,
                ),
            ];
            for (index, (title, value, detail, tone)) in tiles.into_iter().enumerate() {
                let tile = state_tile(title, &value, &detail, tone);
                grid.attach(&tile, (index % 2) as i32, (index / 2) as i32, 1, 1);
            }
            group.add(&grid);
        }
        Err(error) => {
            group.add(&info_row(
                "Runtime overview",
                &format!("unavailable - {error}"),
            ));
        }
    }

    page.add(&group);
}

fn render_gpu_pending_row(pending: Result<Option<GpuModePending>>) -> String {
    match pending {
        Ok(opt) => legion_common::format_gpu_mode_pending_summary(opt.as_ref()),
        Err(error) => format!("state unavailable - {error}"),
    }
}

fn profile_tone(value: Option<&str>) -> PillTone {
    match value {
        Some("performance") => PillTone::Warning,
        Some("balanced") => PillTone::Good,
        Some("quiet" | "low-power" | "power-saver") => PillTone::Good,
        Some(_) => PillTone::Neutral,
        None => PillTone::Error,
    }
}

fn platform_profile(report: &CapabilityRegistry) -> String {
    report
        .platform_profile
        .as_ref()
        .and_then(|profile| profile.current.clone())
        .unwrap_or_else(|| "unavailable".to_owned())
}

fn desktop_power_profile(report: &CapabilityRegistry) -> String {
    report
        .power_profiles
        .as_ref()
        .and_then(|profile| profile.active_profile.clone())
        .unwrap_or_else(|| "unavailable".to_owned())
}

fn battery_charge_type(report: &CapabilityRegistry) -> String {
    report
        .battery_charge_type
        .as_ref()
        .and_then(|charge_type| charge_type.current.clone())
        .unwrap_or_else(|| "unavailable".to_owned())
}

fn battery_summary(report: &CapabilityRegistry) -> String {
    let Some(battery) = &report.telemetry.battery else {
        return "unavailable".to_owned();
    };
    let mut parts = Vec::new();
    if let Some(percent) = battery.capacity_percent {
        parts.push(format!("{percent}%"));
    }
    if let Some(status) = &battery.status {
        parts.push(status.clone());
    }
    if let Some(health) = &battery.health {
        parts.push(format!("Health {health}"));
    }
    if parts.is_empty() {
        battery.name.clone()
    } else {
        parts.join(" · ")
    }
}

fn gpu_summary(report: &CapabilityRegistry, pending: &Result<Option<GpuModePending>>) -> String {
    if let Ok(Some(pending)) = pending {
        return legion_common::format_gpu_mode_pending_summary(Some(pending));
    }
    report
        .gpu
        .as_ref()
        .and_then(|gpu| gpu.mode.clone())
        .unwrap_or_else(|| "unavailable".to_owned())
}

fn gpu_detail(pending: &Result<Option<GpuModePending>>) -> String {
    match pending {
        Ok(Some(_)) => "reboot pending".to_owned(),
        Ok(None) => "active".to_owned(),
        Err(_) => "unknown".to_owned(),
    }
}

fn fan_summary(report: &CapabilityRegistry) -> String {
    let fans = report
        .telemetry
        .sensors
        .iter()
        .filter(|sensor| sensor.kind == "fan")
        .filter_map(|sensor| {
            sensor.value.map(|value| match sensor.label.as_deref() {
                Some(label) if !label.is_empty() => format!("{label} {value} RPM"),
                _ => format!("{value} RPM"),
            })
        })
        .collect::<Vec<_>>();

    if fans.is_empty() {
        "unavailable".to_owned()
    } else {
        fans.join(", ")
    }
}
