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

use crate::{DesktopSession, TrayAction, TrayMenu, TrayMenuItem, TraySummary};

const TRAY_ID: &str = "org.ratvantage.LegionControl";
const ICON_NAME: &str = "applications-system";

pub struct StatusNotifierTray {
    summary: TraySummary,
    bus_address: Option<String>,
    shutdown_requested: Arc<AtomicBool>,
    last_error: Option<String>,
}

impl StatusNotifierTray {
    pub fn new(
        summary: TraySummary,
        bus_address: Option<String>,
        shutdown_requested: Arc<AtomicBool>,
    ) -> Self {
        Self {
            summary,
            bus_address,
            shutdown_requested,
            last_error: None,
        }
    }

    pub fn summary(&self) -> &TraySummary {
        &self.summary
    }

    fn refresh_status(&mut self) {
        match load_summary(self.bus_address.as_deref()) {
            Ok(summary) => {
                self.summary = summary;
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
        let mut items = Vec::new();
        for item in TrayMenu::read_only_scaffold().items {
            items.push(menu_item(item));
        }
        items.push(MenuItem::Separator);
        items.push(
            StandardItem {
                label: "Refresh status".to_owned(),
                activate: Box::new(StatusNotifierTray::refresh_status),
                ..StandardItem::default()
            }
            .into(),
        );
        items.push(
            StandardItem {
                label: "Quit".to_owned(),
                activate: Box::new(StatusNotifierTray::request_shutdown),
                ..StandardItem::default()
            }
            .into(),
        );
        items
    }
}

pub fn run_status_notifier_tray(bus_address: Option<String>) -> Result<()> {
    if let Some(guidance) = DesktopSession::from_env().status_notifier_guidance() {
        eprintln!("Desktop note: {guidance}");
    }

    let summary = load_summary(bus_address.as_deref())?;
    let shutdown_requested = Arc::new(AtomicBool::new(false));
    let tray = StatusNotifierTray::new(summary, bus_address, Arc::clone(&shutdown_requested));
    let handle = tray
        .spawn()
        .context("failed to register StatusNotifier tray item")?;

    while !shutdown_requested.load(Ordering::Relaxed) && !handle.is_closed() {
        thread::sleep(Duration::from_millis(250));
    }

    handle.shutdown().wait();
    Ok(())
}

fn load_summary(bus_address: Option<&str>) -> Result<TraySummary> {
    let client = match bus_address {
        Some(address) => LegionControlClient::address(address)?,
        None => LegionControlClient::system()?,
    };
    Ok(TraySummary::from_status_and_report(
        &client.status()?,
        &client.raw_probe_report()?,
    ))
}

fn dashboard_command_args(bus_address: Option<&str>) -> Vec<String> {
    match bus_address {
        Some(address) => vec!["--bus-address".to_owned(), address.to_owned()],
        None => Vec::new(),
    }
}

fn menu_item(item: TrayMenuItem) -> MenuItem<StatusNotifierTray> {
    let TrayMenuItem {
        label,
        action,
        enabled,
        disabled_reason,
    } = item;
    let tooltip_suffix = disabled_reason
        .as_deref()
        .map(|reason| format!(" ({reason})"))
        .unwrap_or_default();

    StandardItem {
        label: format!("{label}{tooltip_suffix}"),
        enabled,
        activate: Box::new(move |tray: &mut StatusNotifierTray| match action {
            TrayAction::OpenDashboard => tray.open_dashboard(),
            TrayAction::SetPlatformProfile
            | TrayAction::SetBatteryChargeType
            | TrayAction::ApplyFanPreset
            | TrayAction::ToggleLogoLed => {}
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
    fn status_notifier_menu_keeps_write_actions_disabled() {
        let shutdown_requested = Arc::new(AtomicBool::new(false));
        let tray = StatusNotifierTray::new(summary(), None, shutdown_requested);
        let menu = tray.menu();

        assert!(menu_has_enabled_item(&menu, "Open dashboard"));
        assert!(menu_has_enabled_item(&menu, "Refresh status"));
        assert!(menu_has_enabled_item(&menu, "Quit"));
        assert!(menu_has_disabled_item(&menu, "Set platform profile"));
        assert!(menu_has_disabled_item(
            &menu,
            "Set battery charge type: Fast"
        ));
        assert!(menu_has_disabled_item(
            &menu,
            "Set battery charge type: Standard"
        ));
        assert!(menu_has_disabled_item(
            &menu,
            "Set battery charge type: Conservation"
        ));
        assert!(menu_has_disabled_item(
            &menu,
            "Set battery charge type: Long_Life"
        ));
        assert!(menu_has_disabled_item(&menu, "Apply preset: Quiet office"));
        assert!(menu_has_disabled_item(
            &menu,
            "Apply preset: Balanced daily"
        ));
        assert!(menu_has_disabled_item(&menu, "Apply preset: Gaming"));
        assert!(menu_has_disabled_item(&menu, "Apply preset: Max safe"));
        assert!(menu_has_disabled_item(&menu, "Toggle logo LED"));
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

    fn menu_has_enabled_item(menu: &[MenuItem<StatusNotifierTray>], expected_label: &str) -> bool {
        menu.iter().any(|item| match item {
            MenuItem::Standard(item) => item.label == expected_label && item.enabled,
            _ => false,
        })
    }

    fn menu_has_disabled_item(menu: &[MenuItem<StatusNotifierTray>], expected_label: &str) -> bool {
        menu.iter().any(|item| match item {
            MenuItem::Standard(item) => item.label.starts_with(expected_label) && !item.enabled,
            _ => false,
        })
    }
}
