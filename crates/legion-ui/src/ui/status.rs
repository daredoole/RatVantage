use crate::{capability_status_label, DiagnosticsBundle, GpuModePending, UiStatus};
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
    group.set_title("Machine");
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
    page.add(&group);

    let capabilities = adw::PreferencesGroup::new();
    capabilities.set_title("Capability Health");
    capabilities.set_description(Some(
        "Detailed paths and raw probe evidence live in Diagnostics.",
    ));
    for capability in &status.capabilities {
        let row = adw::ActionRow::builder()
            .title(&capability.label)
            .subtitle(capability_status_label(capability.status))
            .selectable(false)
            .build();
        row.add_suffix(&super::shared::risk_badge(capability.risk));
        capabilities.add(&row);
    }
    page.add(&capabilities);
}

fn append_runtime_overview(
    page: &adw::PreferencesPage,
    diagnostics: Result<DiagnosticsBundle>,
    gpu_pending: &Result<Option<GpuModePending>>,
) {
    let group = adw::PreferencesGroup::new();
    group.set_title("Control Center");

    match diagnostics {
        Ok(bundle) => {
            let report = &bundle.raw_probe_report;
            group.add(&section_note(
                "Current daemon readback. Writes count only after kernel read-back agrees.",
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
                    "Platform",
                    platform_profile(report),
                    "kernel profile".to_owned(),
                    profile_tone(
                        report
                            .platform_profile
                            .as_ref()
                            .and_then(|p| p.current.as_deref()),
                    ),
                ),
                (
                    "Fedora power",
                    desktop_power_profile(report),
                    "desktop profile".to_owned(),
                    profile_tone(
                        report
                            .power_profiles
                            .as_ref()
                            .and_then(|p| p.active_profile.as_deref()),
                    ),
                ),
                (
                    "Battery",
                    battery_summary(report),
                    battery_charge_type(report),
                    PillTone::Good,
                ),
                (
                    "GPU",
                    gpu_summary(report, gpu_pending),
                    gpu_detail(gpu_pending),
                    PillTone::Neutral,
                ),
                (
                    "Fans",
                    fan_summary(report),
                    "telemetry".to_owned(),
                    PillTone::Neutral,
                ),
                (
                    "Devices",
                    devices_summary(report),
                    "firmware toggles".to_owned(),
                    PillTone::Warning,
                ),
            ];
            for (index, (title, value, detail, tone)) in tiles.into_iter().enumerate() {
                let tile = state_tile(title, &value, &detail, tone);
                grid.attach(&tile, (index % 3) as i32, (index / 3) as i32, 1, 1);
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

fn toggle_state(report: &CapabilityRegistry, name: &str) -> String {
    report
        .ideapad_toggles
        .iter()
        .find(|toggle| toggle.name == name)
        .and_then(|toggle| toggle.current_value.as_deref())
        .map(binary_state)
        .unwrap_or_else(|| "unavailable".to_owned())
}

fn led_state(report: &CapabilityRegistry, name: &str) -> String {
    report
        .leds
        .iter()
        .find(|led| led.name == name)
        .and_then(|led| led.brightness)
        .map(|brightness| binary_state(&brightness.to_string()))
        .unwrap_or_else(|| "unavailable".to_owned())
}

fn binary_state(value: &str) -> String {
    match value {
        "0" => "off".to_owned(),
        "1" => "on".to_owned(),
        other => other.to_owned(),
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

fn devices_summary(report: &CapabilityRegistry) -> String {
    [
        format!("Fn lock {}", toggle_state(report, "fn_lock")),
        format!("USB charge {}", toggle_state(report, "usb_charging")),
        format!("Logo {}", led_state(report, "platform::ylogo")),
        format!("Camera {}", toggle_state(report, "camera_power")),
    ]
    .join(" · ")
}
