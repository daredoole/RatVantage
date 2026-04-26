use legion_common::{CapabilityRegistry, FanCurveSnapshot, FanPreset, GpuModePending, HwmonSensor};
use legion_control_ui::UiStatus;

const PACKAGED_FAN_PRESETS: &[&str] = &[
    include_str!("../../../data/presets/quiet-office.toml"),
    include_str!("../../../data/presets/balanced-daily.toml"),
    include_str!("../../../data/presets/gaming.toml"),
    include_str!("../../../data/presets/max-safe.toml"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayAction {
    NoOp,
    SetPlatformProfile(String),
    SetBatteryChargeType(String),
    SetLedState(String, bool),
    SetIdeapadToggle(String, bool),
    OpenDashboard,
    RefreshStatus,
    Quit,
}

impl TrayAction {
    fn as_str(&self) -> String {
        match self {
            Self::NoOp => "noop".to_owned(),
            Self::SetPlatformProfile(profile) => format!("set_platform_profile:{profile}"),
            Self::SetBatteryChargeType(charge_type) => {
                format!("set_battery_charge_type:{charge_type}")
            }
            Self::SetLedState(led_id, enabled) => {
                format!(
                    "set_led_state:{led_id}:{}",
                    if *enabled { "on" } else { "off" }
                )
            }
            Self::SetIdeapadToggle(toggle_id, enabled) => {
                format!(
                    "set_ideapad_toggle:{toggle_id}:{}",
                    if *enabled { "on" } else { "off" }
                )
            }
            Self::OpenDashboard => "open_dashboard".to_owned(),
            Self::RefreshStatus => "refresh_status".to_owned(),
            Self::Quit => "quit".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayMenuItem {
    pub label: String,
    pub action: TrayAction,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayMenuEntry {
    Item(TrayMenuItem),
    Separator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayMenu {
    pub entries: Vec<TrayMenuEntry>,
}

impl TrayMenu {
    pub fn from_status_and_report(
        status: &UiStatus,
        report: &CapabilityRegistry,
        gpu_pending: Option<&GpuModePending>,
        fan_snapshot: Option<&FanCurveSnapshot>,
    ) -> Self {
        let mut entries = Vec::new();

        entries.push(TrayMenuEntry::Item(info_item(machine_label(status))));

        if let Some(profile) = report
            .platform_profile
            .as_ref()
            .and_then(|profile| profile.current.as_deref())
        {
            entries.push(TrayMenuEntry::Item(info_item(format!(
                "Platform profile: {profile}"
            ))));
        }
        if let Some(profile_choices) = choices_row(
            "Profile choices",
            report
                .platform_profile
                .as_ref()
                .map(|profile| profile.choices.as_slice()),
        ) {
            entries.push(TrayMenuEntry::Item(info_item(profile_choices)));
        }

        if let Some(charge_type) = report
            .battery_charge_type
            .as_ref()
            .and_then(|charge_type| charge_type.current.as_deref())
        {
            entries.push(TrayMenuEntry::Item(info_item(format!(
                "Battery charge type: {charge_type}"
            ))));
        }
        if let Some(charge_choices) = choices_row(
            "Charge choices",
            report
                .battery_charge_type
                .as_ref()
                .map(|charge_type| charge_type.choices.as_slice()),
        ) {
            entries.push(TrayMenuEntry::Item(info_item(charge_choices)));
        }
        if let Some(battery_row) = battery_row(report) {
            entries.push(TrayMenuEntry::Item(info_item(battery_row)));
        }
        for led_row in led_rows(report) {
            entries.push(TrayMenuEntry::Item(info_item(led_row)));
        }
        for toggle_row in ideapad_toggle_rows(report) {
            entries.push(TrayMenuEntry::Item(info_item(toggle_row)));
        }

        for fan_row in fan_rows(&report.telemetry.sensors) {
            entries.push(TrayMenuEntry::Item(info_item(fan_row)));
        }

        if let Some(gpu_row) = gpu_row(report, gpu_pending) {
            entries.push(TrayMenuEntry::Item(info_item(gpu_row)));
        }
        if let Some(fan_curve_row) = fan_curve_row(fan_snapshot) {
            entries.push(TrayMenuEntry::Item(info_item(fan_curve_row)));
        }
        if let Some(fan_preset_row) = packaged_fan_presets_row() {
            entries.push(TrayMenuEntry::Item(info_item(fan_preset_row)));
        }

        let missing_capabilities = status
            .capabilities
            .iter()
            .filter(|capability| capability.status == legion_common::CapabilityStatus::Missing)
            .map(|capability| capability.id.as_str())
            .collect::<Vec<_>>();
        let missing_count = missing_capabilities.len();
        let available_count = status.capability_count().saturating_sub(missing_count);
        entries.push(TrayMenuEntry::Item(info_item(format!(
            "Capabilities: {available_count} available, {missing_count} missing"
        ))));
        if !missing_capabilities.is_empty() {
            entries.push(TrayMenuEntry::Item(info_item(format!(
                "Missing: {}",
                missing_capabilities.join(", ")
            ))));
        }

        append_quick_actions(&mut entries, report);
        entries.push(TrayMenuEntry::Separator);
        entries.push(TrayMenuEntry::Item(action_item(
            "Open dashboard",
            TrayAction::OpenDashboard,
        )));
        entries.push(TrayMenuEntry::Item(action_item(
            "Refresh status",
            TrayAction::RefreshStatus,
        )));
        entries.push(TrayMenuEntry::Item(action_item("Quit", TrayAction::Quit)));

        Self { entries }
    }

    pub fn render_lines(&self) -> Vec<String> {
        let mut lines = vec!["Legion Control tray menu".to_owned()];
        for (index, entry) in self.entries.iter().enumerate() {
            match entry {
                TrayMenuEntry::Separator => lines.push(format!("entry[{index}]=separator")),
                TrayMenuEntry::Item(item) => lines.push(format!(
                    "entry[{index}]={} action={} label={}",
                    if item.enabled { "enabled" } else { "disabled" },
                    item.action.as_str(),
                    item.label
                )),
            }
        }
        lines
    }
}

fn action_item(label: impl Into<String>, action: TrayAction) -> TrayMenuItem {
    TrayMenuItem {
        label: label.into(),
        action,
        enabled: true,
    }
}

fn info_item(label: String) -> TrayMenuItem {
    TrayMenuItem {
        label,
        action: TrayAction::NoOp,
        enabled: false,
    }
}

fn machine_label(status: &UiStatus) -> String {
    match (
        status.hardware.product_name.trim(),
        status.hardware.product_version.trim(),
    ) {
        ("", "") => "Legion Control".to_owned(),
        ("", version) => version.to_owned(),
        (name, "") => name.to_owned(),
        (name, version) => format!("{name} {version}"),
    }
}

fn choices_row(label: &str, choices: Option<&[String]>) -> Option<String> {
    let choices = choices?;
    if choices.is_empty() {
        None
    } else {
        Some(format!("{label}: {}", choices.join(", ")))
    }
}

fn battery_row(report: &CapabilityRegistry) -> Option<String> {
    let battery = report.telemetry.battery.as_ref()?;
    let mut parts = Vec::new();
    if let Some(capacity_percent) = battery.capacity_percent {
        parts.push(format!("{capacity_percent}%"));
    }
    if let Some(status) = battery.status.as_deref() {
        parts.push(status.to_owned());
    }
    if let Some(health) = battery.health.as_deref() {
        parts.push(health.to_owned());
    }
    if parts.is_empty() {
        None
    } else {
        Some(format!("Battery: {}", parts.join(" / ")))
    }
}

fn led_rows(report: &CapabilityRegistry) -> Vec<String> {
    report.leds.iter().filter_map(legion_led_row).collect()
}

fn append_quick_actions(entries: &mut Vec<TrayMenuEntry>, report: &CapabilityRegistry) {
    let mut sections = Vec::new();

    if let Some(section) = quick_action_section(
        "Platform profile actions",
        "Set platform profile",
        report
            .platform_profile
            .as_ref()
            .map(|profile| profile.current.as_deref()),
        report
            .platform_profile
            .as_ref()
            .map(|profile| profile.choices.as_slice()),
        |choice| TrayAction::SetPlatformProfile(choice.to_owned()),
    ) {
        sections.extend(section);
    }

    if let Some(section) = quick_action_section(
        "Battery charge type actions",
        "Set battery charge type",
        report
            .battery_charge_type
            .as_ref()
            .map(|charge_type| charge_type.current.as_deref()),
        report
            .battery_charge_type
            .as_ref()
            .map(|charge_type| charge_type.choices.as_slice()),
        |choice| TrayAction::SetBatteryChargeType(choice.to_owned()),
    ) {
        sections.extend(section);
    }

    if let Some(section) = led_quick_action_section(report) {
        sections.extend(section);
    }

    if let Some(section) = ideapad_toggle_quick_action_section(report) {
        sections.extend(section);
    }

    if let Some(section) = camera_power_guidance_section(report) {
        sections.extend(section);
    }

    if let Some(section) = usb_charging_guidance_section(report) {
        sections.extend(section);
    }

    if !sections.is_empty() {
        entries.push(TrayMenuEntry::Separator);
        entries.extend(sections);
    }
}

fn quick_action_section<F>(
    header: &str,
    action_prefix: &str,
    current: Option<Option<&str>>,
    choices: Option<&[String]>,
    action: F,
) -> Option<Vec<TrayMenuEntry>>
where
    F: Fn(&str) -> TrayAction,
{
    let choices = choices?;
    let current = current.flatten();
    let actionable = choices
        .iter()
        .filter(|choice| Some(choice.as_str()) != current)
        .collect::<Vec<_>>();
    if actionable.is_empty() {
        return None;
    }

    let mut entries = vec![TrayMenuEntry::Item(info_item(header.to_owned()))];
    for choice in actionable {
        entries.push(TrayMenuEntry::Item(action_item(
            format!("{action_prefix}: {choice}"),
            action(choice),
        )));
    }
    Some(entries)
}

fn led_quick_action_section(report: &CapabilityRegistry) -> Option<Vec<TrayMenuEntry>> {
    let led = report
        .leds
        .iter()
        .find(|led| led.name == "platform::ylogo" && led.max_brightness == Some(1))?;
    let current = led.brightness?;
    if current != 0 && current != 1 {
        return None;
    }

    let mut entries = vec![TrayMenuEntry::Item(info_item("LED actions".to_owned()))];
    entries.push(TrayMenuEntry::Item(action_item(
        format!("Set LED state: {} off", led.name),
        TrayAction::SetLedState(led.name.clone(), false),
    )));
    entries.push(TrayMenuEntry::Item(action_item(
        format!("Set LED state: {} on", led.name),
        TrayAction::SetLedState(led.name.clone(), true),
    )));
    for entry in &mut entries {
        if let TrayMenuEntry::Item(item) = entry {
            if item.label.ends_with(" off") && current == 0 {
                item.enabled = false;
                item.action = TrayAction::NoOp;
            }
            if item.label.ends_with(" on") && current == 1 {
                item.enabled = false;
                item.action = TrayAction::NoOp;
            }
        }
    }
    Some(entries)
}

fn ideapad_toggle_rows(report: &CapabilityRegistry) -> Vec<String> {
    report
        .ideapad_toggles
        .iter()
        .filter_map(legion_toggle_row)
        .collect()
}

fn legion_led_row(led: &legion_common::LedCapability) -> Option<String> {
    let brightness = led.brightness?;
    let state = binary_state_label(brightness)?;

    match led.name.as_str() {
        "platform::ylogo" => Some(format!("Logo LED: {state}")),
        _ => None,
    }
}

fn legion_toggle_row(toggle: &legion_common::IdeapadToggleCapability) -> Option<String> {
    let value = toggle.current_value.as_deref()?;
    let state = binary_toggle_state_label(value)?;

    match toggle.name.as_str() {
        "fn_lock" => Some(format!("Fn-lock: {state}")),
        "camera_power" => Some(format!("Camera power: {state}")),
        "usb_charging" => Some(format!("USB charging: {state}")),
        _ => None,
    }
}

fn binary_state_label(value: i64) -> Option<&'static str> {
    match value {
        0 => Some("off"),
        1 => Some("on"),
        _ => None,
    }
}

fn binary_toggle_state_label(value: &str) -> Option<&'static str> {
    match value {
        "0" => Some("off"),
        "1" => Some("on"),
        _ => None,
    }
}

fn ideapad_toggle_quick_action_section(report: &CapabilityRegistry) -> Option<Vec<TrayMenuEntry>> {
    let toggle = report.ideapad_toggles.iter().find(|toggle| {
        toggle.name == "fn_lock"
            && matches!(toggle.current_value.as_deref(), Some("0" | "1"))
            && toggle.path.as_deref().is_some_and(|path| !path.is_empty())
    })?;
    let indicator_ok = report.leds.iter().any(|led| {
        led.name == "platform::fnlock"
            && led.max_brightness == Some(1)
            && matches!(led.brightness, Some(0 | 1))
            && toggle.current_value.as_deref()
                == led
                    .brightness
                    .map(|brightness| if brightness == 0 { "0" } else { "1" })
    });
    if !indicator_ok {
        return None;
    }
    let current = toggle.current_value.as_deref()?;

    let mut entries = vec![TrayMenuEntry::Item(info_item("Fn-lock actions".to_owned()))];
    entries.push(TrayMenuEntry::Item(action_item(
        "Set Fn-lock off",
        TrayAction::SetIdeapadToggle(toggle.name.clone(), false),
    )));
    entries.push(TrayMenuEntry::Item(action_item(
        "Set Fn-lock on",
        TrayAction::SetIdeapadToggle(toggle.name.clone(), true),
    )));
    for entry in &mut entries {
        if let TrayMenuEntry::Item(item) = entry {
            if item.label.ends_with(" off") && current == "0" {
                item.enabled = false;
                item.action = TrayAction::NoOp;
            }
            if item.label.ends_with(" on") && current == "1" {
                item.enabled = false;
                item.action = TrayAction::NoOp;
            }
        }
    }
    Some(entries)
}

fn camera_power_guidance_section(report: &CapabilityRegistry) -> Option<Vec<TrayMenuEntry>> {
    let toggle = report.ideapad_toggles.iter().find(|toggle| {
        toggle.name == "camera_power"
            && matches!(toggle.current_value.as_deref(), Some("0" | "1"))
            && toggle.path.as_deref().is_some_and(|path| !path.is_empty())
    })?;

    Some(vec![
        TrayMenuEntry::Item(info_item(format!(
            "Camera power: {}",
            if toggle.current_value.as_deref() == Some("1") {
                "dashboard confirmation required"
            } else {
                "currently off - dashboard confirmation required"
            }
        ))),
        TrayMenuEntry::Item(action_item(
            "Open dashboard for camera power controls",
            TrayAction::OpenDashboard,
        )),
    ])
}

fn usb_charging_guidance_section(report: &CapabilityRegistry) -> Option<Vec<TrayMenuEntry>> {
    let toggle = report.ideapad_toggles.iter().find(|toggle| {
        toggle.name == "usb_charging"
            && matches!(toggle.current_value.as_deref(), Some("0" | "1"))
            && toggle.path.as_deref().is_some_and(|path| !path.is_empty())
    })?;

    Some(vec![
        TrayMenuEntry::Item(info_item(format!(
            "USB charging: {}",
            if toggle.current_value.as_deref() == Some("1") {
                "enabled - dashboard warning required"
            } else {
                "disabled - dashboard warning required"
            }
        ))),
        TrayMenuEntry::Item(action_item(
            "Open dashboard for USB charging controls",
            TrayAction::OpenDashboard,
        )),
    ])
}

fn fan_rows(sensors: &[HwmonSensor]) -> Vec<String> {
    sensors
        .iter()
        .filter(|sensor| sensor.kind == "fan")
        .filter_map(|sensor| {
            sensor.value.map(|value| match sensor.label.as_deref() {
                Some(label) if !label.is_empty() => format!("Fan: {label} {value} RPM"),
                _ => format!("Fan: {value} RPM"),
            })
        })
        .collect()
}

fn gpu_row(report: &CapabilityRegistry, gpu_pending: Option<&GpuModePending>) -> Option<String> {
    if let Some(pending) = gpu_pending {
        let detail = match pending.previous_mode.as_deref() {
            Some(previous) => format!(
                "GPU pending: {} (previous {previous}, reboot required)",
                pending.requested_mode
            ),
            None => format!("GPU pending: {} (reboot required)", pending.requested_mode),
        };
        return Some(detail);
    }

    report
        .gpu
        .as_ref()
        .and_then(|gpu| gpu.mode.as_deref())
        .map(|mode| format!("GPU mode: {mode}"))
}

fn fan_curve_row(fan_snapshot: Option<&FanCurveSnapshot>) -> Option<String> {
    fan_snapshot.map(|snapshot| {
        format!(
            "Saved fan curve: {} values from {}",
            snapshot.points.len(),
            snapshot.curve_id
        )
    })
}

fn packaged_fan_presets_row() -> Option<String> {
    let labels = PACKAGED_FAN_PRESETS
        .iter()
        .map(|preset| {
            toml::from_str::<FanPreset>(preset)
                .ok()
                .map(|preset| preset.label)
        })
        .collect::<Option<Vec<_>>>()?;
    Some(format!("Fan presets: {}", labels.join(", ")))
}

#[cfg(test)]
mod tests {
    use legion_common::{
        BatteryChargeTypeCapability, BatteryTelemetry, Capability, CapabilityRegistry,
        CapabilityStatus, FanCurvePointSnapshot, FanCurveSnapshot, GpuModePending, HardwareSummary,
        HwmonSensor, IdeapadToggleCapability, LedCapability, PlatformProfileCapability, RiskLevel,
    };

    use super::*;

    #[test]
    fn menu_builder_reflects_full_runtime_state() {
        let status = UiStatus::from_parts(
            HardwareSummary {
                sysfs_root: "/tmp/fixture".to_owned(),
                vendor: Some("LENOVO".to_owned()),
                product_name: Some("82WM".to_owned()),
                product_version: Some("Legion Pro 5 16ARX8".to_owned()),
                product_sku: None,
            },
            vec![
                capability("platform_profile", CapabilityStatus::ProbeOnly),
                capability("battery_charge_type", CapabilityStatus::ProbeOnly),
                capability("gpu", CapabilityStatus::Missing),
            ],
        )
        .unwrap();
        let report = CapabilityRegistry {
            platform_profile: Some(PlatformProfileCapability {
                current: Some("balanced".to_owned()),
                choices: vec![
                    "low-power".to_owned(),
                    "balanced".to_owned(),
                    "performance".to_owned(),
                ],
                path: "/tmp/platform_profile".to_owned(),
                choices_path: "/tmp/platform_profile_choices".to_owned(),
            }),
            battery_charge_type: Some(BatteryChargeTypeCapability {
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
                sensors: vec![HwmonSensor {
                    hwmon_name: Some("legion".to_owned()),
                    label: Some("CPU Fan".to_owned()),
                    kind: "fan".to_owned(),
                    input_path: "/tmp/fan1_input".to_owned(),
                    value: Some(2410),
                }],
                battery: Some(BatteryTelemetry {
                    name: "BAT0".to_owned(),
                    path: "/tmp/BAT0".to_owned(),
                    capacity_percent: Some(79),
                    status: Some("Charging".to_owned()),
                    health: Some("Good".to_owned()),
                }),
            },
            leds: vec![
                LedCapability {
                    name: "input12::capslock".to_owned(),
                    path: "/tmp/input12::capslock/brightness".to_owned(),
                    brightness: Some(1),
                    max_brightness: Some(1),
                },
                LedCapability {
                    name: "platform::fnlock".to_owned(),
                    path: "/tmp/platform::fnlock/brightness".to_owned(),
                    brightness: Some(0),
                    max_brightness: Some(1),
                },
                LedCapability {
                    name: "platform::ylogo".to_owned(),
                    path: "/tmp/platform::ylogo/brightness".to_owned(),
                    brightness: Some(1),
                    max_brightness: Some(1),
                },
            ],
            ideapad_toggles: vec![
                IdeapadToggleCapability {
                    name: "fn_lock".to_owned(),
                    status: CapabilityStatus::ProbeOnly,
                    path: Some("/tmp/fn_lock".to_owned()),
                    current_value: Some("0".to_owned()),
                },
                IdeapadToggleCapability {
                    name: "camera_power".to_owned(),
                    status: CapabilityStatus::ProbeOnly,
                    path: Some("/tmp/camera_power".to_owned()),
                    current_value: Some("1".to_owned()),
                },
                IdeapadToggleCapability {
                    name: "conservation_mode".to_owned(),
                    status: CapabilityStatus::ProbeOnly,
                    path: Some("/tmp/conservation_mode".to_owned()),
                    current_value: Some("1".to_owned()),
                },
                IdeapadToggleCapability {
                    name: "usb_charging".to_owned(),
                    status: CapabilityStatus::ProbeOnly,
                    path: Some("/tmp/usb_charging".to_owned()),
                    current_value: Some("0".to_owned()),
                },
            ],
            ..Default::default()
        };
        let gpu_pending = GpuModePending {
            requested_mode: "hybrid".to_owned(),
            previous_mode: Some("nvidia".to_owned()),
            reboot_required: true,
        };
        let fan_snapshot = FanCurveSnapshot {
            curve_id: "legion_hwmon".to_owned(),
            path: Some("/tmp/hwmon".to_owned()),
            points: vec![FanCurvePointSnapshot {
                path: "/tmp/hwmon/pwm1_auto_point1_temp".to_owned(),
                value: "42000".to_owned(),
            }],
        };

        let menu = TrayMenu::from_status_and_report(
            &status,
            &report,
            Some(&gpu_pending),
            Some(&fan_snapshot),
        );

        assert_eq!(
            disabled_labels(&menu),
            [
                "82WM Legion Pro 5 16ARX8",
                "Platform profile: balanced",
                "Profile choices: low-power, balanced, performance",
                "Battery charge type: Standard",
                "Charge choices: Standard, Conservation, Fast",
                "Battery: 79% / Charging / Good",
                "Logo LED: on",
                "Fn-lock: off",
                "Camera power: on",
                "USB charging: off",
                "Fan: CPU Fan 2410 RPM",
                "GPU pending: hybrid (previous nvidia, reboot required)",
                "Saved fan curve: 1 values from legion_hwmon",
                "Fan presets: Quiet office, Balanced daily, Gaming, Max safe",
                "Capabilities: 2 available, 1 missing",
                "Missing: gpu",
                "Platform profile actions",
                "Battery charge type actions",
                "LED actions",
                "Set LED state: platform::ylogo on",
                "Fn-lock actions",
                "Set Fn-lock off",
                "Camera power: dashboard confirmation required",
                "USB charging: disabled - dashboard warning required",
            ]
        );
        assert_eq!(
            enabled_labels(&menu),
            [
                "Set platform profile: low-power",
                "Set platform profile: performance",
                "Set battery charge type: Conservation",
                "Set battery charge type: Fast",
                "Set LED state: platform::ylogo off",
                "Set Fn-lock on",
                "Open dashboard for camera power controls",
                "Open dashboard for USB charging controls",
                "Open dashboard",
                "Refresh status",
                "Quit",
            ]
        );
        assert!(!menu_labels(&menu)
            .iter()
            .any(|label| label.starts_with("Apply preset:")));
        assert!(!menu_labels(&menu)
            .iter()
            .any(|label| label.starts_with("Toggle logo LED")));
        assert!(!menu_labels(&menu)
            .iter()
            .any(|label| label.contains("input12::capslock")));
        assert!(!menu_labels(&menu)
            .iter()
            .any(|label| label.contains("conservation_mode")));
    }

    #[test]
    fn menu_builder_omits_missing_runtime_rows() {
        let status = UiStatus::from_parts(
            HardwareSummary {
                sysfs_root: "/tmp/fixture".to_owned(),
                vendor: Some("LENOVO".to_owned()),
                product_name: Some("82WM".to_owned()),
                product_version: Some("Legion Pro 5 16ARX8".to_owned()),
                product_sku: None,
            },
            vec![capability("platform_profile", CapabilityStatus::ProbeOnly)],
        )
        .unwrap();

        let menu =
            TrayMenu::from_status_and_report(&status, &CapabilityRegistry::default(), None, None);

        assert_eq!(
            disabled_labels(&menu),
            [
                "82WM Legion Pro 5 16ARX8",
                "Fan presets: Quiet office, Balanced daily, Gaming, Max safe",
                "Capabilities: 1 available, 0 missing",
            ]
        );
        assert_eq!(
            enabled_labels(&menu),
            ["Open dashboard", "Refresh status", "Quit"]
        );
        assert!(!menu_labels(&menu)
            .iter()
            .any(|label| label.starts_with("Battery:")));
        assert!(!menu_labels(&menu)
            .iter()
            .any(|label| label.starts_with("GPU pending:")));
        assert!(!menu_labels(&menu)
            .iter()
            .any(|label| label.starts_with("Saved fan curve:")));
    }

    #[test]
    fn menu_builder_omits_quick_action_sections_when_choices_are_missing_or_singleton() {
        let status = UiStatus::from_parts(
            HardwareSummary {
                sysfs_root: "/tmp/fixture".to_owned(),
                vendor: Some("LENOVO".to_owned()),
                product_name: Some("82WM".to_owned()),
                product_version: Some("Legion Pro 5 16ARX8".to_owned()),
                product_sku: None,
            },
            vec![
                capability("platform_profile", CapabilityStatus::ProbeOnly),
                capability("battery_charge_type", CapabilityStatus::ProbeOnly),
            ],
        )
        .unwrap();
        let report = CapabilityRegistry {
            platform_profile: Some(PlatformProfileCapability {
                current: Some("balanced".to_owned()),
                choices: vec!["balanced".to_owned()],
                path: "/tmp/platform_profile".to_owned(),
                choices_path: "/tmp/platform_profile_choices".to_owned(),
            }),
            battery_charge_type: Some(BatteryChargeTypeCapability {
                current: Some("Standard".to_owned()),
                choices: vec!["Standard".to_owned()],
                path: "/tmp/charge_type".to_owned(),
                choices_path: "/tmp/charge_types".to_owned(),
            }),
            ..Default::default()
        };

        let menu = TrayMenu::from_status_and_report(&status, &report, None, None);

        assert!(!menu_labels(&menu)
            .iter()
            .any(|label| label == &"Platform profile actions"));
        assert!(!menu_labels(&menu)
            .iter()
            .any(|label| label == &"Battery charge type actions"));
        assert!(!enabled_labels(&menu)
            .iter()
            .any(|label| label.starts_with("Set platform profile:")));
        assert!(!enabled_labels(&menu)
            .iter()
            .any(|label| label.starts_with("Set battery charge type:")));
        assert!(!enabled_labels(&menu)
            .iter()
            .any(|label| label.starts_with("Set LED state:")));
    }

    #[test]
    fn menu_builder_keeps_dashboard_refresh_quit_after_quick_action_sections() {
        let menu = menu_builder_fixture();
        let enabled = enabled_labels(&menu);
        assert_eq!(
            &enabled[enabled.len().saturating_sub(3)..],
            ["Open dashboard", "Refresh status", "Quit"]
        );
    }

    fn capability(id: &str, status: CapabilityStatus) -> Capability {
        Capability {
            id: id.to_owned(),
            label: id.to_owned(),
            status,
            risk: RiskLevel::ReadOnly,
            evidence: Vec::new(),
            details: serde_json::Value::Null,
        }
    }

    fn menu_builder_fixture() -> TrayMenu {
        let status = UiStatus::from_parts(
            HardwareSummary {
                sysfs_root: "/tmp/fixture".to_owned(),
                vendor: Some("LENOVO".to_owned()),
                product_name: Some("82WM".to_owned()),
                product_version: Some("Legion Pro 5 16ARX8".to_owned()),
                product_sku: None,
            },
            vec![
                capability("platform_profile", CapabilityStatus::ProbeOnly),
                capability("battery_charge_type", CapabilityStatus::ProbeOnly),
            ],
        )
        .unwrap();
        let report = CapabilityRegistry {
            platform_profile: Some(PlatformProfileCapability {
                current: Some("balanced".to_owned()),
                choices: vec![
                    "low-power".to_owned(),
                    "balanced".to_owned(),
                    "performance".to_owned(),
                ],
                path: "/tmp/platform_profile".to_owned(),
                choices_path: "/tmp/platform_profile_choices".to_owned(),
            }),
            battery_charge_type: Some(BatteryChargeTypeCapability {
                current: Some("Standard".to_owned()),
                choices: vec![
                    "Standard".to_owned(),
                    "Conservation".to_owned(),
                    "Fast".to_owned(),
                ],
                path: "/tmp/charge_type".to_owned(),
                choices_path: "/tmp/charge_types".to_owned(),
            }),
            leds: vec![
                LedCapability {
                    name: "platform::fnlock".to_owned(),
                    path: "/tmp/platform::fnlock/brightness".to_owned(),
                    brightness: Some(0),
                    max_brightness: Some(1),
                },
                LedCapability {
                    name: "platform::ylogo".to_owned(),
                    path: "/tmp/platform::ylogo/brightness".to_owned(),
                    brightness: Some(1),
                    max_brightness: Some(1),
                },
            ],
            ideapad_toggles: vec![
                IdeapadToggleCapability {
                    name: "fn_lock".to_owned(),
                    status: CapabilityStatus::ProbeOnly,
                    path: Some("/tmp/fn_lock".to_owned()),
                    current_value: Some("0".to_owned()),
                },
                IdeapadToggleCapability {
                    name: "camera_power".to_owned(),
                    status: CapabilityStatus::ProbeOnly,
                    path: Some("/tmp/camera_power".to_owned()),
                    current_value: Some("1".to_owned()),
                },
                IdeapadToggleCapability {
                    name: "usb_charging".to_owned(),
                    status: CapabilityStatus::ProbeOnly,
                    path: Some("/tmp/usb_charging".to_owned()),
                    current_value: Some("0".to_owned()),
                },
            ],
            ..Default::default()
        };

        TrayMenu::from_status_and_report(&status, &report, None, None)
    }

    fn disabled_labels(menu: &TrayMenu) -> Vec<&str> {
        menu.entries
            .iter()
            .filter_map(|entry| match entry {
                TrayMenuEntry::Item(item) if !item.enabled => Some(item.label.as_str()),
                _ => None,
            })
            .collect()
    }

    fn enabled_labels(menu: &TrayMenu) -> Vec<&str> {
        menu.entries
            .iter()
            .filter_map(|entry| match entry {
                TrayMenuEntry::Item(item) if item.enabled => Some(item.label.as_str()),
                _ => None,
            })
            .collect()
    }

    fn menu_labels(menu: &TrayMenu) -> Vec<&str> {
        menu.entries
            .iter()
            .filter_map(|entry| match entry {
                TrayMenuEntry::Item(item) => Some(item.label.as_str()),
                TrayMenuEntry::Separator => None,
            })
            .collect()
    }
}
