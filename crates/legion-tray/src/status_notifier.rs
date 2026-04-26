use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use ksni::blocking::TrayMethods;
use ksni::menu::StandardItem;
use ksni::{Category, MenuItem, Status, ToolTip, Tray};
use legion_control_ui::LegionControlClient;

use crate::{DesktopSession, TrayAction, TrayMenu, TrayMenuEntry, TrayMenuItem, TraySummary};

const TRAY_ID: &str = "org.ratvantage.LegionControl";
const ICON_NAME: &str = "applications-system";

pub struct StatusNotifierTray {
    summary: TraySummary,
    menu: TrayMenu,
    bus_address: Option<String>,
    shutdown_requested: Arc<AtomicBool>,
    last_error: Option<String>,
}

impl StatusNotifierTray {
    pub fn new(
        summary: TraySummary,
        menu: TrayMenu,
        bus_address: Option<String>,
        shutdown_requested: Arc<AtomicBool>,
    ) -> Self {
        Self {
            summary,
            menu,
            bus_address,
            shutdown_requested,
            last_error: None,
        }
    }

    pub fn summary(&self) -> &TraySummary {
        &self.summary
    }

    fn refresh_status(&mut self) {
        match load_tray_state(self.bus_address.as_deref()) {
            Ok(state) => {
                self.summary = state.summary;
                self.menu = state.menu;
                self.last_error = None;
            }
            Err(error) => {
                self.last_error = Some(error.to_string());
            }
        }
    }

    fn open_dashboard(&mut self) {
        let mut command = Command::new("legion-control-ui");
        command.args(dashboard_command_args(self.bus_address.as_deref()));
        if let Err(error) = command.spawn() {
            self.last_error = Some(format!("failed to open dashboard: {error}"));
        }
    }

    fn request_shutdown(&mut self) {
        self.shutdown_requested.store(true, Ordering::Relaxed);
    }

    fn tooltip_description(&self) -> String {
        match &self.last_error {
            Some(error) => format!("{}\nLast tray error: {error}", self.summary.tooltip),
            None => self.summary.tooltip.clone(),
        }
    }
}

impl Tray for StatusNotifierTray {
    const MENU_ON_ACTIVATE: bool = true;

    fn id(&self) -> String {
        TRAY_ID.to_owned()
    }

    fn category(&self) -> Category {
        Category::Hardware
    }

    fn title(&self) -> String {
        self.summary.title.clone()
    }

    fn status(&self) -> Status {
        Status::Active
    }

    fn icon_name(&self) -> String {
        ICON_NAME.to_owned()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            icon_name: ICON_NAME.to_owned(),
            title: self.summary.title.clone(),
            description: self.tooltip_description(),
            ..ToolTip::default()
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        self.menu.entries.iter().cloned().map(menu_entry).collect()
    }
}

pub fn run_status_notifier_tray(bus_address: Option<String>) -> Result<()> {
    if let Some(guidance) = DesktopSession::from_env().status_notifier_guidance() {
        eprintln!("Desktop note: {guidance}");
    }

    let state = load_tray_state(bus_address.as_deref())?;
    let shutdown_requested = Arc::new(AtomicBool::new(false));
    let tray = StatusNotifierTray::new(
        state.summary,
        state.menu,
        bus_address,
        Arc::clone(&shutdown_requested),
    );
    let handle = tray
        .spawn()
        .context("failed to register StatusNotifier tray item")?;

    while !shutdown_requested.load(Ordering::Relaxed) && !handle.is_closed() {
        thread::sleep(Duration::from_millis(250));
    }

    handle.shutdown().wait();
    Ok(())
}

struct LoadedTrayState {
    summary: TraySummary,
    menu: TrayMenu,
}

fn load_tray_state(bus_address: Option<&str>) -> Result<LoadedTrayState> {
    let client = match bus_address {
        Some(address) => LegionControlClient::address(address)?,
        None => LegionControlClient::system()?,
    };
    let status = client.status()?;
    let report = client.raw_probe_report()?;
    let gpu_pending = client.gpu_mode_pending()?;
    let fan_snapshot = client.last_known_good_fan_curve()?;
    Ok(LoadedTrayState {
        summary: TraySummary::from_status_and_report(
            &status,
            &report,
            gpu_pending.as_ref(),
            fan_snapshot.as_ref(),
        ),
        menu: TrayMenu::from_status_and_report(
            &status,
            &report,
            gpu_pending.as_ref(),
            fan_snapshot.as_ref(),
        ),
    })
}

fn dashboard_command_args(bus_address: Option<&str>) -> Vec<String> {
    match bus_address {
        Some(address) => vec!["--bus-address".to_owned(), address.to_owned()],
        None => Vec::new(),
    }
}

fn menu_entry(entry: TrayMenuEntry) -> MenuItem<StatusNotifierTray> {
    match entry {
        TrayMenuEntry::Separator => MenuItem::Separator,
        TrayMenuEntry::Item(item) => menu_item(item),
    }
}

fn menu_item(item: TrayMenuItem) -> MenuItem<StatusNotifierTray> {
    let TrayMenuItem {
        label,
        action,
        enabled,
    } = item;

    StandardItem {
        label,
        enabled,
        activate: Box::new(move |tray: &mut StatusNotifierTray| match action {
            TrayAction::NoOp => {}
            TrayAction::OpenDashboard => tray.open_dashboard(),
            TrayAction::RefreshStatus => tray.refresh_status(),
            TrayAction::Quit => tray.request_shutdown(),
        }),
        ..StandardItem::default()
    }
    .into()
}

#[cfg(test)]
mod tests {
    use legion_common::{Capability, CapabilityStatus, HardwareSummary, RiskLevel};
    use legion_control_ui::UiStatus;

    use super::*;

    #[test]
    fn status_notifier_tray_exposes_read_only_identity_and_tooltip() {
        let shutdown_requested = Arc::new(AtomicBool::new(false));
        let tray = StatusNotifierTray::new(
            summary(),
            menu(),
            Some("unix:path=/tmp/test-bus".to_owned()),
            shutdown_requested,
        );

        assert_eq!(tray.id(), TRAY_ID);
        assert_eq!(tray.category(), Category::Hardware);
        assert_eq!(tray.title(), "Legion Control");
        assert_eq!(tray.status(), Status::Active);
        assert_eq!(tray.icon_name(), ICON_NAME);
        assert_eq!(
            tray.tool_tip().description,
            "82WM Legion Pro 5 16ARX8: 1 available capabilities"
        );
    }

    #[test]
    fn status_notifier_menu_reflects_runtime_state() {
        let shutdown_requested = Arc::new(AtomicBool::new(false));
        let tray = StatusNotifierTray::new(summary(), menu(), None, shutdown_requested);
        let menu = tray.menu();

        assert!(menu_has_disabled_item(&menu, "82WM Legion Pro 5 16ARX8"));
        assert!(menu_has_disabled_item(&menu, "Platform profile: balanced"));
        assert!(menu_has_disabled_item(
            &menu,
            "Profile choices: low-power, balanced, performance"
        ));
        assert!(menu_has_disabled_item(
            &menu,
            "Battery charge type: Standard"
        ));
        assert!(menu_has_disabled_item(
            &menu,
            "Charge choices: Standard, Conservation, Fast"
        ));
        assert!(menu_has_disabled_item(
            &menu,
            "Battery: 79% / Charging / Good"
        ));
        assert!(menu_has_disabled_item(&menu, "Fan: CPU Fan 2410 RPM"));
        assert!(menu_has_disabled_item(
            &menu,
            "GPU pending: hybrid (previous nvidia, reboot required)"
        ));
        assert!(menu_has_disabled_item(
            &menu,
            "Saved fan curve: 1 values from legion_hwmon"
        ));
        assert!(menu_has_disabled_item(
            &menu,
            "Fan presets: Quiet office, Balanced daily, Gaming, Max safe"
        ));
        assert!(menu_has_disabled_item(
            &menu,
            "Capabilities: 1 available, 1 missing"
        ));
        assert!(menu_has_disabled_item(&menu, "Missing: gpu"));
        assert!(menu_has_enabled_item(&menu, "Open dashboard"));
        assert!(menu_has_enabled_item(&menu, "Refresh status"));
        assert!(menu_has_enabled_item(&menu, "Quit"));
        assert!(!menu_has_item_starting_with(&menu, "Set platform profile"));
        assert!(!menu_has_item_starting_with(
            &menu,
            "Set battery charge type:"
        ));
        assert!(!menu_has_item_starting_with(&menu, "Apply preset:"));
        assert!(!menu_has_item_starting_with(&menu, "Toggle logo LED"));
    }

    #[test]
    fn dashboard_command_forwards_private_bus_address() {
        assert!(dashboard_command_args(None).is_empty());
        assert_eq!(
            dashboard_command_args(Some("unix:path=/tmp/test-bus")),
            vec![
                "--bus-address".to_owned(),
                "unix:path=/tmp/test-bus".to_owned()
            ]
        );
    }

    fn summary() -> TraySummary {
        let status = UiStatus::from_parts(
            HardwareSummary {
                sysfs_root: "/tmp/fixture".to_owned(),
                vendor: Some("LENOVO".to_owned()),
                product_name: Some("82WM".to_owned()),
                product_version: Some("Legion Pro 5 16ARX8".to_owned()),
                product_sku: None,
            },
            vec![Capability {
                id: "platform_profile".to_owned(),
                label: "Platform profiles".to_owned(),
                status: CapabilityStatus::ProbeOnly,
                risk: RiskLevel::ReadOnly,
                evidence: vec![],
                details: serde_json::Value::Null,
            }],
        )
        .unwrap();
        TraySummary::from_status(&status)
    }

    fn menu() -> TrayMenu {
        let status = UiStatus::from_parts(
            HardwareSummary {
                sysfs_root: "/tmp/fixture".to_owned(),
                vendor: Some("LENOVO".to_owned()),
                product_name: Some("82WM".to_owned()),
                product_version: Some("Legion Pro 5 16ARX8".to_owned()),
                product_sku: None,
            },
            vec![
                Capability {
                    id: "platform_profile".to_owned(),
                    label: "Platform profiles".to_owned(),
                    status: CapabilityStatus::ProbeOnly,
                    risk: RiskLevel::ReadOnly,
                    evidence: vec![],
                    details: serde_json::Value::Null,
                },
                Capability {
                    id: "gpu".to_owned(),
                    label: "GPU mode".to_owned(),
                    status: CapabilityStatus::Missing,
                    risk: RiskLevel::ReadOnly,
                    evidence: vec![],
                    details: serde_json::Value::Null,
                },
            ],
        )
        .unwrap();
        let report = legion_common::CapabilityRegistry {
            platform_profile: Some(legion_common::PlatformProfileCapability {
                current: Some("balanced".to_owned()),
                choices: vec![
                    "low-power".to_owned(),
                    "balanced".to_owned(),
                    "performance".to_owned(),
                ],
                path: "/tmp/platform_profile".to_owned(),
                choices_path: "/tmp/platform_profile_choices".to_owned(),
            }),
            battery_charge_type: Some(legion_common::BatteryChargeTypeCapability {
                current: Some("Standard".to_owned()),
                choices: vec![
                    "Standard".to_owned(),
                    "Conservation".to_owned(),
                    "Fast".to_owned(),
                ],
                path: "/tmp/charge_type".to_owned(),
                choices_path: "/tmp/charge_types".to_owned(),
            }),
            telemetry: legion_common::TelemetrySnapshot {
                sensors: vec![legion_common::HwmonSensor {
                    hwmon_name: Some("legion".to_owned()),
                    label: Some("CPU Fan".to_owned()),
                    kind: "fan".to_owned(),
                    input_path: "/tmp/fan1_input".to_owned(),
                    value: Some(2410),
                }],
                battery: Some(legion_common::BatteryTelemetry {
                    name: "BAT0".to_owned(),
                    path: "/tmp/BAT0".to_owned(),
                    capacity_percent: Some(79),
                    status: Some("Charging".to_owned()),
                    health: Some("Good".to_owned()),
                }),
            },
            ..Default::default()
        };
        let gpu_pending = legion_common::GpuModePending {
            requested_mode: "hybrid".to_owned(),
            previous_mode: Some("nvidia".to_owned()),
            reboot_required: true,
        };
        let fan_snapshot = legion_common::FanCurveSnapshot {
            curve_id: "legion_hwmon".to_owned(),
            path: Some("/tmp/hwmon".to_owned()),
            points: vec![legion_common::FanCurvePointSnapshot {
                path: "/tmp/hwmon/pwm1_auto_point1_temp".to_owned(),
                value: "42000".to_owned(),
            }],
        };

        TrayMenu::from_status_and_report(&status, &report, Some(&gpu_pending), Some(&fan_snapshot))
    }

    fn menu_has_enabled_item(menu: &[MenuItem<StatusNotifierTray>], expected_label: &str) -> bool {
        menu.iter().any(|item| match item {
            MenuItem::Standard(item) => item.label == expected_label && item.enabled,
            _ => false,
        })
    }

    fn menu_has_disabled_item(menu: &[MenuItem<StatusNotifierTray>], expected_label: &str) -> bool {
        menu.iter().any(|item| match item {
            MenuItem::Standard(item) => item.label == expected_label && !item.enabled,
            _ => false,
        })
    }

    fn menu_has_item_starting_with(menu: &[MenuItem<StatusNotifierTray>], prefix: &str) -> bool {
        menu.iter().any(|item| match item {
            MenuItem::Standard(item) => item.label.starts_with(prefix),
            _ => false,
        })
    }
}
