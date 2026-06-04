use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CapabilityRegistry {
    pub hardware: HardwareSummary,
    pub capabilities: Vec<Capability>,
    pub platform_profile: Option<PlatformProfileCapability>,
    pub battery_charge_type: Option<BatteryChargeTypeCapability>,
    pub hwmon_sensors: Vec<HwmonSensor>,
    pub fan_curves: Vec<FanCurveCapability>,
    pub leds: Vec<LedCapability>,
    /// True keyboard RGB zones/effects. This is separate from simple Linux LED backlight nodes.
    #[serde(default)]
    pub keyboard_rgb: Option<KeyboardRgbCapability>,
    /// Read-only evidence for possible keyboard RGB controllers. These are not write-capable claims.
    #[serde(default)]
    pub keyboard_rgb_candidates: Vec<KeyboardRgbCandidate>,
    /// Read-only OpenRGB discovery/access evidence. This is not a RatVantage write backend claim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyboard_rgb_openrgb: Option<KeyboardRgbOpenRgbStatus>,
    pub firmware_attributes: Vec<FirmwareAttributeCapability>,
    pub ideapad_toggles: Vec<IdeapadToggleCapability>,
    pub gpu: Option<GpuCapability>,
    /// AMD GPU runtime DPM force-level control exposed by amdgpu under `/sys/class/drm`.
    #[serde(default)]
    pub amd_gpu_power_dpm: Option<AmdGpuPowerDpmCapability>,
    /// GPU PCI hotswap capability: runtime integrated<->hybrid switching without display restart.
    #[serde(default)]
    pub gpu_runtime: Option<GpuRuntimeCapability>,
    /// CPU frequency-scaling / amd-pstate parameters under `/sys/devices/system/cpu`.
    #[serde(default)]
    pub cpu_power: Option<CpuPowerCapability>,
    /// ACPI thermal zones under `/sys/class/thermal`.
    #[serde(default)]
    pub thermal_zones: Vec<ThermalZone>,
    /// `org.freedesktop.UPower.PowerProfiles` probe when `sysfs_root` is `/` (fixtures use `null`).
    #[serde(default)]
    pub power_profiles: Option<PowerProfilesCapability>,
    pub telemetry: TelemetrySnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonState {
    pub schema_version: u32,
    #[serde(default)]
    pub gpu_mode_pending: Option<GpuModePending>,
    #[serde(default)]
    pub last_known_good_fan_curve: Option<FanCurveSnapshot>,
    /// Preferred packaged fan preset id when a given `platform_profile` is active (advisory app state only).
    #[serde(default)]
    pub fan_preset_by_platform_profile: BTreeMap<String, String>,
    /// When true, the daemon may run resume-time fan preset planning (dry-run until fan writes ship).
    #[serde(default)]
    pub fan_preset_reapply_after_resume: bool,
    /// Last requested all-core Curve Optimizer write. Write-only until a ryzen_smu read-back backend is available.
    #[serde(default)]
    pub last_curve_optimizer_all_core: Option<CurveOptimizerWriteState>,
    /// Daemon-owned hardware profiles. Applying is planned through dry-run write plans before any action executes.
    #[serde(default)]
    pub hardware_profiles: BTreeMap<String, HardwareProfile>,
    /// Event trigger to hardware profile mapping. Trigger execution is explicit and daemon-gated.
    #[serde(default)]
    pub hardware_profile_triggers: BTreeMap<String, String>,
    /// Last manual hardware profile application run, including per-action write results.
    #[serde(default)]
    pub last_hardware_profile_apply: Option<HardwareProfileApplyRun>,
    /// Last platform profile sampled by the daemon-owned profile-change observer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_observed_platform_profile: Option<String>,
    /// Recent external platform profile changes observed by the daemon.
    #[serde(default)]
    pub recent_platform_profile_changes: Vec<PlatformProfileChangeEvent>,
    /// Last desktop power profile sampled by the daemon-owned power-profile observer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_observed_desktop_power_profile: Option<String>,
    /// Recent desktop power profile changes observed by the daemon.
    #[serde(default)]
    pub recent_desktop_power_profile_changes: Vec<DesktopPowerProfileChangeEvent>,
    /// Daemon-owned automation rules. Rules resolve to hardware profiles before execution.
    #[serde(default)]
    pub automation_rules: BTreeMap<String, AutomationRule>,
    /// Last automation rule evaluations/applications by rule id.
    #[serde(default)]
    pub last_automation_rule_apply: BTreeMap<String, AutomationRuleApplyRun>,
}

impl Default for DaemonState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            gpu_mode_pending: None,
            last_known_good_fan_curve: None,
            fan_preset_by_platform_profile: BTreeMap::new(),
            fan_preset_reapply_after_resume: false,
            last_curve_optimizer_all_core: None,
            hardware_profiles: BTreeMap::new(),
            hardware_profile_triggers: BTreeMap::new(),
            last_hardware_profile_apply: None,
            last_observed_platform_profile: None,
            recent_platform_profile_changes: Vec::new(),
            last_observed_desktop_power_profile: None,
            recent_desktop_power_profile_changes: Vec::new(),
            automation_rules: BTreeMap::new(),
            last_automation_rule_apply: BTreeMap::new(),
        }
    }
}

pub const HARDWARE_PROFILE_TRIGGER_IDS: &[&str] = &[
    "ac_connected",
    "ac_disconnected",
    "resume",
    "platform_profile_changed",
    "desktop_power_profile_changed",
    "gpu_mode_reboot_completed",
    "manual",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HardwareProfile {
    pub schema_version: u32,
    pub label: String,
    #[serde(default)]
    pub actions: HardwareProfileActions,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HardwareProfileActions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform_profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battery_charge_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyboard_rgb: Option<KeyboardRgbWriteRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_governor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_epp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_boost: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conservation_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amd_gpu_dpm_force_level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curve_optimizer_all_core: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub firmware_attributes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HardwareProfileApplyPreview {
    pub profile_id: String,
    pub profile_label: String,
    pub plans: Vec<WriteDryRunPlan>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomThermalPlanPreview {
    pub sequence_id: String,
    pub target: String,
    pub plans: Vec<WriteDryRunPlan>,
    pub rollback_order: Vec<String>,
    pub safety_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FirmwarePptPreset {
    pub id: String,
    pub label: String,
    pub description: String,
    pub values: BTreeMap<String, String>,
    pub safety_note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HardwareProfileApplyRun {
    pub profile_id: String,
    pub profile_label: String,
    pub timestamp_unix_secs: u64,
    pub completed: bool,
    pub message: String,
    pub results: Vec<HardwareProfileApplyActionResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HardwareProfileApplyActionResult {
    pub action_id: String,
    pub result: WriteExecutionResult,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutomationRule {
    pub schema_version: u32,
    pub label: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(flatten)]
    pub kind: AutomationRuleKind,
}

fn default_true() -> bool {
    true
}

fn default_automation_cooldown_secs() -> u64 {
    300
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AutomationRuleKind {
    FastChargeUntilThreshold {
        threshold_percent: u8,
        fast_charge_profile_id: String,
        protect_profile_id: String,
        #[serde(default = "default_true")]
        require_ac: bool,
        #[serde(default = "default_automation_cooldown_secs")]
        cooldown_secs: u64,
    },
    AcProfileRouter {
        ac_profile_id: String,
        battery_profile_id: String,
        #[serde(default = "default_automation_cooldown_secs")]
        cooldown_secs: u64,
    },
    BatteryProfileThreshold {
        threshold_percent: u8,
        profile_id: String,
        #[serde(default)]
        when_below_or_equal: bool,
        #[serde(default)]
        require_ac: Option<bool>,
        #[serde(default = "default_automation_cooldown_secs")]
        cooldown_secs: u64,
    },
    PeriodicIdle {
        profile_id: String,
        #[serde(default = "default_automation_cooldown_secs")]
        cooldown_secs: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutomationRuleEvaluation {
    pub rule_id: String,
    pub rule_label: String,
    pub enabled: bool,
    pub matched: bool,
    pub reason: String,
    pub battery_capacity_percent: Option<i64>,
    pub ac_online: Option<bool>,
    pub selected_profile_id: Option<String>,
    pub profile_preview: Option<HardwareProfileApplyPreview>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutomationRuleApplyRun {
    pub timestamp_unix_secs: u64,
    pub evaluation: AutomationRuleEvaluation,
    pub profile_run: Option<HardwareProfileApplyRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlatformProfileChangeEvent {
    pub timestamp_unix_secs: u64,
    pub previous_profile: String,
    pub current_profile: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopPowerProfileChangeEvent {
    pub timestamp_unix_secs: u64,
    pub previous_profile: String,
    pub current_profile: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CurveOptimizerWriteState {
    pub signed_offset: i32,
    pub encoded_value: u32,
    pub backend: String,
    pub readback_status: CurveOptimizerReadbackStatus,
    pub timestamp_unix_secs: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CurveOptimizerReadbackStatus {
    WriteOnly,
    Verified,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RyzenBackendStatus {
    pub ryzenadj: RyzenAdjBackendStatus,
    pub ryzen_smu: RyzenSmuBackendStatus,
    pub curve_optimizer_backend: String,
    pub curve_optimizer_readback_status: CurveOptimizerReadbackStatus,
    pub setup_assistant: RyzenSmuSetupAssistant,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RyzenAdjBackendStatus {
    pub path: String,
    pub available: bool,
    pub executable: bool,
    pub supports_curve_optimizer: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RyzenSmuBackendStatus {
    pub module_loaded: bool,
    pub sysfs_path: String,
    pub sysfs_available: bool,
    pub pm_table_available: bool,
    pub readback_available: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RyzenSmuSetupAssistant {
    pub recommended: bool,
    pub reason: String,
    pub commands: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GpuModePending {
    pub requested_mode: String,
    pub previous_mode: Option<String>,
    pub reboot_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FanCurveSnapshot {
    pub curve_id: String,
    pub path: Option<String>,
    pub points: Vec<FanCurvePointSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FanCurvePointSnapshot {
    pub path: String,
    pub value: String,
}

/// Human-readable value for `gpu_pending_reboot=` in `--overview`, tray status lines, and tooltips.
/// Returns `"none"` when there is no pending switch.
/// One-line summary for `--overview` and similar surfaces (`not_applicable` when the probe skipped D-Bus).
pub fn format_power_profiles_probe_summary(value: Option<&PowerProfilesCapability>) -> String {
    match value {
        None => "not_applicable".to_owned(),
        Some(p) if p.unique_owner.is_some() => {
            let owner = p.unique_owner.as_deref().unwrap_or("");
            let active = p.active_profile.as_deref().unwrap_or("unknown");
            format!("bus={} owner={owner} active={active}", p.bus)
        }
        Some(p) => p.detail.clone().unwrap_or_else(|| "unavailable".to_owned()),
    }
}

pub fn format_gpu_mode_pending_summary(pending: Option<&GpuModePending>) -> String {
    match pending {
        None => "none".to_owned(),
        Some(pending) => match pending.previous_mode.as_deref() {
            Some(prev) if pending.reboot_required => format!(
                "{} pending (was {}); reboot required",
                pending.requested_mode, prev
            ),
            Some(prev) => format!("{} pending (was {})", pending.requested_mode, prev),
            None if pending.reboot_required => {
                format!("{} pending; reboot required", pending.requested_mode)
            }
            None => format!("{} pending", pending.requested_mode),
        },
    }
}

/// Human-readable value for `last_known_good_fan_curve=` in `--overview`, tray status lines, and tooltips.
/// Returns `"none"` when no snapshot is stored.
pub fn format_fan_curve_snapshot_summary(snapshot: Option<&FanCurveSnapshot>) -> String {
    match snapshot {
        None => "none".to_owned(),
        Some(snapshot) => {
            let n = snapshot.points.len();
            let unit = if n == 1 { "point" } else { "points" };
            format!("{n} {unit} on {}", snapshot.curve_id)
        }
    }
}

/// Human-readable value for the last manual hardware profile apply run.
/// Returns `"none"` when no profile has been applied.
pub fn format_hardware_profile_apply_run_summary(run: Option<&HardwareProfileApplyRun>) -> String {
    let Some(run) = run else {
        return "none".to_owned();
    };

    if run.completed {
        let n = run.results.len();
        let unit = if n == 1 { "action" } else { "actions" };
        return format!("{} completed; {n} {unit} applied", run.profile_id);
    }

    if let Some(result) = run.results.iter().find(|result| !result.result.applied) {
        return format!(
            "{} stopped at {}: {} - {}",
            run.profile_id,
            result.action_id,
            write_execution_status_label(result.result.status),
            result.result.message
        );
    }

    format!("{} stopped: {}", run.profile_id, run.message)
}

fn write_execution_status_label(status: WriteExecutionStatus) -> &'static str {
    match status {
        WriteExecutionStatus::BlockedByPolicy => "blocked_by_policy",
        WriteExecutionStatus::BlockedByAuthorization => "blocked_by_authorization",
        WriteExecutionStatus::Failed => "failed",
        WriteExecutionStatus::Applied => "applied",
    }
}

/// Read-only comparison of two fan curve sysfs snapshots (for example live readings vs last-known-good).
pub fn format_fan_curve_live_vs_saved(live: &FanCurveSnapshot, saved: &FanCurveSnapshot) -> String {
    use std::collections::BTreeMap;
    use std::fmt::Write;

    let mut out = String::new();

    if live.curve_id != saved.curve_id {
        let _ = writeln!(
            out,
            "Note: curve_id differs (live=\"{}\", saved=\"{}\").",
            live.curve_id, saved.curve_id
        );
    }

    match (&live.path, &saved.path) {
        (Some(l_root), Some(s_root)) if l_root != s_root => {
            let _ = writeln!(
                out,
                "Note: hwmon root paths differ (snapshots may come from different probes or machines)."
            );
        }
        _ => {}
    }

    let live_map: BTreeMap<&str, &str> = live
        .points
        .iter()
        .map(|point| (point.path.as_str(), point.value.as_str()))
        .collect();
    let saved_map: BTreeMap<&str, &str> = saved
        .points
        .iter()
        .map(|point| (point.path.as_str(), point.value.as_str()))
        .collect();

    if live_map.is_empty() && saved_map.is_empty() {
        let _ = writeln!(out, "Both snapshots are empty (no sysfs point rows).");
        return out;
    }

    let mut same = 0usize;
    let mut changed: Vec<(&str, &str, &str)> = Vec::new();
    for (path, live_value) in &live_map {
        if let Some(saved_value) = saved_map.get(path) {
            if live_value == saved_value {
                same += 1;
            } else {
                changed.push((path, live_value, saved_value));
            }
        }
    }

    let mut live_only = Vec::new();
    for (path, live_value) in &live_map {
        if !saved_map.contains_key(path) {
            live_only.push((*path, *live_value));
        }
    }

    let mut saved_only = Vec::new();
    for (path, saved_value) in &saved_map {
        if !live_map.contains_key(path) {
            saved_only.push((*path, *saved_value));
        }
    }

    let _ = writeln!(
        out,
        "Summary: {} path(s) with identical values, {} differing, {} only in live, {} only in saved.",
        same,
        changed.len(),
        live_only.len(),
        saved_only.len()
    );

    if !changed.is_empty() {
        let _ = writeln!(out, "\nDiffering values:");
        const MAX_DIFF_LINES: usize = 40;
        for (path, live_value, saved_value) in changed.iter().take(MAX_DIFF_LINES) {
            let _ = writeln!(out, "  {path}");
            let _ = writeln!(out, "    live={live_value}");
            let _ = writeln!(out, "    saved={saved_value}");
        }
        if changed.len() > MAX_DIFF_LINES {
            let _ = writeln!(
                out,
                "... {} more differing path(s) not shown.",
                changed.len() - MAX_DIFF_LINES
            );
        }
    }

    if !live_only.is_empty() {
        let _ = writeln!(out, "\nPaths present only in live snapshot:");
        const MAX_SIDE: usize = 20;
        for (path, live_value) in live_only.iter().take(MAX_SIDE) {
            let _ = writeln!(out, "  {path} = {live_value}");
        }
        if live_only.len() > MAX_SIDE {
            let _ = writeln!(out, "... {} more", live_only.len() - MAX_SIDE);
        }
    }

    if !saved_only.is_empty() {
        let _ = writeln!(out, "\nPaths present only in saved snapshot:");
        const MAX_SIDE: usize = 20;
        for (path, saved_value) in saved_only.iter().take(MAX_SIDE) {
            let _ = writeln!(out, "  {path} = {saved_value}");
        }
        if saved_only.len() > MAX_SIDE {
            let _ = writeln!(out, "... {} more", saved_only.len() - MAX_SIDE);
        }
    }

    out
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct HardwareSummary {
    pub sysfs_root: String,
    pub vendor: Option<String>,
    pub product_name: Option<String>,
    pub product_version: Option<String>,
    pub product_sku: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Capability {
    pub id: String,
    pub label: String,
    pub status: CapabilityStatus,
    pub risk: RiskLevel,
    pub evidence: Vec<String>,
    pub details: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityStatus {
    Detected,
    Missing,
    ProbeOnly,
    Unsupported,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    ReadOnly,
    ReversibleWrite,
    ExperimentalWrite,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlatformProfileCapability {
    pub current: Option<String>,
    pub choices: Vec<String>,
    pub path: String,
    pub choices_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BatteryChargeTypeCapability {
    pub current: Option<String>,
    pub choices: Vec<String>,
    pub path: String,
    pub choices_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HwmonSensor {
    pub hwmon_name: Option<String>,
    pub label: Option<String>,
    pub kind: String,
    pub input_path: String,
    pub value: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BatteryTelemetry {
    pub name: String,
    pub path: String,
    pub capacity_percent: Option<i64>,
    pub status: Option<String>,
    pub health: Option<String>,
    /// Instantaneous power draw in microwatts (µW). Divide by 1_000_000 for watts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power_now_uw: Option<i64>,
    /// Number of full charge/discharge cycles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycle_count: Option<i64>,
    /// Present full-charge energy capacity in microwatt-hours (µWh).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub energy_full_uwh: Option<i64>,
    /// Design (factory) full-charge energy capacity in microwatt-hours (µWh).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub energy_full_design_uwh: Option<i64>,
    /// Current stored energy in microwatt-hours (µWh).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub energy_now_uwh: Option<i64>,
    /// Instantaneous terminal voltage in microvolts (µV).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voltage_now_uv: Option<i64>,
    /// Coarse charge level label (for example `Normal`, `Low`, `Critical`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity_level: Option<String>,
    /// Cell chemistry, for example `Li-poly`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub technology: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
}

/// Read-only AC adapter (mains) connection state from `/sys/class/power_supply`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AcAdapterTelemetry {
    pub name: String,
    pub path: String,
    /// `true` when the adapter reports `online=1`.
    pub online: Option<bool>,
}

/// Read-only ACPI thermal zone reading from `/sys/class/thermal/thermal_zoneN`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThermalZone {
    pub name: String,
    pub zone_type: Option<String>,
    /// Temperature in millidegrees Celsius (m°C). Divide by 1000 for °C.
    pub temp_millicelsius: Option<i64>,
    pub path: String,
}

/// CPU frequency-scaling / amd-pstate parameters under `/sys/devices/system/cpu`.
///
/// Read-only by default. `governor`, `epp`, and `boost` become write targets only once
/// their validators, daemon `--enable-*-write` flag, and rollback path ship.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CpuPowerCapability {
    pub status: CapabilityStatus,
    /// cpufreq driver, for example `amd-pstate-epp`.
    pub scaling_driver: Option<String>,
    /// amd-pstate operating mode (`active`, `guided`, `passive`) from `/sys/devices/system/cpu/amd_pstate/status`.
    pub amd_pstate_status: Option<String>,
    pub governor: Option<String>,
    pub available_governors: Vec<String>,
    /// `energy_performance_preference` (EPP) current value.
    pub epp: Option<String>,
    pub available_epp: Vec<String>,
    /// Core-performance boost state (`/sys/devices/system/cpu/cpufreq/boost`).
    pub boost: Option<bool>,
    pub scaling_min_khz: Option<i64>,
    pub scaling_max_khz: Option<i64>,
    pub scaling_cur_khz: Option<i64>,
    pub cpuinfo_min_khz: Option<i64>,
    pub cpuinfo_max_khz: Option<i64>,
    /// Write paths (per-policy for governor/EPP, global for boost). Empty string when absent.
    pub governor_path: String,
    pub epp_path: String,
    pub boost_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FanCurveCapability {
    pub id: String,
    pub status: CapabilityStatus,
    pub path: Option<String>,
    pub point_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FanPreset {
    pub schema_version: u8,
    pub id: String,
    pub label: String,
    pub description: String,
    pub target_profiles: Vec<String>,
    pub safety_note: String,
    pub points: Vec<FanPresetPoint>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct FanPresetPoint {
    pub temperature_c: i16,
    pub pwm: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LedCapability {
    pub name: String,
    pub path: String,
    pub brightness: Option<i64>,
    pub max_brightness: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyboardRgbCapability {
    pub backend: String,
    pub device_id: String,
    pub path: String,
    pub zones: Vec<KeyboardRgbZone>,
    pub effects: Vec<String>,
    pub current_effect: Option<String>,
    #[serde(default)]
    pub current_colors: BTreeMap<String, String>,
    pub current_brightness: Option<u8>,
    pub min_brightness: u8,
    pub max_brightness: u8,
    pub current_speed: Option<u8>,
    pub min_speed: u8,
    pub max_speed: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyboardRgbCandidate {
    pub backend: String,
    pub device_id: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modalias: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_descriptor_bytes: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub report_ids: Vec<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hid_reports: Vec<KeyboardRgbHidReport>,
    #[serde(default)]
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyboardRgbHidReport {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_id: Option<u8>,
    pub kind: String,
    pub report_size_bits: u32,
    pub report_count: u32,
    pub bit_length: u32,
    pub byte_length: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyboardRgbOpenRgbStatus {
    pub installed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default)]
    pub devices: Vec<KeyboardRgbOpenRgbDevice>,
    #[serde(default)]
    pub i2c_dev_loaded: bool,
    #[serde(default)]
    pub user_in_i2c_group: bool,
    #[serde(default)]
    pub has_i2c_rw_access: bool,
    #[serde(default)]
    pub has_hidraw_rw_access: bool,
    /// True only after RatVantage has a backend with read-back and rollback coverage.
    #[serde(default)]
    pub backend_ready: bool,
    #[serde(default)]
    pub write_support_claimed: bool,
    #[serde(default)]
    pub sdk_helper_installed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sdk_helper_path: Option<String>,
    #[serde(default)]
    pub sdk_server_running: bool,
    #[serde(default)]
    pub sdk_snapshot_supported: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sdk_active_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sdk_color_zones: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sdk_colors: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyboardRgbOpenRgbDevice {
    pub index: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub modes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_mode: Option<String>,
    #[serde(default)]
    pub zones: Vec<String>,
    #[serde(default)]
    pub leds: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyboardRgbZone {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyboardRgbWriteRequest {
    pub effect: String,
    #[serde(default)]
    pub colors: BTreeMap<String, String>,
    pub brightness: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FirmwareAttributeCapability {
    pub name: String,
    pub current_value: Option<String>,
    pub display_name: Option<String>,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribute_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scalar_increment: Option<String>,
}

pub const SUPPORTED_FIRMWARE_SCALAR_ATTRIBUTES: &[&str] =
    &["ppt_pl1_spl", "ppt_pl2_sppt", "ppt_pl3_fppt"];
pub const FIRMWARE_PPT_PRESET_IDS: &[&str] = &[
    "conservative",
    "balanced-custom",
    "performance-custom",
    "reset-defaults",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IdeapadToggleCapability {
    pub name: String,
    pub status: CapabilityStatus,
    pub path: Option<String>,
    pub current_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GpuCapability {
    pub provider: String,
    pub status: CapabilityStatus,
    pub mode: Option<String>,
    #[serde(default)]
    pub switch_type: GpuSwitchType,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub switch_notes: Vec<String>,
}

/// PCI hotswap GPU switching candidate: integrated<->hybrid without display restart.
/// This is evidence-gated; detection alone does not promote runtime switching.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GpuRuntimeCapability {
    pub status: CapabilityStatus,
    pub current_mode: String,
    /// Candidate modes that may be reachable via PCI hotswap.
    #[serde(default)]
    pub candidate_runtime_modes: Vec<String>,
    /// True only after dedicated live review evidence has promoted this path.
    #[serde(default)]
    pub promotion_ready: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GpuSwitchType {
    #[default]
    Unknown,
    RebootRequired,
    SessionRestartRequired,
    RuntimeMux,
}

pub fn format_gpu_switch_type(switch_type: GpuSwitchType) -> &'static str {
    match switch_type {
        GpuSwitchType::Unknown => "unknown",
        GpuSwitchType::RebootRequired => "reboot-required",
        GpuSwitchType::SessionRestartRequired => "session-restart-required",
        GpuSwitchType::RuntimeMux => "runtime-mux",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AmdGpuPowerDpmCapability {
    pub card: String,
    pub status: CapabilityStatus,
    pub vendor: String,
    pub force_performance_level_path: String,
    pub current_force_performance_level: Option<String>,
    pub power_dpm_state: Option<String>,
    pub current_sclk: Option<String>,
    pub current_mclk: Option<String>,
    pub choices: Vec<String>,
}

/// Read-only snapshot of the generic desktop power profile API (power-profiles-daemon or another owner).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PowerProfilesCapability {
    /// Usually `system` on Fedora; older/nonstandard stacks may expose this on `session`.
    pub bus: String,
    pub well_known_name: String,
    pub unique_owner: Option<String>,
    pub active_profile: Option<String>,
    pub status: CapabilityStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TelemetrySnapshot {
    pub sensors: Vec<HwmonSensor>,
    pub battery: Option<BatteryTelemetry>,
    #[serde(default)]
    pub ac_adapters: Vec<AcAdapterTelemetry>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct WriteMethodContract {
    pub method: &'static str,
    pub capability_id: &'static str,
    pub polkit_action: &'static str,
    pub request_type: &'static str,
    pub risk: RiskLevel,
    pub enabled: bool,
    pub reboot_required: bool,
    pub preconditions: &'static [&'static str],
    pub validators: &'static [&'static str],
    pub rollback: &'static [&'static str],
    pub safety_notes: &'static [&'static str],
}

pub const WRITE_METHOD_CONTRACTS: &[WriteMethodContract] = &[
    WriteMethodContract {
        method: "SetPlatformProfile",
        capability_id: "platform_profile",
        polkit_action: "org.ratvantage.LegionControl1.set-platform-profile",
        request_type: r#"{"profile":"string"}"#,
        risk: RiskLevel::ReversibleWrite,
        enabled: false,
        reboot_required: false,
        preconditions: &[
            "platform_profile capability is detected",
            "daemon has read current profile and platform_profile_choices",
        ],
        validators: &[
            "requested profile exactly matches one listed platform_profile_choices value",
            "custom and max-power profiles remain blocked until explicitly supported",
            "post-write read-back matches requested profile",
        ],
        rollback: &[
            "store previous profile before write",
            "restore previous profile if read-back fails and previous value is still listed",
        ],
        safety_notes: &["write method remains disabled; dry-run planning only"],
    },
    WriteMethodContract {
        method: "PrepareCustomThermalMode",
        capability_id: "platform_profile",
        polkit_action: "org.ratvantage.LegionControl1.set-platform-profile",
        request_type: r#"{}"#,
        risk: RiskLevel::ReversibleWrite,
        enabled: false,
        reboot_required: false,
        preconditions: &[
            "platform_profile capability is detected",
            "daemon has read current profile and platform_profile_choices",
            "platform_profile_choices lists custom",
        ],
        validators: &[
            "custom exactly matches one listed platform_profile_choices value",
            "post-write read-back matches custom before dependent PPT or fan plan runs",
        ],
        rollback: &[
            "store previous profile before switching to custom",
            "restore previous profile after dependent PPT or fan operation unless caller explicitly keeps custom",
        ],
        safety_notes: &[
            "plan-only custom thermal prerequisite; use before firmware PPT or fan writes",
            "normal SetPlatformProfile still blocks custom until a live execution bundle proves safety",
        ],
    },
    WriteMethodContract {
        method: "SetBatteryChargeType",
        capability_id: "battery_charge_type",
        polkit_action: "org.ratvantage.LegionControl1.set-battery-charge-type",
        request_type: r#"{"charge_type":"string"}"#,
        risk: RiskLevel::ReversibleWrite,
        enabled: false,
        reboot_required: false,
        preconditions: &[
            "battery_charge_type capability is detected",
            "daemon has read current charge_type and charge_types choices",
        ],
        validators: &[
            "requested charge type exactly matches one listed charge_types value",
            "charge_types and conservation_mode are not controlled in the same request",
            "post-write read-back matches requested charge type",
        ],
        rollback: &[
            "store previous charge type before write",
            "restore previous charge type if read-back fails and previous value is still listed",
        ],
        safety_notes: &["write method remains disabled; dry-run planning only"],
    },
    WriteMethodContract {
        method: "SetLedState",
        capability_id: "leds",
        polkit_action: "org.ratvantage.LegionControl1.set-led-state",
        request_type: r#"{"led_id":"platform::ylogo","enabled":"bool"}"#,
        risk: RiskLevel::ReversibleWrite,
        enabled: false,
        reboot_required: false,
        preconditions: &[
            "target LED capability is detected",
            "daemon has read current brightness and max_brightness for the LED",
        ],
        validators: &[
            "requested led_id exactly matches a probed LED node",
            "only explicitly allowed binary LEDs are writable",
            "post-write read-back matches requested LED state",
        ],
        rollback: &[
            "store previous LED brightness before write",
            "restore previous LED brightness if read-back fails",
        ],
        safety_notes: &["write method remains disabled; dry-run planning only"],
    },
    WriteMethodContract {
        method: "SetKeyboardRgb",
        capability_id: "keyboard_rgb",
        polkit_action: "org.ratvantage.LegionControl1.set-keyboard-rgb",
        request_type: r##"{"effect":"string","colors":{"zone_id":"#RRGGBB"},"brightness":"u8","speed":"u8|null"}"##,
        risk: RiskLevel::ReversibleWrite,
        enabled: false,
        reboot_required: false,
        preconditions: &[
            "keyboard_rgb capability is detected",
            "daemon has read backend identity, zones, effects, current effect, current colors, brightness, and speed",
        ],
        validators: &[
            "requested effect exactly matches a detected effect id",
            "requested zone ids exactly match detected keyboard RGB zones",
            "requested colors are #RRGGBB hex values",
            "requested brightness and speed are inside backend ranges",
            "post-write read-back matches requested RGB state",
        ],
        rollback: &[
            "store previous keyboard RGB effect, colors, brightness, and speed before write",
            "restore previous keyboard RGB state if read-back fails",
        ],
        safety_notes: &[
            "keyboard RGB protocol support is plan-only until backend read-back and live reset evidence exist",
            "write method remains disabled; dry-run planning only",
        ],
    },
    WriteMethodContract {
        method: "SetIdeapadToggle",
        capability_id: "ideapad_toggles",
        polkit_action: "org.ratvantage.LegionControl1.set-ideapad-toggle",
        request_type: r#"{"toggle_id":"fn_lock|camera_power|usb_charging|fan_mode","enabled":"bool"}"#,
        risk: RiskLevel::ReversibleWrite,
        enabled: false,
        reboot_required: false,
        preconditions: &[
            "ideapad toggle capability is detected",
            "daemon has read current toggle state and sysfs path",
        ],
        validators: &[
            "requested toggle exactly matches one detected ideapad toggle id",
            "only fn_lock, camera_power, usb_charging, and fan_mode are enabled for reversible ideapad toggle writes right now",
            "fn_lock requires paired platform::fnlock LED corroboration before and after write",
            "camera_power requires binary current value and explicit UI warning/confirmation before frontend exposure",
            "usb_charging requires binary current value and explicit UI warning/confirmation before frontend exposure",
            "post-write toggle read-back matches requested state",
        ],
        rollback: &[
            "store previous toggle value before write",
            "restore previous toggle value if toggle or corroborating LED read-back fails",
        ],
        safety_notes: &[
            "write method remains disabled; dry-run planning only",
            "current rollout is restricted to fn_lock, camera_power, usb_charging, and fan_mode with per-toggle safety rules",
        ],
    },
    WriteMethodContract {
        method: "SetGpuMode",
        capability_id: "gpu",
        polkit_action: "org.ratvantage.LegionControl1.set-gpu-mode",
        request_type: r#"{"mode":"integrated|hybrid|nvidia"}"#,
        risk: RiskLevel::ExperimentalWrite,
        enabled: false,
        reboot_required: true,
        preconditions: &[
            "gpu capability is detected through EnvyControl",
            "daemon has read the current EnvyControl GPU mode",
        ],
        validators: &[
            "requested mode exactly matches integrated, hybrid, or nvidia",
            "GPU mode changes require reboot-required user messaging",
            "post-reboot read-back matches requested GPU mode",
            "execution remains disabled until rollback and manual validation exist",
        ],
        rollback: &[
            "store previous GPU mode before execution",
            "restore previous GPU mode through EnvyControl and require another reboot if validation fails",
        ],
        safety_notes: &[
            "EnvyControl changes can affect display availability after reboot",
            "write method remains disabled; dry-run planning only",
        ],
    },
    WriteMethodContract {
        method: "ApplyFanPreset",
        capability_id: "fan_curves",
        polkit_action: "org.ratvantage.LegionControl1.apply-fan-preset",
        request_type: r#"{"preset_id":"string"}"#,
        risk: RiskLevel::ExperimentalWrite,
        enabled: false,
        reboot_required: false,
        preconditions: &[
            "fan_curves capability is detected",
            "packaged preset schema is valid",
            "detected fan curve exposes enough auto-point files for the preset",
        ],
        validators: &[
            "requested preset exactly matches a packaged preset id",
            "preset has exactly 10 ascending temperature points",
            "preset PWM values are 0..255 and non-decreasing",
            "post-write read-back matches the complete requested fan curve",
        ],
        rollback: &[
            "store the complete previous fan curve before write",
            "restore the complete previous fan curve if read-back fails",
        ],
        safety_notes: &[
            "fan curve changes affect thermals and acoustics",
            "write method remains disabled; dry-run planning only",
        ],
    },
    WriteMethodContract {
        method: "RestoreAutoFan",
        capability_id: "fan_curves",
        polkit_action: "org.ratvantage.LegionControl1.restore-auto-fan",
        request_type: r#"{}"#,
        risk: RiskLevel::ExperimentalWrite,
        enabled: false,
        reboot_required: false,
        preconditions: &[
            "fan_curves capability is detected",
            "daemon has captured the current fan-control state before restore",
        ],
        validators: &[
            "detected fan curve exposes a restore-capable control path",
            "post-restore telemetry and read-back remain within expected bounds",
        ],
        rollback: &[
            "store current fan-control state before restore",
            "restore previous fan-control state if read-back fails",
        ],
        safety_notes: &[
            "restoring automatic fan control can change thermal behavior immediately",
            "write method remains disabled; dry-run planning only",
        ],
    },
    WriteMethodContract {
        method: "SetCpuGovernor",
        capability_id: "cpu_power",
        polkit_action: "org.ratvantage.LegionControl1.set-cpu-governor",
        request_type: r#"{"governor":"string"}"#,
        risk: RiskLevel::ReversibleWrite,
        enabled: false,
        reboot_required: false,
        preconditions: &[
            "cpu_power capability is detected",
            "daemon has read current governor and available_governors",
            "governor_path is non-empty",
        ],
        validators: &[
            "requested governor exactly matches one listed available_governors value",
            "post-write read-back matches requested governor",
        ],
        rollback: &[
            "store previous governor before write",
            "restore previous governor if read-back fails and previous value is still listed",
        ],
        safety_notes: &["write method remains disabled; dry-run planning only"],
    },
    WriteMethodContract {
        method: "SetCpuEpp",
        capability_id: "cpu_power",
        polkit_action: "org.ratvantage.LegionControl1.set-cpu-epp",
        request_type: r#"{"epp":"string"}"#,
        risk: RiskLevel::ReversibleWrite,
        enabled: false,
        reboot_required: false,
        preconditions: &[
            "cpu_power capability is detected",
            "daemon has read current epp and available_epp",
            "epp_path is non-empty",
            "amd-pstate-epp driver is active",
        ],
        validators: &[
            "requested epp exactly matches one listed available_epp value",
            "post-write read-back matches requested epp",
        ],
        rollback: &[
            "store previous epp before write",
            "restore previous epp if read-back fails and previous value is still listed",
        ],
        safety_notes: &[
            "EPP is only writable when amd-pstate-epp driver is active",
            "write method remains disabled; dry-run planning only",
        ],
    },
    WriteMethodContract {
        method: "SetFirmwareAttribute",
        capability_id: "firmware_attributes",
        polkit_action: "org.ratvantage.LegionControl1.set-firmware-attribute",
        request_type: r#"{"attribute_id":"ppt_pl1_spl|ppt_pl2_sppt|ppt_pl3_fppt","value":"integer"}"#,
        risk: RiskLevel::ReversibleWrite,
        enabled: false,
        reboot_required: false,
        preconditions: &[
            "firmware_attributes capability is detected",
            "target attribute is one of the allowlisted 82WM PPT scalar ids",
            "daemon has read current_value, min_value, max_value, and scalar_increment",
        ],
        validators: &[
            "requested value parses as an integer",
            "requested value is inside min_value..=max_value",
            "requested value aligns with scalar_increment from min_value",
            "post-write read-back matches requested value",
        ],
        rollback: &[
            "store previous current_value before write",
            "restore previous current_value if read-back fails",
        ],
        safety_notes: &[
            "firmware power limits can change sustained CPU package power immediately",
            "write method remains disabled unless daemon firmware attribute write policy is enabled",
        ],
    },
    WriteMethodContract {
        method: "SetCpuBoost",
        capability_id: "cpu_power",
        polkit_action: "org.ratvantage.LegionControl1.set-cpu-boost",
        request_type: r#"{"enabled":"0|1"}"#,
        risk: RiskLevel::ReversibleWrite,
        enabled: false,
        reboot_required: false,
        preconditions: &[
            "cpu_power capability is detected",
            "daemon has read current boost state",
            "boost_path is non-empty",
        ],
        validators: &[
            "requested boost value is exactly 0 or 1",
            "post-write read-back matches requested boost value",
        ],
        rollback: &[
            "store previous boost value before write",
            "restore previous boost value if read-back fails",
        ],
        safety_notes: &["CPU boost changes can affect peak thermals and power draw immediately"],
    },
    WriteMethodContract {
        method: "SetConservationMode",
        capability_id: "ideapad_toggles",
        polkit_action: "org.ratvantage.LegionControl1.set-conservation-mode",
        request_type: r#"{"enabled":"0|1"}"#,
        risk: RiskLevel::ReversibleWrite,
        enabled: false,
        reboot_required: false,
        preconditions: &[
            "ideapad conservation_mode toggle is detected",
            "daemon has read current conservation_mode state",
        ],
        validators: &[
            "requested conservation mode value is exactly 0 or 1",
            "post-write read-back matches requested conservation_mode value",
        ],
        rollback: &[
            "store previous conservation_mode value before write",
            "restore previous conservation_mode value if read-back fails",
        ],
        safety_notes: &[
            "conservation_mode overlaps battery charge behavior and must not be applied in the same request as battery charge type",
        ],
    },
    WriteMethodContract {
        method: "SetAmdGpuDpmForceLevel",
        capability_id: "amd_gpu_power_dpm",
        polkit_action: "org.ratvantage.LegionControl1.set-amd-gpu-dpm-force-level",
        request_type: r#"{"force_level":"auto|low"}"#,
        risk: RiskLevel::ReversibleWrite,
        enabled: false,
        reboot_required: false,
        preconditions: &[
            "amd_gpu_power_dpm capability is detected",
            "daemon has read current power_dpm_force_performance_level",
            "force_performance_level_path is non-empty",
        ],
        validators: &[
            "requested force level exactly matches one listed capability choice",
            "post-write read-back matches requested force level",
        ],
        rollback: &[
            "store previous force level before write",
            "restore previous force level if read-back fails",
        ],
        safety_notes: &[
            "AMD GPU DPM force-level changes can affect graphics power and thermals immediately",
            "manual GPU clock writes remain unsupported",
        ],
    },
    WriteMethodContract {
        method: "SetCurveOptimizerAllCore",
        capability_id: "curve_optimizer_all_core",
        polkit_action: "org.ratvantage.LegionControl1.set-curve-optimizer",
        request_type: r#"{"offset":"0|-1..-30"}"#,
        risk: RiskLevel::ExperimentalWrite,
        enabled: false,
        reboot_required: false,
        preconditions: &[
            "advanced CPU tuning is explicitly enabled",
            "daemon can execute the selected Curve Optimizer backend",
            "RyzenAdj fallback through /dev/mem is treated as write-only unless ryzen_smu read-back exists",
        ],
        validators: &[
            "requested offset parses as a signed integer",
            "requested offset is between -30 and 0 inclusive",
            "requested offset is encoded to the u32 RyzenAdj value before execution",
            "success requires backend exit success and expected success marker",
            "read-back is marked write-only until a ryzen_smu backend is available",
        ],
        rollback: &[
            "automatic rollback is not claimed for write-only Curve Optimizer writes",
            "provide explicit reset-to-zero restore command through the same backend",
        ],
        safety_notes: &[
            "negative Curve Optimizer values can cause crashes, reboots, app instability, or silent performance loss",
            "write method remains disabled unless daemon Curve Optimizer policy is enabled",
            "read-back is unavailable on this machine until a ryzen_smu backend is detected",
        ],
    },
    WriteMethodContract {
        method: "SetGpuModeRuntime",
        capability_id: "gpu_runtime",
        polkit_action: "org.ratvantage.LegionControl1.set-gpu-mode-runtime",
        request_type: r#"{"mode":"integrated|hybrid"}"#,
        risk: RiskLevel::ExperimentalWrite,
        enabled: false,
        reboot_required: false,
        preconditions: &[
            "gpu_runtime capability is detected as a candidate",
            "requested mode is in candidate_runtime_modes (integrated or hybrid only)",
            "review-gpu-mux-evidence --require-session-restart-confirmed or a stricter runtime evidence gate has promoted this path",
            "current mode differs from requested mode",
        ],
        validators: &[
            "requested mode exactly matches integrated or hybrid",
            "gpu_runtime.status is ProbeOnly",
            "gpu_runtime.promotion_ready is true",
            "promotion evidence must prove post-change read-back matches the requested mode",
        ],
        rollback: &[
            "execution is not implemented; rollback is a promotion requirement, not a current claim",
            "future promotion must prove previous-mode restore through live read-back evidence",
        ],
        safety_notes: &[
            "nvidia mode is not a runtime-safe target; it requires display manager restart",
            "execution is not implemented in the daemon; this is a plan-only evidence gate",
        ],
    },
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum ValidationError {
    MissingCapability {
        capability_id: String,
    },
    MissingCurrentValue {
        capability_id: String,
    },
    NoChoicesDetected {
        capability_id: String,
    },
    EmptyValue {
        field: String,
    },
    UnsupportedChoice {
        capability_id: String,
        requested: String,
        choices: Vec<String>,
    },
    BlockedChoice {
        capability_id: String,
        requested: String,
        reason: String,
    },
}

pub fn validate_platform_profile_choice(
    capability: Option<&PlatformProfileCapability>,
    requested: &str,
) -> Result<(), ValidationError> {
    let capability = capability.ok_or_else(|| ValidationError::MissingCapability {
        capability_id: "platform_profile".to_owned(),
    })?;
    require_current("platform_profile", capability.current.as_deref())?;
    validate_choice(
        "platform_profile",
        "profile",
        requested,
        &capability.choices,
        &[
            (
                "custom",
                "custom profile needs firmware attribute validators",
            ),
            ("max-power", "max-power needs explicit high-risk policy"),
            ("extreme", "extreme profile needs explicit high-risk policy"),
        ],
    )
}

pub fn validate_custom_thermal_mode_prepare(
    capability: Option<&PlatformProfileCapability>,
) -> Result<(), ValidationError> {
    let capability = capability.ok_or_else(|| ValidationError::MissingCapability {
        capability_id: "platform_profile".to_owned(),
    })?;
    require_current("platform_profile", capability.current.as_deref())?;
    if capability.choices.is_empty() {
        return Err(ValidationError::NoChoicesDetected {
            capability_id: "platform_profile".to_owned(),
        });
    }
    if !capability.choices.iter().any(|choice| choice == "custom") {
        return Err(ValidationError::UnsupportedChoice {
            capability_id: "platform_profile".to_owned(),
            requested: "custom".to_owned(),
            choices: capability.choices.clone(),
        });
    }
    Ok(())
}

pub fn validate_battery_charge_type_choice(
    capability: Option<&BatteryChargeTypeCapability>,
    requested: &str,
) -> Result<(), ValidationError> {
    let capability = capability.ok_or_else(|| ValidationError::MissingCapability {
        capability_id: "battery_charge_type".to_owned(),
    })?;
    require_current("battery_charge_type", capability.current.as_deref())?;
    validate_choice(
        "battery_charge_type",
        "charge_type",
        requested,
        &capability.choices,
        &[],
    )
}

pub fn validate_cpu_governor_choice(
    capability: Option<&CpuPowerCapability>,
    requested: &str,
) -> Result<(), ValidationError> {
    let capability = capability.ok_or_else(|| ValidationError::MissingCapability {
        capability_id: "cpu_power".to_owned(),
    })?;
    require_current("cpu_power", capability.governor.as_deref())?;
    if capability.governor_path.is_empty() {
        return Err(ValidationError::MissingCapability {
            capability_id: "cpu_power:governor_path".to_owned(),
        });
    }
    validate_choice(
        "cpu_power",
        "governor",
        requested,
        &capability.available_governors,
        &[],
    )
}

pub fn validate_cpu_epp_choice(
    capability: Option<&CpuPowerCapability>,
    requested: &str,
) -> Result<(), ValidationError> {
    let capability = capability.ok_or_else(|| ValidationError::MissingCapability {
        capability_id: "cpu_power".to_owned(),
    })?;
    require_current("cpu_power", capability.epp.as_deref())?;
    if capability.epp_path.is_empty() {
        return Err(ValidationError::MissingCapability {
            capability_id: "cpu_power:epp_path".to_owned(),
        });
    }
    validate_choice(
        "cpu_power",
        "epp",
        requested,
        &capability.available_epp,
        &[],
    )
}

pub fn validate_cpu_boost_request(
    capability: Option<&CpuPowerCapability>,
    requested: &str,
) -> Result<(), ValidationError> {
    let capability = capability.ok_or_else(|| ValidationError::MissingCapability {
        capability_id: "cpu_power".to_owned(),
    })?;
    if requested != "0" && requested != "1" {
        return Err(ValidationError::UnsupportedChoice {
            capability_id: "cpu_power:boost".to_owned(),
            requested: requested.to_owned(),
            choices: vec!["0".to_owned(), "1".to_owned()],
        });
    }
    if capability.boost_path.is_empty() {
        return Err(ValidationError::MissingCapability {
            capability_id: "cpu_power:boost_path".to_owned(),
        });
    }
    if capability.boost.is_none() {
        return Err(ValidationError::MissingCurrentValue {
            capability_id: "cpu_power:boost".to_owned(),
        });
    }
    Ok(())
}

pub fn encode_curve_optimizer_offset(offset: i32) -> Result<u32, ValidationError> {
    if !(-30..=0).contains(&offset) {
        return Err(ValidationError::UnsupportedChoice {
            capability_id: "curve_optimizer_all_core".to_owned(),
            requested: offset.to_string(),
            choices: curve_optimizer_offset_choices(),
        });
    }
    Ok(if offset == 0 {
        0
    } else {
        (u32::MAX as i64 + 1 + offset as i64) as u32
    })
}

pub fn validate_curve_optimizer_all_core_offset(requested: &str) -> Result<i32, ValidationError> {
    let offset = requested
        .parse::<i32>()
        .map_err(|_| ValidationError::BlockedChoice {
            capability_id: "curve_optimizer_all_core".to_owned(),
            requested: requested.to_owned(),
            reason: "Curve Optimizer offset must be an integer from -30 through 0".to_owned(),
        })?;
    encode_curve_optimizer_offset(offset)?;
    Ok(offset)
}

pub fn validate_conservation_mode_request(
    toggles: &[IdeapadToggleCapability],
    requested: &str,
) -> Result<(), ValidationError> {
    if requested != "0" && requested != "1" {
        return Err(ValidationError::UnsupportedChoice {
            capability_id: "ideapad_toggles:conservation_mode".to_owned(),
            requested: requested.to_owned(),
            choices: vec!["0".to_owned(), "1".to_owned()],
        });
    }
    let toggle = toggles
        .iter()
        .find(|toggle| toggle.name == "conservation_mode")
        .ok_or_else(|| ValidationError::MissingCapability {
            capability_id: "ideapad_toggles:conservation_mode".to_owned(),
        })?;
    if toggle.path.as_deref().unwrap_or_default().is_empty() {
        return Err(ValidationError::MissingCapability {
            capability_id: "ideapad_toggles:conservation_mode:path".to_owned(),
        });
    }
    require_current(
        "ideapad_toggles:conservation_mode",
        toggle.current_value.as_deref(),
    )?;
    let current = toggle.current_value.as_deref().unwrap_or_default();
    if current != "0" && current != "1" {
        return Err(ValidationError::BlockedChoice {
            capability_id: "ideapad_toggles".to_owned(),
            requested: "conservation_mode".to_owned(),
            reason: format!(
                "only binary conservation_mode states are supported; detected current_value={current}"
            ),
        });
    }
    Ok(())
}

pub fn validate_amd_gpu_dpm_force_level_choice(
    capability: Option<&AmdGpuPowerDpmCapability>,
    requested: &str,
) -> Result<(), ValidationError> {
    let capability = capability.ok_or_else(|| ValidationError::MissingCapability {
        capability_id: "amd_gpu_power_dpm".to_owned(),
    })?;
    require_current(
        "amd_gpu_power_dpm",
        capability.current_force_performance_level.as_deref(),
    )?;
    if capability.force_performance_level_path.trim().is_empty() {
        return Err(ValidationError::MissingCapability {
            capability_id: "amd_gpu_power_dpm:force_performance_level_path".to_owned(),
        });
    }
    validate_choice(
        "amd_gpu_power_dpm",
        "force_level",
        requested,
        &capability.choices,
        &[],
    )
}

pub fn validate_firmware_scalar_attribute_request(
    attributes: &[FirmwareAttributeCapability],
    attribute_id: &str,
    requested: &str,
) -> Result<(), ValidationError> {
    if attribute_id.trim().is_empty() {
        return Err(ValidationError::EmptyValue {
            field: "attribute_id".to_owned(),
        });
    }
    if requested.trim().is_empty() {
        return Err(ValidationError::EmptyValue {
            field: "value".to_owned(),
        });
    }
    if !SUPPORTED_FIRMWARE_SCALAR_ATTRIBUTES
        .iter()
        .any(|supported| supported == &attribute_id)
    {
        return Err(ValidationError::BlockedChoice {
            capability_id: "firmware_attributes".to_owned(),
            requested: attribute_id.to_owned(),
            reason: "only 82WM PPT scalar firmware attributes are enabled for writes right now"
                .to_owned(),
        });
    }

    let attribute = attributes
        .iter()
        .find(|attribute| attribute.name == attribute_id)
        .ok_or_else(|| ValidationError::MissingCapability {
            capability_id: format!("firmware_attributes:{attribute_id}"),
        })?;
    require_current(
        &format!("firmware_attributes:{attribute_id}"),
        attribute.current_value.as_deref(),
    )?;

    let requested_value = requested
        .parse::<i64>()
        .map_err(|_| ValidationError::BlockedChoice {
            capability_id: "firmware_attributes".to_owned(),
            requested: requested.to_owned(),
            reason: "requested firmware attribute value must be an integer".to_owned(),
        })?;
    let min_value =
        parse_firmware_integer(attribute_id, "min_value", attribute.min_value.as_deref())?;
    let max_value =
        parse_firmware_integer(attribute_id, "max_value", attribute.max_value.as_deref())?;
    let step = parse_firmware_integer(
        attribute_id,
        "scalar_increment",
        attribute.scalar_increment.as_deref(),
    )?;

    if step <= 0 {
        return Err(ValidationError::BlockedChoice {
            capability_id: "firmware_attributes".to_owned(),
            requested: requested.to_owned(),
            reason: format!("scalar_increment must be positive; detected {step}"),
        });
    }
    if requested_value < min_value || requested_value > max_value {
        return Err(ValidationError::UnsupportedChoice {
            capability_id: format!("firmware_attributes:{attribute_id}"),
            requested: requested.to_owned(),
            choices: firmware_integer_choices(min_value, max_value, step),
        });
    }
    if (requested_value - min_value) % step != 0 {
        return Err(ValidationError::BlockedChoice {
            capability_id: "firmware_attributes".to_owned(),
            requested: requested.to_owned(),
            reason: format!("requested value must align to step {step} from minimum {min_value}"),
        });
    }
    Ok(())
}

pub fn validate_gpu_mode_choice(
    capability: Option<&GpuCapability>,
    requested: &str,
) -> Result<(), ValidationError> {
    let capability = capability.ok_or_else(|| ValidationError::MissingCapability {
        capability_id: "gpu".to_owned(),
    })?;
    if capability.provider != "envycontrol" {
        return Err(ValidationError::BlockedChoice {
            capability_id: "gpu".to_owned(),
            requested: requested.to_owned(),
            reason: "GPU mode planning is only supported for EnvyControl".to_owned(),
        });
    }
    if capability.status != CapabilityStatus::ProbeOnly {
        return Err(ValidationError::MissingCapability {
            capability_id: "gpu".to_owned(),
        });
    }
    if !matches!(
        capability.switch_type,
        GpuSwitchType::Unknown | GpuSwitchType::RebootRequired
    ) {
        return Err(ValidationError::BlockedChoice {
            capability_id: "gpu:switch_type".to_owned(),
            requested: format_gpu_switch_type(capability.switch_type).to_owned(),
            reason: "session-restart and runtime-mux GPU switching are research-only until a dedicated backend and live display recovery evidence exist".to_owned(),
        });
    }
    require_current("gpu", capability.mode.as_deref())?;
    validate_choice(
        "gpu",
        "mode",
        requested,
        &[
            "integrated".to_owned(),
            "hybrid".to_owned(),
            "nvidia".to_owned(),
        ],
        &[],
    )
}

pub fn validate_led_state_request(
    leds: &[LedCapability],
    led_id: &str,
) -> Result<(), ValidationError> {
    if led_id.is_empty() {
        return Err(ValidationError::EmptyValue {
            field: "led_id".to_owned(),
        });
    }

    let Some(led) = leds.iter().find(|led| led.name == led_id) else {
        return Err(ValidationError::MissingCapability {
            capability_id: format!("leds:{led_id}"),
        });
    };

    if led.name == "platform::fnlock" {
        return Err(ValidationError::BlockedChoice {
            capability_id: "leds".to_owned(),
            requested: led_id.to_owned(),
            reason: "platform::fnlock remains indicator-only until functional fn_lock writes exist"
                .to_owned(),
        });
    }

    if led.name != "platform::ylogo" {
        return Err(ValidationError::BlockedChoice {
            capability_id: "leds".to_owned(),
            requested: led_id.to_owned(),
            reason: "only platform::ylogo is enabled for reversible LED writes right now"
                .to_owned(),
        });
    }

    match led.max_brightness {
        Some(1) => {}
        Some(max_brightness) => {
            return Err(ValidationError::BlockedChoice {
                capability_id: "leds".to_owned(),
                requested: led_id.to_owned(),
                reason: format!(
                    "only binary LED nodes are supported; detected max_brightness={max_brightness}"
                ),
            })
        }
        None => {
            return Err(ValidationError::BlockedChoice {
                capability_id: "leds".to_owned(),
                requested: led_id.to_owned(),
                reason: "LED max_brightness is required before enabling writes".to_owned(),
            })
        }
    }

    match led.brightness {
        Some(0 | 1) => Ok(()),
        Some(value) => Err(ValidationError::BlockedChoice {
            capability_id: "leds".to_owned(),
            requested: led_id.to_owned(),
            reason: format!("only binary LED states are supported; detected brightness={value}"),
        }),
        None => Err(ValidationError::MissingCurrentValue {
            capability_id: format!("leds:{led_id}"),
        }),
    }
}

pub fn validate_keyboard_rgb_request(
    capability: Option<&KeyboardRgbCapability>,
    request: &KeyboardRgbWriteRequest,
) -> Result<(), ValidationError> {
    let capability = capability.ok_or_else(|| ValidationError::MissingCapability {
        capability_id: "keyboard_rgb".to_owned(),
    })?;

    if capability.backend.trim().is_empty() {
        return Err(ValidationError::MissingCapability {
            capability_id: "keyboard_rgb:backend".to_owned(),
        });
    }
    if capability.device_id.trim().is_empty() {
        return Err(ValidationError::MissingCapability {
            capability_id: "keyboard_rgb:device_id".to_owned(),
        });
    }
    if capability.path.trim().is_empty() {
        return Err(ValidationError::MissingCapability {
            capability_id: "keyboard_rgb:path".to_owned(),
        });
    }
    require_current("keyboard_rgb:effect", capability.current_effect.as_deref())?;
    if capability.current_brightness.is_none() {
        return Err(ValidationError::MissingCurrentValue {
            capability_id: "keyboard_rgb:brightness".to_owned(),
        });
    }
    if capability.min_brightness > capability.max_brightness {
        return Err(ValidationError::BlockedChoice {
            capability_id: "keyboard_rgb".to_owned(),
            requested: request.brightness.to_string(),
            reason: format!(
                "invalid brightness range {}..={}",
                capability.min_brightness, capability.max_brightness
            ),
        });
    }
    if capability.min_speed > capability.max_speed {
        return Err(ValidationError::BlockedChoice {
            capability_id: "keyboard_rgb".to_owned(),
            requested: request.speed.unwrap_or_default().to_string(),
            reason: format!(
                "invalid speed range {}..={}",
                capability.min_speed, capability.max_speed
            ),
        });
    }

    validate_choice(
        "keyboard_rgb",
        "effect",
        &request.effect,
        &capability.effects,
        &[],
    )?;
    if request.brightness < capability.min_brightness
        || request.brightness > capability.max_brightness
    {
        return Err(ValidationError::UnsupportedChoice {
            capability_id: "keyboard_rgb:brightness".to_owned(),
            requested: request.brightness.to_string(),
            choices: u8_range_choices(capability.min_brightness, capability.max_brightness),
        });
    }
    if let Some(speed) = request.speed {
        if speed < capability.min_speed || speed > capability.max_speed {
            return Err(ValidationError::UnsupportedChoice {
                capability_id: "keyboard_rgb:speed".to_owned(),
                requested: speed.to_string(),
                choices: u8_range_choices(capability.min_speed, capability.max_speed),
            });
        }
    }

    let supported_zones = capability
        .zones
        .iter()
        .map(|zone| zone.id.as_str())
        .collect::<Vec<_>>();
    if supported_zones.is_empty() {
        return Err(ValidationError::NoChoicesDetected {
            capability_id: "keyboard_rgb:zones".to_owned(),
        });
    }
    if request.colors.is_empty() {
        return Err(ValidationError::EmptyValue {
            field: "colors".to_owned(),
        });
    }
    for (zone_id, color) in &request.colors {
        if !supported_zones.iter().any(|supported| supported == zone_id) {
            return Err(ValidationError::UnsupportedChoice {
                capability_id: "keyboard_rgb:zone".to_owned(),
                requested: zone_id.clone(),
                choices: supported_zones
                    .iter()
                    .map(|zone| (*zone).to_owned())
                    .collect(),
            });
        }
        if !is_hex_rgb_color(color) {
            return Err(ValidationError::BlockedChoice {
                capability_id: "keyboard_rgb:color".to_owned(),
                requested: color.clone(),
                reason: "keyboard RGB colors must use #RRGGBB hex".to_owned(),
            });
        }
    }
    for zone in &capability.zones {
        if !capability.current_colors.contains_key(&zone.id) {
            return Err(ValidationError::MissingCurrentValue {
                capability_id: format!("keyboard_rgb:zone:{}", zone.id),
            });
        }
    }

    Ok(())
}

pub fn validate_ideapad_toggle_request(
    toggles: &[IdeapadToggleCapability],
    leds: &[LedCapability],
    toggle_id: &str,
) -> Result<(), ValidationError> {
    if toggle_id.is_empty() {
        return Err(ValidationError::EmptyValue {
            field: "toggle_id".to_owned(),
        });
    }

    let Some(toggle) = toggles.iter().find(|toggle| toggle.name == toggle_id) else {
        return Err(ValidationError::MissingCapability {
            capability_id: format!("ideapad_toggles:{toggle_id}"),
        });
    };

    if toggle.name != "fn_lock"
        && toggle.name != "camera_power"
        && toggle.name != "usb_charging"
        && toggle.name != "fan_mode"
    {
        return Err(ValidationError::BlockedChoice {
            capability_id: "ideapad_toggles".to_owned(),
            requested: toggle_id.to_owned(),
            reason:
                "only fn_lock, camera_power, usb_charging, and fan_mode are enabled for reversible ideapad toggle writes right now; touchpad remains blocked until dedicated fixture coverage and recovery validation exist"
                    .to_owned(),
        });
    }

    let Some(path) = toggle.path.as_deref() else {
        return Err(ValidationError::MissingCapability {
            capability_id: format!("ideapad_toggles:{toggle_id}:path"),
        });
    };
    if path.trim().is_empty() {
        return Err(ValidationError::MissingCapability {
            capability_id: format!("ideapad_toggles:{toggle_id}:path"),
        });
    }

    require_current(
        &format!("ideapad_toggles:{toggle_id}"),
        toggle.current_value.as_deref(),
    )?;
    let current = toggle
        .current_value
        .as_deref()
        .expect("validated ideapad toggle current value must exist");
    if current != "0" && current != "1" {
        return Err(ValidationError::BlockedChoice {
            capability_id: "ideapad_toggles".to_owned(),
            requested: toggle_id.to_owned(),
            reason: format!(
                "only binary ideapad toggle states are supported; detected current_value={current}"
            ),
        });
    }

    if toggle.name == "camera_power" || toggle.name == "usb_charging" || toggle.name == "fan_mode" {
        return Ok(());
    }

    let Some(indicator) = leds.iter().find(|led| led.name == "platform::fnlock") else {
        return Err(ValidationError::MissingCapability {
            capability_id: "leds:platform::fnlock".to_owned(),
        });
    };

    match indicator.max_brightness {
        Some(1) => {}
        Some(max_brightness) => {
            return Err(ValidationError::BlockedChoice {
                capability_id: "leds".to_owned(),
                requested: "platform::fnlock".to_owned(),
                reason: format!(
                    "paired fn_lock indicator LED must be binary; detected max_brightness={max_brightness}"
                ),
            })
        }
        None => {
            return Err(ValidationError::MissingCurrentValue {
                capability_id: "leds:platform::fnlock:max_brightness".to_owned(),
            })
        }
    }

    let Some(led_brightness) = indicator.brightness else {
        return Err(ValidationError::MissingCurrentValue {
            capability_id: "leds:platform::fnlock".to_owned(),
        });
    };
    if led_brightness != 0 && led_brightness != 1 {
        return Err(ValidationError::BlockedChoice {
            capability_id: "leds".to_owned(),
            requested: "platform::fnlock".to_owned(),
            reason: format!(
                "paired fn_lock indicator LED must be binary; detected brightness={led_brightness}"
            ),
        });
    }

    if current != led_brightness.to_string() {
        return Err(ValidationError::BlockedChoice {
            capability_id: "ideapad_toggles".to_owned(),
            requested: toggle_id.to_owned(),
            reason: format!(
                "fn_lock toggle state `{current}` does not match paired platform::fnlock LED `{led_brightness}`"
            ),
        });
    }

    Ok(())
}

pub fn validate_fan_preset_choice(
    fan_curves: &[FanCurveCapability],
    presets: &[FanPreset],
    requested: &str,
) -> Result<(), ValidationError> {
    if requested.is_empty() {
        return Err(ValidationError::EmptyValue {
            field: "preset_id".to_owned(),
        });
    }

    let preset =
        find_fan_preset(presets, requested).ok_or_else(|| ValidationError::UnsupportedChoice {
            capability_id: "fan_preset".to_owned(),
            requested: requested.to_owned(),
            choices: presets.iter().map(|preset| preset.id.clone()).collect(),
        })?;
    validate_fan_preset_schema(preset)?;

    let curve = select_fan_curve(fan_curves).ok_or_else(|| ValidationError::MissingCapability {
        capability_id: "fan_curves".to_owned(),
    })?;
    validate_fan_curve_supports_preset(curve, preset)
}

/// Validates storing a packaged fan preset id for a detected platform profile choice (daemon app state only).
pub fn validate_fan_preset_platform_profile_entry(
    platform_profile_cap: Option<&PlatformProfileCapability>,
    fan_curves: &[FanCurveCapability],
    presets: &[FanPreset],
    platform_profile: &str,
    fan_preset_id: &str,
) -> Result<(), ValidationError> {
    if platform_profile.trim().is_empty() {
        return Err(ValidationError::EmptyValue {
            field: "platform_profile".to_owned(),
        });
    }
    if fan_preset_id.trim().is_empty() {
        return Err(ValidationError::EmptyValue {
            field: "fan_preset_id".to_owned(),
        });
    }

    let capability = platform_profile_cap.ok_or_else(|| ValidationError::MissingCapability {
        capability_id: "platform_profile".to_owned(),
    })?;
    validate_choice(
        "platform_profile",
        "profile",
        platform_profile,
        &capability.choices,
        &[],
    )?;
    validate_fan_preset_choice(fan_curves, presets, fan_preset_id)?;
    Ok(())
}

pub fn validate_hardware_profile_id(profile_id: &str) -> Result<(), ValidationError> {
    if profile_id.trim().is_empty() {
        return Err(ValidationError::EmptyValue {
            field: "profile_id".to_owned(),
        });
    }
    if profile_id
        .chars()
        .any(|ch| !(ch.is_ascii_alphanumeric() || ch == '-' || ch == '_'))
    {
        return Err(ValidationError::BlockedChoice {
            capability_id: "hardware_profiles".to_owned(),
            requested: profile_id.to_owned(),
            reason: "profile_id may contain only ASCII letters, digits, '-' and '_'".to_owned(),
        });
    }
    Ok(())
}

pub fn validate_hardware_profile_trigger_id(trigger_id: &str) -> Result<(), ValidationError> {
    if HARDWARE_PROFILE_TRIGGER_IDS
        .iter()
        .any(|supported| supported == &trigger_id)
    {
        return Ok(());
    }
    Err(ValidationError::BlockedChoice {
        capability_id: "hardware_profile_triggers".to_owned(),
        requested: trigger_id.to_owned(),
        reason: format!(
            "supported trigger ids are: {}",
            HARDWARE_PROFILE_TRIGGER_IDS.join(", ")
        ),
    })
}

pub fn validate_automation_rule_id(rule_id: &str) -> Result<(), ValidationError> {
    if rule_id.trim().is_empty() {
        return Err(ValidationError::EmptyValue {
            field: "rule_id".to_owned(),
        });
    }
    if rule_id
        .chars()
        .any(|ch| !(ch.is_ascii_alphanumeric() || ch == '-' || ch == '_'))
    {
        return Err(ValidationError::BlockedChoice {
            capability_id: "automation_rules".to_owned(),
            requested: rule_id.to_owned(),
            reason: "rule_id may contain only ASCII letters, digits, '-' and '_'".to_owned(),
        });
    }
    Ok(())
}

pub fn validate_automation_rule(rule: &AutomationRule) -> Result<(), ValidationError> {
    if rule.schema_version != 1 {
        return Err(ValidationError::BlockedChoice {
            capability_id: "automation_rules".to_owned(),
            requested: rule.schema_version.to_string(),
            reason: "only automation rule schema_version 1 is supported".to_owned(),
        });
    }
    if rule.label.trim().is_empty() {
        return Err(ValidationError::EmptyValue {
            field: "label".to_owned(),
        });
    }

    match &rule.kind {
        AutomationRuleKind::FastChargeUntilThreshold {
            threshold_percent,
            fast_charge_profile_id,
            protect_profile_id,
            cooldown_secs,
            ..
        } => {
            if !(1..=100).contains(threshold_percent) {
                return Err(ValidationError::BlockedChoice {
                    capability_id: "automation_rules:threshold_percent".to_owned(),
                    requested: threshold_percent.to_string(),
                    reason: "threshold_percent must be 1..=100".to_owned(),
                });
            }
            validate_hardware_profile_id(fast_charge_profile_id)?;
            validate_hardware_profile_id(protect_profile_id)?;
            if fast_charge_profile_id == protect_profile_id {
                return Err(ValidationError::BlockedChoice {
                    capability_id: "automation_rules".to_owned(),
                    requested: fast_charge_profile_id.clone(),
                    reason: "fast_charge_profile_id and protect_profile_id must be different"
                        .to_owned(),
                });
            }
            if *cooldown_secs > 86_400 {
                return Err(ValidationError::BlockedChoice {
                    capability_id: "automation_rules:cooldown_secs".to_owned(),
                    requested: cooldown_secs.to_string(),
                    reason: "cooldown_secs must be 0..=86400".to_owned(),
                });
            }
        }
        AutomationRuleKind::AcProfileRouter {
            ac_profile_id,
            battery_profile_id,
            cooldown_secs,
        } => {
            validate_hardware_profile_id(ac_profile_id)?;
            validate_hardware_profile_id(battery_profile_id)?;
            if ac_profile_id == battery_profile_id {
                return Err(ValidationError::BlockedChoice {
                    capability_id: "automation_rules".to_owned(),
                    requested: ac_profile_id.clone(),
                    reason: "ac_profile_id and battery_profile_id must be different".to_owned(),
                });
            }
            if *cooldown_secs > 86_400 {
                return Err(ValidationError::BlockedChoice {
                    capability_id: "automation_rules:cooldown_secs".to_owned(),
                    requested: cooldown_secs.to_string(),
                    reason: "cooldown_secs must be 0..=86400".to_owned(),
                });
            }
        }
        AutomationRuleKind::BatteryProfileThreshold {
            threshold_percent,
            profile_id,
            cooldown_secs,
            ..
        } => {
            if !(1..=100).contains(threshold_percent) {
                return Err(ValidationError::BlockedChoice {
                    capability_id: "automation_rules:threshold_percent".to_owned(),
                    requested: threshold_percent.to_string(),
                    reason: "threshold_percent must be 1..=100".to_owned(),
                });
            }
            validate_hardware_profile_id(profile_id)?;
            if *cooldown_secs > 86_400 {
                return Err(ValidationError::BlockedChoice {
                    capability_id: "automation_rules:cooldown_secs".to_owned(),
                    requested: cooldown_secs.to_string(),
                    reason: "cooldown_secs must be 0..=86400".to_owned(),
                });
            }
        }
        AutomationRuleKind::PeriodicIdle {
            profile_id,
            cooldown_secs,
        } => {
            validate_hardware_profile_id(profile_id)?;
            if *cooldown_secs > 86_400 {
                return Err(ValidationError::BlockedChoice {
                    capability_id: "automation_rules:cooldown_secs".to_owned(),
                    requested: cooldown_secs.to_string(),
                    reason: "cooldown_secs must be 0..=86400".to_owned(),
                });
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WriteDryRunPlan {
    pub method: String,
    pub capability_id: String,
    pub polkit_action: String,
    pub path: String,
    pub previous_value: String,
    pub requested_value: String,
    pub readback_required: bool,
    pub rollback_value: String,
    pub rollback_instructions: Vec<String>,
    pub reboot_required: bool,
    pub safety_notes: Vec<String>,
    pub steps: Vec<WritePlanStep>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WritePlanStep {
    AuthorizeCaller,
    StorePreviousValue,
    WriteRequestedValue,
    ReadBackValue,
    RestorePreviousOnReadbackFailure,
    RequireReboot,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WriteExecutionStatus {
    BlockedByPolicy,
    BlockedByAuthorization,
    Failed,
    Applied,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WriteExecutionResult {
    pub status: WriteExecutionStatus,
    pub applied: bool,
    pub message: String,
    pub readback_value: Option<String>,
    pub plan: WriteDryRunPlan,
}

impl WriteExecutionResult {
    pub fn blocked_by_policy(plan: WriteDryRunPlan, message: impl Into<String>) -> Self {
        Self {
            status: WriteExecutionStatus::BlockedByPolicy,
            applied: false,
            message: message.into(),
            readback_value: None,
            plan,
        }
    }

    pub fn blocked_by_authorization(plan: WriteDryRunPlan, message: impl Into<String>) -> Self {
        Self {
            status: WriteExecutionStatus::BlockedByAuthorization,
            applied: false,
            message: message.into(),
            readback_value: None,
            plan,
        }
    }

    pub fn failed(
        plan: WriteDryRunPlan,
        message: impl Into<String>,
        readback_value: Option<String>,
    ) -> Self {
        Self {
            status: WriteExecutionStatus::Failed,
            applied: false,
            message: message.into(),
            readback_value,
            plan,
        }
    }

    pub fn applied(
        plan: WriteDryRunPlan,
        message: impl Into<String>,
        readback_value: Option<String>,
    ) -> Self {
        Self {
            status: WriteExecutionStatus::Applied,
            applied: true,
            message: message.into(),
            readback_value,
            plan,
        }
    }
}

pub fn plan_platform_profile_write(
    capability: Option<&PlatformProfileCapability>,
    requested: &str,
) -> Result<WriteDryRunPlan, ValidationError> {
    validate_platform_profile_choice(capability, requested)?;
    let capability = capability.expect("validated platform profile capability must exist");
    plan_write(
        write_contract("SetPlatformProfile"),
        &capability.path,
        capability
            .current
            .as_deref()
            .expect("validated platform profile current value must exist"),
        requested,
    )
}

pub fn plan_prepare_custom_thermal_mode(
    capability: Option<&PlatformProfileCapability>,
) -> Result<WriteDryRunPlan, ValidationError> {
    validate_custom_thermal_mode_prepare(capability)?;
    let capability = capability.expect("validated platform profile capability must exist");
    let current = capability
        .current
        .as_deref()
        .expect("validated platform profile current value must exist");
    let mut plan = plan_write(
        write_contract("PrepareCustomThermalMode"),
        &capability.path,
        current,
        "custom",
    )?;
    if current == "custom" {
        plan.safety_notes.push(
            "custom thermal mode is already active; caller can verify read-back and skip the profile write"
                .to_owned(),
        );
    } else {
        plan.safety_notes.push(format!(
            "custom thermal mode preparation required: switch platform_profile from `{current}` to `custom`, read back `custom`, then run dependent PPT or fan plan"
        ));
    }
    Ok(plan)
}

pub fn plan_battery_charge_type_write(
    capability: Option<&BatteryChargeTypeCapability>,
    requested: &str,
) -> Result<WriteDryRunPlan, ValidationError> {
    validate_battery_charge_type_choice(capability, requested)?;
    let capability = capability.expect("validated battery charge type capability must exist");
    plan_write(
        write_contract("SetBatteryChargeType"),
        &capability.path,
        capability
            .current
            .as_deref()
            .expect("validated battery charge type current value must exist"),
        requested,
    )
}

pub fn plan_gpu_mode_write(
    capability: Option<&GpuCapability>,
    requested: &str,
) -> Result<WriteDryRunPlan, ValidationError> {
    validate_gpu_mode_choice(capability, requested)?;
    let capability = capability.expect("validated GPU capability must exist");
    let mut plan = plan_write(
        write_contract("SetGpuMode"),
        "envycontrol",
        capability
            .mode
            .as_deref()
            .expect("validated GPU mode current value must exist"),
        requested,
    )?;
    plan.safety_notes.push(format!(
        "GPU switch type is {}; current promoted backend is EnvyControl and remains reboot-required",
        format_gpu_switch_type(capability.switch_type)
    ));
    for note in &capability.switch_notes {
        plan.safety_notes
            .push(format!("GPU switch evidence: {note}"));
    }
    Ok(plan)
}

pub fn validate_gpu_mode_runtime_choice(
    capability: Option<&GpuRuntimeCapability>,
    requested: &str,
) -> Result<(), ValidationError> {
    let cap = capability.ok_or_else(|| ValidationError::MissingCapability {
        capability_id: "gpu_runtime".to_owned(),
    })?;
    if cap.status != CapabilityStatus::ProbeOnly {
        return Err(ValidationError::MissingCapability {
            capability_id: "gpu_runtime".to_owned(),
        });
    }
    if cap.current_mode == requested {
        return Err(ValidationError::BlockedChoice {
            capability_id: "gpu_runtime".to_owned(),
            requested: requested.to_owned(),
            reason: format!("GPU is already in {requested} mode"),
        });
    }
    if !cap.candidate_runtime_modes.contains(&requested.to_owned()) {
        return Err(ValidationError::BlockedChoice {
            capability_id: "gpu_runtime".to_owned(),
            requested: requested.to_owned(),
            reason: format!(
                "candidate runtime modes are {:?}; nvidia requires display manager restart instead",
                cap.candidate_runtime_modes
            ),
        });
    }
    if !cap.promotion_ready {
        return Err(ValidationError::BlockedChoice {
            capability_id: "gpu_runtime".to_owned(),
            requested: requested.to_owned(),
            reason:
                "runtime GPU switching is not promoted; capture and review GPU mux evidence first"
                    .to_owned(),
        });
    }
    Ok(())
}

pub fn plan_gpu_mode_runtime_write(
    capability: Option<&GpuRuntimeCapability>,
    requested: &str,
) -> Result<WriteDryRunPlan, ValidationError> {
    validate_gpu_mode_runtime_choice(capability, requested)?;
    let cap = capability.expect("validated GPU runtime capability must exist");
    let mut plan = plan_write(
        write_contract("SetGpuModeRuntime"),
        "envycontrol+pci-hotswap-plan",
        &cap.current_mode,
        requested,
    )?;
    for evidence in &cap.evidence {
        plan.safety_notes
            .push(format!("GPU runtime evidence: {evidence}"));
    }
    Ok(plan)
}

pub fn plan_led_state_write(
    leds: &[LedCapability],
    led_id: &str,
    enabled: bool,
) -> Result<WriteDryRunPlan, ValidationError> {
    validate_led_state_request(leds, led_id)?;
    let led = leds
        .iter()
        .find(|led| led.name == led_id)
        .expect("validated LED capability must exist");
    let previous_value = led
        .brightness
        .expect("validated LED brightness must exist")
        .to_string();
    let requested_value = if enabled { "1" } else { "0" };
    plan_write(
        write_contract("SetLedState"),
        &led.path,
        &previous_value,
        requested_value,
    )
}

pub fn plan_keyboard_rgb_write(
    capability: Option<&KeyboardRgbCapability>,
    request: &KeyboardRgbWriteRequest,
) -> Result<WriteDryRunPlan, ValidationError> {
    validate_keyboard_rgb_request(capability, request)?;
    let capability = capability.expect("validated keyboard RGB capability must exist");
    plan_write(
        write_contract("SetKeyboardRgb"),
        &capability.path,
        &format_keyboard_rgb_state(
            capability
                .current_effect
                .as_deref()
                .expect("validated keyboard RGB current effect must exist"),
            &capability.current_colors,
            capability
                .current_brightness
                .expect("validated keyboard RGB current brightness must exist"),
            capability.current_speed,
        ),
        &format_keyboard_rgb_request(request),
    )
}

pub fn plan_openrgb_keyboard_rgb_bridge(
    openrgb: Option<&KeyboardRgbOpenRgbStatus>,
    request: &KeyboardRgbWriteRequest,
) -> Result<WriteDryRunPlan, ValidationError> {
    let openrgb = openrgb.ok_or_else(|| ValidationError::MissingCapability {
        capability_id: "keyboard_rgb_openrgb".to_owned(),
    })?;
    if !openrgb.installed {
        return Err(ValidationError::MissingCapability {
            capability_id: "keyboard_rgb_openrgb:openrgb".to_owned(),
        });
    }
    let device = openrgb
        .devices
        .first()
        .ok_or_else(|| ValidationError::NoChoicesDetected {
            capability_id: "keyboard_rgb_openrgb:devices".to_owned(),
        })?;
    let mode = device
        .modes
        .iter()
        .find(|mode| mode.eq_ignore_ascii_case(&request.effect))
        .ok_or_else(|| ValidationError::UnsupportedChoice {
            capability_id: "keyboard_rgb_openrgb:mode".to_owned(),
            requested: request.effect.clone(),
            choices: device.modes.clone(),
        })?;
    if request.brightness > 100 {
        return Err(ValidationError::UnsupportedChoice {
            capability_id: "keyboard_rgb_openrgb:brightness".to_owned(),
            requested: request.brightness.to_string(),
            choices: u8_range_choices(0, 100),
        });
    }
    if let Some(speed) = request.speed {
        if speed > 100 {
            return Err(ValidationError::UnsupportedChoice {
                capability_id: "keyboard_rgb_openrgb:speed".to_owned(),
                requested: speed.to_string(),
                choices: u8_range_choices(0, 100),
            });
        }
    }

    let colors = openrgb_color_argument(device, &request.colors)?;
    let mut command = vec![
        "openrgb".to_owned(),
        "--device".to_owned(),
        shell_word(&device.name),
        "--mode".to_owned(),
        shell_word(mode),
        "--brightness".to_owned(),
        request.brightness.to_string(),
    ];
    if let Some(speed) = request.speed {
        command.extend(["--speed".to_owned(), speed.to_string()]);
    }
    if let Some(colors) = colors {
        command.extend(["--color".to_owned(), shell_word(&colors)]);
    }

    Ok(WriteDryRunPlan {
        method: "PlanOpenRgbKeyboardRgbBridge".to_owned(),
        capability_id: "keyboard_rgb_openrgb".to_owned(),
        polkit_action: "org.ratvantage.LegionControl1.set-keyboard-rgb".to_owned(),
        path: openrgb
            .path
            .as_deref()
            .map(|path| format!("openrgb:{path}"))
            .unwrap_or_else(|| "openrgb:openrgb".to_owned()),
        previous_value: "unknown; capture OpenRGB profile/read-back before execution".to_owned(),
        requested_value: format!(
            "{};command={}",
            format_keyboard_rgb_request(request),
            command.join(" ")
        ),
        readback_required: true,
        rollback_value: "saved OpenRGB profile or previous mode/colors".to_owned(),
        rollback_instructions: vec![
            "save current OpenRGB profile before executing any command".to_owned(),
            "after command execution, re-list/read back OpenRGB device mode and colors".to_owned(),
            "restore the saved OpenRGB profile if read-back disagrees".to_owned(),
        ],
        reboot_required: false,
        safety_notes: vec![
            "OpenRGB bridge is dry-run only; RatVantage does not execute this command yet".to_owned(),
            "promotion requires live read-back and reset-to-previous-mode evidence".to_owned(),
            "do not silently change Linux groups from the GUI; report access state and use setup flow when needed".to_owned(),
        ],
        steps: vec![
            WritePlanStep::StorePreviousValue,
            WritePlanStep::WriteRequestedValue,
            WritePlanStep::ReadBackValue,
            WritePlanStep::RestorePreviousOnReadbackFailure,
        ],
    })
}

pub fn plan_openrgb_keyboard_rgb_sdk_write(
    openrgb: Option<&KeyboardRgbOpenRgbStatus>,
    request: &KeyboardRgbWriteRequest,
) -> Result<WriteDryRunPlan, ValidationError> {
    let openrgb = openrgb.ok_or_else(|| ValidationError::MissingCapability {
        capability_id: "keyboard_rgb_openrgb:sdk".to_owned(),
    })?;
    if !openrgb.installed {
        return Err(ValidationError::MissingCapability {
            capability_id: "keyboard_rgb_openrgb:openrgb".to_owned(),
        });
    }
    let device = openrgb
        .devices
        .first()
        .ok_or_else(|| ValidationError::NoChoicesDetected {
            capability_id: "keyboard_rgb_openrgb:devices".to_owned(),
        })?;
    let mode = device
        .modes
        .iter()
        .find(|mode| mode.eq_ignore_ascii_case(&request.effect))
        .ok_or_else(|| ValidationError::UnsupportedChoice {
            capability_id: "keyboard_rgb_openrgb:mode".to_owned(),
            requested: request.effect.clone(),
            choices: device.modes.clone(),
        })?;
    if request.brightness > 100 {
        return Err(ValidationError::UnsupportedChoice {
            capability_id: "keyboard_rgb_openrgb:brightness".to_owned(),
            requested: request.brightness.to_string(),
            choices: u8_range_choices(0, 100),
        });
    }
    if let Some(speed) = request.speed {
        if speed > 100 {
            return Err(ValidationError::UnsupportedChoice {
                capability_id: "keyboard_rgb_openrgb:speed".to_owned(),
                requested: speed.to_string(),
                choices: u8_range_choices(0, 100),
            });
        }
    }

    let colors = openrgb_color_argument(device, &request.colors)?;
    Ok(WriteDryRunPlan {
        method: "SetOpenRgbKeyboardRgbSdk".to_owned(),
        capability_id: "keyboard_rgb_openrgb:sdk".to_owned(),
        polkit_action: "org.ratvantage.LegionControl1.set-keyboard-rgb".to_owned(),
        path: openrgb
            .path
            .as_deref()
            .map(|path| format!("openrgb-sdk:{path}"))
            .unwrap_or_else(|| "openrgb-sdk:openrgb".to_owned()),
        previous_value: "SDK read-back snapshot of active mode and per-zone colors".to_owned(),
        requested_value: format!(
            "{};sdk_packet=RGBCONTROLLER_UPDATELEDS;mode={};colors={}",
            format_keyboard_rgb_request(request),
            mode,
            colors.unwrap_or_else(|| "unchanged".to_owned())
        ),
        readback_required: true,
        rollback_value: "SDK before snapshot colors and active mode".to_owned(),
        rollback_instructions: vec![
            "read active mode and colors from OpenRGB SDK before writing".to_owned(),
            "send RGBCONTROLLER_UPDATELEDS through the OpenRGB SDK only".to_owned(),
            "read back requested colors from the SDK after write".to_owned(),
            "restore the before-snapshot colors through the SDK if read-back disagrees".to_owned(),
        ],
        reboot_required: false,
        safety_notes: vec![
            "OpenRGB SDK execution remains disabled in the real daemon until a process-boundary SDK writer ships".to_owned(),
            "live operator evidence must prove requested mode/colors and restore mode/colors through SDK read-back".to_owned(),
            "daemon must not write hidraw, i2c, sysfs, WMI, or EC directly for this backend".to_owned(),
        ],
        steps: vec![
            WritePlanStep::StorePreviousValue,
            WritePlanStep::WriteRequestedValue,
            WritePlanStep::ReadBackValue,
            WritePlanStep::RestorePreviousOnReadbackFailure,
        ],
    })
}

pub fn plan_hardware_profile_keyboard_rgb_write(
    capability: Option<&KeyboardRgbCapability>,
    openrgb: Option<&KeyboardRgbOpenRgbStatus>,
    request: &KeyboardRgbWriteRequest,
) -> Result<WriteDryRunPlan, ValidationError> {
    if capability.is_some() {
        plan_keyboard_rgb_write(capability, request)
    } else {
        plan_openrgb_keyboard_rgb_sdk_write(openrgb, request)
    }
}

pub fn plan_ideapad_toggle_write(
    toggles: &[IdeapadToggleCapability],
    leds: &[LedCapability],
    toggle_id: &str,
    enabled: bool,
) -> Result<WriteDryRunPlan, ValidationError> {
    validate_ideapad_toggle_request(toggles, leds, toggle_id)?;
    let toggle = toggles
        .iter()
        .find(|toggle| toggle.name == toggle_id)
        .expect("validated ideapad toggle capability must exist");
    let previous_value = toggle
        .current_value
        .as_deref()
        .expect("validated ideapad toggle current value must exist");
    let requested_value = if enabled { "1" } else { "0" };
    plan_write(
        write_contract("SetIdeapadToggle"),
        toggle
            .path
            .as_deref()
            .expect("validated ideapad toggle path must exist"),
        previous_value,
        requested_value,
    )
}

pub fn plan_fan_preset_write(
    fan_curves: &[FanCurveCapability],
    presets: &[FanPreset],
    requested: &str,
) -> Result<WriteDryRunPlan, ValidationError> {
    plan_fan_preset_write_with_platform_profile(fan_curves, presets, None, requested)
}

pub fn plan_fan_preset_write_with_platform_profile(
    fan_curves: &[FanCurveCapability],
    presets: &[FanPreset],
    platform_profile: Option<&PlatformProfileCapability>,
    requested: &str,
) -> Result<WriteDryRunPlan, ValidationError> {
    validate_fan_preset_choice(fan_curves, presets, requested)?;
    let preset = find_fan_preset(presets, requested).expect("validated fan preset must exist");
    let curve = select_fan_curve(fan_curves).expect("validated fan curve capability must exist");
    let mut plan = plan_write(
        write_contract("ApplyFanPreset"),
        curve.path.as_deref().unwrap_or("fan_curves"),
        "current fan curve snapshot",
        &preset.id,
    )?;
    add_custom_thermal_prerequisite(&mut plan, platform_profile, "fan preset writes");
    plan.safety_notes.push(preset.safety_note.clone());
    Ok(plan)
}

pub fn plan_restore_auto_fan_write(
    fan_curves: &[FanCurveCapability],
) -> Result<WriteDryRunPlan, ValidationError> {
    plan_restore_auto_fan_write_with_platform_profile(fan_curves, None)
}

pub fn plan_restore_auto_fan_write_with_platform_profile(
    fan_curves: &[FanCurveCapability],
    platform_profile: Option<&PlatformProfileCapability>,
) -> Result<WriteDryRunPlan, ValidationError> {
    let curve = select_fan_curve(fan_curves).ok_or_else(|| ValidationError::MissingCapability {
        capability_id: "fan_curves".to_owned(),
    })?;
    let mut plan = plan_write(
        write_contract("RestoreAutoFan"),
        curve.path.as_deref().unwrap_or("fan_curves"),
        "current fan-control state",
        "auto/default fan control",
    )?;
    add_custom_thermal_prerequisite(&mut plan, platform_profile, "fan restore/default writes");
    Ok(plan)
}

pub fn plan_cpu_governor_write(
    capability: Option<&CpuPowerCapability>,
    requested: &str,
) -> Result<WriteDryRunPlan, ValidationError> {
    validate_cpu_governor_choice(capability, requested)?;
    let capability = capability.expect("validated cpu_power capability must exist");
    plan_write(
        write_contract("SetCpuGovernor"),
        &capability.governor_path,
        capability
            .governor
            .as_deref()
            .expect("validated cpu governor current value must exist"),
        requested,
    )
}

pub fn plan_cpu_epp_write(
    capability: Option<&CpuPowerCapability>,
    requested: &str,
) -> Result<WriteDryRunPlan, ValidationError> {
    validate_cpu_epp_choice(capability, requested)?;
    let capability = capability.expect("validated cpu_power capability must exist");
    plan_write(
        write_contract("SetCpuEpp"),
        &capability.epp_path,
        capability
            .epp
            .as_deref()
            .expect("validated cpu epp current value must exist"),
        requested,
    )
}

pub fn plan_cpu_boost_write(
    capability: Option<&CpuPowerCapability>,
    requested: &str,
) -> Result<WriteDryRunPlan, ValidationError> {
    validate_cpu_boost_request(capability, requested)?;
    let capability = capability.expect("validated cpu_power capability must exist");
    let previous_value = if capability
        .boost
        .expect("validated CPU boost current value must exist")
    {
        "1"
    } else {
        "0"
    };
    plan_write(
        write_contract("SetCpuBoost"),
        &capability.boost_path,
        previous_value,
        requested,
    )
}

pub fn plan_curve_optimizer_all_core_write(
    requested: &str,
) -> Result<WriteDryRunPlan, ValidationError> {
    let offset = validate_curve_optimizer_all_core_offset(requested)?;
    let encoded = encode_curve_optimizer_offset(offset)?;
    let contract = write_contract("SetCurveOptimizerAllCore");
    Ok(WriteDryRunPlan {
        method: contract.method.to_owned(),
        capability_id: contract.capability_id.to_owned(),
        polkit_action: contract.polkit_action.to_owned(),
        path: "/usr/local/bin/ryzenadj --set-coall".to_owned(),
        previous_value: "write-only".to_owned(),
        requested_value: format!("{offset} (encoded {encoded})"),
        readback_required: false,
        rollback_value: "0".to_owned(),
        rollback_instructions: rollback_instructions(contract, "write-only", &offset.to_string()),
        reboot_required: contract.reboot_required,
        safety_notes: contract
            .safety_notes
            .iter()
            .map(|note| (*note).to_owned())
            .collect(),
        steps: vec![
            WritePlanStep::AuthorizeCaller,
            WritePlanStep::WriteRequestedValue,
        ],
    })
}

pub fn plan_conservation_mode_write(
    toggles: &[IdeapadToggleCapability],
    requested: &str,
) -> Result<WriteDryRunPlan, ValidationError> {
    validate_conservation_mode_request(toggles, requested)?;
    let toggle = toggles
        .iter()
        .find(|toggle| toggle.name == "conservation_mode")
        .expect("validated conservation_mode capability must exist");
    let previous_value = toggle
        .current_value
        .as_deref()
        .expect("validated conservation_mode current value must exist");
    plan_write(
        write_contract("SetConservationMode"),
        toggle
            .path
            .as_deref()
            .expect("validated conservation_mode path must exist"),
        previous_value,
        requested,
    )
}

pub fn plan_amd_gpu_dpm_force_level_write(
    capability: Option<&AmdGpuPowerDpmCapability>,
    requested: &str,
) -> Result<WriteDryRunPlan, ValidationError> {
    validate_amd_gpu_dpm_force_level_choice(capability, requested)?;
    let capability = capability.expect("validated AMD GPU DPM capability must exist");
    plan_write(
        write_contract("SetAmdGpuDpmForceLevel"),
        &capability.force_performance_level_path,
        capability
            .current_force_performance_level
            .as_deref()
            .expect("validated AMD GPU DPM current force level must exist"),
        requested,
    )
}

pub fn plan_firmware_attribute_write(
    attributes: &[FirmwareAttributeCapability],
    attribute_id: &str,
    requested: &str,
) -> Result<WriteDryRunPlan, ValidationError> {
    plan_firmware_attribute_write_with_platform_profile(attributes, None, attribute_id, requested)
}

pub fn plan_firmware_attribute_reset_write(
    attributes: &[FirmwareAttributeCapability],
    attribute_id: &str,
) -> Result<WriteDryRunPlan, ValidationError> {
    plan_firmware_attribute_reset_write_with_platform_profile(attributes, None, attribute_id)
}

pub fn plan_firmware_attribute_reset_write_with_platform_profile(
    attributes: &[FirmwareAttributeCapability],
    platform_profile: Option<&PlatformProfileCapability>,
    attribute_id: &str,
) -> Result<WriteDryRunPlan, ValidationError> {
    let attribute = attributes
        .iter()
        .find(|attribute| attribute.name == attribute_id)
        .ok_or_else(|| ValidationError::MissingCapability {
            capability_id: format!("firmware_attributes:{attribute_id}"),
        })?;
    let default_value =
        attribute
            .default_value
            .as_deref()
            .ok_or_else(|| ValidationError::MissingCapability {
                capability_id: format!("firmware_attributes:{attribute_id}:default_value"),
            })?;
    if default_value.trim().is_empty() {
        return Err(ValidationError::EmptyValue {
            field: format!("firmware_attributes:{attribute_id}:default_value"),
        });
    }
    let mut plan = plan_firmware_attribute_write_with_platform_profile(
        attributes,
        platform_profile,
        attribute_id,
        default_value,
    )?;
    plan.safety_notes.push(format!(
        "reset-to-default plan uses firmware default_value `{default_value}` for `{attribute_id}`"
    ));
    Ok(plan)
}

pub fn firmware_ppt_preset(
    attributes: &[FirmwareAttributeCapability],
    preset_id: &str,
) -> Result<FirmwarePptPreset, ValidationError> {
    if preset_id.trim().is_empty() {
        return Err(ValidationError::EmptyValue {
            field: "preset_id".to_owned(),
        });
    }

    let preset = match preset_id {
        "conservative" => static_firmware_ppt_preset(
            "conservative",
            "Conservative",
            "Lower PPT limits for cooler sustained loads.",
            &[("ppt_pl1_spl", "60"), ("ppt_pl2_sppt", "75"), ("ppt_pl3_fppt", "90")],
            "conservative PPT preset is preview-only until live thermal/read-back evidence exists",
        ),
        "balanced-custom" => static_firmware_ppt_preset(
            "balanced-custom",
            "Balanced Custom",
            "Fixture-confirmed default-like PPT limits for custom thermal mode.",
            &[("ppt_pl1_spl", "70"), ("ppt_pl2_sppt", "85"), ("ppt_pl3_fppt", "102")],
            "balanced custom PPT preset is preview-only and matches the current 82WM fixture defaults",
        ),
        "performance-custom" => static_firmware_ppt_preset(
            "performance-custom",
            "Performance Custom",
            "Higher PPT limits that remain inside detected firmware ranges.",
            &[("ppt_pl1_spl", "80"), ("ppt_pl2_sppt", "115"), ("ppt_pl3_fppt", "135")],
            "performance custom PPT preset needs live thermal validation before any execution path is enabled",
        ),
        "reset-defaults" => firmware_ppt_reset_defaults_preset(attributes)?,
        _ => {
            return Err(ValidationError::UnsupportedChoice {
                capability_id: "firmware_ppt_preset".to_owned(),
                requested: preset_id.to_owned(),
                choices: FIRMWARE_PPT_PRESET_IDS
                    .iter()
                    .map(|preset| (*preset).to_owned())
                    .collect(),
            })
        }
    };

    for attribute_id in SUPPORTED_FIRMWARE_SCALAR_ATTRIBUTES {
        let Some(value) = preset.values.get(*attribute_id) else {
            return Err(ValidationError::MissingCapability {
                capability_id: format!("firmware_ppt_preset:{preset_id}:{attribute_id}"),
            });
        };
        validate_firmware_scalar_attribute_request(attributes, attribute_id, value)?;
    }

    Ok(preset)
}

pub fn plan_firmware_ppt_preset_write_with_platform_profile(
    attributes: &[FirmwareAttributeCapability],
    platform_profile: Option<&PlatformProfileCapability>,
    preset_id: &str,
) -> Result<Vec<WriteDryRunPlan>, ValidationError> {
    let preset = firmware_ppt_preset(attributes, preset_id)?;
    let mut plans = Vec::new();
    for attribute_id in SUPPORTED_FIRMWARE_SCALAR_ATTRIBUTES {
        let requested = preset
            .values
            .get(*attribute_id)
            .expect("validated firmware PPT preset value must exist");
        let mut plan = plan_firmware_attribute_write_with_platform_profile(
            attributes,
            platform_profile,
            attribute_id,
            requested,
        )?;
        plan.safety_notes.push(preset.safety_note.clone());
        plans.push(plan);
    }
    Ok(plans)
}

pub fn plan_firmware_attribute_write_with_platform_profile(
    attributes: &[FirmwareAttributeCapability],
    platform_profile: Option<&PlatformProfileCapability>,
    attribute_id: &str,
    requested: &str,
) -> Result<WriteDryRunPlan, ValidationError> {
    validate_firmware_scalar_attribute_request(attributes, attribute_id, requested)?;
    let attribute = attributes
        .iter()
        .find(|attribute| attribute.name == attribute_id)
        .expect("validated firmware attribute capability must exist");
    let current_value = attribute
        .current_value
        .as_deref()
        .expect("validated firmware attribute current value must exist");
    let current_value_path = format!("{}/current_value", attribute.path);
    let mut plan = plan_write(
        write_contract("SetFirmwareAttribute"),
        &current_value_path,
        current_value,
        requested,
    )?;
    add_custom_thermal_prerequisite(&mut plan, platform_profile, "firmware PPT writes");
    Ok(plan)
}

fn static_firmware_ppt_preset(
    id: &str,
    label: &str,
    description: &str,
    values: &[(&str, &str)],
    safety_note: &str,
) -> FirmwarePptPreset {
    FirmwarePptPreset {
        id: id.to_owned(),
        label: label.to_owned(),
        description: description.to_owned(),
        values: values
            .iter()
            .map(|(attribute_id, value)| ((*attribute_id).to_owned(), (*value).to_owned()))
            .collect(),
        safety_note: safety_note.to_owned(),
    }
}

fn firmware_ppt_reset_defaults_preset(
    attributes: &[FirmwareAttributeCapability],
) -> Result<FirmwarePptPreset, ValidationError> {
    let mut values = BTreeMap::new();
    for attribute_id in SUPPORTED_FIRMWARE_SCALAR_ATTRIBUTES {
        let attribute = attributes
            .iter()
            .find(|attribute| attribute.name == *attribute_id)
            .ok_or_else(|| ValidationError::MissingCapability {
                capability_id: format!("firmware_attributes:{attribute_id}"),
            })?;
        let default_value = attribute.default_value.as_deref().ok_or_else(|| {
            ValidationError::MissingCapability {
                capability_id: format!("firmware_attributes:{attribute_id}:default_value"),
            }
        })?;
        if default_value.trim().is_empty() {
            return Err(ValidationError::EmptyValue {
                field: format!("firmware_attributes:{attribute_id}:default_value"),
            });
        }
        values.insert((*attribute_id).to_owned(), default_value.to_owned());
    }

    Ok(FirmwarePptPreset {
        id: "reset-defaults".to_owned(),
        label: "Reset Defaults".to_owned(),
        description: "Use each firmware PPT attribute default_value from the detected platform."
            .to_owned(),
        values,
        safety_note:
            "reset-defaults PPT preset uses detected firmware default_value metadata for each PPT attribute"
                .to_owned(),
    })
}

fn write_contract(method: &str) -> &'static WriteMethodContract {
    WRITE_METHOD_CONTRACTS
        .iter()
        .find(|contract| contract.method == method)
        .expect("write contract must exist")
}

fn plan_write(
    contract: &WriteMethodContract,
    path: &str,
    previous_value: &str,
    requested_value: &str,
) -> Result<WriteDryRunPlan, ValidationError> {
    let mut steps = vec![
        WritePlanStep::AuthorizeCaller,
        WritePlanStep::StorePreviousValue,
        WritePlanStep::WriteRequestedValue,
        WritePlanStep::ReadBackValue,
        WritePlanStep::RestorePreviousOnReadbackFailure,
    ];
    if contract.reboot_required {
        steps.push(WritePlanStep::RequireReboot);
    }

    Ok(WriteDryRunPlan {
        method: contract.method.to_owned(),
        capability_id: contract.capability_id.to_owned(),
        polkit_action: contract.polkit_action.to_owned(),
        path: path.to_owned(),
        previous_value: previous_value.to_owned(),
        requested_value: requested_value.to_owned(),
        readback_required: true,
        rollback_value: previous_value.to_owned(),
        rollback_instructions: rollback_instructions(contract, previous_value, requested_value),
        reboot_required: contract.reboot_required,
        safety_notes: contract
            .safety_notes
            .iter()
            .map(|note| (*note).to_owned())
            .collect(),
        steps,
    })
}

fn rollback_instructions(
    contract: &WriteMethodContract,
    previous_value: &str,
    requested_value: &str,
) -> Vec<String> {
    let mut instructions: Vec<String> = contract
        .rollback
        .iter()
        .map(|instruction| (*instruction).to_owned())
        .collect();

    if contract.method == "SetGpuMode" {
        instructions.push(format!(
            "future rollback target is previous GPU mode `{previous_value}` if `{requested_value}` fails after reboot"
        ));
        instructions.push(
            "if graphical login is unavailable, use a TTY or rescue session to restore the previous EnvyControl mode, then reboot again"
                .to_owned(),
        );
    }

    instructions
}

fn add_custom_thermal_prerequisite(
    plan: &mut WriteDryRunPlan,
    platform_profile: Option<&PlatformProfileCapability>,
    target: &str,
) {
    match platform_profile {
        None => {
            plan.safety_notes.push(format!(
                "custom thermal prerequisite unknown for {target}: platform_profile capability is missing; keep execution disabled"
            ));
        }
        Some(profile) if !profile.choices.iter().any(|choice| choice == "custom") => {
            plan.safety_notes.push(format!(
                "custom thermal prerequisite unavailable for {target}: platform_profile_choices does not list `custom`; keep execution disabled"
            ));
        }
        Some(profile) if profile.current.as_deref() == Some("custom") => {
            plan.safety_notes.push(format!(
                "custom thermal prerequisite satisfied for {target}: current platform_profile is `custom`"
            ));
        }
        Some(profile) => {
            let current = profile.current.as_deref().unwrap_or("unknown");
            plan.safety_notes.push(format!(
                "custom thermal prerequisite required for {target}: switch platform_profile from `{current}` to `custom`, read back `custom`, then apply this plan"
            ));
            plan.rollback_instructions.push(format!(
                "after {target}, restore previous platform_profile `{current}` unless the caller explicitly keeps `custom`"
            ));
        }
    }
}

fn require_current(capability_id: &str, current: Option<&str>) -> Result<(), ValidationError> {
    match current {
        Some(value) if !value.is_empty() => Ok(()),
        _ => Err(ValidationError::MissingCurrentValue {
            capability_id: capability_id.to_owned(),
        }),
    }
}

fn parse_firmware_integer(
    attribute_id: &str,
    field: &str,
    value: Option<&str>,
) -> Result<i64, ValidationError> {
    let value = value.ok_or_else(|| ValidationError::MissingCurrentValue {
        capability_id: format!("firmware_attributes:{attribute_id}:{field}"),
    })?;
    value
        .parse::<i64>()
        .map_err(|_| ValidationError::BlockedChoice {
            capability_id: "firmware_attributes".to_owned(),
            requested: value.to_owned(),
            reason: format!("{field} must be an integer for {attribute_id}"),
        })
}

fn firmware_integer_choices(min_value: i64, max_value: i64, step: i64) -> Vec<String> {
    let mut choices = Vec::new();
    let mut value = min_value;
    while value <= max_value && choices.len() < 256 {
        choices.push(value.to_string());
        value += step;
    }
    choices
}

fn u8_range_choices(min_value: u8, max_value: u8) -> Vec<String> {
    (min_value..=max_value)
        .map(|value| value.to_string())
        .collect()
}

fn is_hex_rgb_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..].chars().all(|ch| ch.is_ascii_hexdigit())
}

fn format_keyboard_rgb_state(
    effect: &str,
    colors: &BTreeMap<String, String>,
    brightness: u8,
    speed: Option<u8>,
) -> String {
    format!(
        "effect={effect};brightness={brightness};speed={};colors={}",
        speed
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_owned()),
        colors
            .iter()
            .map(|(zone, color)| format!("{zone}:{color}"))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn format_keyboard_rgb_request(request: &KeyboardRgbWriteRequest) -> String {
    format_keyboard_rgb_state(
        &request.effect,
        &request.colors,
        request.brightness,
        request.speed,
    )
}

fn openrgb_color_argument(
    device: &KeyboardRgbOpenRgbDevice,
    colors: &BTreeMap<String, String>,
) -> Result<Option<String>, ValidationError> {
    if colors.is_empty() {
        return Ok(None);
    }
    if device.leds.is_empty() {
        return Err(ValidationError::NoChoicesDetected {
            capability_id: "keyboard_rgb_openrgb:leds".to_owned(),
        });
    }
    let choices = device
        .leds
        .iter()
        .flat_map(|label| [label.clone(), openrgb_zone_key(label)])
        .collect::<Vec<_>>();
    for (zone_id, color) in colors {
        if !is_hex_rgb_color(color) {
            return Err(ValidationError::BlockedChoice {
                capability_id: "keyboard_rgb_openrgb:color".to_owned(),
                requested: color.clone(),
                reason: "OpenRGB colors must use #RRGGBB hex".to_owned(),
            });
        }
        if !device
            .leds
            .iter()
            .any(|label| zone_id == label || zone_id == &openrgb_zone_key(label))
        {
            return Err(ValidationError::UnsupportedChoice {
                capability_id: "keyboard_rgb_openrgb:zone".to_owned(),
                requested: zone_id.clone(),
                choices: choices.clone(),
            });
        }
    }

    let mut ordered = Vec::new();
    for label in &device.leds {
        let color = colors
            .get(label)
            .or_else(|| colors.get(&openrgb_zone_key(label)))
            .ok_or_else(|| ValidationError::MissingCapability {
                capability_id: format!("keyboard_rgb_openrgb:zone:{}", openrgb_zone_key(label)),
            })?;
        ordered.push(color.trim_start_matches('#').to_ascii_uppercase());
    }
    Ok(Some(ordered.join(",")))
}

fn openrgb_zone_key(label: &str) -> String {
    label
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

fn shell_word(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/' | ':' | ','))
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn curve_optimizer_offset_choices() -> Vec<String> {
    (-30..=0).map(|value| value.to_string()).collect()
}

fn find_fan_preset<'a>(presets: &'a [FanPreset], requested: &str) -> Option<&'a FanPreset> {
    presets.iter().find(|preset| preset.id == requested)
}

fn select_fan_curve(fan_curves: &[FanCurveCapability]) -> Option<&FanCurveCapability> {
    fan_curves.iter().find(|curve| {
        curve.status == CapabilityStatus::ProbeOnly
            && curve.path.as_deref().is_some_and(|path| !path.is_empty())
    })
}

fn validate_fan_preset_schema(preset: &FanPreset) -> Result<(), ValidationError> {
    if preset.schema_version != 1 {
        return blocked_fan_preset(preset, "unsupported fan preset schema version");
    }
    if preset.id.is_empty()
        || preset.label.is_empty()
        || preset.description.is_empty()
        || preset.safety_note.is_empty()
        || preset.target_profiles.is_empty()
        || preset
            .target_profiles
            .iter()
            .any(|profile| profile.is_empty())
    {
        return blocked_fan_preset(preset, "fan preset metadata is incomplete");
    }
    if preset.points.len() != 10 {
        return blocked_fan_preset(preset, "fan preset must contain exactly 10 points");
    }

    let mut previous_temperature = None;
    let mut previous_pwm = None;
    for point in &preset.points {
        if previous_temperature.is_some_and(|previous| point.temperature_c <= previous) {
            return blocked_fan_preset(preset, "fan preset temperatures must be ascending");
        }
        if point.pwm > 255 {
            return blocked_fan_preset(preset, "fan preset PWM values must be 0..255");
        }
        if previous_pwm.is_some_and(|previous| point.pwm < previous) {
            return blocked_fan_preset(preset, "fan preset PWM values must be non-decreasing");
        }
        previous_temperature = Some(point.temperature_c);
        previous_pwm = Some(point.pwm);
    }

    Ok(())
}

fn validate_fan_curve_supports_preset(
    curve: &FanCurveCapability,
    preset: &FanPreset,
) -> Result<(), ValidationError> {
    let point_count = preset.points.len();
    let has_required_last_temp = curve
        .point_paths
        .iter()
        .any(|path| path.contains(&format!("_auto_point{point_count}_temp")));
    let has_required_last_pwm = curve
        .point_paths
        .iter()
        .any(|path| path.contains(&format!("_auto_point{point_count}_pwm")));
    if curve.point_paths.len() < point_count * 2
        || !has_required_last_temp
        || !has_required_last_pwm
    {
        return Err(ValidationError::BlockedChoice {
            capability_id: "fan_curves".to_owned(),
            requested: preset.id.clone(),
            reason: "detected fan curve does not expose a complete 10-point writable shape"
                .to_owned(),
        });
    }

    Ok(())
}

/// Matched `_auto_pointN_{temp,pwm}` sysfs paths for one index `N` under a fan curve capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FanCurveHwmonPointPair {
    pub index: u32,
    pub temp_path: String,
    pub pwm_path: String,
}

fn parse_fan_auto_point_path(path: &str) -> Option<(u32, bool)> {
    const NEEDLE: &str = "_auto_point";
    let start = path.find(NEEDLE)? + NEEDLE.len();
    let rest = path.get(start..)?;
    let num_digits = rest
        .char_indices()
        .find(|(_, ch)| !ch.is_ascii_digit())
        .map(|(offset, _)| offset)
        .unwrap_or(rest.len());
    if num_digits == 0 {
        return None;
    }
    let digits_end = start + num_digits;
    let index: u32 = path.get(start..digits_end)?.parse().ok()?;
    let suffix = &path[digits_end..];
    if suffix.starts_with("_temp") {
        Some((index, true))
    } else if suffix.starts_with("_pwm") {
        Some((index, false))
    } else {
        None
    }
}

/// Returns temp/pwm path pairs for each index that has both nodes in `curve.point_paths`, sorted by index.
pub fn fan_curve_hwmon_point_pairs(curve: &FanCurveCapability) -> Vec<FanCurveHwmonPointPair> {
    use std::collections::BTreeMap;

    let mut temps: BTreeMap<u32, String> = BTreeMap::new();
    let mut pwms: BTreeMap<u32, String> = BTreeMap::new();
    for path in &curve.point_paths {
        if let Some((index, is_temp)) = parse_fan_auto_point_path(path) {
            if is_temp {
                temps.insert(index, path.clone());
            } else {
                pwms.insert(index, path.clone());
            }
        }
    }

    temps
        .into_iter()
        .filter_map(|(index, temp_path)| {
            pwms.get(&index).map(|pwm_path| FanCurveHwmonPointPair {
                index,
                temp_path,
                pwm_path: pwm_path.clone(),
            })
        })
        .collect()
}

fn sysfs_u32_from_snapshot_points(points: &[FanCurvePointSnapshot], path: &str) -> Option<u32> {
    points
        .iter()
        .find(|point| point.path == path)
        .and_then(|point| point.value.trim().parse::<u32>().ok())
}

/// Temperature and PWM sysfs integers from a [`FanCurveSnapshot`], aligned to `curve` auto-point
/// indices (same pairing rules as [`fan_curve_hwmon_point_pairs`]). Incomplete indices are skipped.
pub fn fan_curve_snapshot_chart_pairs(
    curve: &FanCurveCapability,
    snapshot: &FanCurveSnapshot,
) -> Vec<(u32, u32)> {
    fan_curve_hwmon_point_pairs(curve)
        .into_iter()
        .filter_map(|pair| {
            let temp = sysfs_u32_from_snapshot_points(&snapshot.points, &pair.temp_path)?;
            let pwm = sysfs_u32_from_snapshot_points(&snapshot.points, &pair.pwm_path)?;
            Some((temp, pwm))
        })
        .collect()
}

/// Validates raw sysfs integers for a manual curve scratchpad (e.g. millidegree temps, 0–255 pwm).
/// Rules mirror packaged presets: strictly increasing temperature channel values, non-decreasing pwm, pwm ≤ 255.
pub fn validate_manual_fan_curve_pairs(pairs: &[(u32, u32)]) -> Result<(), ValidationError> {
    if pairs.is_empty() {
        return Err(ValidationError::BlockedChoice {
            capability_id: "fan_curves".to_owned(),
            requested: "manual_scratchpad".to_owned(),
            reason: "no (temp, pwm) pairs to validate".to_owned(),
        });
    }

    let mut previous_temperature = None;
    let mut previous_pwm = None;
    for (temp, pwm) in pairs {
        if *pwm > 255 {
            return Err(ValidationError::BlockedChoice {
                capability_id: "fan_curves".to_owned(),
                requested: "manual_scratchpad".to_owned(),
                reason: format!("pwm sysfs value {pwm} is out of range (max 255)"),
            });
        }
        if previous_temperature.is_some_and(|previous| *temp <= previous) {
            return Err(ValidationError::BlockedChoice {
                capability_id: "fan_curves".to_owned(),
                requested: "manual_scratchpad".to_owned(),
                reason: "temp channel values must be strictly increasing (check sysfs units, e.g. millidegree)"
                    .to_owned(),
            });
        }
        if previous_pwm.is_some_and(|previous| *pwm < previous) {
            return Err(ValidationError::BlockedChoice {
                capability_id: "fan_curves".to_owned(),
                requested: "manual_scratchpad".to_owned(),
                reason: "pwm values must be non-decreasing".to_owned(),
            });
        }
        previous_temperature = Some(*temp);
        previous_pwm = Some(*pwm);
    }

    Ok(())
}

/// Multiline summary of sysfs paths and integer values implied by validated scratchpad rows.
/// `hw_pairs` must be in the same order as the scratchpad table (typically [`fan_curve_hwmon_point_pairs`]).
/// Performs no I/O; does not contact the daemon.
pub fn format_manual_fan_scratchpad_sysfs_preview(
    hw_pairs: &[FanCurveHwmonPointPair],
    temps_pwms: &[(u32, u32)],
) -> Result<String, ValidationError> {
    if hw_pairs.len() != temps_pwms.len() {
        return Err(ValidationError::BlockedChoice {
            capability_id: "fan_curves".to_owned(),
            requested: "manual_scratchpad".to_owned(),
            reason: format!(
                "hwmon pair count {} does not match scratchpad row count {}",
                hw_pairs.len(),
                temps_pwms.len()
            ),
        });
    }
    validate_manual_fan_curve_pairs(temps_pwms)?;

    let mut out = String::from(
        "Validated scratchpad: sysfs nodes and values that match your integers (preview only — not written by RatVantage).\n\n",
    );
    for (pair, (temp, pwm)) in hw_pairs.iter().zip(temps_pwms.iter()) {
        let pwm_clamped = (*pwm).min(255);
        out.push_str(&format!(
            "Point {}\n  {}\n    {temp}\n  {}\n    {pwm_clamped}\n\n",
            pair.index, pair.temp_path, pair.pwm_path,
        ));
    }
    out.push_str("Monotonic temp and non-decreasing pwm rules passed for these integers.");
    Ok(out)
}

/// Lossless import/export for the GTK fan scratchpad (raw sysfs integers + paths).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FanScratchpadTomlV1 {
    pub schema_version: u8,
    #[serde(default = "fan_scratchpad_toml_kind_default")]
    pub kind: String,
    #[serde(default)]
    pub note: String,
    pub pairs: Vec<FanScratchpadTomlPair>,
}

fn fan_scratchpad_toml_kind_default() -> String {
    "ratvantage_fan_scratchpad_v1".to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FanScratchpadTomlPair {
    pub index: u32,
    pub temp_path: String,
    pub pwm_path: String,
    pub temp_raw: u32,
    pub pwm_raw: u32,
}

/// Serializes the current scratchpad rows to TOML (`ratvantage_fan_scratchpad_v1`).
pub fn encode_fan_scratchpad_toml_v1(
    rows: &[(FanCurveHwmonPointPair, u32, u32)],
) -> Result<String, toml::ser::Error> {
    let doc = FanScratchpadTomlV1 {
        schema_version: 1,
        kind: fan_scratchpad_toml_kind_default(),
        note: "temp_raw is usually millidegree Celsius from hwmon; pwm_raw is typically 0–255."
            .to_owned(),
        pairs: rows
            .iter()
            .map(|(pair, temp_raw, pwm_raw)| FanScratchpadTomlPair {
                index: pair.index,
                temp_path: pair.temp_path.clone(),
                pwm_path: pair.pwm_path.clone(),
                temp_raw: *temp_raw,
                pwm_raw: *pwm_raw,
            })
            .collect(),
    };
    toml::to_string_pretty(&doc)
}

/// Parses a scratchpad TOML document.
pub fn decode_fan_scratchpad_toml_v1(source: &str) -> Result<FanScratchpadTomlV1, toml::de::Error> {
    toml::from_str(source)
}

/// Parses a packaged fan preset TOML file (`[[points]]`, metadata, etc.).
pub fn parse_fan_preset_toml(source: &str) -> Result<FanPreset, toml::de::Error> {
    toml::from_str(source)
}

/// Validates preset metadata and point rules (same checks used before dry-run planning).
pub fn validate_fan_preset_document(preset: &FanPreset) -> Result<(), ValidationError> {
    validate_fan_preset_schema(preset)
}

/// Maps preset degrees + pwm into typical hwmon millidegree + pwm raw integers for the scratchpad.
pub fn fan_preset_points_as_sysfs_raw(
    preset: &FanPreset,
) -> Result<Vec<(u32, u32)>, ValidationError> {
    validate_fan_preset_schema(preset)?;
    Ok(preset
        .points
        .iter()
        .map(|point| {
            let temp_raw = (i32::from(point.temperature_c))
                .saturating_mul(1000)
                .clamp(0, i32::MAX) as u32;
            let pwm_raw = u32::from(point.pwm.min(255));
            (temp_raw, pwm_raw)
        })
        .collect())
}

fn blocked_fan_preset<T>(preset: &FanPreset, reason: &str) -> Result<T, ValidationError> {
    Err(ValidationError::BlockedChoice {
        capability_id: "fan_preset".to_owned(),
        requested: preset.id.clone(),
        reason: reason.to_owned(),
    })
}

fn validate_choice(
    capability_id: &str,
    field: &str,
    requested: &str,
    choices: &[String],
    blocked: &[(&str, &str)],
) -> Result<(), ValidationError> {
    if requested.is_empty() {
        return Err(ValidationError::EmptyValue {
            field: field.to_owned(),
        });
    }

    if choices.is_empty() {
        return Err(ValidationError::NoChoicesDetected {
            capability_id: capability_id.to_owned(),
        });
    }

    if !choices.iter().any(|choice| choice == requested) {
        return Err(ValidationError::UnsupportedChoice {
            capability_id: capability_id.to_owned(),
            requested: requested.to_owned(),
            choices: choices.to_vec(),
        });
    }

    if let Some((_, reason)) = blocked
        .iter()
        .find(|(blocked_choice, _)| *blocked_choice == requested)
    {
        return Err(ValidationError::BlockedChoice {
            capability_id: capability_id.to_owned(),
            requested: requested.to_owned(),
            reason: (*reason).to_owned(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DBUS_ACTION_PREFIX: &str = "org.ratvantage.LegionControl1.";

    #[test]
    fn format_gpu_mode_pending_summary_none_and_hybrid_case() {
        assert_eq!(format_gpu_mode_pending_summary(None), "none");
        let pending = GpuModePending {
            requested_mode: "hybrid".to_owned(),
            previous_mode: Some("nvidia".to_owned()),
            reboot_required: true,
        };
        assert_eq!(
            format_gpu_mode_pending_summary(Some(&pending)),
            "hybrid pending (was nvidia); reboot required"
        );
    }

    #[test]
    fn format_power_profiles_probe_summary_fixture_and_live_shapes() {
        assert_eq!(format_power_profiles_probe_summary(None), "not_applicable");
        let live = PowerProfilesCapability {
            bus: "session".to_owned(),
            well_known_name: "org.freedesktop.UPower.PowerProfiles".to_owned(),
            unique_owner: Some(":1.42".to_owned()),
            active_profile: Some("balanced".to_owned()),
            status: CapabilityStatus::ProbeOnly,
            detail: None,
        };
        assert_eq!(
            format_power_profiles_probe_summary(Some(&live)),
            "bus=session owner=:1.42 active=balanced"
        );
        let no_owner = PowerProfilesCapability {
            bus: "session".to_owned(),
            well_known_name: "org.freedesktop.UPower.PowerProfiles".to_owned(),
            unique_owner: None,
            active_profile: None,
            status: CapabilityStatus::Missing,
            detail: Some("name_has_no_owner".to_owned()),
        };
        assert_eq!(
            format_power_profiles_probe_summary(Some(&no_owner)),
            "name_has_no_owner"
        );
    }

    #[test]
    fn format_fan_curve_snapshot_summary_none_and_plural() {
        assert_eq!(format_fan_curve_snapshot_summary(None), "none");
        let one = FanCurveSnapshot {
            curve_id: "legion_hwmon".to_owned(),
            path: None,
            points: vec![FanCurvePointSnapshot {
                path: "/p".to_owned(),
                value: "1".to_owned(),
            }],
        };
        assert_eq!(
            format_fan_curve_snapshot_summary(Some(&one)),
            "1 point on legion_hwmon"
        );
        let two = FanCurveSnapshot {
            curve_id: "legion_hwmon".to_owned(),
            path: None,
            points: vec![
                FanCurvePointSnapshot {
                    path: "/a".to_owned(),
                    value: "1".to_owned(),
                },
                FanCurvePointSnapshot {
                    path: "/b".to_owned(),
                    value: "2".to_owned(),
                },
            ],
        };
        assert_eq!(
            format_fan_curve_snapshot_summary(Some(&two)),
            "2 points on legion_hwmon"
        );
    }

    #[test]
    fn format_hardware_profile_apply_run_summary_reports_completion_and_stop_reason() {
        assert_eq!(format_hardware_profile_apply_run_summary(None), "none");

        let plan = WriteDryRunPlan {
            method: "SetCurveOptimizerAllCore".to_owned(),
            capability_id: "curve_optimizer_all_core".to_owned(),
            polkit_action: "org.ratvantage.LegionControl1.set-curve-optimizer".to_owned(),
            path: "ryzenadj:/usr/local/bin/ryzenadj".to_owned(),
            previous_value: "unknown".to_owned(),
            requested_value: "-20 (encoded 4294967276)".to_owned(),
            readback_required: false,
            rollback_value: "0".to_owned(),
            rollback_instructions: Vec::new(),
            reboot_required: false,
            safety_notes: Vec::new(),
            steps: Vec::new(),
        };
        let applied = HardwareProfileApplyRun {
            profile_id: "co_test".to_owned(),
            profile_label: "CO test".to_owned(),
            timestamp_unix_secs: 1,
            completed: true,
            message: "hardware profile applied".to_owned(),
            results: vec![HardwareProfileApplyActionResult {
                action_id: "curve_optimizer_all_core".to_owned(),
                result: WriteExecutionResult::applied(plan.clone(), "applied", None),
            }],
        };
        assert_eq!(
            format_hardware_profile_apply_run_summary(Some(&applied)),
            "co_test completed; 1 action applied"
        );

        let stopped = HardwareProfileApplyRun {
            completed: false,
            message: "hardware profile apply stopped after first non-applied action".to_owned(),
            results: vec![HardwareProfileApplyActionResult {
                action_id: "curve_optimizer_all_core".to_owned(),
                result: WriteExecutionResult::blocked_by_policy(
                    plan,
                    "Curve Optimizer writes are disabled by daemon policy",
                ),
            }],
            ..applied
        };
        assert_eq!(
            format_hardware_profile_apply_run_summary(Some(&stopped)),
            "co_test stopped at curve_optimizer_all_core: blocked_by_policy - Curve Optimizer writes are disabled by daemon policy"
        );
    }

    #[test]
    fn hardware_profile_trigger_ids_are_allowlisted() {
        assert!(validate_hardware_profile_trigger_id("ac_connected").is_ok());
        assert!(validate_hardware_profile_trigger_id("resume").is_ok());
        assert!(validate_hardware_profile_trigger_id("desktop_power_profile_changed").is_ok());
        assert!(validate_hardware_profile_trigger_id("gpu_mode_reboot_completed").is_ok());
        assert!(matches!(
            validate_hardware_profile_trigger_id("lid_open"),
            Err(ValidationError::BlockedChoice { .. })
        ));
    }

    #[test]
    fn ac_profile_router_automation_rule_validates_profiles_and_cooldown() {
        let rule = AutomationRule {
            schema_version: 1,
            label: "AC profile router".to_owned(),
            enabled: true,
            kind: AutomationRuleKind::AcProfileRouter {
                ac_profile_id: "plugged_in".to_owned(),
                battery_profile_id: "on_battery".to_owned(),
                cooldown_secs: 120,
            },
        };
        validate_automation_rule(&rule).unwrap();
        let encoded = serde_json::to_value(&rule).unwrap();
        assert_eq!(encoded["kind"], "ac_profile_router");

        let mut invalid = rule.clone();
        invalid.kind = AutomationRuleKind::AcProfileRouter {
            ac_profile_id: "plugged_in".to_owned(),
            battery_profile_id: "plugged_in".to_owned(),
            cooldown_secs: 120,
        };
        assert!(matches!(
            validate_automation_rule(&invalid),
            Err(ValidationError::BlockedChoice { .. })
        ));

        let mut invalid = rule;
        invalid.kind = AutomationRuleKind::AcProfileRouter {
            ac_profile_id: "plugged_in".to_owned(),
            battery_profile_id: "on_battery".to_owned(),
            cooldown_secs: 86_401,
        };
        assert!(matches!(
            validate_automation_rule(&invalid),
            Err(ValidationError::BlockedChoice { .. })
        ));
    }

    #[test]
    fn write_contracts_are_drafted_but_disabled() {
        let methods = WRITE_METHOD_CONTRACTS
            .iter()
            .map(|contract| contract.method)
            .collect::<Vec<_>>();

        assert_eq!(
            methods,
            [
                "SetPlatformProfile",
                "PrepareCustomThermalMode",
                "SetBatteryChargeType",
                "SetLedState",
                "SetKeyboardRgb",
                "SetIdeapadToggle",
                "SetGpuMode",
                "ApplyFanPreset",
                "RestoreAutoFan",
                "SetCpuGovernor",
                "SetCpuEpp",
                "SetFirmwareAttribute",
                "SetCpuBoost",
                "SetConservationMode",
                "SetAmdGpuDpmForceLevel",
                "SetCurveOptimizerAllCore",
                "SetGpuModeRuntime"
            ]
        );
        assert!(WRITE_METHOD_CONTRACTS
            .iter()
            .all(|contract| !contract.enabled));
    }

    #[test]
    fn write_contracts_require_polkit_validation_and_rollback() {
        for contract in WRITE_METHOD_CONTRACTS {
            assert!(contract.polkit_action.starts_with(DBUS_ACTION_PREFIX));
            assert!(matches!(
                contract.risk,
                RiskLevel::ReversibleWrite | RiskLevel::ExperimentalWrite
            ));
            assert!(!contract.preconditions.is_empty());
            assert!(!contract.validators.is_empty());
            assert!(!contract.rollback.is_empty());
            assert!(!contract.safety_notes.is_empty());
            assert!(contract
                .validators
                .iter()
                .any(|rule| rule.contains("read-back")));
            assert!(contract
                .rollback
                .iter()
                .any(|rule| rule.contains("restore")));
        }
    }

    #[test]
    fn platform_profile_validator_accepts_exact_runtime_choice() {
        let capability = PlatformProfileCapability {
            current: Some("balanced".to_owned()),
            choices: vec![
                "quiet".to_owned(),
                "balanced".to_owned(),
                "performance".to_owned(),
            ],
            path: "/sys/firmware/acpi/platform_profile".to_owned(),
            choices_path: "/sys/firmware/acpi/platform_profile_choices".to_owned(),
        };

        assert_eq!(
            validate_platform_profile_choice(Some(&capability), "performance"),
            Ok(())
        );
    }

    #[test]
    fn platform_profile_validator_rejects_missing_unsupported_and_blocked_choices() {
        let capability = PlatformProfileCapability {
            current: Some("balanced".to_owned()),
            choices: vec![
                "balanced".to_owned(),
                "custom".to_owned(),
                "extreme".to_owned(),
            ],
            path: "/sys/firmware/acpi/platform_profile".to_owned(),
            choices_path: "/sys/firmware/acpi/platform_profile_choices".to_owned(),
        };
        let missing_current = PlatformProfileCapability {
            current: None,
            choices: vec!["balanced".to_owned()],
            path: "/sys/firmware/acpi/platform_profile".to_owned(),
            choices_path: "/sys/firmware/acpi/platform_profile_choices".to_owned(),
        };
        let missing_choices = PlatformProfileCapability {
            current: Some("balanced".to_owned()),
            choices: vec![],
            path: "/sys/firmware/acpi/platform_profile".to_owned(),
            choices_path: "/sys/firmware/acpi/platform_profile_choices".to_owned(),
        };

        assert!(matches!(
            validate_platform_profile_choice(None, "balanced"),
            Err(ValidationError::MissingCapability { .. })
        ));
        assert!(matches!(
            validate_platform_profile_choice(Some(&missing_current), "balanced"),
            Err(ValidationError::MissingCurrentValue { .. })
        ));
        assert!(matches!(
            validate_platform_profile_choice(Some(&missing_choices), "balanced"),
            Err(ValidationError::NoChoicesDetected { .. })
        ));
        assert!(matches!(
            validate_platform_profile_choice(Some(&capability), " balance "),
            Err(ValidationError::UnsupportedChoice { .. })
        ));
        assert!(matches!(
            validate_platform_profile_choice(Some(&capability), "custom"),
            Err(ValidationError::BlockedChoice { .. })
        ));
        assert!(matches!(
            validate_platform_profile_choice(Some(&capability), "extreme"),
            Err(ValidationError::BlockedChoice { .. })
        ));
    }

    #[test]
    fn battery_charge_type_validator_accepts_exact_runtime_choice() {
        let capability = BatteryChargeTypeCapability {
            current: Some("Standard".to_owned()),
            choices: vec![
                "Fast".to_owned(),
                "Standard".to_owned(),
                "Long_Life".to_owned(),
            ],
            path: "/sys/class/power_supply/BAT0/charge_type".to_owned(),
            choices_path: "/sys/class/power_supply/BAT0/charge_types".to_owned(),
        };

        assert_eq!(
            validate_battery_charge_type_choice(Some(&capability), "Long_Life"),
            Ok(())
        );
    }

    #[test]
    fn led_state_validator_accepts_supported_binary_ylogo() {
        assert_eq!(
            validate_led_state_request(
                &[LedCapability {
                    name: "platform::ylogo".to_owned(),
                    path: "/tmp/platform::ylogo/brightness".to_owned(),
                    brightness: Some(1),
                    max_brightness: Some(1),
                }],
                "platform::ylogo"
            ),
            Ok(())
        );
    }

    #[test]
    fn led_state_validator_rejects_unknown_non_binary_and_blocked_leds() {
        assert!(matches!(
            validate_led_state_request(&[], "platform::ylogo"),
            Err(ValidationError::MissingCapability { .. })
        ));
        assert!(matches!(
            validate_led_state_request(
                &[LedCapability {
                    name: "platform::ylogo".to_owned(),
                    path: "/tmp/platform::ylogo/brightness".to_owned(),
                    brightness: Some(2),
                    max_brightness: Some(2),
                }],
                "platform::ylogo"
            ),
            Err(ValidationError::BlockedChoice { .. })
        ));
        assert!(matches!(
            validate_led_state_request(
                &[LedCapability {
                    name: "platform::fnlock".to_owned(),
                    path: "/tmp/platform::fnlock/brightness".to_owned(),
                    brightness: Some(0),
                    max_brightness: Some(1),
                }],
                "platform::fnlock"
            ),
            Err(ValidationError::BlockedChoice { .. })
        ));
    }

    #[test]
    fn keyboard_rgb_validator_accepts_supported_effect_zones_and_colors() {
        let capability = keyboard_rgb_capability();
        let request = KeyboardRgbWriteRequest {
            effect: "static".to_owned(),
            colors: BTreeMap::from([
                ("left".to_owned(), "#ff0000".to_owned()),
                ("right".to_owned(), "#00AAff".to_owned()),
            ]),
            brightness: 80,
            speed: Some(40),
        };

        assert_eq!(
            validate_keyboard_rgb_request(Some(&capability), &request),
            Ok(())
        );
    }

    #[test]
    fn keyboard_rgb_validator_rejects_missing_and_unsupported_requests() {
        let capability = keyboard_rgb_capability();
        let valid = KeyboardRgbWriteRequest {
            effect: "static".to_owned(),
            colors: BTreeMap::from([("left".to_owned(), "#ffffff".to_owned())]),
            brightness: 80,
            speed: Some(40),
        };

        assert!(matches!(
            validate_keyboard_rgb_request(None, &valid),
            Err(ValidationError::MissingCapability { .. })
        ));
        assert!(matches!(
            validate_keyboard_rgb_request(
                Some(&capability),
                &KeyboardRgbWriteRequest {
                    effect: "rain".to_owned(),
                    ..valid.clone()
                }
            ),
            Err(ValidationError::UnsupportedChoice { .. })
        ));
        assert!(matches!(
            validate_keyboard_rgb_request(
                Some(&capability),
                &KeyboardRgbWriteRequest {
                    colors: BTreeMap::from([("center".to_owned(), "#ffffff".to_owned())]),
                    ..valid.clone()
                }
            ),
            Err(ValidationError::UnsupportedChoice { .. })
        ));
        assert!(matches!(
            validate_keyboard_rgb_request(
                Some(&capability),
                &KeyboardRgbWriteRequest {
                    colors: BTreeMap::from([("left".to_owned(), "ffffff".to_owned())]),
                    ..valid.clone()
                }
            ),
            Err(ValidationError::BlockedChoice { .. })
        ));
        assert!(matches!(
            validate_keyboard_rgb_request(
                Some(&capability),
                &KeyboardRgbWriteRequest {
                    brightness: 101,
                    ..valid.clone()
                }
            ),
            Err(ValidationError::UnsupportedChoice { .. })
        ));
        assert!(matches!(
            validate_keyboard_rgb_request(
                Some(&capability),
                &KeyboardRgbWriteRequest {
                    speed: Some(101),
                    ..valid
                }
            ),
            Err(ValidationError::UnsupportedChoice { .. })
        ));
    }

    #[test]
    fn keyboard_rgb_plan_records_previous_and_requested_state() {
        let capability = keyboard_rgb_capability();
        let request = KeyboardRgbWriteRequest {
            effect: "breath".to_owned(),
            colors: BTreeMap::from([
                ("left".to_owned(), "#112233".to_owned()),
                ("right".to_owned(), "#445566".to_owned()),
            ]),
            brightness: 75,
            speed: Some(30),
        };

        let plan = plan_keyboard_rgb_write(Some(&capability), &request).unwrap();

        assert_eq!(plan.method, "SetKeyboardRgb");
        assert_eq!(plan.capability_id, "keyboard_rgb");
        assert_eq!(
            plan.polkit_action,
            "org.ratvantage.LegionControl1.set-keyboard-rgb"
        );
        assert_eq!(plan.path, "hidraw:legion-test");
        assert!(plan
            .previous_value
            .contains("colors=left:#111111,right:#222222"));
        assert!(plan
            .requested_value
            .contains("colors=left:#112233,right:#445566"));
        assert_eq!(plan.rollback_value, plan.previous_value);
        assert!(plan.readback_required);
        assert!(plan
            .safety_notes
            .iter()
            .any(|note| note.contains("plan-only")));
    }

    #[test]
    fn openrgb_keyboard_rgb_bridge_plan_builds_command_preview() {
        let status = keyboard_rgb_openrgb_status();
        let request = KeyboardRgbWriteRequest {
            effect: "Breathing".to_owned(),
            colors: BTreeMap::from([
                ("left_side".to_owned(), "#ff0000".to_owned()),
                ("left_center".to_owned(), "#00ff00".to_owned()),
                ("right_center".to_owned(), "#0000ff".to_owned()),
                ("right_side".to_owned(), "#ffffff".to_owned()),
            ]),
            brightness: 75,
            speed: Some(30),
        };

        let plan = plan_openrgb_keyboard_rgb_bridge(Some(&status), &request).unwrap();

        assert_eq!(plan.method, "PlanOpenRgbKeyboardRgbBridge");
        assert_eq!(plan.capability_id, "keyboard_rgb_openrgb");
        assert_eq!(plan.path, "openrgb:/usr/bin/openrgb");
        assert!(plan.readback_required);
        assert!(plan.requested_value.contains(
            "command=openrgb --device 'Lenovo 5 2023' --mode Breathing --brightness 75 --speed 30 --color FF0000,00FF00,0000FF,FFFFFF"
        ));
        assert!(plan
            .rollback_instructions
            .iter()
            .any(|step| step.contains("save current OpenRGB profile")));
        assert!(plan
            .safety_notes
            .iter()
            .any(|note| note.contains("dry-run only")));
    }

    #[test]
    fn openrgb_keyboard_rgb_sdk_write_plan_builds_sdk_backend_preview() {
        let status = keyboard_rgb_openrgb_status();
        let request = KeyboardRgbWriteRequest {
            effect: "Breathing".to_owned(),
            colors: BTreeMap::from([
                ("left_side".to_owned(), "#ff0000".to_owned()),
                ("left_center".to_owned(), "#00ff00".to_owned()),
                ("right_center".to_owned(), "#0000ff".to_owned()),
                ("right_side".to_owned(), "#ffffff".to_owned()),
            ]),
            brightness: 75,
            speed: Some(30),
        };

        let plan = plan_openrgb_keyboard_rgb_sdk_write(Some(&status), &request).unwrap();

        assert_eq!(plan.method, "SetOpenRgbKeyboardRgbSdk");
        assert_eq!(plan.capability_id, "keyboard_rgb_openrgb:sdk");
        assert_eq!(plan.path, "openrgb-sdk:/usr/bin/openrgb");
        assert!(plan.readback_required);
        assert!(plan
            .requested_value
            .contains("sdk_packet=RGBCONTROLLER_UPDATELEDS"));
        assert!(plan
            .requested_value
            .contains("colors=FF0000,00FF00,0000FF,FFFFFF"));
        assert!(plan
            .rollback_instructions
            .iter()
            .any(|step| step.contains("before-snapshot colors")));
        assert!(plan.safety_notes.iter().any(|note| {
            note.contains("daemon must not write hidraw, i2c, sysfs, WMI, or EC directly")
        }));
    }

    #[test]
    fn hardware_profile_keyboard_rgb_plan_uses_native_backend_when_available() {
        let capability = keyboard_rgb_capability();
        let status = keyboard_rgb_openrgb_status();
        let request = KeyboardRgbWriteRequest {
            effect: "breath".to_owned(),
            colors: BTreeMap::from([
                ("left".to_owned(), "#333333".to_owned()),
                ("right".to_owned(), "#444444".to_owned()),
            ]),
            brightness: 80,
            speed: Some(30),
        };

        let plan =
            plan_hardware_profile_keyboard_rgb_write(Some(&capability), Some(&status), &request)
                .unwrap();

        assert_eq!(plan.method, "SetKeyboardRgb");
        assert_eq!(plan.capability_id, "keyboard_rgb");
    }

    #[test]
    fn hardware_profile_keyboard_rgb_plan_falls_back_to_openrgb_sdk_write() {
        let status = keyboard_rgb_openrgb_status();
        let request = KeyboardRgbWriteRequest {
            effect: "Breathing".to_owned(),
            colors: BTreeMap::from([
                ("left_side".to_owned(), "#ff0000".to_owned()),
                ("left_center".to_owned(), "#00ff00".to_owned()),
                ("right_center".to_owned(), "#0000ff".to_owned()),
                ("right_side".to_owned(), "#ffffff".to_owned()),
            ]),
            brightness: 75,
            speed: Some(30),
        };

        let plan = plan_hardware_profile_keyboard_rgb_write(None, Some(&status), &request).unwrap();

        assert_eq!(plan.method, "SetOpenRgbKeyboardRgbSdk");
        assert_eq!(plan.capability_id, "keyboard_rgb_openrgb:sdk");
        assert!(plan.requested_value.contains("mode=Breathing"));
        assert!(plan
            .safety_notes
            .iter()
            .any(|note| note.contains("process-boundary SDK writer")));
    }

    #[test]
    fn openrgb_keyboard_rgb_bridge_plan_rejects_unsupported_inputs() {
        let status = keyboard_rgb_openrgb_status();
        let valid = KeyboardRgbWriteRequest {
            effect: "Breathing".to_owned(),
            colors: BTreeMap::from([
                ("left_side".to_owned(), "#ff0000".to_owned()),
                ("left_center".to_owned(), "#00ff00".to_owned()),
                ("right_center".to_owned(), "#0000ff".to_owned()),
                ("right_side".to_owned(), "#ffffff".to_owned()),
            ]),
            brightness: 75,
            speed: Some(30),
        };

        assert!(matches!(
            plan_openrgb_keyboard_rgb_bridge(
                Some(&status),
                &KeyboardRgbWriteRequest {
                    effect: "Twinkle".to_owned(),
                    ..valid.clone()
                }
            ),
            Err(ValidationError::UnsupportedChoice { capability_id, .. })
                if capability_id == "keyboard_rgb_openrgb:mode"
        ));
        assert!(matches!(
            plan_openrgb_keyboard_rgb_bridge(
                Some(&status),
                &KeyboardRgbWriteRequest {
                    brightness: 101,
                    ..valid.clone()
                }
            ),
            Err(ValidationError::UnsupportedChoice { capability_id, .. })
                if capability_id == "keyboard_rgb_openrgb:brightness"
        ));
        assert!(matches!(
            plan_openrgb_keyboard_rgb_bridge(
                Some(&status),
                &KeyboardRgbWriteRequest {
                    colors: BTreeMap::from([
                        ("left_side".to_owned(), "#ff0000".to_owned()),
                        ("left_center".to_owned(), "#00ff00".to_owned()),
                    ]),
                    ..valid.clone()
                }
            ),
            Err(ValidationError::MissingCapability { capability_id })
                if capability_id == "keyboard_rgb_openrgb:zone:right_center"
        ));
        assert!(matches!(
            plan_openrgb_keyboard_rgb_bridge(
                Some(&status),
                &KeyboardRgbWriteRequest {
                    colors: BTreeMap::from([
                        ("left_side".to_owned(), "red".to_owned()),
                        ("left_center".to_owned(), "#00ff00".to_owned()),
                        ("right_center".to_owned(), "#0000ff".to_owned()),
                        ("right_side".to_owned(), "#ffffff".to_owned()),
                    ]),
                    ..valid
                }
            ),
            Err(ValidationError::BlockedChoice { capability_id, .. })
                if capability_id == "keyboard_rgb_openrgb:color"
        ));
    }

    #[test]
    fn ideapad_toggle_validator_accepts_fn_lock_with_paired_indicator_led() {
        assert_eq!(
            validate_ideapad_toggle_request(
                &[IdeapadToggleCapability {
                    name: "fn_lock".to_owned(),
                    status: CapabilityStatus::ProbeOnly,
                    path: Some("/tmp/fn_lock".to_owned()),
                    current_value: Some("1".to_owned()),
                }],
                &[LedCapability {
                    name: "platform::fnlock".to_owned(),
                    path: "/tmp/platform::fnlock/brightness".to_owned(),
                    brightness: Some(1),
                    max_brightness: Some(1),
                }],
                "fn_lock"
            ),
            Ok(())
        );
    }

    #[test]
    fn ideapad_toggle_validator_accepts_camera_power_without_indicator_led() {
        assert_eq!(
            validate_ideapad_toggle_request(
                &[IdeapadToggleCapability {
                    name: "camera_power".to_owned(),
                    status: CapabilityStatus::ProbeOnly,
                    path: Some("/tmp/camera_power".to_owned()),
                    current_value: Some("1".to_owned()),
                }],
                &[],
                "camera_power"
            ),
            Ok(())
        );
    }

    #[test]
    fn ideapad_toggle_validator_accepts_usb_charging_without_indicator_led() {
        assert_eq!(
            validate_ideapad_toggle_request(
                &[IdeapadToggleCapability {
                    name: "usb_charging".to_owned(),
                    status: CapabilityStatus::ProbeOnly,
                    path: Some("/tmp/usb_charging".to_owned()),
                    current_value: Some("0".to_owned()),
                }],
                &[],
                "usb_charging"
            ),
            Ok(())
        );
    }

    #[test]
    fn ideapad_toggle_validator_rejects_missing_non_binary_and_blocked_toggles() {
        assert!(matches!(
            validate_ideapad_toggle_request(&[], &[], "fn_lock"),
            Err(ValidationError::MissingCapability { .. })
        ));
        assert!(matches!(
            validate_ideapad_toggle_request(
                &[IdeapadToggleCapability {
                    name: "camera_power".to_owned(),
                    status: CapabilityStatus::ProbeOnly,
                    path: Some("/tmp/camera_power".to_owned()),
                    current_value: Some("1".to_owned()),
                }],
                &[],
                "camera_power"
            ),
            Ok(())
        ));
        assert!(matches!(
            validate_ideapad_toggle_request(
                &[IdeapadToggleCapability {
                    name: "usb_charging".to_owned(),
                    status: CapabilityStatus::ProbeOnly,
                    path: Some("/tmp/usb_charging".to_owned()),
                    current_value: Some("1".to_owned()),
                }],
                &[],
                "usb_charging"
            ),
            Ok(())
        ));
        assert!(matches!(
            validate_ideapad_toggle_request(
                &[IdeapadToggleCapability {
                    name: "conservation_mode".to_owned(),
                    status: CapabilityStatus::ProbeOnly,
                    path: Some("/tmp/conservation_mode".to_owned()),
                    current_value: Some("1".to_owned()),
                }],
                &[],
                "conservation_mode"
            ),
            Err(ValidationError::BlockedChoice { .. })
        ));
        assert!(matches!(
            validate_ideapad_toggle_request(
                &[IdeapadToggleCapability {
                    name: "fan_mode".to_owned(),
                    status: CapabilityStatus::ProbeOnly,
                    path: Some("/tmp/fan_mode".to_owned()),
                    current_value: Some("1".to_owned()),
                }],
                &[],
                "fan_mode"
            ),
            Ok(())
        ));
        assert!(matches!(
            validate_ideapad_toggle_request(
                &[IdeapadToggleCapability {
                    name: "touchpad".to_owned(),
                    status: CapabilityStatus::ProbeOnly,
                    path: Some("/tmp/touchpad".to_owned()),
                    current_value: Some("1".to_owned()),
                }],
                &[],
                "touchpad"
            ),
            Err(ValidationError::BlockedChoice { .. })
        ));
        assert!(matches!(
            validate_ideapad_toggle_request(
                &[IdeapadToggleCapability {
                    name: "fn_lock".to_owned(),
                    status: CapabilityStatus::ProbeOnly,
                    path: Some("/tmp/fn_lock".to_owned()),
                    current_value: Some("2".to_owned()),
                }],
                &[LedCapability {
                    name: "platform::fnlock".to_owned(),
                    path: "/tmp/platform::fnlock/brightness".to_owned(),
                    brightness: Some(1),
                    max_brightness: Some(1),
                }],
                "fn_lock"
            ),
            Err(ValidationError::BlockedChoice { .. })
        ));
        assert!(matches!(
            validate_ideapad_toggle_request(
                &[IdeapadToggleCapability {
                    name: "fn_lock".to_owned(),
                    status: CapabilityStatus::ProbeOnly,
                    path: Some("/tmp/fn_lock".to_owned()),
                    current_value: Some("0".to_owned()),
                }],
                &[LedCapability {
                    name: "platform::fnlock".to_owned(),
                    path: "/tmp/platform::fnlock/brightness".to_owned(),
                    brightness: Some(1),
                    max_brightness: Some(1),
                }],
                "fn_lock"
            ),
            Err(ValidationError::BlockedChoice { .. })
        ));
    }

    #[test]
    fn battery_charge_type_validator_rejects_empty_missing_and_non_exact_choices() {
        let capability = BatteryChargeTypeCapability {
            current: Some("Standard".to_owned()),
            choices: vec!["Fast".to_owned(), "Standard".to_owned()],
            path: "/sys/class/power_supply/BAT0/charge_type".to_owned(),
            choices_path: "/sys/class/power_supply/BAT0/charge_types".to_owned(),
        };
        let missing_current = BatteryChargeTypeCapability {
            current: None,
            choices: vec!["Standard".to_owned()],
            path: "/sys/class/power_supply/BAT0/charge_type".to_owned(),
            choices_path: "/sys/class/power_supply/BAT0/charge_types".to_owned(),
        };
        let missing_choices = BatteryChargeTypeCapability {
            current: Some("Standard".to_owned()),
            choices: vec![],
            path: "/sys/class/power_supply/BAT0/charge_type".to_owned(),
            choices_path: "/sys/class/power_supply/BAT0/charge_types".to_owned(),
        };

        assert!(matches!(
            validate_battery_charge_type_choice(Some(&capability), ""),
            Err(ValidationError::EmptyValue { .. })
        ));
        assert!(matches!(
            validate_battery_charge_type_choice(None, "Standard"),
            Err(ValidationError::MissingCapability { .. })
        ));
        assert!(matches!(
            validate_battery_charge_type_choice(Some(&missing_current), "Standard"),
            Err(ValidationError::MissingCurrentValue { .. })
        ));
        assert!(matches!(
            validate_battery_charge_type_choice(Some(&missing_choices), "Standard"),
            Err(ValidationError::NoChoicesDetected { .. })
        ));
        assert!(matches!(
            validate_battery_charge_type_choice(Some(&capability), "standard"),
            Err(ValidationError::UnsupportedChoice { .. })
        ));
    }

    #[test]
    fn gpu_mode_validator_accepts_exact_envycontrol_modes() {
        let capability = GpuCapability {
            provider: "envycontrol".to_owned(),
            status: CapabilityStatus::ProbeOnly,
            mode: Some("hybrid".to_owned()),
            switch_type: GpuSwitchType::RebootRequired,
            switch_notes: vec![],
        };

        assert_eq!(
            validate_gpu_mode_choice(Some(&capability), "integrated"),
            Ok(())
        );
        assert_eq!(
            validate_gpu_mode_choice(Some(&capability), "hybrid"),
            Ok(())
        );
        assert_eq!(
            validate_gpu_mode_choice(Some(&capability), "nvidia"),
            Ok(())
        );
    }

    #[test]
    fn gpu_switch_type_defaults_to_unknown_for_older_json() {
        let capability: GpuCapability = serde_json::from_value(serde_json::json!({
            "provider": "envycontrol",
            "status": "probe_only",
            "mode": "hybrid"
        }))
        .unwrap();

        assert_eq!(capability.switch_type, GpuSwitchType::Unknown);
        assert!(capability.switch_notes.is_empty());
        assert_eq!(
            format_gpu_switch_type(GpuSwitchType::RebootRequired),
            "reboot-required"
        );
        assert_eq!(
            format_gpu_switch_type(GpuSwitchType::SessionRestartRequired),
            "session-restart-required"
        );
        assert_eq!(
            format_gpu_switch_type(GpuSwitchType::RuntimeMux),
            "runtime-mux"
        );
    }

    #[test]
    fn gpu_runtime_capability_defaults_to_unpromoted_candidate() {
        let capability: GpuRuntimeCapability = serde_json::from_value(serde_json::json!({
            "status": "probe_only",
            "current_mode": "hybrid"
        }))
        .unwrap();

        assert!(capability.candidate_runtime_modes.is_empty());
        assert!(!capability.promotion_ready);
        assert!(capability.evidence.is_empty());
    }

    #[test]
    fn gpu_runtime_validator_blocks_until_promoted() {
        let capability = GpuRuntimeCapability {
            status: CapabilityStatus::ProbeOnly,
            current_mode: "hybrid".to_owned(),
            candidate_runtime_modes: vec!["integrated".to_owned()],
            promotion_ready: false,
            evidence: vec![
                "strict mux/session evidence has not promoted runtime switching".to_owned(),
            ],
        };

        assert!(matches!(
            validate_gpu_mode_runtime_choice(None, "integrated"),
            Err(ValidationError::MissingCapability { .. })
        ));
        assert!(matches!(
            validate_gpu_mode_runtime_choice(Some(&capability), "hybrid"),
            Err(ValidationError::BlockedChoice { .. })
        ));
        assert!(matches!(
            validate_gpu_mode_runtime_choice(Some(&capability), "nvidia"),
            Err(ValidationError::BlockedChoice { .. })
        ));
        assert!(matches!(
            validate_gpu_mode_runtime_choice(Some(&capability), "integrated"),
            Err(ValidationError::BlockedChoice { .. })
        ));
    }

    #[test]
    fn gpu_runtime_plan_requires_promotion_and_keeps_execution_disabled() {
        let capability = GpuRuntimeCapability {
            status: CapabilityStatus::ProbeOnly,
            current_mode: "hybrid".to_owned(),
            candidate_runtime_modes: vec!["integrated".to_owned()],
            promotion_ready: true,
            evidence: vec!["strict reviewer accepted live mux evidence".to_owned()],
        };

        let plan = plan_gpu_mode_runtime_write(Some(&capability), "integrated").unwrap();

        assert_eq!(plan.method, "SetGpuModeRuntime");
        assert_eq!(plan.capability_id, "gpu_runtime");
        assert_eq!(plan.path, "envycontrol+pci-hotswap-plan");
        assert_eq!(plan.previous_value, "hybrid");
        assert_eq!(plan.requested_value, "integrated");
        assert!(!plan.reboot_required);
        assert!(plan
            .safety_notes
            .iter()
            .any(|note| note.contains("execution is not implemented")));
        assert!(plan
            .safety_notes
            .iter()
            .any(|note| note.contains("strict reviewer accepted live mux evidence")));
    }

    #[test]
    fn gpu_mode_validator_rejects_missing_unsupported_and_non_exact_choices() {
        let capability = GpuCapability {
            provider: "envycontrol".to_owned(),
            status: CapabilityStatus::ProbeOnly,
            mode: Some("hybrid".to_owned()),
            switch_type: GpuSwitchType::RebootRequired,
            switch_notes: vec![],
        };
        let missing_current = GpuCapability {
            mode: None,
            ..capability.clone()
        };
        let unsupported_status = GpuCapability {
            status: CapabilityStatus::Unsupported,
            ..capability.clone()
        };
        let unsupported_provider = GpuCapability {
            provider: "other".to_owned(),
            ..capability.clone()
        };
        let session_restart_switch = GpuCapability {
            switch_type: GpuSwitchType::SessionRestartRequired,
            ..capability.clone()
        };
        let runtime_mux_switch = GpuCapability {
            switch_type: GpuSwitchType::RuntimeMux,
            ..capability.clone()
        };

        assert!(matches!(
            validate_gpu_mode_choice(None, "hybrid"),
            Err(ValidationError::MissingCapability { .. })
        ));
        assert!(matches!(
            validate_gpu_mode_choice(Some(&missing_current), "hybrid"),
            Err(ValidationError::MissingCurrentValue { .. })
        ));
        assert!(matches!(
            validate_gpu_mode_choice(Some(&unsupported_status), "hybrid"),
            Err(ValidationError::MissingCapability { .. })
        ));
        assert!(matches!(
            validate_gpu_mode_choice(Some(&unsupported_provider), "hybrid"),
            Err(ValidationError::BlockedChoice { .. })
        ));
        assert!(matches!(
            validate_gpu_mode_choice(Some(&session_restart_switch), "hybrid"),
            Err(ValidationError::BlockedChoice { .. })
        ));
        assert!(matches!(
            validate_gpu_mode_choice(Some(&runtime_mux_switch), "hybrid"),
            Err(ValidationError::BlockedChoice { .. })
        ));
        assert!(matches!(
            validate_gpu_mode_choice(Some(&capability), "Hybrid"),
            Err(ValidationError::UnsupportedChoice { .. })
        ));
        assert!(matches!(
            validate_gpu_mode_choice(Some(&capability), " nvidia "),
            Err(ValidationError::UnsupportedChoice { .. })
        ));
    }

    #[test]
    fn platform_profile_dry_run_plan_uses_validator_and_contract_metadata() {
        let capability = PlatformProfileCapability {
            current: Some("balanced".to_owned()),
            choices: vec!["quiet".to_owned(), "balanced".to_owned()],
            path: "/sys/firmware/acpi/platform_profile".to_owned(),
            choices_path: "/sys/firmware/acpi/platform_profile_choices".to_owned(),
        };

        let plan = plan_platform_profile_write(Some(&capability), "quiet").unwrap();

        assert_eq!(plan.method, "SetPlatformProfile");
        assert_eq!(plan.capability_id, "platform_profile");
        assert_eq!(
            plan.polkit_action,
            "org.ratvantage.LegionControl1.set-platform-profile"
        );
        assert_eq!(plan.previous_value, "balanced");
        assert_eq!(plan.requested_value, "quiet");
        assert_eq!(plan.rollback_value, "balanced");
        assert!(plan.readback_required);
        assert!(plan.steps.contains(&WritePlanStep::AuthorizeCaller));
        assert!(plan.steps.contains(&WritePlanStep::ReadBackValue));
        assert!(plan
            .steps
            .contains(&WritePlanStep::RestorePreviousOnReadbackFailure));
    }

    #[test]
    fn prepare_custom_thermal_mode_plan_allows_listed_custom_without_unblocking_normal_write() {
        let capability = platform_profile("balanced", &["balanced", "performance", "custom"]);

        assert!(matches!(
            plan_platform_profile_write(Some(&capability), "custom"),
            Err(ValidationError::BlockedChoice { .. })
        ));

        let plan = plan_prepare_custom_thermal_mode(Some(&capability)).unwrap();

        assert_eq!(plan.method, "PrepareCustomThermalMode");
        assert_eq!(plan.capability_id, "platform_profile");
        assert_eq!(
            plan.polkit_action,
            "org.ratvantage.LegionControl1.set-platform-profile"
        );
        assert_eq!(plan.previous_value, "balanced");
        assert_eq!(plan.requested_value, "custom");
        assert_eq!(plan.rollback_value, "balanced");
        assert!(plan.readback_required);
        assert!(!plan.reboot_required);
        assert!(plan
            .steps
            .contains(&WritePlanStep::RestorePreviousOnReadbackFailure));
        assert!(plan
            .safety_notes
            .iter()
            .any(|note| note.contains("custom thermal mode preparation required")));
    }

    #[test]
    fn prepare_custom_thermal_mode_plan_handles_active_and_missing_custom() {
        let active = platform_profile("custom", &["balanced", "performance", "custom"]);
        let plan = plan_prepare_custom_thermal_mode(Some(&active)).unwrap();
        assert_eq!(plan.previous_value, "custom");
        assert_eq!(plan.requested_value, "custom");
        assert!(plan
            .safety_notes
            .iter()
            .any(|note| note.contains("already active")));

        let unavailable = platform_profile("balanced", &["balanced", "performance"]);
        assert!(matches!(
            plan_prepare_custom_thermal_mode(Some(&unavailable)),
            Err(ValidationError::UnsupportedChoice { .. })
        ));
        assert!(matches!(
            plan_prepare_custom_thermal_mode(None),
            Err(ValidationError::MissingCapability { .. })
        ));
    }

    #[test]
    fn battery_charge_type_dry_run_plan_uses_validator_and_contract_metadata() {
        let capability = BatteryChargeTypeCapability {
            current: Some("Standard".to_owned()),
            choices: vec!["Standard".to_owned(), "Conservation".to_owned()],
            path: "/sys/class/power_supply/BAT0/charge_type".to_owned(),
            choices_path: "/sys/class/power_supply/BAT0/charge_types".to_owned(),
        };

        let plan = plan_battery_charge_type_write(Some(&capability), "Conservation").unwrap();

        assert_eq!(plan.method, "SetBatteryChargeType");
        assert_eq!(plan.capability_id, "battery_charge_type");
        assert_eq!(
            plan.polkit_action,
            "org.ratvantage.LegionControl1.set-battery-charge-type"
        );
        assert_eq!(plan.previous_value, "Standard");
        assert_eq!(plan.requested_value, "Conservation");
        assert_eq!(plan.rollback_value, "Standard");
        assert!(plan.readback_required);
        assert!(!plan.reboot_required);
    }

    #[test]
    fn led_state_dry_run_plan_uses_validator_and_contract_metadata() {
        let plan = plan_led_state_write(
            &[LedCapability {
                name: "platform::ylogo".to_owned(),
                path: "/tmp/platform::ylogo/brightness".to_owned(),
                brightness: Some(1),
                max_brightness: Some(1),
            }],
            "platform::ylogo",
            false,
        )
        .unwrap();

        assert_eq!(plan.method, "SetLedState");
        assert_eq!(plan.capability_id, "leds");
        assert_eq!(
            plan.polkit_action,
            "org.ratvantage.LegionControl1.set-led-state"
        );
        assert_eq!(plan.path, "/tmp/platform::ylogo/brightness");
        assert_eq!(plan.previous_value, "1");
        assert_eq!(plan.requested_value, "0");
        assert_eq!(plan.rollback_value, "1");
        assert!(plan.readback_required);
    }

    #[test]
    fn ideapad_toggle_dry_run_plan_uses_validator_and_contract_metadata() {
        let plan = plan_ideapad_toggle_write(
            &[IdeapadToggleCapability {
                name: "fn_lock".to_owned(),
                status: CapabilityStatus::ProbeOnly,
                path: Some("/tmp/fn_lock".to_owned()),
                current_value: Some("0".to_owned()),
            }],
            &[LedCapability {
                name: "platform::fnlock".to_owned(),
                path: "/tmp/platform::fnlock/brightness".to_owned(),
                brightness: Some(0),
                max_brightness: Some(1),
            }],
            "fn_lock",
            true,
        )
        .unwrap();

        assert_eq!(plan.method, "SetIdeapadToggle");
        assert_eq!(plan.capability_id, "ideapad_toggles");
        assert_eq!(
            plan.polkit_action,
            "org.ratvantage.LegionControl1.set-ideapad-toggle"
        );
        assert_eq!(plan.path, "/tmp/fn_lock");
        assert_eq!(plan.previous_value, "0");
        assert_eq!(plan.requested_value, "1");
        assert_eq!(plan.rollback_value, "0");
        assert!(plan.readback_required);
    }

    #[test]
    fn camera_power_dry_run_plan_uses_validator_and_contract_metadata() {
        let plan = plan_ideapad_toggle_write(
            &[IdeapadToggleCapability {
                name: "camera_power".to_owned(),
                status: CapabilityStatus::ProbeOnly,
                path: Some("/tmp/camera_power".to_owned()),
                current_value: Some("1".to_owned()),
            }],
            &[],
            "camera_power",
            false,
        )
        .unwrap();

        assert_eq!(plan.method, "SetIdeapadToggle");
        assert_eq!(plan.capability_id, "ideapad_toggles");
        assert_eq!(plan.path, "/tmp/camera_power");
        assert_eq!(plan.previous_value, "1");
        assert_eq!(plan.requested_value, "0");
        assert_eq!(plan.rollback_value, "1");
        assert!(plan
            .safety_notes
            .iter()
            .any(|note| note.contains("fn_lock, camera_power, usb_charging, and fan_mode")));
        assert!(plan.readback_required);
    }

    #[test]
    fn usb_charging_dry_run_plan_uses_validator_and_contract_metadata() {
        let plan = plan_ideapad_toggle_write(
            &[IdeapadToggleCapability {
                name: "usb_charging".to_owned(),
                status: CapabilityStatus::ProbeOnly,
                path: Some("/tmp/usb_charging".to_owned()),
                current_value: Some("0".to_owned()),
            }],
            &[],
            "usb_charging",
            true,
        )
        .unwrap();

        assert_eq!(plan.method, "SetIdeapadToggle");
        assert_eq!(plan.capability_id, "ideapad_toggles");
        assert_eq!(plan.path, "/tmp/usb_charging");
        assert_eq!(plan.previous_value, "0");
        assert_eq!(plan.requested_value, "1");
        assert_eq!(plan.rollback_value, "0");
        assert!(plan
            .safety_notes
            .iter()
            .any(|note| note.contains("usb_charging")));
        assert!(plan.readback_required);
    }

    #[test]
    fn gpu_mode_dry_run_plan_uses_validator_and_reboot_contract_metadata() {
        let capability = GpuCapability {
            provider: "envycontrol".to_owned(),
            status: CapabilityStatus::ProbeOnly,
            mode: Some("hybrid".to_owned()),
            switch_type: GpuSwitchType::RebootRequired,
            switch_notes: vec!["fixture classifies EnvyControl as reboot-required".to_owned()],
        };

        let plan = plan_gpu_mode_write(Some(&capability), "nvidia").unwrap();

        assert_eq!(plan.method, "SetGpuMode");
        assert_eq!(plan.capability_id, "gpu");
        assert_eq!(plan.path, "envycontrol");
        assert_eq!(plan.previous_value, "hybrid");
        assert_eq!(plan.requested_value, "nvidia");
        assert_eq!(plan.rollback_value, "hybrid");
        assert!(plan
            .rollback_instructions
            .iter()
            .any(|instruction| instruction.contains("previous GPU mode `hybrid`")));
        assert!(plan
            .rollback_instructions
            .iter()
            .any(|instruction| instruction.contains("TTY or rescue session")));
        assert!(plan.readback_required);
        assert!(plan.reboot_required);
        assert!(plan.steps.contains(&WritePlanStep::RequireReboot));
        assert_eq!(
            plan.polkit_action,
            "org.ratvantage.LegionControl1.set-gpu-mode"
        );
        assert!(plan
            .safety_notes
            .iter()
            .any(|note| note.contains("GPU switch type is reboot-required")));
        assert!(plan
            .safety_notes
            .iter()
            .any(|note| note.contains("fixture classifies EnvyControl")));
        assert!(plan
            .safety_notes
            .iter()
            .any(|note| note.contains("dry-run planning only")));
    }

    #[test]
    fn fan_preset_validator_accepts_packaged_shape_with_complete_curve() {
        let preset = fan_preset("balanced-daily");
        let curve = complete_fan_curve();

        assert_eq!(
            validate_fan_preset_choice(&[curve], &[preset], "balanced-daily"),
            Ok(())
        );
    }

    #[test]
    fn fan_preset_platform_profile_entry_requires_listed_profile_and_preset() {
        let presets = vec![fan_preset("balanced-daily")];
        let curve = complete_fan_curve();
        let capability = PlatformProfileCapability {
            current: Some("balanced".to_owned()),
            choices: vec!["balanced".to_owned(), "performance".to_owned()],
            path: "/tmp/platform_profile".to_owned(),
            choices_path: "/tmp/platform_profile_choices".to_owned(),
        };
        assert!(validate_fan_preset_platform_profile_entry(
            Some(&capability),
            std::slice::from_ref(&curve),
            &presets,
            "balanced",
            "balanced-daily",
        )
        .is_ok());
        assert!(validate_fan_preset_platform_profile_entry(
            Some(&capability),
            std::slice::from_ref(&curve),
            &presets,
            "unknown-profile",
            "balanced-daily",
        )
        .is_err());
        assert!(validate_fan_preset_platform_profile_entry(
            Some(&capability),
            &[curve],
            &presets,
            "balanced",
            "not-a-packaged-preset",
        )
        .is_err());
    }

    #[test]
    fn fan_preset_validator_rejects_missing_invalid_and_incomplete_curve() {
        let preset = fan_preset("balanced-daily");
        let short_curve = FanCurveCapability {
            point_paths: vec![
                "/sys/class/hwmon/hwmon7/pwm1_auto_point1_temp".to_owned(),
                "/sys/class/hwmon/hwmon7/pwm1_auto_point1_pwm".to_owned(),
            ],
            ..complete_fan_curve()
        };
        let bad_preset = FanPreset {
            points: vec![FanPresetPoint {
                temperature_c: 35,
                pwm: 10,
            }],
            ..preset.clone()
        };

        assert!(matches!(
            validate_fan_preset_choice(&[], std::slice::from_ref(&preset), "balanced-daily"),
            Err(ValidationError::MissingCapability { .. })
        ));
        assert!(matches!(
            validate_fan_preset_choice(
                &[complete_fan_curve()],
                std::slice::from_ref(&preset),
                "unknown"
            ),
            Err(ValidationError::UnsupportedChoice { .. })
        ));
        assert!(matches!(
            validate_fan_preset_choice(&[complete_fan_curve()], &[bad_preset], "balanced-daily"),
            Err(ValidationError::BlockedChoice { .. })
        ));
        assert!(matches!(
            validate_fan_preset_choice(&[short_curve], &[preset], "balanced-daily"),
            Err(ValidationError::BlockedChoice { .. })
        ));
    }

    #[test]
    fn fan_curve_hwmon_point_pairs_extracts_sorted_indices() {
        let curve = complete_fan_curve();
        let pairs = fan_curve_hwmon_point_pairs(&curve);
        assert_eq!(pairs.len(), 10);
        assert_eq!(pairs[0].index, 1);
        assert!(pairs[0].temp_path.ends_with("pwm1_auto_point1_temp"));
        assert!(pairs[0].pwm_path.ends_with("pwm1_auto_point1_pwm"));
        assert_eq!(pairs[9].index, 10);
    }

    #[test]
    fn fan_curve_hwmon_point_pairs_handles_single_point_fixture() {
        let curve = FanCurveCapability {
            point_paths: vec![
                "/tmp/hwmon7/pwm1_auto_point1_temp".to_owned(),
                "/tmp/hwmon7/pwm1_auto_point1_pwm".to_owned(),
            ],
            ..complete_fan_curve()
        };
        let pairs = fan_curve_hwmon_point_pairs(&curve);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].index, 1);
    }

    #[test]
    fn fan_curve_snapshot_chart_pairs_aligns_temp_and_pwm_paths() {
        let curve = FanCurveCapability {
            point_paths: vec![
                "/tmp/hwmon7/pwm1_auto_point1_temp".to_owned(),
                "/tmp/hwmon7/pwm1_auto_point1_pwm".to_owned(),
            ],
            ..complete_fan_curve()
        };
        let snapshot = FanCurveSnapshot {
            curve_id: "x".to_owned(),
            path: None,
            points: vec![
                FanCurvePointSnapshot {
                    path: "/tmp/hwmon7/pwm1_auto_point1_temp".to_owned(),
                    value: " 50000 ".to_owned(),
                },
                FanCurvePointSnapshot {
                    path: "/tmp/hwmon7/pwm1_auto_point1_pwm".to_owned(),
                    value: "120".to_owned(),
                },
            ],
        };
        let pairs = fan_curve_snapshot_chart_pairs(&curve, &snapshot);
        assert_eq!(pairs, vec![(50000, 120)]);
    }

    #[test]
    fn validate_manual_fan_curve_pairs_accepts_monotonic_sysfs_integers() {
        let pairs: Vec<(u32, u32)> = (1..=10).map(|i| (35_000 + i * 1_000, 20 + i)).collect();
        assert_eq!(validate_manual_fan_curve_pairs(&pairs), Ok(()));
    }

    #[test]
    fn format_manual_fan_scratchpad_sysfs_preview_lists_paths() {
        let curve = complete_fan_curve();
        let hw = fan_curve_hwmon_point_pairs(&curve);
        assert!(!hw.is_empty());
        let temps_pwms: Vec<(u32, u32)> = hw
            .iter()
            .enumerate()
            .map(|(i, _)| (35_000 + (i as u32) * 5_000, 30 + i as u32))
            .collect();
        let out = format_manual_fan_scratchpad_sysfs_preview(&hw, &temps_pwms).unwrap();
        assert!(out.contains("Validated scratchpad:"));
        assert!(out.contains("preview only"));
        for pair in &hw {
            assert!(out.contains(&pair.temp_path));
            assert!(out.contains(&pair.pwm_path));
        }
    }

    #[test]
    fn format_fan_curve_live_vs_saved_reports_matches_and_diffs() {
        let path = "/sys/class/hwmon/hwmon7/pwm1_auto_point1_temp";
        let live = FanCurveSnapshot {
            curve_id: "a".to_owned(),
            path: Some("/sys/class/hwmon/hwmon7".to_owned()),
            points: vec![
                FanCurvePointSnapshot {
                    path: path.to_owned(),
                    value: "42000".to_owned(),
                },
                FanCurvePointSnapshot {
                    path: "/sys/class/hwmon/hwmon7/pwm1_auto_point1_pwm".to_owned(),
                    value: "99".to_owned(),
                },
            ],
        };
        let saved_same = FanCurveSnapshot {
            curve_id: "a".to_owned(),
            path: Some("/sys/class/hwmon/hwmon7".to_owned()),
            points: live.points.clone(),
        };
        let report = format_fan_curve_live_vs_saved(&live, &saved_same);
        assert!(report.contains("2 path(s) with identical values"));
        assert!(report.contains("0 differing"));

        let saved_diff = FanCurveSnapshot {
            curve_id: "b".to_owned(),
            path: Some("/other".to_owned()),
            points: vec![FanCurvePointSnapshot {
                path: path.to_owned(),
                value: "43000".to_owned(),
            }],
        };
        let report2 = format_fan_curve_live_vs_saved(&live, &saved_diff);
        assert!(report2.contains("curve_id differs"));
        assert!(report2.contains("hwmon root paths differ"));
        assert!(report2.contains("Differing values:"));
        assert!(report2.contains("1 only in live"));
        assert!(report2.contains("0 only in saved"));
        assert!(report2.contains("Paths present only in live snapshot:"));
    }

    #[test]
    fn fan_scratchpad_toml_v1_round_trips() {
        let pair = FanCurveHwmonPointPair {
            index: 1,
            temp_path: "/sys/hwmon/pwm1_auto_point1_temp".to_owned(),
            pwm_path: "/sys/hwmon/pwm1_auto_point1_pwm".to_owned(),
        };
        let encoded = encode_fan_scratchpad_toml_v1(&[(pair.clone(), 42_000, 30)]).unwrap();
        let decoded = decode_fan_scratchpad_toml_v1(&encoded).unwrap();
        assert_eq!(decoded.schema_version, 1);
        assert_eq!(decoded.pairs.len(), 1);
        assert_eq!(decoded.pairs[0].temp_raw, 42_000);
        assert_eq!(decoded.pairs[0].pwm_raw, 30);
        assert_eq!(decoded.pairs[0].temp_path, pair.temp_path);
    }

    #[test]
    fn fan_preset_points_as_sysfs_raw_scales_degrees_to_millicelsius() {
        let preset = fan_preset("balanced-daily");
        let raw = fan_preset_points_as_sysfs_raw(&preset).unwrap();
        assert_eq!(raw.len(), 10);
        assert_eq!(raw[0].0, 35_000);
        assert_eq!(raw[0].1, 10);
        assert_eq!(raw[9].0, 80_000);
        assert_eq!(raw[9].1, 190);
    }

    #[test]
    fn validate_manual_fan_curve_pairs_rejects_empty_pwm_range_and_order() {
        assert!(matches!(
            validate_manual_fan_curve_pairs(&[]),
            Err(ValidationError::BlockedChoice { .. })
        ));
        assert!(matches!(
            validate_manual_fan_curve_pairs(&[(40_000, 300)]),
            Err(ValidationError::BlockedChoice { .. })
        ));
        assert!(matches!(
            validate_manual_fan_curve_pairs(&[(50_000, 10), (40_000, 20)]),
            Err(ValidationError::BlockedChoice { .. })
        ));
        assert!(matches!(
            validate_manual_fan_curve_pairs(&[(40_000, 80), (50_000, 60)]),
            Err(ValidationError::BlockedChoice { .. })
        ));
    }

    #[test]
    fn fan_preset_dry_run_plan_uses_validator_and_contract_metadata() {
        let preset = fan_preset("balanced-daily");
        let curve = complete_fan_curve();

        let plan = plan_fan_preset_write(&[curve], &[preset], "balanced-daily").unwrap();

        assert_eq!(plan.method, "ApplyFanPreset");
        assert_eq!(plan.capability_id, "fan_curves");
        assert_eq!(plan.path, "/sys/class/hwmon/hwmon9");
        assert_eq!(plan.previous_value, "current fan curve snapshot");
        assert_eq!(plan.requested_value, "balanced-daily");
        assert_eq!(plan.rollback_value, "current fan curve snapshot");
        assert!(plan.readback_required);
        assert!(!plan.reboot_required);
        assert_eq!(
            plan.polkit_action,
            "org.ratvantage.LegionControl1.apply-fan-preset"
        );
        assert!(plan
            .safety_notes
            .iter()
            .any(|note| note.contains("Middle-ground fan ramp")));
    }

    #[test]
    fn fan_preset_dry_run_plan_notes_custom_thermal_prerequisite() {
        let preset = fan_preset("balanced-daily");
        let curve = complete_fan_curve();
        let platform = platform_profile("balanced", &["balanced", "performance", "custom"]);

        let plan = plan_fan_preset_write_with_platform_profile(
            &[curve],
            &[preset],
            Some(&platform),
            "balanced-daily",
        )
        .unwrap();

        assert!(plan.safety_notes.iter().any(
            |note| note.contains("custom thermal prerequisite required for fan preset writes")
        ));
        assert!(plan.rollback_instructions.iter().any(|instruction| {
            instruction.contains("restore previous platform_profile `balanced`")
        }));
    }

    #[test]
    fn restore_auto_fan_dry_run_plan_requires_detected_fan_curve() {
        let plan = plan_restore_auto_fan_write(&[complete_fan_curve()]).unwrap();

        assert_eq!(plan.method, "RestoreAutoFan");
        assert_eq!(plan.capability_id, "fan_curves");
        assert_eq!(plan.path, "/sys/class/hwmon/hwmon9");
        assert_eq!(plan.previous_value, "current fan-control state");
        assert_eq!(plan.requested_value, "auto/default fan control");
        assert_eq!(plan.rollback_value, "current fan-control state");
        assert!(plan.readback_required);
        assert!(!plan.reboot_required);
        assert!(plan
            .safety_notes
            .iter()
            .any(|note| note.contains("dry-run planning only")));

        assert!(matches!(
            plan_restore_auto_fan_write(&[]),
            Err(ValidationError::MissingCapability { .. })
        ));
    }

    #[test]
    fn dry_run_plans_reject_invalid_requests_before_planning() {
        let platform = PlatformProfileCapability {
            current: Some("balanced".to_owned()),
            choices: vec!["balanced".to_owned(), "custom".to_owned()],
            path: "/sys/firmware/acpi/platform_profile".to_owned(),
            choices_path: "/sys/firmware/acpi/platform_profile_choices".to_owned(),
        };
        let battery = BatteryChargeTypeCapability {
            current: Some("Standard".to_owned()),
            choices: vec!["Standard".to_owned()],
            path: "/sys/class/power_supply/BAT0/charge_type".to_owned(),
            choices_path: "/sys/class/power_supply/BAT0/charge_types".to_owned(),
        };

        assert!(matches!(
            plan_platform_profile_write(Some(&platform), "custom"),
            Err(ValidationError::BlockedChoice { .. })
        ));
        assert!(matches!(
            plan_battery_charge_type_write(Some(&battery), "Fast"),
            Err(ValidationError::UnsupportedChoice { .. })
        ));
        assert!(matches!(
            plan_gpu_mode_write(None, "hybrid"),
            Err(ValidationError::MissingCapability { .. })
        ));
        assert!(matches!(
            plan_fan_preset_write(&[], &[fan_preset("balanced-daily")], "balanced-daily"),
            Err(ValidationError::MissingCapability { .. })
        ));
    }

    #[test]
    fn firmware_attribute_validator_accepts_allowlisted_scalar_range() {
        let attrs = vec![FirmwareAttributeCapability {
            name: "ppt_pl1_spl".to_owned(),
            current_value: Some("70".to_owned()),
            display_name: Some("Set the CPU sustained power limit".to_owned()),
            path: "/sys/class/firmware-attributes/lenovo-wmi-other-0/attributes/ppt_pl1_spl"
                .to_owned(),
            attribute_type: Some("integer".to_owned()),
            default_value: Some("70".to_owned()),
            min_value: Some("50".to_owned()),
            max_value: Some("85".to_owned()),
            scalar_increment: Some("5".to_owned()),
        }];

        assert_eq!(
            validate_firmware_scalar_attribute_request(&attrs, "ppt_pl1_spl", "75"),
            Ok(())
        );
        assert!(matches!(
            validate_firmware_scalar_attribute_request(&attrs, "other", "75"),
            Err(ValidationError::BlockedChoice { .. })
        ));
        assert!(matches!(
            validate_firmware_scalar_attribute_request(&attrs, "ppt_pl1_spl", "90"),
            Err(ValidationError::UnsupportedChoice { .. })
        ));
        assert!(matches!(
            validate_firmware_scalar_attribute_request(&attrs, "ppt_pl1_spl", "76"),
            Err(ValidationError::BlockedChoice { .. })
        ));
        assert!(matches!(
            validate_firmware_scalar_attribute_request(&attrs, "ppt_pl1_spl", "fast"),
            Err(ValidationError::BlockedChoice { .. })
        ));
    }

    #[test]
    fn firmware_attribute_dry_run_plan_uses_validator_and_contract_metadata() {
        let attrs = vec![FirmwareAttributeCapability {
            name: "ppt_pl2_sppt".to_owned(),
            current_value: Some("85".to_owned()),
            display_name: None,
            path: "/sys/class/firmware-attributes/lenovo-wmi-other-0/attributes/ppt_pl2_sppt"
                .to_owned(),
            attribute_type: Some("integer".to_owned()),
            default_value: Some("90".to_owned()),
            min_value: Some("60".to_owned()),
            max_value: Some("130".to_owned()),
            scalar_increment: Some("1".to_owned()),
        }];

        let plan = plan_firmware_attribute_write(&attrs, "ppt_pl2_sppt", "90").unwrap();

        assert_eq!(plan.method, "SetFirmwareAttribute");
        assert_eq!(plan.capability_id, "firmware_attributes");
        assert_eq!(
            plan.polkit_action,
            "org.ratvantage.LegionControl1.set-firmware-attribute"
        );
        assert_eq!(
            plan.path,
            "/sys/class/firmware-attributes/lenovo-wmi-other-0/attributes/ppt_pl2_sppt/current_value"
        );
        assert_eq!(plan.previous_value, "85");
        assert_eq!(plan.requested_value, "90");
        assert_eq!(plan.rollback_value, "85");
        assert!(plan.readback_required);
    }

    #[test]
    fn firmware_attribute_reset_plan_requires_and_validates_default_value() {
        let attrs = vec![firmware_ppt_attribute_with_default("85", Some("90"))];
        let platform = platform_profile("custom", &["balanced", "performance", "custom"]);

        let plan = plan_firmware_attribute_reset_write_with_platform_profile(
            &attrs,
            Some(&platform),
            "ppt_pl2_sppt",
        )
        .unwrap();

        assert_eq!(plan.method, "SetFirmwareAttribute");
        assert_eq!(plan.previous_value, "85");
        assert_eq!(plan.requested_value, "90");
        assert!(plan.safety_notes.iter().any(|note| {
            note.contains("reset-to-default plan uses firmware default_value `90`")
        }));
        assert!(plan.safety_notes.iter().any(|note| {
            note.contains("custom thermal prerequisite satisfied for firmware PPT writes")
        }));

        let missing_default = vec![firmware_ppt_attribute_with_default("85", None)];
        assert!(matches!(
            plan_firmware_attribute_reset_write(&missing_default, "ppt_pl2_sppt"),
            Err(ValidationError::MissingCapability { .. })
        ));

        let invalid_default = vec![firmware_ppt_attribute_with_default("85", Some("999"))];
        assert!(matches!(
            plan_firmware_attribute_reset_write(&invalid_default, "ppt_pl2_sppt"),
            Err(ValidationError::UnsupportedChoice { .. })
        ));
    }

    #[test]
    fn firmware_ppt_preset_plans_validate_static_values_and_defaults() {
        let attrs = firmware_ppt_attributes_with_defaults();
        let platform = platform_profile("custom", &["balanced", "performance", "custom"]);

        let preset = firmware_ppt_preset(&attrs, "performance-custom").unwrap();
        assert_eq!(preset.id, "performance-custom");
        assert_eq!(preset.values["ppt_pl1_spl"], "80");
        assert_eq!(preset.values["ppt_pl2_sppt"], "115");
        assert_eq!(preset.values["ppt_pl3_fppt"], "135");

        let plans = plan_firmware_ppt_preset_write_with_platform_profile(
            &attrs,
            Some(&platform),
            &preset.id,
        )
        .unwrap();
        assert_eq!(plans.len(), 3);
        assert_eq!(plans[0].method, "SetFirmwareAttribute");
        assert_eq!(plans[0].requested_value, "80");
        assert_eq!(plans[1].requested_value, "115");
        assert_eq!(plans[2].requested_value, "135");
        assert!(plans
            .iter()
            .all(|plan| plan.safety_notes.iter().any(|note| {
                note.contains("custom thermal prerequisite satisfied for firmware PPT writes")
            })));
        assert!(plans
            .iter()
            .all(|plan| plan.safety_notes.iter().any(|note| {
                note.contains("performance custom PPT preset needs live thermal validation")
            })));

        let reset = firmware_ppt_preset(&attrs, "reset-defaults").unwrap();
        assert_eq!(reset.values["ppt_pl1_spl"], "70");
        assert_eq!(reset.values["ppt_pl2_sppt"], "85");
        assert_eq!(reset.values["ppt_pl3_fppt"], "102");
    }

    #[test]
    fn firmware_ppt_preset_rejects_bad_id_range_and_missing_defaults() {
        let attrs = firmware_ppt_attributes_with_defaults();

        assert!(matches!(
            firmware_ppt_preset(&attrs, "unknown"),
            Err(ValidationError::UnsupportedChoice { .. })
        ));

        let mut constrained = attrs.clone();
        constrained[0].max_value = Some("75".to_owned());
        assert!(matches!(
            firmware_ppt_preset(&constrained, "performance-custom"),
            Err(ValidationError::UnsupportedChoice { .. })
        ));

        let mut missing_default = attrs;
        missing_default[2].default_value = None;
        assert!(matches!(
            firmware_ppt_preset(&missing_default, "reset-defaults"),
            Err(ValidationError::MissingCapability { .. })
        ));
    }

    #[test]
    fn firmware_attribute_plan_notes_custom_thermal_prerequisite_states() {
        let attrs = vec![firmware_ppt_attribute("85")];

        let balanced = platform_profile("balanced", &["balanced", "performance", "custom"]);
        let plan = plan_firmware_attribute_write_with_platform_profile(
            &attrs,
            Some(&balanced),
            "ppt_pl2_sppt",
            "90",
        )
        .unwrap();
        assert!(plan
            .safety_notes
            .iter()
            .any(|note| note
                .contains("custom thermal prerequisite required for firmware PPT writes")));
        assert!(plan.rollback_instructions.iter().any(|instruction| {
            instruction.contains("restore previous platform_profile `balanced`")
        }));

        let custom = platform_profile("custom", &["balanced", "performance", "custom"]);
        let plan = plan_firmware_attribute_write_with_platform_profile(
            &attrs,
            Some(&custom),
            "ppt_pl2_sppt",
            "90",
        )
        .unwrap();
        assert!(plan
            .safety_notes
            .iter()
            .any(|note| note
                .contains("custom thermal prerequisite satisfied for firmware PPT writes")));

        let unavailable = platform_profile("balanced", &["balanced", "performance"]);
        let plan = plan_firmware_attribute_write_with_platform_profile(
            &attrs,
            Some(&unavailable),
            "ppt_pl2_sppt",
            "90",
        )
        .unwrap();
        assert!(plan
            .safety_notes
            .iter()
            .any(|note| note
                .contains("custom thermal prerequisite unavailable for firmware PPT writes")));
    }

    #[test]
    fn curve_optimizer_offset_encoding_matches_ryzenadj_u32_values() {
        assert_eq!(encode_curve_optimizer_offset(0).unwrap(), 0);
        assert_eq!(encode_curve_optimizer_offset(-10).unwrap(), 4_294_967_286);
        assert_eq!(encode_curve_optimizer_offset(-15).unwrap(), 4_294_967_281);
        assert_eq!(encode_curve_optimizer_offset(-20).unwrap(), 4_294_967_276);
        assert_eq!(encode_curve_optimizer_offset(-25).unwrap(), 4_294_967_271);
        assert_eq!(encode_curve_optimizer_offset(-30).unwrap(), 4_294_967_266);
        assert!(matches!(
            encode_curve_optimizer_offset(-40),
            Err(ValidationError::UnsupportedChoice { .. })
        ));
        assert!(matches!(
            validate_curve_optimizer_all_core_offset("fast"),
            Err(ValidationError::BlockedChoice { .. })
        ));
    }

    #[test]
    fn curve_optimizer_dry_run_plan_is_write_only_and_experimental() {
        let plan = plan_curve_optimizer_all_core_write("-20").unwrap();

        assert_eq!(plan.method, "SetCurveOptimizerAllCore");
        assert_eq!(plan.capability_id, "curve_optimizer_all_core");
        assert_eq!(plan.requested_value, "-20 (encoded 4294967276)");
        assert_eq!(plan.rollback_value, "0");
        assert!(!plan.readback_required);
        assert!(plan
            .safety_notes
            .iter()
            .any(|note| note.contains("read-back is unavailable")));
    }

    fn keyboard_rgb_capability() -> KeyboardRgbCapability {
        KeyboardRgbCapability {
            backend: "hidraw".to_owned(),
            device_id: "legion-test".to_owned(),
            path: "hidraw:legion-test".to_owned(),
            zones: vec![
                KeyboardRgbZone {
                    id: "left".to_owned(),
                    label: "Left".to_owned(),
                },
                KeyboardRgbZone {
                    id: "right".to_owned(),
                    label: "Right".to_owned(),
                },
            ],
            effects: vec!["static".to_owned(), "breath".to_owned()],
            current_effect: Some("static".to_owned()),
            current_colors: BTreeMap::from([
                ("left".to_owned(), "#111111".to_owned()),
                ("right".to_owned(), "#222222".to_owned()),
            ]),
            current_brightness: Some(50),
            min_brightness: 0,
            max_brightness: 100,
            current_speed: Some(20),
            min_speed: 0,
            max_speed: 100,
        }
    }

    fn keyboard_rgb_openrgb_status() -> KeyboardRgbOpenRgbStatus {
        KeyboardRgbOpenRgbStatus {
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
            sdk_helper_installed: true,
            sdk_helper_path: Some(
                "/home/test/.local/bin/ratvantage-openrgb-keyboard-rgb-sdk-helper".to_owned(),
            ),
            sdk_server_running: true,
            sdk_snapshot_supported: true,
            sdk_active_mode: Some("Direct".to_owned()),
            sdk_color_zones: vec![
                "left_center".to_owned(),
                "left_side".to_owned(),
                "right_center".to_owned(),
                "right_side".to_owned(),
            ],
            sdk_colors: BTreeMap::from([
                ("left_center".to_owned(), "#000000".to_owned()),
                ("left_side".to_owned(), "#000000".to_owned()),
                ("right_center".to_owned(), "#000000".to_owned()),
                ("right_side".to_owned(), "#000000".to_owned()),
            ]),
        }
    }

    fn fan_preset(id: &str) -> FanPreset {
        FanPreset {
            schema_version: 1,
            id: id.to_owned(),
            label: "Balanced daily".to_owned(),
            description: "General-purpose curve".to_owned(),
            target_profiles: vec!["balanced".to_owned()],
            safety_note:
                "Middle-ground fan ramp; daemon must write a complete validated curve only."
                    .to_owned(),
            points: (0..10)
                .map(|index| FanPresetPoint {
                    temperature_c: 35 + (index * 5),
                    pwm: 10 + (index as u16 * 20),
                })
                .collect(),
        }
    }

    fn platform_profile(current: &str, choices: &[&str]) -> PlatformProfileCapability {
        PlatformProfileCapability {
            current: Some(current.to_owned()),
            choices: choices.iter().map(|choice| (*choice).to_owned()).collect(),
            path: "/sys/firmware/acpi/platform_profile".to_owned(),
            choices_path: "/sys/firmware/acpi/platform_profile_choices".to_owned(),
        }
    }

    fn firmware_ppt_attribute(current_value: &str) -> FirmwareAttributeCapability {
        firmware_ppt_attribute_with_default(current_value, Some("90"))
    }

    fn firmware_ppt_attribute_with_default(
        current_value: &str,
        default_value: Option<&str>,
    ) -> FirmwareAttributeCapability {
        firmware_ppt_attribute_named("ppt_pl2_sppt", current_value, default_value, "60", "130")
    }

    fn firmware_ppt_attributes_with_defaults() -> Vec<FirmwareAttributeCapability> {
        vec![
            firmware_ppt_attribute_named("ppt_pl1_spl", "70", Some("70"), "50", "85"),
            firmware_ppt_attribute_named("ppt_pl2_sppt", "85", Some("85"), "60", "130"),
            firmware_ppt_attribute_named("ppt_pl3_fppt", "102", Some("102"), "70", "150"),
        ]
    }

    fn firmware_ppt_attribute_named(
        name: &str,
        current_value: &str,
        default_value: Option<&str>,
        min_value: &str,
        max_value: &str,
    ) -> FirmwareAttributeCapability {
        FirmwareAttributeCapability {
            name: name.to_owned(),
            current_value: Some(current_value.to_owned()),
            display_name: None,
            path: format!("/sys/class/firmware-attributes/lenovo-wmi-other-0/attributes/{name}"),
            attribute_type: Some("integer".to_owned()),
            default_value: default_value.map(ToOwned::to_owned),
            min_value: Some(min_value.to_owned()),
            max_value: Some(max_value.to_owned()),
            scalar_increment: Some("1".to_owned()),
        }
    }

    fn complete_fan_curve() -> FanCurveCapability {
        let mut point_paths = Vec::new();
        for point in 1..=10 {
            point_paths.push(format!(
                "/sys/class/hwmon/hwmon9/pwm1_auto_point{point}_temp"
            ));
            point_paths.push(format!(
                "/sys/class/hwmon/hwmon9/pwm1_auto_point{point}_pwm"
            ));
        }

        FanCurveCapability {
            id: "legion-hwmon".to_owned(),
            status: CapabilityStatus::ProbeOnly,
            path: Some("/sys/class/hwmon/hwmon9".to_owned()),
            point_paths,
        }
    }
}
