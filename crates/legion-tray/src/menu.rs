use legion_common::{
    format_gpu_switch_type, CapabilityRegistry, FanCurveSnapshot, GpuModePending,
    HardwareProfileApplyRun, HwmonSensor, KeyboardRgbWriteRequest,
};
use legion_control_ui::{GpuSwitchingDiagnostics, UiStatus};
use std::collections::BTreeMap;
use std::str::FromStr;

const KEYBOARD_BACKLIGHT_LED: &str = "platform::kbd_backlight";
const Y_LOGO_LED: &str = "platform::ylogo";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayAction {
    NoOp,
    SetPlatformProfile(String),
    SetBatteryChargeType(String),
    SetLedState(String, bool),
    SetIdeapadToggle(String, bool),
    SetKeyboardRgbPreset(String),
    OpenDashboard,
    RefreshStatus,
    Quit,
}

impl TrayAction {
    pub fn as_str(&self) -> String {
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
            Self::SetKeyboardRgbPreset(preset) => format!("set_keyboard_rgb:{preset}"),
            Self::OpenDashboard => "open_dashboard".to_owned(),
            Self::RefreshStatus => "refresh_status".to_owned(),
            Self::Quit => "quit".to_owned(),
        }
    }
}

impl FromStr for TrayAction {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value == "noop" {
            return Ok(Self::NoOp);
        }
        if value == "open_dashboard" {
            return Ok(Self::OpenDashboard);
        }
        if value == "refresh_status" {
            return Ok(Self::RefreshStatus);
        }
        if value == "quit" {
            return Ok(Self::Quit);
        }
        if let Some(profile) = value.strip_prefix("set_platform_profile:") {
            return non_empty_arg(value, profile).map(Self::SetPlatformProfile);
        }
        if let Some(charge_type) = value.strip_prefix("set_battery_charge_type:") {
            return non_empty_arg(value, charge_type).map(Self::SetBatteryChargeType);
        }
        if let Some(spec) = value.strip_prefix("set_led_state:") {
            let (led_id, enabled) = parse_enabled_spec(value, spec)?;
            return Ok(Self::SetLedState(led_id, enabled));
        }
        if let Some(spec) = value.strip_prefix("set_ideapad_toggle:") {
            let (toggle_id, enabled) = parse_enabled_spec(value, spec)?;
            return Ok(Self::SetIdeapadToggle(toggle_id, enabled));
        }
        if let Some(preset) = value.strip_prefix("set_keyboard_rgb:") {
            let preset = non_empty_arg(value, preset)?;
            tray_keyboard_rgb_request(&preset)?;
            return Ok(Self::SetKeyboardRgbPreset(preset));
        }
        Err(format!("unknown tray action `{value}`"))
    }
}

fn non_empty_arg(action: &str, value: &str) -> Result<String, String> {
    if value.is_empty() {
        Err(format!("tray action `{action}` is missing a value"))
    } else {
        Ok(value.to_owned())
    }
}

fn parse_enabled_spec(action: &str, spec: &str) -> Result<(String, bool), String> {
    let (id, state) = spec
        .rsplit_once(':')
        .ok_or_else(|| format!("tray action `{action}` must end in `:on` or `:off`"))?;
    let id = non_empty_arg(action, id)?;
    let enabled = match state {
        "on" => true,
        "off" => false,
        _ => {
            return Err(format!(
                "tray action `{action}` must end in `:on` or `:off`"
            ))
        }
    };
    Ok((id, enabled))
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
        gpu_switching: Option<&GpuSwitchingDiagnostics>,
        _fan_snapshot: Option<&FanCurveSnapshot>,
        hardware_profile_apply: Option<&HardwareProfileApplyRun>,
    ) -> Self {
        let mut entries = Vec::new();

        entries.push(TrayMenuEntry::Item(info_item(machine_label(status))));

        if let Some(profile) = report
            .platform_profile
            .as_ref()
            .and_then(|profile| profile.current.as_deref())
        {
            entries.push(TrayMenuEntry::Item(info_item(format!(
                "Platform profile: {}",
                humanize_choice(profile)
            ))));
        }

        if let Some(power_profile) = report
            .power_profiles
            .as_ref()
            .and_then(|profile| profile.active_profile.as_deref())
        {
            entries.push(TrayMenuEntry::Item(info_item(format!(
                "Desktop power profile: {}",
                humanize_choice(power_profile)
            ))));
        }

        if let Some(charge_type) = report
            .battery_charge_type
            .as_ref()
            .and_then(|charge_type| charge_type.current.as_deref())
        {
            entries.push(TrayMenuEntry::Item(info_item(format!(
                "Charge type: {}",
                humanize_choice(charge_type)
            ))));
        }

        if let Some(cpu_row) = cpu_row(report) {
            entries.push(TrayMenuEntry::Item(info_item(cpu_row)));
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
        if let Some(rgb_row) = keyboard_rgb_row(report) {
            entries.push(TrayMenuEntry::Item(info_item(rgb_row)));
        }
        if let Some(openrgb_row) = keyboard_rgb_openrgb_row(report) {
            entries.push(TrayMenuEntry::Item(info_item(openrgb_row)));
        }

        for fan_row in fan_rows(&report.telemetry.sensors) {
            entries.push(TrayMenuEntry::Item(info_item(fan_row)));
        }

        if let Some(gpu_row) = gpu_row(report, gpu_pending) {
            entries.push(TrayMenuEntry::Item(info_item(gpu_row)));
        }
        if let Some(gpu_switching_row) = gpu_switching_row(gpu_switching) {
            entries.push(TrayMenuEntry::Item(info_item(gpu_switching_row)));
        }

        if let Some(profile_apply) = hardware_profile_apply {
            entries.push(TrayMenuEntry::Item(info_item(format!(
                "Last profile apply: {}",
                legion_common::format_hardware_profile_apply_run_summary(Some(profile_apply))
            ))));
        }

        let missing_capabilities = status
            .capabilities
            .iter()
            .filter(|capability| capability.status == legion_common::CapabilityStatus::Missing)
            .map(|capability| capability.id.as_str())
            .collect::<Vec<_>>();
        if !missing_capabilities.is_empty() {
            entries.push(TrayMenuEntry::Item(info_item(format!(
                "Unavailable: {}",
                missing_capabilities.join(", ")
            ))));
        }

        append_quick_actions(&mut entries, report);
        entries.push(TrayMenuEntry::Separator);
        entries.push(TrayMenuEntry::Item(action_item(
            "Dashboard",
            TrayAction::OpenDashboard,
        )));
        entries.push(TrayMenuEntry::Item(action_item(
            "Refresh",
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

fn cpu_row(report: &CapabilityRegistry) -> Option<String> {
    let cpu = report.cpu_power.as_ref()?;
    let mut parts = Vec::new();
    if let Some(governor) = cpu.governor.as_deref() {
        parts.push(humanize_choice(governor));
    }
    if let Some(epp) = cpu.epp.as_deref() {
        parts.push(format!("EPP {}", humanize_choice(epp)));
    }
    if let Some(boost) = cpu.boost {
        parts.push(format!("boost {}", if boost { "on" } else { "off" }));
    }
    if parts.is_empty() {
        None
    } else {
        Some(format!("CPU: {}", parts.join(" / ")))
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
    let mut sections: Vec<Vec<TrayMenuEntry>> = Vec::new();

    if let Some(section) = quick_action_section(
        "Platform profile",
        report
            .platform_profile
            .as_ref()
            .map(|profile| profile.current.as_deref()),
        report
            .platform_profile
            .as_ref()
            .map(|profile| profile.choices.as_slice()),
        humanize_choice,
        |choice| TrayAction::SetPlatformProfile(choice.to_owned()),
    ) {
        sections.push(section);
    }

    if let Some(section) = quick_action_section(
        "Battery charging",
        report
            .battery_charge_type
            .as_ref()
            .map(|charge_type| charge_type.current.as_deref()),
        report
            .battery_charge_type
            .as_ref()
            .map(|charge_type| charge_type.choices.as_slice()),
        humanize_choice,
        |choice| TrayAction::SetBatteryChargeType(choice.to_owned()),
    ) {
        sections.push(section);
    }

    if let Some(section) = led_quick_action_section(report) {
        sections.push(section);
    }

    if let Some(section) = keyboard_backlight_quick_action_section(report) {
        sections.push(section);
    }

    if let Some(section) = keyboard_rgb_guidance_section(report) {
        sections.push(section);
    }

    if let Some(section) = ideapad_toggle_quick_action_section(report) {
        sections.push(section);
    }

    if let Some(section) = camera_power_guidance_section(report) {
        sections.push(section);
    }

    if let Some(section) = usb_charging_guidance_section(report) {
        sections.push(section);
    }

    if !sections.is_empty() {
        entries.push(TrayMenuEntry::Separator);
        let mut first = true;
        for section in sections {
            if !first {
                entries.push(TrayMenuEntry::Separator);
            }
            entries.extend(section);
            first = false;
        }
    }
}

fn quick_action_section<F>(
    header: &str,
    current: Option<Option<&str>>,
    choices: Option<&[String]>,
    label: impl Fn(&str) -> String,
    action: F,
) -> Option<Vec<TrayMenuEntry>>
where
    F: Fn(&str) -> TrayAction,
{
    let choices = choices?;
    let current = current.flatten();
    if choices.len() <= 1 {
        return None;
    }

    let header = match current {
        Some(current) => format!("{header} (current: {})", label(current)),
        None => header.to_owned(),
    };
    let mut entries = vec![TrayMenuEntry::Item(info_item(header))];
    for choice in choices {
        if Some(choice.as_str()) == current {
            entries.push(TrayMenuEntry::Item(info_item(format!(
                "{} (current)",
                label(choice)
            ))));
        } else {
            entries.push(TrayMenuEntry::Item(action_item(
                label(choice),
                action(choice),
            )));
        }
    }
    Some(entries)
}

fn led_quick_action_section(report: &CapabilityRegistry) -> Option<Vec<TrayMenuEntry>> {
    let led = report
        .leds
        .iter()
        .find(|led| led.name == Y_LOGO_LED && led.max_brightness == Some(1))?;
    let current = led.brightness?;
    if current != 0 && current != 1 {
        return None;
    }

    let mut entries = vec![TrayMenuEntry::Item(info_item(format!(
        "Logo light (current: {}, guarded)",
        binary_state_label(current).unwrap_or("unknown")
    )))];
    entries.push(TrayMenuEntry::Item(action_item(
        "Turn off",
        TrayAction::SetLedState(led.name.clone(), false),
    )));
    entries.push(TrayMenuEntry::Item(action_item(
        "Turn on",
        TrayAction::SetLedState(led.name.clone(), true),
    )));
    for entry in &mut entries {
        if let TrayMenuEntry::Item(item) = entry {
            if item.label == "Turn off" && current == 0 {
                item.label = "Turn off (current)".to_owned();
                item.enabled = false;
                item.action = TrayAction::NoOp;
            }
            if item.label == "Turn on" && current == 1 {
                item.label = "Turn on (current)".to_owned();
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

    match led.name.as_str() {
        Y_LOGO_LED => Some(format!("Logo LED: {}", binary_state_label(brightness)?)),
        KEYBOARD_BACKLIGHT_LED => Some(format!(
            "Keyboard backlight: {}",
            backlight_level_label(brightness)
        )),
        _ => None,
    }
}

fn keyboard_rgb_row(report: &CapabilityRegistry) -> Option<String> {
    if let Some(rgb) = report.keyboard_rgb.as_ref() {
        let effect = rgb.current_effect.as_deref().unwrap_or("unknown");
        return Some(format!(
            "Keyboard RGB: {} zones / effect {}",
            rgb.zones.len(),
            humanize_choice(effect)
        ));
    }

    let candidates = &report.keyboard_rgb_candidates;
    if candidates.is_empty() {
        return None;
    }
    let mut ids = candidates
        .iter()
        .filter_map(|candidate| {
            match (
                candidate.vendor_id.as_deref(),
                candidate.product_id.as_deref(),
            ) {
                (Some(vendor), Some(product)) => Some(format!("{vendor}:{product}")),
                _ => None,
            }
        })
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    let ids = if ids.is_empty() {
        "unknown VID:PID".to_owned()
    } else {
        ids.join(", ")
    };
    Some(format!(
        "Keyboard RGB: {} HID research candidate{} ({ids})",
        candidates.len(),
        if candidates.len() == 1 { "" } else { "s" }
    ))
}

fn keyboard_rgb_openrgb_row(report: &CapabilityRegistry) -> Option<String> {
    let openrgb = report.keyboard_rgb_openrgb.as_ref()?;
    if !openrgb.installed {
        return Some("Keyboard RGB OpenRGB: not installed".to_owned());
    }
    let Some(device) = openrgb.devices.first() else {
        return Some("Keyboard RGB OpenRGB: installed, keyboard not detected".to_owned());
    };
    let modes = if device.modes.is_empty() {
        "modes unknown".to_owned()
    } else {
        device.modes.join(", ")
    };
    Some(format!(
        "Keyboard RGB OpenRGB: {} ({modes}) backend_ready={}",
        device.description.as_deref().unwrap_or(&device.name),
        openrgb.backend_ready
    ))
}

fn keyboard_backlight_quick_action_section(
    report: &CapabilityRegistry,
) -> Option<Vec<TrayMenuEntry>> {
    let led = report
        .leds
        .iter()
        .find(|led| led.name == KEYBOARD_BACKLIGHT_LED && led.max_brightness.unwrap_or(0) >= 1)?;
    let current = led.brightness?;
    if current < 0 {
        return None;
    }

    let mut entries = vec![TrayMenuEntry::Item(info_item(format!(
        "Keyboard backlight (current: {}, guarded)",
        backlight_level_label(current)
    )))];
    entries.push(TrayMenuEntry::Item(action_item(
        "Turn off",
        TrayAction::SetLedState(led.name.clone(), false),
    )));
    entries.push(TrayMenuEntry::Item(action_item(
        "Turn on",
        TrayAction::SetLedState(led.name.clone(), true),
    )));
    for entry in &mut entries {
        if let TrayMenuEntry::Item(item) = entry {
            if item.label == "Turn off" && current == 0 {
                item.label = "Turn off (current)".to_owned();
                item.enabled = false;
                item.action = TrayAction::NoOp;
            }
            if item.label == "Turn on" && current > 0 {
                item.label = "Turn on (current)".to_owned();
                item.enabled = false;
                item.action = TrayAction::NoOp;
            }
        }
    }
    Some(entries)
}

fn keyboard_rgb_guidance_section(report: &CapabilityRegistry) -> Option<Vec<TrayMenuEntry>> {
    if let Some(section) = keyboard_rgb_quick_action_section(report) {
        return Some(section);
    }

    if report.keyboard_rgb_candidates.is_empty() {
        return None;
    }

    Some(vec![
        TrayMenuEntry::Item(info_item(format!(
            "Keyboard RGB research ({} candidate{}, no safe write backend)",
            report.keyboard_rgb_candidates.len(),
            if report.keyboard_rgb_candidates.len() == 1 {
                ""
            } else {
                "s"
            }
        ))),
        TrayMenuEntry::Item(action_item("RGB evidence", TrayAction::OpenDashboard)),
    ])
}

fn keyboard_rgb_quick_action_section(report: &CapabilityRegistry) -> Option<Vec<TrayMenuEntry>> {
    let (current, modes, backend_label) = if let Some(rgb) = report.keyboard_rgb.as_ref() {
        (
            rgb.current_effect.as_deref(),
            rgb.effects.as_slice(),
            "native backend",
        )
    } else {
        let openrgb = report.keyboard_rgb_openrgb.as_ref()?;
        if !openrgb.backend_ready {
            return None;
        }
        let device = openrgb.devices.first()?;
        (
            device.current_mode.as_deref(),
            device.modes.as_slice(),
            "OpenRGB SDK",
        )
    };

    let mut entries = vec![TrayMenuEntry::Item(info_item(format!(
        "Keyboard RGB (current: {}, guarded via {backend_label})",
        current
            .map(humanize_choice)
            .unwrap_or_else(|| "Unknown".to_owned())
    )))];

    for (preset, label, effect) in [
        ("direct-dim", "Static dim", "Direct"),
        ("breathing-dim", "Breathing dim", "Breathing"),
        ("rainbow-wave", "Rainbow wave", "Rainbow Wave"),
        ("spectrum-cycle", "Spectrum cycle", "Spectrum Cycle"),
    ] {
        if modes.iter().any(|mode| mode.eq_ignore_ascii_case(effect)) {
            entries.push(TrayMenuEntry::Item(action_item(
                label,
                TrayAction::SetKeyboardRgbPreset(preset.to_owned()),
            )));
        }
    }

    entries.push(TrayMenuEntry::Item(action_item(
        "RGB settings",
        TrayAction::OpenDashboard,
    )));

    if entries.len() <= 2 {
        None
    } else {
        Some(entries)
    }
}

pub fn tray_keyboard_rgb_request(preset: &str) -> Result<KeyboardRgbWriteRequest, String> {
    let effect = match preset {
        "direct-dim" => "Direct",
        "breathing-dim" => "Breathing",
        "rainbow-wave" => "Rainbow Wave",
        "spectrum-cycle" => "Spectrum Cycle",
        _ => return Err(format!("unknown keyboard RGB tray preset `{preset}`")),
    };

    Ok(KeyboardRgbWriteRequest {
        effect: effect.to_owned(),
        colors: BTreeMap::from([
            ("left_side".to_owned(), "#333333".to_owned()),
            ("left_center".to_owned(), "#333333".to_owned()),
            ("right_center".to_owned(), "#333333".to_owned()),
            ("right_side".to_owned(), "#333333".to_owned()),
        ]),
        brightness: 40,
        speed: Some(30),
    })
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

fn backlight_level_label(value: i64) -> String {
    match binary_state_label(value) {
        Some(label) => label.to_owned(),
        None => format!("level {value}"),
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

    let mut entries = vec![TrayMenuEntry::Item(info_item(format!(
        "Fn-lock (current: {})",
        binary_toggle_state_label(current).unwrap_or("unknown")
    )))];
    entries.push(TrayMenuEntry::Item(action_item(
        "Turn off",
        TrayAction::SetIdeapadToggle(toggle.name.clone(), false),
    )));
    entries.push(TrayMenuEntry::Item(action_item(
        "Turn on",
        TrayAction::SetIdeapadToggle(toggle.name.clone(), true),
    )));
    for entry in &mut entries {
        if let TrayMenuEntry::Item(item) = entry {
            if item.label == "Turn off" && current == "0" {
                item.label = "Turn off (current)".to_owned();
                item.enabled = false;
                item.action = TrayAction::NoOp;
            }
            if item.label == "Turn on" && current == "1" {
                item.label = "Turn on (current)".to_owned();
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
                "on · guarded change in Dashboard"
            } else {
                "off · guarded change in Dashboard"
            }
        ))),
        TrayMenuEntry::Item(action_item("Camera settings", TrayAction::OpenDashboard)),
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
                "on · confirmed change in Dashboard"
            } else {
                "off · confirmed change in Dashboard"
            }
        ))),
        TrayMenuEntry::Item(action_item(
            "USB charging settings",
            TrayAction::OpenDashboard,
        )),
    ])
}

fn humanize_choice(value: &str) -> String {
    value
        .split(['-', '_'])
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(first) => {
                    let mut label = first.to_ascii_uppercase().to_string();
                    label.push_str(chars.as_str());
                    label
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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
                "GPU: switch to {} pending (was {}) — reboot required",
                pending.requested_mode, previous
            ),
            None => format!(
                "GPU: switch to {} pending — reboot required",
                pending.requested_mode
            ),
        };
        return Some(detail);
    }

    report.gpu.as_ref().and_then(|gpu| {
        gpu.mode.as_deref().map(|mode| {
            format!(
                "GPU: {mode} (switch type: {})",
                format_gpu_switch_type(gpu.switch_type)
            )
        })
    })
}

fn gpu_switching_row(gpu_switching: Option<&GpuSwitchingDiagnostics>) -> Option<String> {
    let gpu_switching = gpu_switching?;
    if gpu_switching.status == "unavailable" {
        return None;
    }

    let mut details = vec![
        format!("switch type {}", gpu_switching.switch_type),
        if gpu_switching.runtime_plan_available {
            "runtime plan available".to_owned()
        } else {
            "runtime plan blocked".to_owned()
        },
    ];
    if let Some(provider) = gpu_switching.provider.as_deref() {
        details.push(format!("provider {provider}"));
    }
    if let Some(mode) = gpu_switching.current_mode.as_deref() {
        details.push(format!("mode {mode}"));
    }

    Some(format!(
        "GPU switching: {} ({})",
        humanize_choice(&gpu_switching.status),
        details.join("; ")
    ))
}

#[cfg(test)]
mod tests {
    use legion_common::{
        BatteryChargeTypeCapability, BatteryTelemetry, Capability, CapabilityRegistry,
        CapabilityStatus, FanCurvePointSnapshot, FanCurveSnapshot, GpuCapability, GpuModePending,
        GpuSwitchType, HardwareProfileApplyRun, HardwareSummary, HwmonSensor,
        IdeapadToggleCapability, KeyboardRgbOpenRgbDevice, KeyboardRgbOpenRgbStatus, LedCapability,
        PlatformProfileCapability, PowerProfilesCapability, RiskLevel,
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
            power_profiles: Some(PowerProfilesCapability {
                bus: "system".to_owned(),
                well_known_name: "org.freedesktop.UPower.PowerProfiles".to_owned(),
                unique_owner: Some(":1.42".to_owned()),
                active_profile: Some("power-saver".to_owned()),
                status: CapabilityStatus::ProbeOnly,
                detail: None,
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
                    power_now_uw: None,
                    cycle_count: None,
                    ..Default::default()
                }),
                ac_adapters: Vec::new(),
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
                LedCapability {
                    name: "platform::kbd_backlight".to_owned(),
                    path: "/tmp/platform::kbd_backlight/brightness".to_owned(),
                    brightness: Some(1),
                    max_brightness: Some(2),
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
            keyboard_rgb_openrgb: Some(KeyboardRgbOpenRgbStatus {
                installed: true,
                path: Some("/usr/bin/openrgb".to_owned()),
                devices: vec![KeyboardRgbOpenRgbDevice {
                    index: 0,
                    name: "Lenovo 5 2023".to_owned(),
                    device_type: Some("Laptop".to_owned()),
                    description: Some("Lenovo 4-Zone device".to_owned()),
                    modes: vec![
                        "Direct".to_owned(),
                        "Breathing".to_owned(),
                        "Rainbow Wave".to_owned(),
                        "Spectrum Cycle".to_owned(),
                    ],
                    current_mode: Some("Direct".to_owned()),
                    zones: vec!["Keyboard".to_owned()],
                    leds: vec![
                        "Left side".to_owned(),
                        "Left center".to_owned(),
                        "Right center".to_owned(),
                        "Right side".to_owned(),
                    ],
                }],
                i2c_dev_loaded: true,
                user_in_i2c_group: true,
                has_i2c_rw_access: true,
                has_hidraw_rw_access: true,
                backend_ready: false,
                write_support_claimed: false,
                sdk_helper_installed: false,
                sdk_helper_path: None,
                sdk_server_running: false,
                sdk_snapshot_supported: false,
                sdk_active_mode: None,
                sdk_color_zones: vec![],
                sdk_colors: std::collections::BTreeMap::new(),
            }),
            ..Default::default()
        };
        let gpu_pending = GpuModePending {
            requested_mode: "hybrid".to_owned(),
            previous_mode: Some("nvidia".to_owned()),
            reboot_required: true,
        };
        let gpu_switching = GpuSwitchingDiagnostics {
            status: "runtime_mux_research_blocked".to_owned(),
            provider: Some("fixture-mux".to_owned()),
            current_mode: Some("hybrid".to_owned()),
            switch_type: "runtime-mux".to_owned(),
            execution_model: "runtime_mux".to_owned(),
            runtime_plan_available: false,
            blockers: vec!["no automatic display recovery evidence has been captured".to_owned()],
            evidence: vec!["provider=fixture-mux".to_owned()],
            next_action:
                "capture read-only mux state and recovery evidence before adding a switch plan"
                    .to_owned(),
        };
        let fan_snapshot = FanCurveSnapshot {
            curve_id: "legion_hwmon".to_owned(),
            path: Some("/tmp/hwmon".to_owned()),
            points: vec![FanCurvePointSnapshot {
                path: "/tmp/hwmon/pwm1_auto_point1_temp".to_owned(),
                value: "42000".to_owned(),
            }],
        };
        let profile_apply = HardwareProfileApplyRun {
            profile_id: "co_test".to_owned(),
            profile_label: "CO test".to_owned(),
            timestamp_unix_secs: 1,
            completed: false,
            message: "hardware profile apply stopped after first non-applied action".to_owned(),
            results: Vec::new(),
        };

        let menu = TrayMenu::from_status_and_report(
            &status,
            &report,
            Some(&gpu_pending),
            Some(&gpu_switching),
            Some(&fan_snapshot),
            Some(&profile_apply),
        );

        assert_eq!(
            disabled_labels(&menu),
            [
                "82WM Legion Pro 5 16ARX8",
                "Platform profile: Balanced",
                "Desktop power profile: Power Saver",
                "Charge type: Standard",
                "Battery: 79% / Charging / Good",
                "Logo LED: on",
                "Keyboard backlight: on",
                "Fn-lock: off",
                "Camera power: on",
                "USB charging: off",
                "Keyboard RGB OpenRGB: Lenovo 4-Zone device (Direct, Breathing, Rainbow Wave, Spectrum Cycle) backend_ready=false",
                "Fan: CPU Fan 2410 RPM",
                "GPU: switch to hybrid pending (was nvidia) — reboot required",
                "GPU switching: Runtime Mux Research Blocked (switch type runtime-mux; runtime plan blocked; provider fixture-mux; mode hybrid)",
                "Last profile apply: co_test stopped: hardware profile apply stopped after first non-applied action",
                "Unavailable: gpu",
                "Platform profile (current: Balanced)",
                "Balanced (current)",
                "Battery charging (current: Standard)",
                "Standard (current)",
                "Logo light (current: on, guarded)",
                "Turn on (current)",
                "Keyboard backlight (current: on, guarded)",
                "Turn on (current)",
                "Fn-lock (current: off)",
                "Turn off (current)",
                "Camera power: on · guarded change in Dashboard",
                "USB charging: off · confirmed change in Dashboard",
            ]
        );
        assert_eq!(
            enabled_labels(&menu),
            [
                "Low Power",
                "Performance",
                "Conservation",
                "Fast",
                "Turn off",
                "Turn off",
                "Turn on",
                "Camera settings",
                "USB charging settings",
                "Dashboard",
                "Refresh",
                "Quit",
            ]
        );
        assert!(!menu_labels(&menu)
            .iter()
            .any(|label| label.contains("input12::capslock")));
        assert!(menu_labels(&menu)
            .iter()
            .any(|label| label == &"Keyboard backlight: on"));
        assert!(!menu_labels(&menu)
            .iter()
            .any(|label| label.contains("conservation_mode")));
        assert!(!menu_labels(&menu)
            .iter()
            .any(|label| label.starts_with("Available profiles:")));
        assert!(!menu_labels(&menu)
            .iter()
            .any(|label| label.starts_with("Available charging modes:")));
        assert!(!menu_labels(&menu)
            .iter()
            .any(|label| label.starts_with("Fan presets:")));
    }

    #[test]
    fn menu_builder_adds_keyboard_rgb_presets_when_openrgb_sdk_is_ready() {
        let status = UiStatus::from_parts(
            HardwareSummary {
                sysfs_root: "/tmp/fixture".to_owned(),
                vendor: Some("LENOVO".to_owned()),
                product_name: Some("82WM".to_owned()),
                product_version: Some("Legion Pro 5 16ARX8".to_owned()),
                product_sku: None,
            },
            vec![capability(
                "keyboard_rgb_openrgb",
                CapabilityStatus::ProbeOnly,
            )],
        )
        .unwrap();
        let report = CapabilityRegistry {
            keyboard_rgb_openrgb: Some(KeyboardRgbOpenRgbStatus {
                installed: true,
                path: Some("/usr/bin/openrgb".to_owned()),
                devices: vec![KeyboardRgbOpenRgbDevice {
                    index: 0,
                    name: "Lenovo 5 2023".to_owned(),
                    device_type: Some("Laptop".to_owned()),
                    description: Some("Lenovo 4-Zone device".to_owned()),
                    modes: vec![
                        "Direct".to_owned(),
                        "Breathing".to_owned(),
                        "Rainbow Wave".to_owned(),
                        "Spectrum Cycle".to_owned(),
                    ],
                    current_mode: Some("Breathing".to_owned()),
                    zones: vec!["Keyboard".to_owned()],
                    leds: vec![
                        "Left side".to_owned(),
                        "Left center".to_owned(),
                        "Right center".to_owned(),
                        "Right side".to_owned(),
                    ],
                }],
                i2c_dev_loaded: true,
                user_in_i2c_group: false,
                has_i2c_rw_access: true,
                has_hidraw_rw_access: true,
                backend_ready: true,
                write_support_claimed: true,
                sdk_helper_installed: true,
                sdk_helper_path: Some(
                    "/home/darrian/.local/bin/ratvantage-openrgb-keyboard-rgb-sdk-helper"
                        .to_owned(),
                ),
                sdk_server_running: true,
                sdk_snapshot_supported: true,
                sdk_active_mode: Some("Breathing".to_owned()),
                sdk_color_zones: vec![
                    "left_center".to_owned(),
                    "left_side".to_owned(),
                    "right_center".to_owned(),
                    "right_side".to_owned(),
                ],
                sdk_colors: std::collections::BTreeMap::new(),
            }),
            ..Default::default()
        };

        let menu = TrayMenu::from_status_and_report(&status, &report, None, None, None, None);
        let labels = menu_labels(&menu);

        assert!(labels.iter().any(|label| {
            label == &"Keyboard RGB (current: Breathing, guarded via OpenRGB SDK)"
        }));
        assert_eq!(
            enabled_labels(&menu)
                .into_iter()
                .filter(|label| matches!(
                    *label,
                    "Static dim" | "Breathing dim" | "Rainbow wave" | "Spectrum cycle"
                ))
                .collect::<Vec<_>>(),
            [
                "Static dim",
                "Breathing dim",
                "Rainbow wave",
                "Spectrum cycle"
            ]
        );
        assert!(menu.entries.iter().any(|entry| matches!(
            entry,
            TrayMenuEntry::Item(item)
                if item.action == TrayAction::SetKeyboardRgbPreset("breathing-dim".to_owned())
        )));
    }

    #[test]
    fn tray_keyboard_rgb_action_parser_accepts_known_presets() {
        assert_eq!(
            "set_keyboard_rgb:breathing-dim".parse::<TrayAction>(),
            Ok(TrayAction::SetKeyboardRgbPreset("breathing-dim".to_owned()))
        );
        assert!("set_keyboard_rgb:unknown".parse::<TrayAction>().is_err());

        let request = tray_keyboard_rgb_request("breathing-dim").unwrap();
        assert_eq!(request.effect, "Breathing");
        assert_eq!(request.brightness, 40);
        assert_eq!(request.speed, Some(30));
        assert_eq!(
            request.colors.get("left_side").map(String::as_str),
            Some("#333333")
        );
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

        let menu = TrayMenu::from_status_and_report(
            &status,
            &CapabilityRegistry::default(),
            None,
            None,
            None,
            None,
        );

        assert_eq!(disabled_labels(&menu), ["82WM Legion Pro 5 16ARX8"]);
        assert_eq!(enabled_labels(&menu), ["Dashboard", "Refresh", "Quit"]);
        assert!(!menu_labels(&menu)
            .iter()
            .any(|label| label.starts_with("Battery:")));
        assert!(!menu_labels(&menu)
            .iter()
            .any(|label| label.starts_with("GPU:")));
        assert!(!menu_labels(&menu)
            .iter()
            .any(|label| label.starts_with("Saved fan curve:")));
        assert!(!menu_labels(&menu)
            .iter()
            .any(|label| label.starts_with("Fan presets:")));
        assert!(!menu_labels(&menu)
            .iter()
            .any(|label| label.starts_with("Capabilities:")));
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

        let menu = TrayMenu::from_status_and_report(&status, &report, None, None, None, None);

        assert!(!menu_labels(&menu)
            .iter()
            .any(|label| label == &"Platform profile"));
        assert!(!menu_labels(&menu)
            .iter()
            .any(|label| label == &"Battery charging"));
        assert!(!enabled_labels(&menu)
            .iter()
            .any(|label| label == &"Balanced"));
        assert!(!enabled_labels(&menu)
            .iter()
            .any(|label| label == &"Standard"));
        assert!(!enabled_labels(&menu)
            .iter()
            .any(|label| label == &"Turn on" || label == &"Turn off"));
    }

    #[test]
    fn menu_builder_keeps_dashboard_refresh_quit_after_quick_action_sections() {
        let menu = menu_builder_fixture();
        assert!(menu_labels(&menu)
            .iter()
            .any(|label| label == &"GPU: integrated (switch type: reboot-required)"));
        let enabled = enabled_labels(&menu);
        assert_eq!(
            &enabled[enabled.len().saturating_sub(3)..],
            ["Dashboard", "Refresh", "Quit"]
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
            gpu: Some(GpuCapability {
                provider: "envycontrol".to_owned(),
                status: CapabilityStatus::ProbeOnly,
                mode: Some("integrated".to_owned()),
                switch_type: GpuSwitchType::RebootRequired,
                switch_notes: vec!["envycontrol switch evidence".to_owned()],
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

        TrayMenu::from_status_and_report(&status, &report, None, None, None, None)
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
