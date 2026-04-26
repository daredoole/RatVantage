use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use ksni::blocking::TrayMethods;
use ksni::menu::StandardItem;
use ksni::{Category, MenuItem, Status, ToolTip, Tray};
use legion_common::WriteExecutionResult;
use legion_control_ui::LegionControlClient;

use crate::{DesktopSession, TrayAction, TrayMenu, TrayMenuEntry, TrayMenuItem, TraySummary};

const TRAY_ID: &str = "org.ratvantage.LegionControl";
const ICON_NAME: &str = "applications-system";
const AUTO_REFRESH_INTERVAL: Duration = Duration::from_secs(30);
const RESUME_REFRESH_GAP: Duration = Duration::from_secs(90);

pub struct StatusNotifierTray {
    summary: TraySummary,
    menu: TrayMenu,
    bus_address: Option<String>,
    shutdown_requested: Arc<AtomicBool>,
    last_error: Option<String>,
    state_loader: StateLoader,
    action_executor: ActionExecutor,
}

type StateLoader = Arc<dyn Fn(Option<&str>) -> Result<LoadedTrayState> + Send + Sync>;
type ActionExecutor =
    Arc<dyn Fn(Option<&str>, &TrayAction) -> Result<WriteExecutionResult> + Send + Sync>;

impl StatusNotifierTray {
    pub fn new(
        summary: TraySummary,
        menu: TrayMenu,
        bus_address: Option<String>,
        shutdown_requested: Arc<AtomicBool>,
    ) -> Self {
        Self::new_with_runtime(
            summary,
            menu,
            bus_address,
            shutdown_requested,
            Arc::new(load_tray_state),
            Arc::new(execute_tray_action),
        )
    }

    fn new_with_runtime(
        summary: TraySummary,
        menu: TrayMenu,
        bus_address: Option<String>,
        shutdown_requested: Arc<AtomicBool>,
        state_loader: StateLoader,
        action_executor: ActionExecutor,
    ) -> Self {
        Self {
            summary,
            menu,
            bus_address,
            shutdown_requested,
            last_error: None,
            state_loader,
            action_executor,
        }
    }

    pub fn summary(&self) -> &TraySummary {
        &self.summary
    }

    fn refresh_status(&mut self) {
        match (self.state_loader)(self.bus_address.as_deref()) {
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

    fn handle_action(&mut self, action: TrayAction) {
        match action {
            TrayAction::NoOp => {}
            TrayAction::SetPlatformProfile(_)
            | TrayAction::SetBatteryChargeType(_)
            | TrayAction::SetLedState(_, _) => self.execute_write_action(action),
            TrayAction::OpenDashboard => self.open_dashboard(),
            TrayAction::RefreshStatus => self.refresh_status(),
            TrayAction::Quit => self.request_shutdown(),
        }
    }

    fn execute_write_action(&mut self, action: TrayAction) {
        let mut error = match (self.action_executor)(self.bus_address.as_deref(), &action) {
            Ok(result) if result.applied => None,
            Ok(result) => Some(result.message),
            Err(action_error) => Some(format!("failed to execute tray action: {action_error}")),
        };

        match (self.state_loader)(self.bus_address.as_deref()) {
            Ok(state) => {
                self.summary = state.summary;
                self.menu = state.menu;
            }
            Err(refresh_error) => {
                error = Some(match error {
                    Some(previous) => format!("{previous}; refresh failed: {refresh_error}"),
                    None => format!("failed to refresh tray state: {refresh_error}"),
                });
            }
        }

        self.last_error = error;
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
    let mut last_refresh = Instant::now();
    let mut last_tick = last_refresh;

    while !shutdown_requested.load(Ordering::Relaxed) && !handle.is_closed() {
        thread::sleep(Duration::from_millis(250));
        let now = Instant::now();
        if should_auto_refresh(now, last_refresh, last_tick) {
            let _ = handle.update(|tray: &mut StatusNotifierTray| tray.refresh_status());
            last_refresh = now;
        }
        last_tick = now;
    }

    handle.shutdown().wait();
    Ok(())
}

#[derive(Clone)]
struct LoadedTrayState {
    summary: TraySummary,
    menu: TrayMenu,
}

fn load_tray_state(bus_address: Option<&str>) -> Result<LoadedTrayState> {
    let client = match bus_address {
        Some(address) => LegionControlClient::address(address)?,
        None => LegionControlClient::system()?,
    };
    let snapshot = client.refresh_runtime_snapshot()?;
    let status = snapshot.status;
    let report = snapshot.diagnostics.raw_probe_report;
    let gpu_pending = snapshot.diagnostics.gpu_mode_pending;
    let fan_snapshot = snapshot.diagnostics.last_known_good_fan_curve;
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

fn execute_tray_action(
    bus_address: Option<&str>,
    action: &TrayAction,
) -> Result<WriteExecutionResult> {
    let client = match bus_address {
        Some(address) => LegionControlClient::address(address)?,
        None => LegionControlClient::system()?,
    };

    match action {
        TrayAction::SetPlatformProfile(profile) => client.set_platform_profile(profile),
        TrayAction::SetBatteryChargeType(charge_type) => {
            client.set_battery_charge_type(charge_type)
        }
        TrayAction::SetLedState(led_id, enabled) => client.set_led_state(led_id, *enabled),
        _ => anyhow::bail!("unsupported tray write action"),
    }
}

fn should_auto_refresh(now: Instant, last_refresh: Instant, last_tick: Instant) -> bool {
    now.duration_since(last_refresh) >= AUTO_REFRESH_INTERVAL
        || now.duration_since(last_tick) >= RESUME_REFRESH_GAP
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
        activate: Box::new(move |tray: &mut StatusNotifierTray| tray.handle_action(action.clone())),
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
        assert!(menu_has_disabled_item(&menu, "LED: platform::ylogo on"));
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
        assert!(menu_has_disabled_item(&menu, "Platform profile actions"));
        assert!(menu_has_disabled_item(&menu, "Battery charge type actions"));
        assert!(menu_has_disabled_item(&menu, "LED actions"));
        assert!(menu_has_enabled_item(
            &menu,
            "Set platform profile: low-power"
        ));
        assert!(menu_has_enabled_item(
            &menu,
            "Set platform profile: performance"
        ));
        assert!(menu_has_enabled_item(
            &menu,
            "Set battery charge type: Conservation"
        ));
        assert!(menu_has_enabled_item(
            &menu,
            "Set battery charge type: Fast"
        ));
        assert!(menu_has_enabled_item(
            &menu,
            "Set LED state: platform::ylogo off"
        ));
        assert!(menu_has_enabled_item(&menu, "Open dashboard"));
        assert!(menu_has_enabled_item(&menu, "Refresh status"));
        assert!(menu_has_enabled_item(&menu, "Quit"));
        assert!(!menu_has_item_starting_with(&menu, "Apply preset:"));
        assert!(!menu_has_item_starting_with(&menu, "Toggle logo LED"));
    }

    #[test]
    fn status_notifier_activation_executes_platform_profile_action_and_refreshes_menu() {
        let shutdown_requested = Arc::new(AtomicBool::new(false));
        let state = Arc::new(std::sync::Mutex::new(tray_state_fixture(
            "balanced", "Standard", true,
        )));
        let menu = state.lock().unwrap().menu.clone();
        let tray = StatusNotifierTray::new_with_runtime(
            summary(),
            menu,
            None,
            shutdown_requested,
            loader_for_state(Arc::clone(&state)),
            Arc::new({
                let state = Arc::clone(&state);
                move |_, action| match action {
                    TrayAction::SetPlatformProfile(profile) => {
                        let mut guard = state.lock().unwrap();
                        *guard = tray_state_fixture(profile, "Standard", true);
                        Ok(applied_result("SetPlatformProfile", profile))
                    }
                    other => anyhow::bail!("unexpected action {other:?}"),
                }
            }),
        );
        let mut tray = tray;

        tray.handle_action(TrayAction::SetPlatformProfile("performance".to_owned()));

        let menu = tray.menu();
        assert!(menu_has_disabled_item(
            &menu,
            "Platform profile: performance"
        ));
        assert!(menu_has_enabled_item(
            &menu,
            "Set platform profile: balanced"
        ));
        assert!(tray.last_error.is_none());
    }

    #[test]
    fn status_notifier_activation_executes_battery_charge_type_action_and_refreshes_menu() {
        let shutdown_requested = Arc::new(AtomicBool::new(false));
        let state = Arc::new(std::sync::Mutex::new(tray_state_fixture(
            "balanced", "Standard", true,
        )));
        let menu = state.lock().unwrap().menu.clone();
        let tray = StatusNotifierTray::new_with_runtime(
            summary(),
            menu,
            None,
            shutdown_requested,
            loader_for_state(Arc::clone(&state)),
            Arc::new({
                let state = Arc::clone(&state);
                move |_, action| match action {
                    TrayAction::SetBatteryChargeType(charge_type) => {
                        let mut guard = state.lock().unwrap();
                        *guard = tray_state_fixture("balanced", charge_type, true);
                        Ok(applied_result("SetBatteryChargeType", charge_type))
                    }
                    other => anyhow::bail!("unexpected action {other:?}"),
                }
            }),
        );
        let mut tray = tray;

        tray.handle_action(TrayAction::SetBatteryChargeType("Conservation".to_owned()));

        let menu = tray.menu();
        assert!(menu_has_disabled_item(
            &menu,
            "Battery charge type: Conservation"
        ));
        assert!(menu_has_enabled_item(
            &menu,
            "Set battery charge type: Standard"
        ));
        assert!(tray.last_error.is_none());
    }

    #[test]
    fn status_notifier_activation_executes_led_action_and_refreshes_menu() {
        let shutdown_requested = Arc::new(AtomicBool::new(false));
        let state = Arc::new(std::sync::Mutex::new(tray_state_fixture(
            "balanced", "Standard", true,
        )));
        let menu = state.lock().unwrap().menu.clone();
        let tray = StatusNotifierTray::new_with_runtime(
            summary(),
            menu,
            None,
            shutdown_requested,
            loader_for_state(Arc::clone(&state)),
            Arc::new({
                let state = Arc::clone(&state);
                move |_, action| match action {
                    TrayAction::SetLedState(led_id, enabled) => {
                        let mut guard = state.lock().unwrap();
                        *guard = tray_state_fixture("balanced", "Standard", *enabled);
                        Ok(applied_result(
                            "SetLedState",
                            &format!("{led_id}={}", if *enabled { "1" } else { "0" }),
                        ))
                    }
                    other => anyhow::bail!("unexpected action {other:?}"),
                }
            }),
        );
        let mut tray = tray;

        tray.handle_action(TrayAction::SetLedState("platform::ylogo".to_owned(), false));

        let menu = tray.menu();
        assert!(menu_has_disabled_item(&menu, "LED: platform::ylogo off"));
        assert!(menu_has_enabled_item(
            &menu,
            "Set LED state: platform::ylogo on"
        ));
        assert!(tray.last_error.is_none());
    }

    #[test]
    fn status_notifier_activation_preserves_menu_and_records_error_on_write_failure() {
        let shutdown_requested = Arc::new(AtomicBool::new(false));
        let state = Arc::new(std::sync::Mutex::new(tray_state_fixture(
            "balanced", "Standard", true,
        )));
        let menu = state.lock().unwrap().menu.clone();
        let tray = StatusNotifierTray::new_with_runtime(
            summary(),
            menu,
            None,
            shutdown_requested,
            loader_for_state(Arc::clone(&state)),
            Arc::new(|_, _| {
                Ok(WriteExecutionResult::failed(
                    write_plan("SetPlatformProfile"),
                    "platform profile read-back mismatch after write",
                    Some("balanced".to_owned()),
                ))
            }),
        );
        let mut tray = tray;

        tray.handle_action(TrayAction::SetPlatformProfile("performance".to_owned()));

        let menu = tray.menu();
        assert!(menu_has_disabled_item(&menu, "Platform profile: balanced"));
        assert_eq!(
            tray.last_error.as_deref(),
            Some("platform profile read-back mismatch after write")
        );
    }

    #[test]
    fn status_notifier_activation_ignores_current_choice_noop_rows() {
        let shutdown_requested = Arc::new(AtomicBool::new(false));
        let state = Arc::new(std::sync::Mutex::new(tray_state_fixture(
            "balanced", "Standard", true,
        )));
        let menu = state.lock().unwrap().menu.clone();
        let call_count = Arc::new(std::sync::Mutex::new(0usize));
        let tray = StatusNotifierTray::new_with_runtime(
            summary(),
            menu,
            None,
            shutdown_requested,
            loader_for_state(state),
            Arc::new({
                let call_count = Arc::clone(&call_count);
                move |_, _| {
                    *call_count.lock().unwrap() += 1;
                    Ok(applied_result("SetPlatformProfile", "performance"))
                }
            }),
        );
        let mut tray = tray;

        tray.handle_action(TrayAction::NoOp);

        assert_eq!(*call_count.lock().unwrap(), 0);
        assert!(tray.last_error.is_none());
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
            leds: vec![legion_common::LedCapability {
                name: "platform::ylogo".to_owned(),
                path: "/tmp/platform::ylogo/brightness".to_owned(),
                brightness: Some(1),
                max_brightness: Some(1),
            }],
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

    #[test]
    fn auto_refresh_triggers_for_periodic_and_resume_gaps() {
        let base = Instant::now();
        assert!(!should_auto_refresh(
            base + Duration::from_secs(5),
            base,
            base + Duration::from_secs(4)
        ));
        assert!(should_auto_refresh(
            base + AUTO_REFRESH_INTERVAL,
            base,
            base + Duration::from_secs(1)
        ));
        assert!(should_auto_refresh(
            base + RESUME_REFRESH_GAP + Duration::from_secs(1),
            base + Duration::from_secs(2),
            base
        ));
    }

    fn tray_state_fixture(profile: &str, charge_type: &str, ylogo_on: bool) -> LoadedTrayState {
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
                    id: "battery_charge_type".to_owned(),
                    label: "Battery charge type".to_owned(),
                    status: CapabilityStatus::ProbeOnly,
                    risk: RiskLevel::ReadOnly,
                    evidence: vec![],
                    details: serde_json::Value::Null,
                },
            ],
        )
        .unwrap();
        let report = legion_common::CapabilityRegistry {
            platform_profile: Some(legion_common::PlatformProfileCapability {
                current: Some(profile.to_owned()),
                choices: vec![
                    "low-power".to_owned(),
                    "balanced".to_owned(),
                    "performance".to_owned(),
                ],
                path: "/tmp/platform_profile".to_owned(),
                choices_path: "/tmp/platform_profile_choices".to_owned(),
            }),
            battery_charge_type: Some(legion_common::BatteryChargeTypeCapability {
                current: Some(charge_type.to_owned()),
                choices: vec![
                    "Standard".to_owned(),
                    "Conservation".to_owned(),
                    "Fast".to_owned(),
                ],
                path: "/tmp/charge_type".to_owned(),
                choices_path: "/tmp/charge_types".to_owned(),
            }),
            leds: vec![legion_common::LedCapability {
                name: "platform::ylogo".to_owned(),
                path: "/tmp/platform::ylogo/brightness".to_owned(),
                brightness: Some(if ylogo_on { 1 } else { 0 }),
                max_brightness: Some(1),
            }],
            ..Default::default()
        };
        LoadedTrayState {
            summary: TraySummary::from_status_and_report(&status, &report, None, None),
            menu: TrayMenu::from_status_and_report(&status, &report, None, None),
        }
    }

    fn loader_for_state(state: Arc<std::sync::Mutex<LoadedTrayState>>) -> StateLoader {
        Arc::new(move |_| Ok(state.lock().unwrap().clone()))
    }

    fn applied_result(method: &'static str, requested: &str) -> WriteExecutionResult {
        WriteExecutionResult::applied(
            write_plan(method),
            "write applied and read back successfully",
            Some(requested.to_owned()),
        )
    }

    fn write_plan(method: &'static str) -> legion_common::WriteDryRunPlan {
        legion_common::WriteDryRunPlan {
            method: method.to_owned(),
            capability_id: "test".to_owned(),
            polkit_action: "org.ratvantage.LegionControl1.test".to_owned(),
            path: "/tmp/test".to_owned(),
            previous_value: "previous".to_owned(),
            requested_value: "requested".to_owned(),
            readback_required: true,
            rollback_value: "previous".to_owned(),
            rollback_instructions: Vec::new(),
            reboot_required: false,
            safety_notes: Vec::new(),
            steps: Vec::new(),
        }
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
