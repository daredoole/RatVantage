use std::collections::BTreeMap;
use std::process::Command;

use anyhow::Result;
use legion_common::{
    format_fan_curve_snapshot_summary, format_gpu_mode_pending_summary, format_gpu_switch_type,
    format_power_profiles_probe_summary, AutomationRule, AutomationRuleApplyRun,
    AutomationRuleEvaluation, AutomationRuleKind, Capability, CapabilityRegistry, CapabilityStatus,
    CurveOptimizerWriteState, CustomThermalPlanPreview, DesktopPowerProfileChangeEvent,
    FanCurveSnapshot, GpuModePending, GpuSwitchType, HardwareProfile,
    HardwareProfileApplyActionResult, HardwareProfileApplyPreview, HardwareProfileApplyRun,
    HardwareSummary, KeyboardRgbWriteRequest, PlatformProfileChangeEvent, RiskLevel,
    RyzenBackendStatus, TelemetrySnapshot, WriteDryRunPlan, WriteExecutionResult,
};
use serde::{de::DeserializeOwned, Serialize};
use zbus::blocking::{Connection, ConnectionBuilder, Proxy};

#[cfg(feature = "gtk-ui")]
pub mod gtk_shell;
#[cfg(feature = "gtk-ui")]
pub mod ui;

pub const DBUS_INTERFACE: &str = "org.ratvantage.LegionControl1";
pub const DBUS_PATH: &str = "/org/ratvantage/LegionControl1";

pub struct LegionControlClient {
    connection: Connection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSnapshot {
    pub status: UiStatus,
    pub diagnostics: DiagnosticsBundle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeRefreshNotice {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiStatus {
    pub hardware: UiHardwareStatus,
    pub capabilities: Vec<UiCapabilityStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiHardwareStatus {
    pub vendor: String,
    pub product_name: String,
    pub product_version: String,
    pub product_sku: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiCapabilityStatus {
    pub id: String,
    pub label: String,
    pub status: CapabilityStatus,
    pub risk: RiskLevel,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DiagnosticsBundle {
    pub hardware: HardwareSummary,
    pub kernel_version: Option<String>,
    pub summary: DiagnosticsSummary,
    pub gpu_mode_pending: Option<GpuModePending>,
    pub last_known_good_fan_curve: Option<FanCurveSnapshot>,
    pub fan_curve_drift: FanCurveDriftReport,
    pub fan_preset_by_platform_profile: BTreeMap<String, String>,
    pub fan_preset_reapply_after_resume: bool,
    pub hardware_profiles: BTreeMap<String, HardwareProfile>,
    pub hardware_profile_triggers: BTreeMap<String, String>,
    pub automation_rules: BTreeMap<String, AutomationRule>,
    pub last_automation_rule_apply: BTreeMap<String, AutomationRuleApplyRun>,
    pub ryzen_backend_status: Option<RyzenBackendStatus>,
    pub last_hardware_profile_apply: Option<HardwareProfileApplyRun>,
    pub hardware_profile_drift: HardwareProfileDriftReport,
    pub gpu_switching: GpuSwitchingDiagnostics,
    pub recent_platform_profile_changes: Vec<PlatformProfileChangeEvent>,
    pub recent_desktop_power_profile_changes: Vec<DesktopPowerProfileChangeEvent>,
    pub detected_sysfs_paths: Vec<String>,
    pub recent_daemon_logs: Vec<String>,
    pub raw_probe_report: CapabilityRegistry,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiagnosticsRuntimeState {
    pub gpu_mode_pending: Option<GpuModePending>,
    pub last_known_good_fan_curve: Option<FanCurveSnapshot>,
    pub live_fan_curve: Option<FanCurveSnapshot>,
    pub fan_preset_by_platform_profile: BTreeMap<String, String>,
    pub fan_preset_reapply_after_resume: bool,
    pub hardware_profiles: BTreeMap<String, HardwareProfile>,
    pub hardware_profile_triggers: BTreeMap<String, String>,
    pub automation_rules: BTreeMap<String, AutomationRule>,
    pub last_automation_rule_apply: BTreeMap<String, AutomationRuleApplyRun>,
    pub ryzen_backend_status: Option<RyzenBackendStatus>,
    pub last_hardware_profile_apply: Option<HardwareProfileApplyRun>,
    pub recent_platform_profile_changes: Vec<PlatformProfileChangeEvent>,
    pub recent_desktop_power_profile_changes: Vec<DesktopPowerProfileChangeEvent>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HardwareProfileDriftReport {
    pub status: String,
    pub profile_id: Option<String>,
    pub checked_count: usize,
    pub drifted_count: usize,
    pub items: Vec<HardwareProfileDriftItem>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HardwareProfileDriftItem {
    pub action_id: String,
    pub method: String,
    pub requested_value: String,
    pub readback_value: Option<String>,
    pub current_value: Option<String>,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FanCurveDriftReport {
    pub status: String,
    pub curve_id: Option<String>,
    pub checked_count: usize,
    pub drifted_count: usize,
    pub detail: String,
    pub items: Vec<FanCurveDriftItem>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FanCurveDriftItem {
    pub path: String,
    pub saved_value: String,
    pub live_value: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GpuSwitchingDiagnostics {
    pub status: String,
    pub provider: Option<String>,
    pub current_mode: Option<String>,
    pub switch_type: String,
    pub execution_model: String,
    pub runtime_plan_available: bool,
    pub blockers: Vec<String>,
    pub evidence: Vec<String>,
    pub next_action: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DiagnosticsSummary {
    pub capability_count: usize,
    pub available_capability_count: usize,
    pub missing_capability_count: usize,
    pub capability_status_counts: BTreeMap<String, usize>,
    pub sensor_count: usize,
    pub fan_curve_count: usize,
    pub detected_sysfs_path_count: usize,
}

impl DiagnosticsBundle {
    pub fn from_report(report: CapabilityRegistry, kernel_version: Option<String>) -> Self {
        Self::from_report_with_logs(report, kernel_version, Vec::new())
    }

    pub fn from_report_with_logs(
        report: CapabilityRegistry,
        kernel_version: Option<String>,
        recent_daemon_logs: Vec<String>,
    ) -> Self {
        let detected_sysfs_paths = detected_sysfs_paths(&report);
        let summary = DiagnosticsSummary::from_report(&report, detected_sysfs_paths.len());
        Self {
            hardware: report.hardware.clone(),
            kernel_version,
            summary,
            gpu_mode_pending: None,
            last_known_good_fan_curve: None,
            fan_curve_drift: FanCurveDriftReport::no_saved_snapshot(),
            fan_preset_by_platform_profile: BTreeMap::new(),
            fan_preset_reapply_after_resume: false,
            hardware_profiles: BTreeMap::new(),
            hardware_profile_triggers: BTreeMap::new(),
            automation_rules: BTreeMap::new(),
            last_automation_rule_apply: BTreeMap::new(),
            ryzen_backend_status: None,
            last_hardware_profile_apply: None,
            hardware_profile_drift: HardwareProfileDriftReport::no_last_apply(),
            gpu_switching: gpu_switching_diagnostics(&report),
            recent_platform_profile_changes: Vec::new(),
            recent_desktop_power_profile_changes: Vec::new(),
            detected_sysfs_paths,
            recent_daemon_logs,
            raw_probe_report: report,
        }
    }

    pub fn with_runtime_state(mut self, state: DiagnosticsRuntimeState) -> Self {
        self.gpu_mode_pending = state.gpu_mode_pending;
        self.last_known_good_fan_curve = state.last_known_good_fan_curve;
        self.fan_curve_drift = fan_curve_drift_report(
            self.last_known_good_fan_curve.as_ref(),
            state.live_fan_curve.as_ref(),
        );
        self.fan_preset_by_platform_profile = state.fan_preset_by_platform_profile;
        self.fan_preset_reapply_after_resume = state.fan_preset_reapply_after_resume;
        self.hardware_profiles = state.hardware_profiles;
        self.hardware_profile_triggers = state.hardware_profile_triggers;
        self.automation_rules = state.automation_rules;
        self.last_automation_rule_apply = state.last_automation_rule_apply;
        self.ryzen_backend_status = state.ryzen_backend_status;
        self.last_hardware_profile_apply = state.last_hardware_profile_apply;
        self.hardware_profile_drift = hardware_profile_drift_report(
            &self.raw_probe_report,
            self.last_hardware_profile_apply.as_ref(),
        );
        self.recent_platform_profile_changes = state.recent_platform_profile_changes;
        self.recent_desktop_power_profile_changes = state.recent_desktop_power_profile_changes;
        self
    }
}

impl HardwareProfileDriftReport {
    fn no_last_apply() -> Self {
        Self {
            status: "no_last_apply".to_owned(),
            profile_id: None,
            checked_count: 0,
            drifted_count: 0,
            items: Vec::new(),
        }
    }
}

impl FanCurveDriftReport {
    fn no_saved_snapshot() -> Self {
        Self {
            status: "no_saved_snapshot".to_owned(),
            curve_id: None,
            checked_count: 0,
            drifted_count: 0,
            detail: "No last-known-good fan curve snapshot is stored.".to_owned(),
            items: Vec::new(),
        }
    }
}

fn fan_curve_drift_report(
    saved: Option<&FanCurveSnapshot>,
    live: Option<&FanCurveSnapshot>,
) -> FanCurveDriftReport {
    let Some(saved) = saved else {
        return FanCurveDriftReport::no_saved_snapshot();
    };
    let Some(live) = live else {
        return FanCurveDriftReport {
            status: "missing_live_readings".to_owned(),
            curve_id: Some(saved.curve_id.clone()),
            checked_count: 0,
            drifted_count: 0,
            detail: "Live fan curve readings are unavailable in the latest daemon snapshot."
                .to_owned(),
            items: Vec::new(),
        };
    };

    let live_points = live
        .points
        .iter()
        .map(|point| (point.path.as_str(), point.value.as_str()))
        .collect::<BTreeMap<_, _>>();
    let items = saved
        .points
        .iter()
        .map(|saved_point| {
            let live_value = live_points
                .get(saved_point.path.as_str())
                .map(|value| (*value).to_owned());
            let status = if live_value.as_deref() == Some(saved_point.value.as_str()) {
                "in_sync"
            } else if live_value.is_some() {
                "drifted"
            } else {
                "missing_live_value"
            };
            FanCurveDriftItem {
                path: saved_point.path.clone(),
                saved_value: saved_point.value.clone(),
                live_value,
                status: status.to_owned(),
            }
        })
        .collect::<Vec<_>>();
    let checked_count = items.len();
    let drifted_count = items.iter().filter(|item| item.status != "in_sync").count();
    let status = if drifted_count > 0 {
        "drifted"
    } else {
        "in_sync"
    };
    let detail = if status == "in_sync" {
        format!(
            "Live fan curve still matches the saved {}.",
            format_fan_curve_snapshot_summary(Some(saved))
        )
    } else {
        format!(
            "Live fan curve differs from the saved {}; inspect changed point values before reapplying fan presets.",
            format_fan_curve_snapshot_summary(Some(saved))
        )
    };

    FanCurveDriftReport {
        status: status.to_owned(),
        curve_id: Some(saved.curve_id.clone()),
        checked_count,
        drifted_count,
        detail,
        items,
    }
}

fn gpu_switching_diagnostics(report: &CapabilityRegistry) -> GpuSwitchingDiagnostics {
    let Some(gpu) = report.gpu.as_ref() else {
        return GpuSwitchingDiagnostics {
            status: "unavailable".to_owned(),
            provider: None,
            current_mode: None,
            switch_type: format_gpu_switch_type(GpuSwitchType::Unknown).to_owned(),
            execution_model: "unavailable".to_owned(),
            runtime_plan_available: false,
            blockers: vec![
                "GPU switching capability is unavailable in the latest probe report".to_owned(),
            ],
            evidence: Vec::new(),
            next_action: "capture a compatibility bundle on hardware with GPU switching support"
                .to_owned(),
        };
    };

    let gpu_runtime = report.gpu_runtime.as_ref();
    let mut evidence = Vec::new();
    if let Some(runtime) = gpu_runtime {
        let modes = if runtime.candidate_runtime_modes.is_empty() {
            "none".to_owned()
        } else {
            runtime.candidate_runtime_modes.join("|")
        };
        evidence.push(format!("gpu_runtime_candidate_modes={modes}"));
        evidence.push(format!(
            "gpu_runtime_promotion_ready={}",
            runtime.promotion_ready
        ));
        evidence.push(format!("gpu_runtime_current_mode={}", runtime.current_mode));
        for item in &runtime.evidence {
            evidence.push(format!("gpu_runtime_evidence={item}"));
        }
    }
    evidence.extend(gpu.switch_notes.clone());
    evidence.push(format!("provider={}", gpu.provider));
    if let Some(mode) = &gpu.mode {
        evidence.push(format!("current_mode={mode}"));
    }

    if let Some(runtime) = gpu_runtime {
        if !runtime.promotion_ready {
            return GpuSwitchingDiagnostics {
                status: "runtime_candidate_blocked".to_owned(),
                provider: Some(gpu.provider.clone()),
                current_mode: gpu.mode.clone(),
                switch_type: format_gpu_switch_type(gpu.switch_type).to_owned(),
                execution_model: "runtime_candidate".to_owned(),
                runtime_plan_available: false,
                blockers: vec![
                    "gpu_runtime candidate is detected but not promoted".to_owned(),
                    "strict GPU mux/session evidence review has not accepted this path".to_owned(),
                    "runtime execution remains disabled in the daemon".to_owned(),
                ],
                evidence,
                next_action:
                    "run ratvantage-review-gpu-mux-evidence --require-session-restart-confirmed before promoting runtime planning"
                        .to_owned(),
            };
        }
        return GpuSwitchingDiagnostics {
            status: "runtime_candidate_plan_ready".to_owned(),
            provider: Some(gpu.provider.clone()),
            current_mode: gpu.mode.clone(),
            switch_type: format_gpu_switch_type(gpu.switch_type).to_owned(),
            execution_model: "runtime_candidate".to_owned(),
            runtime_plan_available: true,
            blockers: vec!["runtime execution remains disabled in the daemon".to_owned()],
            evidence,
            next_action:
                "review PlanGpuModeRuntimeWrite output; do not add execution until rollback/read-back evidence exists"
                    .to_owned(),
        };
    }

    match gpu.switch_type {
        GpuSwitchType::RebootRequired => GpuSwitchingDiagnostics {
            status: "reboot_required_baseline".to_owned(),
            provider: Some(gpu.provider.clone()),
            current_mode: gpu.mode.clone(),
            switch_type: format_gpu_switch_type(gpu.switch_type).to_owned(),
            execution_model: "reboot_required".to_owned(),
            runtime_plan_available: false,
            blockers: vec![
                "runtime or session-restart switching has not been detected for this provider"
                    .to_owned(),
            ],
            evidence,
            next_action:
                "continue using the reboot-gated EnvyControl path and pending-reboot tracking"
                    .to_owned(),
        },
        GpuSwitchType::SessionRestartRequired => GpuSwitchingDiagnostics {
            status: "session_restart_research_blocked".to_owned(),
            provider: Some(gpu.provider.clone()),
            current_mode: gpu.mode.clone(),
            switch_type: format_gpu_switch_type(gpu.switch_type).to_owned(),
            execution_model: "session_restart_required".to_owned(),
            runtime_plan_available: false,
            blockers: vec![
                "no dedicated session-restart backend exists yet".to_owned(),
                "no live display recovery evidence has been captured".to_owned(),
                "session teardown/recovery UX is not implemented".to_owned(),
            ],
            evidence,
            next_action:
                "keep this path plan-only until backend, read-back, and recovery evidence exist"
                    .to_owned(),
        },
        GpuSwitchType::RuntimeMux => GpuSwitchingDiagnostics {
            status: "runtime_mux_research_blocked".to_owned(),
            provider: Some(gpu.provider.clone()),
            current_mode: gpu.mode.clone(),
            switch_type: format_gpu_switch_type(gpu.switch_type).to_owned(),
            execution_model: "runtime_mux".to_owned(),
            runtime_plan_available: false,
            blockers: vec![
                "no dedicated runtime mux backend exists yet".to_owned(),
                "no automatic display recovery evidence has been captured".to_owned(),
                "current mux state read-back is not validated".to_owned(),
            ],
            evidence,
            next_action:
                "capture read-only mux state and recovery evidence before adding a switch plan"
                    .to_owned(),
        },
        GpuSwitchType::Unknown => GpuSwitchingDiagnostics {
            status: "unknown_switch_type".to_owned(),
            provider: Some(gpu.provider.clone()),
            current_mode: gpu.mode.clone(),
            switch_type: format_gpu_switch_type(gpu.switch_type).to_owned(),
            execution_model: "unknown".to_owned(),
            runtime_plan_available: false,
            blockers: vec![
                "switch type is unknown; runtime/session switching remains research-only"
                    .to_owned(),
            ],
            evidence,
            next_action: "capture a compatibility bundle and classify the GPU switching provider"
                .to_owned(),
        },
    }
}

fn hardware_profile_drift_report(
    report: &CapabilityRegistry,
    last_apply: Option<&HardwareProfileApplyRun>,
) -> HardwareProfileDriftReport {
    let Some(last_apply) = last_apply else {
        return HardwareProfileDriftReport::no_last_apply();
    };
    if !last_apply.completed {
        return HardwareProfileDriftReport {
            status: "last_apply_incomplete".to_owned(),
            profile_id: Some(last_apply.profile_id.clone()),
            checked_count: 0,
            drifted_count: 0,
            items: Vec::new(),
        };
    }

    let mut items = Vec::new();
    for action in &last_apply.results {
        if !action.result.applied {
            continue;
        }
        let current = current_value_for_profile_action(report, action);
        let expected = expected_value_for_profile_action(action);
        let status = match &current {
            Some(current) if current == &expected => "in_sync",
            Some(_) => "drifted",
            None => {
                if comparable_profile_action(&action.action_id) {
                    "missing_current_value"
                } else {
                    "not_comparable"
                }
            }
        };
        let detail = match (&current, status) {
            (Some(current), "in_sync") => {
                format!("current value `{current}` still matches last requested value")
            }
            (Some(current), "drifted") => format!(
                "current value `{current}` differs from last observed `{}`",
                expected
            ),
            (None, "missing_current_value") => {
                "current value is unavailable in the latest probe report".to_owned()
            }
            _ => "this action is not yet comparable from read-only probe data".to_owned(),
        };
        items.push(HardwareProfileDriftItem {
            action_id: action.action_id.clone(),
            method: action.result.plan.method.clone(),
            requested_value: action.result.plan.requested_value.clone(),
            readback_value: action.result.readback_value.clone(),
            current_value: current,
            status: status.to_owned(),
            detail,
        });
    }

    let checked_count = items
        .iter()
        .filter(|item| matches!(item.status.as_str(), "in_sync" | "drifted"))
        .count();
    let drifted_count = items.iter().filter(|item| item.status == "drifted").count();
    let status = if drifted_count > 0 {
        "drifted"
    } else if checked_count > 0 {
        "in_sync"
    } else {
        "no_comparable_actions"
    };

    HardwareProfileDriftReport {
        status: status.to_owned(),
        profile_id: Some(last_apply.profile_id.clone()),
        checked_count,
        drifted_count,
        items,
    }
}

fn comparable_profile_action(action_id: &str) -> bool {
    matches!(
        action_id,
        "platform_profile"
            | "battery_charge_type"
            | "keyboard_rgb"
            | "gpu_mode"
            | "cpu_governor"
            | "cpu_epp"
            | "cpu_boost"
            | "conservation_mode"
            | "amd_gpu_dpm_force_level"
    ) || action_id.starts_with("firmware_attribute:")
}

fn expected_value_for_profile_action(action: &HardwareProfileApplyActionResult) -> String {
    if action.action_id == "keyboard_rgb" && action.result.plan.method == "SetOpenRgbKeyboardRgbSdk"
    {
        if let Some(readback) = &action.result.readback_value {
            return readback.clone();
        }
    }
    action.result.plan.requested_value.clone()
}

fn current_value_for_profile_action(
    report: &CapabilityRegistry,
    action: &HardwareProfileApplyActionResult,
) -> Option<String> {
    match action.action_id.as_str() {
        "platform_profile" => report
            .platform_profile
            .as_ref()
            .and_then(|profile| profile.current.clone()),
        "battery_charge_type" => report
            .battery_charge_type
            .as_ref()
            .and_then(|charge_type| charge_type.current.clone()),
        "keyboard_rgb" => current_keyboard_rgb_value(report, &action.result.plan.method),
        "gpu_mode" => report.gpu.as_ref().and_then(|gpu| gpu.mode.clone()),
        "cpu_governor" => report
            .cpu_power
            .as_ref()
            .and_then(|cpu| cpu.governor.clone()),
        "cpu_epp" => report.cpu_power.as_ref().and_then(|cpu| cpu.epp.clone()),
        "cpu_boost" => report.cpu_power.as_ref().and_then(|cpu| {
            cpu.boost
                .map(|enabled| if enabled { "1" } else { "0" }.to_owned())
        }),
        "conservation_mode" => report
            .ideapad_toggles
            .iter()
            .find(|toggle| toggle.name == "conservation_mode")
            .and_then(|toggle| toggle.current_value.clone()),
        "amd_gpu_dpm_force_level" => report
            .amd_gpu_power_dpm
            .as_ref()
            .and_then(|gpu| gpu.current_force_performance_level.clone()),
        action_id => action_id
            .strip_prefix("firmware_attribute:")
            .and_then(|attribute_id| {
                report
                    .firmware_attributes
                    .iter()
                    .find(|attribute| attribute.name == attribute_id)
                    .and_then(|attribute| attribute.current_value.clone())
            }),
    }
}

fn current_keyboard_rgb_value(report: &CapabilityRegistry, method: &str) -> Option<String> {
    if method == "SetOpenRgbKeyboardRgbSdk" {
        let openrgb = report.keyboard_rgb_openrgb.as_ref()?;
        let active_mode = openrgb.sdk_active_mode.as_ref()?;
        if openrgb.sdk_colors.is_empty() {
            return None;
        }
        return Some(format_openrgb_sdk_snapshot_summary(
            active_mode,
            &openrgb.sdk_colors,
        ));
    }

    let rgb = report.keyboard_rgb.as_ref()?;
    let effect = rgb.current_effect.as_ref()?;
    let brightness = rgb.current_brightness?;
    Some(format_keyboard_rgb_state_summary(
        effect,
        &rgb.current_colors,
        brightness,
        rgb.current_speed,
    ))
}

fn format_keyboard_rgb_state_summary(
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

fn format_openrgb_sdk_snapshot_summary(
    active_mode: &str,
    colors: &BTreeMap<String, String>,
) -> String {
    format!(
        "active_mode={active_mode};colors={}",
        colors
            .iter()
            .map(|(zone, color)| format!("{zone}:{color}"))
            .collect::<Vec<_>>()
            .join(",")
    )
}

impl DiagnosticsSummary {
    fn from_report(report: &CapabilityRegistry, detected_sysfs_path_count: usize) -> Self {
        let mut capability_status_counts = BTreeMap::new();
        for capability in &report.capabilities {
            *capability_status_counts
                .entry(capability_status_label(capability.status).to_owned())
                .or_insert(0) += 1;
        }

        let missing_capability_count = report
            .capabilities
            .iter()
            .filter(|capability| capability.status == CapabilityStatus::Missing)
            .count();

        Self {
            capability_count: report.capabilities.len(),
            available_capability_count: report.capabilities.len() - missing_capability_count,
            missing_capability_count,
            capability_status_counts,
            sensor_count: report.telemetry.sensors.len(),
            fan_curve_count: report.fan_curves.len(),
            detected_sysfs_path_count,
        }
    }
}

impl UiStatus {
    pub fn from_client(client: &LegionControlClient) -> Result<Self> {
        Self::from_parts(client.hardware_summary()?, client.capabilities()?)
    }

    pub fn from_parts(hardware: HardwareSummary, capabilities: Vec<Capability>) -> Result<Self> {
        let mut capabilities = capabilities
            .into_iter()
            .map(|capability| UiCapabilityStatus {
                id: capability.id,
                label: capability.label,
                status: capability.status,
                risk: capability.risk,
            })
            .collect::<Vec<_>>();
        capabilities.sort_by(|left, right| left.id.cmp(&right.id));

        Ok(Self {
            hardware: UiHardwareStatus {
                vendor: hardware.vendor.unwrap_or_else(|| "unknown".to_owned()),
                product_name: hardware
                    .product_name
                    .unwrap_or_else(|| "unknown".to_owned()),
                product_version: hardware
                    .product_version
                    .unwrap_or_else(|| "unknown".to_owned()),
                product_sku: hardware.product_sku,
            },
            capabilities,
        })
    }

    pub fn capability_count(&self) -> usize {
        self.capabilities.len()
    }

    pub fn capability_ids(&self) -> Vec<&str> {
        self.capabilities
            .iter()
            .map(|capability| capability.id.as_str())
            .collect()
    }

    pub fn capability_statuses(&self) -> Vec<String> {
        self.capabilities
            .iter()
            .map(|capability| {
                format!(
                    "{}:{}:{}",
                    capability.id,
                    capability_status_label(capability.status),
                    risk_level_label(capability.risk)
                )
            })
            .collect()
    }

    pub fn render_lines(&self) -> Vec<String> {
        vec![
            "RatVantage status".to_owned(),
            format!("vendor={}", self.hardware.vendor),
            format!("product_name={}", self.hardware.product_name),
            format!("product_version={}", self.hardware.product_version),
            format!("capability_count={}", self.capability_count()),
            format!("capabilities={}", self.capability_ids().join(",")),
            format!(
                "capability_statuses={}",
                self.capability_statuses().join(",")
            ),
        ]
    }
}

pub fn capability_status_label(status: CapabilityStatus) -> &'static str {
    match status {
        CapabilityStatus::Detected => "detected",
        CapabilityStatus::Missing => "missing",
        CapabilityStatus::ProbeOnly => "probe_only",
        CapabilityStatus::Unsupported => "unsupported",
    }
}

pub fn risk_level_label(risk: RiskLevel) -> &'static str {
    match risk {
        RiskLevel::ReadOnly => "read_only",
        RiskLevel::ReversibleWrite => "reversible_write",
        RiskLevel::ExperimentalWrite => "experimental_write",
        RiskLevel::Unsupported => "unsupported",
    }
}

pub fn render_overview_lines(report: &CapabilityRegistry) -> Vec<String> {
    render_overview_lines_with_pending(report, None, None, &BTreeMap::new(), false)
}

pub fn render_overview_lines_with_pending(
    report: &CapabilityRegistry,
    pending: Option<&GpuModePending>,
    fan_snapshot: Option<&FanCurveSnapshot>,
    fan_preset_by_platform_profile: &BTreeMap<String, String>,
    fan_preset_reapply_after_resume: bool,
) -> Vec<String> {
    let mut lines = vec![
        "RatVantage overview".to_owned(),
        format!(
            "platform_profile={}",
            report
                .platform_profile
                .as_ref()
                .and_then(|profile| profile.current.as_deref())
                .unwrap_or("unknown")
        ),
        format!(
            "battery_charge_type={}",
            report
                .battery_charge_type
                .as_ref()
                .and_then(|charge_type| charge_type.current.as_deref())
                .unwrap_or("unknown")
        ),
        format!(
            "fan_rpm={}",
            render_sensor_values(&report.telemetry.sensors, "fan")
        ),
        format!(
            "temperatures={}",
            render_sensor_values(&report.telemetry.sensors, "temp")
        ),
        format!(
            "gpu_mode={}",
            report
                .gpu
                .as_ref()
                .and_then(|gpu| gpu.mode.as_deref())
                .unwrap_or("unknown")
        ),
        format!(
            "gpu_switch_type={}",
            format_gpu_switch_type(
                report
                    .gpu
                    .as_ref()
                    .map(|gpu| gpu.switch_type)
                    .unwrap_or(GpuSwitchType::Unknown)
            )
        ),
        format!(
            "desktop_power_profiles={}",
            format_power_profiles_probe_summary(report.power_profiles.as_ref())
        ),
        format!(
            "gpu_pending_reboot={}",
            format_gpu_mode_pending_summary(pending)
        ),
        format!(
            "last_known_good_fan_curve={}",
            format_fan_curve_snapshot_summary(fan_snapshot)
        ),
        format!(
            "fan_preset_by_platform_profile={}",
            format_fan_preset_profile_map_line(fan_preset_by_platform_profile)
        ),
        format!("fan_preset_reapply_after_resume={fan_preset_reapply_after_resume}"),
        format!(
            "battery_capacity_percent={}",
            report
                .telemetry
                .battery
                .as_ref()
                .and_then(|battery| battery.capacity_percent)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_owned())
        ),
        format!(
            "battery_status={}",
            report
                .telemetry
                .battery
                .as_ref()
                .and_then(|battery| battery.status.as_deref())
                .unwrap_or("unknown")
        ),
        format!(
            "battery_health={}",
            report
                .telemetry
                .battery
                .as_ref()
                .and_then(|battery| battery.health.as_deref())
                .unwrap_or("unknown")
        ),
        format!(
            "battery_power_now_w={}",
            report
                .telemetry
                .battery
                .as_ref()
                .and_then(|battery| battery.power_now_uw)
                .map(|uw| format!("{:.1}", uw as f64 / 1_000_000.0))
                .unwrap_or_else(|| "unknown".to_owned())
        ),
        format!(
            "battery_cycle_count={}",
            report
                .telemetry
                .battery
                .as_ref()
                .and_then(|battery| battery.cycle_count)
                .map(|c| c.to_string())
                .unwrap_or_else(|| "unknown".to_owned())
        ),
    ];
    lines.push(format!("leds={}", render_led_values(report)));
    lines.push(format!(
        "keyboard_rgb_status={}",
        render_keyboard_rgb_status(report)
    ));
    lines.push(format!(
        "keyboard_rgb_candidates={}",
        render_keyboard_rgb_candidates(report)
    ));
    if let Some(openrgb) = report.keyboard_rgb_openrgb.as_ref() {
        lines.push(format!(
            "keyboard_rgb_openrgb={}",
            render_keyboard_rgb_openrgb(openrgb)
        ));
    }
    lines.push(format!(
        "firmware_toggles={}",
        render_ideapad_toggle_values(report)
    ));
    lines
}

fn format_fan_preset_profile_map_line(map: &BTreeMap<String, String>) -> String {
    if map.is_empty() {
        "none".to_owned()
    } else {
        map.iter()
            .map(|(profile, preset_id)| format!("{profile}={preset_id}"))
            .collect::<Vec<_>>()
            .join(",")
    }
}

fn render_sensor_values(sensors: &[legion_common::HwmonSensor], kind: &str) -> String {
    let values = sensors
        .iter()
        .filter(|sensor| sensor.kind == kind)
        .map(|sensor| {
            format!(
                "{}:{}",
                sensor.label.as_deref().unwrap_or("unknown"),
                sensor
                    .value
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_owned())
            )
        })
        .collect::<Vec<_>>();

    if values.is_empty() {
        "unknown".to_owned()
    } else {
        values.join(",")
    }
}

fn render_led_values(report: &CapabilityRegistry) -> String {
    let values = report
        .leds
        .iter()
        .map(|led| {
            format!(
                "{}:{}",
                led.name,
                led.brightness
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_owned())
            )
        })
        .collect::<Vec<_>>();

    if values.is_empty() {
        "unknown".to_owned()
    } else {
        values.join(",")
    }
}

fn render_keyboard_rgb_status(report: &CapabilityRegistry) -> String {
    if let Some(rgb) = report.keyboard_rgb.as_ref() {
        return format!(
            "backend_ready=true backend={} device={} zones={} effect={}",
            rgb.backend,
            rgb.device_id,
            rgb.zones.len(),
            rgb.current_effect.as_deref().unwrap_or("unknown")
        );
    }

    if report.keyboard_rgb_candidates.is_empty() {
        return "not_detected backend_ready=false".to_owned();
    }

    let mut ids = report
        .keyboard_rgb_candidates
        .iter()
        .filter_map(|candidate| {
            match (
                candidate.vendor_id.as_deref(),
                candidate.product_id.as_deref(),
            ) {
                (Some(vendor), Some(product)) => {
                    Some(format!("{vendor}:{product}").to_ascii_lowercase())
                }
                _ => None,
            }
        })
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    let ids = if ids.is_empty() {
        "unknown".to_owned()
    } else {
        ids.join("|")
    };
    format!(
        "research_candidates={} backend_ready=false vid_pid={ids}",
        report.keyboard_rgb_candidates.len()
    )
}

fn render_keyboard_rgb_candidates(report: &CapabilityRegistry) -> String {
    let values = report
        .keyboard_rgb_candidates
        .iter()
        .map(|candidate| {
            let vendor = candidate
                .vendor_id
                .as_deref()
                .unwrap_or("unknown")
                .to_ascii_lowercase();
            let product = candidate
                .product_id
                .as_deref()
                .unwrap_or("unknown")
                .to_ascii_lowercase();
            let report_ids = if candidate.report_ids.is_empty() {
                "reports=unknown".to_owned()
            } else {
                format!(
                    "reports={}",
                    candidate
                        .report_ids
                        .iter()
                        .map(|id| id.to_string())
                        .collect::<Vec<_>>()
                        .join("|")
                )
            };
            let report_shapes = render_keyboard_rgb_report_shapes(candidate);
            format!(
                "{}:{}:{}:{}:{}",
                candidate.device_id, vendor, product, report_ids, report_shapes
            )
        })
        .collect::<Vec<_>>();

    if values.is_empty() {
        "unknown".to_owned()
    } else {
        values.join(",")
    }
}

fn render_keyboard_rgb_report_shapes(candidate: &legion_common::KeyboardRgbCandidate) -> String {
    if candidate.hid_reports.is_empty() {
        return "shapes=unknown".to_owned();
    }
    format!(
        "shapes={}",
        candidate
            .hid_reports
            .iter()
            .map(|report| {
                let id = report
                    .report_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "none".to_owned());
                format!("{id}/{}:{}B", report.kind, report.byte_length)
            })
            .collect::<Vec<_>>()
            .join("|")
    )
}

fn render_keyboard_rgb_openrgb(openrgb: &legion_common::KeyboardRgbOpenRgbStatus) -> String {
    let device = openrgb
        .devices
        .first()
        .map(|device| {
            format!(
                "{}:{}",
                device.index,
                device.description.as_deref().unwrap_or(&device.name)
            )
        })
        .unwrap_or_else(|| "none".to_owned());
    let modes = openrgb
        .devices
        .first()
        .map(|device| {
            if device.modes.is_empty() {
                "unknown".to_owned()
            } else {
                device.modes.join("|")
            }
        })
        .unwrap_or_else(|| "unknown".to_owned());
    format!(
        "installed={} detected={} device={} modes={} i2c_dev_loaded={} user_in_i2c_group={} i2c_rw={} hidraw_rw={} sdk_helper={} sdk_server={} sdk_snapshot={} backend_ready={}",
        openrgb.installed,
        !openrgb.devices.is_empty(),
        device,
        modes,
        openrgb.i2c_dev_loaded,
        openrgb.user_in_i2c_group,
        openrgb.has_i2c_rw_access,
        openrgb.has_hidraw_rw_access,
        openrgb.sdk_helper_installed,
        openrgb.sdk_server_running,
        openrgb.sdk_snapshot_supported,
        openrgb.backend_ready
    )
}

fn render_ideapad_toggle_values(report: &CapabilityRegistry) -> String {
    let values = report
        .ideapad_toggles
        .iter()
        .map(|toggle| {
            format!(
                "{}:{}",
                toggle.name,
                toggle.current_value.as_deref().unwrap_or("unknown")
            )
        })
        .collect::<Vec<_>>();

    if values.is_empty() {
        "unknown".to_owned()
    } else {
        values.join(",")
    }
}

pub fn render_diagnostics_json(bundle: &DiagnosticsBundle) -> Result<String> {
    Ok(serde_json::to_string_pretty(bundle)?)
}

pub fn render_write_plan_json<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string_pretty(value)?)
}

pub fn runtime_refresh_notice(
    previous: Option<&RuntimeSnapshot>,
    current: &RuntimeSnapshot,
    recovered_from_error: bool,
) -> Option<RuntimeRefreshNotice> {
    let mut messages = Vec::new();
    if recovered_from_error {
        messages.push("Daemon communication recovered; live state is available again.".to_owned());
    }

    if let Some(previous) = previous {
        let previous_profile = previous
            .diagnostics
            .raw_probe_report
            .platform_profile
            .as_ref()
            .and_then(|profile| profile.current.as_deref())
            .unwrap_or("unknown");
        let current_profile = current
            .diagnostics
            .raw_probe_report
            .platform_profile
            .as_ref()
            .and_then(|profile| profile.current.as_deref())
            .unwrap_or("unknown");
        if previous_profile != current_profile {
            messages.push(format!(
                "Platform profile changed from `{previous_profile}` to `{current_profile}`; re-check fan behavior because profile changes can reset thermal behavior."
            ));
        }

        let previous_power_profile = previous
            .diagnostics
            .raw_probe_report
            .power_profiles
            .as_ref()
            .and_then(|profile| profile.active_profile.as_deref())
            .unwrap_or("unknown");
        let current_power_profile = current
            .diagnostics
            .raw_probe_report
            .power_profiles
            .as_ref()
            .and_then(|profile| profile.active_profile.as_deref())
            .unwrap_or("unknown");
        if previous_power_profile != current_power_profile {
            messages.push(format!(
                "Desktop power profile changed from `{previous_power_profile}` to `{current_power_profile}`."
            ));
        }

        let previous_charge_type = previous
            .diagnostics
            .raw_probe_report
            .battery_charge_type
            .as_ref()
            .and_then(|charge_type| charge_type.current.as_deref())
            .unwrap_or("unknown");
        let current_charge_type = current
            .diagnostics
            .raw_probe_report
            .battery_charge_type
            .as_ref()
            .and_then(|charge_type| charge_type.current.as_deref())
            .unwrap_or("unknown");
        if previous_charge_type != current_charge_type {
            messages.push(format!(
                "Battery charge type changed from `{previous_charge_type}` to `{current_charge_type}`."
            ));
        }

        let previous_available = previous.diagnostics.summary.available_capability_count;
        let previous_missing = previous.diagnostics.summary.missing_capability_count;
        let current_available = current.diagnostics.summary.available_capability_count;
        let current_missing = current.diagnostics.summary.missing_capability_count;
        if previous_available != current_available || previous_missing != current_missing {
            messages.push(format!(
                "Capability probe changed from {previous_available} available/{previous_missing} missing to {current_available} available/{current_missing} missing."
            ));
        }

        let previous_snapshot = previous
            .diagnostics
            .last_known_good_fan_curve
            .as_ref()
            .map(|snapshot| format_fan_curve_snapshot_summary(Some(snapshot)));
        let current_snapshot = current
            .diagnostics
            .last_known_good_fan_curve
            .as_ref()
            .map(|snapshot| format_fan_curve_snapshot_summary(Some(snapshot)));
        if previous_snapshot != current_snapshot {
            match (previous_snapshot, current_snapshot) {
                (Some(previous_snapshot), Some(current_snapshot)) => messages.push(format!(
                    "Saved fan-curve snapshot changed from {previous_snapshot} to {current_snapshot}."
                )),
                (Some(previous_snapshot), None) => messages.push(format!(
                    "Saved fan-curve snapshot `{previous_snapshot}` is no longer present in daemon state."
                )),
                (None, Some(current_snapshot)) => messages.push(format!(
                    "Saved fan-curve snapshot is now available: {current_snapshot}."
                )),
                (None, None) => {}
            }
        }

        if current.diagnostics.fan_curve_drift.status == "drifted" {
            messages.push(format!(
                "Live fan curve drifted from the saved snapshot: {}",
                current.diagnostics.fan_curve_drift.detail
            ));
        }
    }

    if messages.is_empty() {
        None
    } else {
        Some(RuntimeRefreshNotice {
            message: messages.join(" "),
        })
    }
}

fn detected_sysfs_paths(report: &CapabilityRegistry) -> Vec<String> {
    let mut paths = Vec::new();
    push_path(&mut paths, &report.hardware.sysfs_root);

    if let Some(profile) = &report.platform_profile {
        push_path(&mut paths, &profile.path);
        push_path(&mut paths, &profile.choices_path);
    }
    if let Some(charge_type) = &report.battery_charge_type {
        push_path(&mut paths, &charge_type.path);
        push_path(&mut paths, &charge_type.choices_path);
    }
    for sensor in &report.hwmon_sensors {
        push_path(&mut paths, &sensor.input_path);
    }
    for curve in &report.fan_curves {
        if let Some(path) = &curve.path {
            push_path(&mut paths, path);
        }
        for point_path in &curve.point_paths {
            push_path(&mut paths, point_path);
        }
    }
    for led in &report.leds {
        push_path(&mut paths, &led.path);
    }
    if let Some(keyboard_rgb) = &report.keyboard_rgb {
        push_path(&mut paths, &keyboard_rgb.path);
    }
    for candidate in &report.keyboard_rgb_candidates {
        push_path(&mut paths, &candidate.path);
    }
    for attribute in &report.firmware_attributes {
        push_path(&mut paths, &attribute.path);
    }
    for toggle in &report.ideapad_toggles {
        if let Some(path) = &toggle.path {
            push_path(&mut paths, path);
        }
    }
    for sensor in &report.telemetry.sensors {
        push_path(&mut paths, &sensor.input_path);
    }
    if let Some(battery) = &report.telemetry.battery {
        push_path(&mut paths, &battery.path);
    }

    paths.sort();
    paths.dedup();
    paths
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use legion_common::GpuRuntimeCapability;
    use legion_common::{
        Capability, CapabilityRegistry, CapabilityStatus, FanCurvePointSnapshot, FanCurveSnapshot,
        GpuCapability, HardwareProfileApplyActionResult, HardwareProfileApplyRun, HardwareSummary,
        KeyboardRgbOpenRgbDevice, KeyboardRgbOpenRgbStatus, PlatformProfileCapability,
        PowerProfilesCapability, RiskLevel, WriteDryRunPlan, WriteExecutionResult,
    };

    #[test]
    fn platform_profiles_map_to_matching_desktop_profiles() {
        assert_eq!(
            desktop_profile_for_platform_profile("low-power"),
            Some("power-saver")
        );
        assert_eq!(
            desktop_profile_for_platform_profile("balanced"),
            Some("balanced")
        );
        assert_eq!(
            desktop_profile_for_platform_profile("performance"),
            Some("performance")
        );
        assert_eq!(
            desktop_profile_for_platform_profile("max-power"),
            Some("performance")
        );
        assert_eq!(desktop_profile_for_platform_profile("custom"), None);
    }

    #[test]
    fn repeated_platform_selection_uses_enabled_full_profile_router() {
        let mut mappings = BTreeMap::new();
        mappings.insert("performance".to_owned(), "fnq_performance_full".to_owned());
        let rules = BTreeMap::from([(
            "fnq_router".to_owned(),
            AutomationRule {
                schema_version: 1,
                label: "Fn+Q router".to_owned(),
                enabled: true,
                kind: AutomationRuleKind::PlatformProfileRouter {
                    mappings,
                    cooldown_secs: 1,
                },
            },
        )]);

        assert_eq!(
            mapped_platform_profile(&rules, "performance"),
            Some("fnq_performance_full")
        );
        assert_eq!(mapped_platform_profile(&rules, "balanced"), None);
    }

    #[test]
    fn transitioning_profile_updates_firmware_before_desktop() {
        let events = std::cell::RefCell::new(Vec::new());
        let result = set_transitioning_platform_profile_with_desktop_sync(
            "performance",
            "low-power",
            "power-saver",
            |profile| {
                events.borrow_mut().push(format!("platform:{profile}"));
                Ok(true)
            },
            |profile| {
                events.borrow_mut().push(format!("desktop:{profile}"));
                Ok(())
            },
            |applied| *applied,
        )
        .expect("transition should succeed");

        assert!(result);
        assert_eq!(
            events.into_inner(),
            vec!["platform:performance", "desktop:performance"]
        );
    }

    #[test]
    fn transitioning_profile_restores_firmware_when_desktop_fails() {
        let events = std::cell::RefCell::new(Vec::new());
        let error = set_transitioning_platform_profile_with_desktop_sync(
            "performance",
            "low-power",
            "power-saver",
            |profile| {
                events.borrow_mut().push(format!("platform:{profile}"));
                Ok(true)
            },
            |profile| {
                events.borrow_mut().push(format!("desktop:{profile}"));
                anyhow::bail!("boost write failed")
            },
            |applied| *applied,
        )
        .expect_err("desktop failure should be reported");

        assert!(error
            .to_string()
            .contains("restored firmware profile `low-power`"));
        assert_eq!(
            events.into_inner(),
            vec![
                "platform:performance",
                "desktop:performance",
                "platform:low-power"
            ]
        );
    }

    #[test]
    fn synchronized_profile_change_updates_desktop_before_platform() {
        let events = std::cell::RefCell::new(Vec::new());
        let result = set_platform_profile_with_desktop_sync(
            "balanced",
            Some("power-saver"),
            |profile| {
                events.borrow_mut().push(format!("desktop:{profile}"));
                Ok(())
            },
            |profile| {
                events.borrow_mut().push(format!("platform:{profile}"));
                Ok(true)
            },
            |applied| *applied,
        )
        .expect("synchronized profile change should succeed");

        assert!(result);
        assert_eq!(
            events.into_inner(),
            vec!["desktop:balanced", "platform:balanced"]
        );
    }

    #[test]
    fn synchronized_profile_change_restores_desktop_when_platform_is_blocked() {
        let events = std::cell::RefCell::new(Vec::new());
        let result = set_platform_profile_with_desktop_sync(
            "balanced",
            Some("power-saver"),
            |profile| {
                events.borrow_mut().push(format!("desktop:{profile}"));
                Ok(())
            },
            |profile| {
                events.borrow_mut().push(format!("platform:{profile}"));
                Ok(false)
            },
            |applied| *applied,
        )
        .expect("desktop rollback should succeed");

        assert!(!result);
        assert_eq!(
            events.into_inner(),
            vec![
                "desktop:balanced",
                "platform:balanced",
                "desktop:power-saver"
            ]
        );
    }

    #[test]
    fn synchronized_profile_change_restores_desktop_after_desktop_write_error() {
        let events = std::cell::RefCell::new(Vec::new());
        let error = set_platform_profile_with_desktop_sync(
            "balanced",
            Some("power-saver"),
            |profile| {
                events.borrow_mut().push(format!("desktop:{profile}"));
                if profile == "balanced" {
                    anyhow::bail!("read-back mismatch");
                }
                Ok(())
            },
            |profile| {
                events.borrow_mut().push(format!("platform:{profile}"));
                Ok(true)
            },
            |applied| *applied,
        )
        .expect_err("desktop write error should stop the platform write");

        assert!(error.to_string().contains("read-back mismatch"));
        assert_eq!(
            events.into_inner(),
            vec!["desktop:balanced", "desktop:power-saver"]
        );
    }

    #[test]
    fn custom_platform_profile_does_not_change_desktop_profile() {
        let events = std::cell::RefCell::new(Vec::new());
        let result = set_platform_profile_with_desktop_sync(
            "custom",
            Some("power-saver"),
            |profile| {
                events.borrow_mut().push(format!("desktop:{profile}"));
                Ok(())
            },
            |profile| {
                events.borrow_mut().push(format!("platform:{profile}"));
                Ok(true)
            },
            |applied| *applied,
        )
        .expect("custom platform profile should still be applied");

        assert!(result);
        assert_eq!(events.into_inner(), vec!["platform:custom"]);
    }

    #[test]
    fn render_overview_lines_include_fan_preset_fields() {
        use std::collections::BTreeMap;

        let report = CapabilityRegistry::default();
        let mut map = BTreeMap::new();
        map.insert("performance".to_owned(), "gaming".to_owned());
        map.insert("balanced".to_owned(), "quiet-office".to_owned());
        let lines = render_overview_lines_with_pending(&report, None, None, &map, true);
        assert!(lines.contains(
            &"fan_preset_by_platform_profile=balanced=quiet-office,performance=gaming".to_owned()
        ));
        assert!(lines.contains(&"fan_preset_reapply_after_resume=true".to_owned()));
        assert!(lines
            .iter()
            .any(|line| line.starts_with("desktop_power_profiles=not_applicable")));
    }

    #[test]
    fn render_overview_lines_include_keyboard_rgb_readiness() {
        let report = CapabilityRegistry {
            keyboard_rgb_candidates: vec![
                legion_common::KeyboardRgbCandidate {
                    backend: "hidraw-sysfs-candidate".to_owned(),
                    device_id: "hidraw2".to_owned(),
                    path: "/sys/class/hidraw/hidraw2".to_owned(),
                    vendor_id: Some("048D".to_owned()),
                    product_id: Some("C985".to_owned()),
                    name: None,
                    modalias: None,
                    report_descriptor_bytes: Some(179),
                    report_ids: vec![90],
                    hid_reports: vec![legion_common::KeyboardRgbHidReport {
                        report_id: Some(90),
                        kind: "feature".to_owned(),
                        report_size_bits: 8,
                        report_count: 16,
                        bit_length: 128,
                        byte_length: 16,
                    }],
                    evidence: Vec::new(),
                },
                legion_common::KeyboardRgbCandidate {
                    backend: "hidraw-sysfs-candidate".to_owned(),
                    device_id: "hidraw1".to_owned(),
                    path: "/sys/class/hidraw/hidraw1".to_owned(),
                    vendor_id: Some("048D".to_owned()),
                    product_id: Some("C103".to_owned()),
                    name: None,
                    modalias: None,
                    report_descriptor_bytes: Some(156),
                    report_ids: vec![1, 90],
                    hid_reports: Vec::new(),
                    evidence: Vec::new(),
                },
            ],
            ..Default::default()
        };

        let lines = render_overview_lines(&report);
        assert!(lines.contains(
            &"keyboard_rgb_status=research_candidates=2 backend_ready=false vid_pid=048d:c103|048d:c985"
                .to_owned()
        ));
        assert!(lines.iter().any(|line| {
            line.contains(
                "keyboard_rgb_candidates=hidraw2:048d:c985:reports=90:shapes=90/feature:16B",
            )
        }));
    }

    #[test]
    fn render_overview_lines_include_openrgb_readiness_when_checked() {
        let report = CapabilityRegistry {
            keyboard_rgb_openrgb: Some(legion_common::KeyboardRgbOpenRgbStatus {
                installed: true,
                path: Some("/usr/bin/openrgb".to_owned()),
                devices: vec![legion_common::KeyboardRgbOpenRgbDevice {
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
                sdk_server_running: false,
                sdk_snapshot_supported: false,
                sdk_active_mode: None,
                sdk_color_zones: vec![],
                sdk_colors: std::collections::BTreeMap::new(),
            }),
            ..Default::default()
        };

        let lines = render_overview_lines(&report);
        assert!(lines.contains(
            &"keyboard_rgb_openrgb=installed=true detected=true device=0:Lenovo 4-Zone device modes=Direct|Breathing|Rainbow Wave|Spectrum Cycle i2c_dev_loaded=true user_in_i2c_group=true i2c_rw=true hidraw_rw=true sdk_helper=true sdk_server=false sdk_snapshot=false backend_ready=false"
                .to_owned()
        ));
    }

    #[test]
    fn gpu_switching_diagnostics_blocks_runtime_mux_until_recovery_evidence_exists() {
        let report = CapabilityRegistry {
            gpu: Some(GpuCapability {
                provider: "runtime-mux-fixture".to_owned(),
                status: CapabilityStatus::ProbeOnly,
                mode: Some("hybrid".to_owned()),
                switch_type: GpuSwitchType::RuntimeMux,
                switch_notes: vec!["fixture exposes runtime mux metadata".to_owned()],
            }),
            ..Default::default()
        };

        let diagnostics = gpu_switching_diagnostics(&report);

        assert_eq!(diagnostics.status, "runtime_mux_research_blocked");
        assert_eq!(diagnostics.switch_type, "runtime-mux");
        assert_eq!(diagnostics.execution_model, "runtime_mux");
        assert!(!diagnostics.runtime_plan_available);
        assert!(diagnostics
            .blockers
            .iter()
            .any(|blocker| blocker.contains("display recovery evidence")));
        assert!(diagnostics
            .evidence
            .iter()
            .any(|item| item == "provider=runtime-mux-fixture"));
    }

    #[test]
    fn gpu_switching_diagnostics_surfaces_unpromoted_runtime_candidate() {
        let report = CapabilityRegistry {
            gpu: Some(GpuCapability {
                provider: "envycontrol".to_owned(),
                status: CapabilityStatus::ProbeOnly,
                mode: Some("hybrid".to_owned()),
                switch_type: GpuSwitchType::RebootRequired,
                switch_notes: vec!["reboot baseline is still available".to_owned()],
            }),
            gpu_runtime: Some(GpuRuntimeCapability {
                status: CapabilityStatus::ProbeOnly,
                current_mode: "hybrid".to_owned(),
                candidate_runtime_modes: vec!["integrated".to_owned()],
                promotion_ready: false,
                evidence: vec!["/sys/bus/pci/rescan exists".to_owned()],
            }),
            ..Default::default()
        };

        let diagnostics = gpu_switching_diagnostics(&report);

        assert_eq!(diagnostics.status, "runtime_candidate_blocked");
        assert_eq!(diagnostics.execution_model, "runtime_candidate");
        assert!(!diagnostics.runtime_plan_available);
        assert!(diagnostics
            .blockers
            .iter()
            .any(|blocker| blocker.contains("not promoted")));
        assert!(diagnostics
            .evidence
            .iter()
            .any(|item| item == "gpu_runtime_candidate_modes=integrated"));
        assert!(diagnostics
            .evidence
            .iter()
            .any(|item| item == "gpu_runtime_promotion_ready=false"));
        assert!(diagnostics
            .next_action
            .contains("ratvantage-review-gpu-mux-evidence"));
    }

    #[test]
    fn gpu_switching_diagnostics_marks_promoted_runtime_candidate_plan_ready() {
        let report = CapabilityRegistry {
            gpu: Some(GpuCapability {
                provider: "envycontrol".to_owned(),
                status: CapabilityStatus::ProbeOnly,
                mode: Some("hybrid".to_owned()),
                switch_type: GpuSwitchType::RebootRequired,
                switch_notes: vec![],
            }),
            gpu_runtime: Some(GpuRuntimeCapability {
                status: CapabilityStatus::ProbeOnly,
                current_mode: "hybrid".to_owned(),
                candidate_runtime_modes: vec!["integrated".to_owned()],
                promotion_ready: true,
                evidence: vec!["strict reviewer accepted live mux evidence".to_owned()],
            }),
            ..Default::default()
        };

        let diagnostics = gpu_switching_diagnostics(&report);

        assert_eq!(diagnostics.status, "runtime_candidate_plan_ready");
        assert!(diagnostics.runtime_plan_available);
        assert_eq!(diagnostics.execution_model, "runtime_candidate");
        assert!(diagnostics
            .blockers
            .iter()
            .any(|blocker| blocker.contains("execution remains disabled")));
    }

    #[test]
    fn hardware_profile_drift_reports_openrgb_sdk_rgb_in_sync() {
        let report = report_with_openrgb_sdk_snapshot(
            "Breathing",
            [
                ("left_center", "#00FF00"),
                ("left_side", "#FF0000"),
                ("right_center", "#0000FF"),
                ("right_side", "#FFFFFF"),
            ],
        );
        let last_apply = rgb_sdk_apply_run(
            "active_mode=Breathing;colors=left_center:#00FF00,left_side:#FF0000,right_center:#0000FF,right_side:#FFFFFF",
        );

        let drift = hardware_profile_drift_report(&report, Some(&last_apply));

        assert_eq!(drift.status, "in_sync");
        assert_eq!(drift.checked_count, 1);
        assert_eq!(drift.drifted_count, 0);
        assert_eq!(drift.items[0].action_id, "keyboard_rgb");
        assert_eq!(drift.items[0].status, "in_sync");
    }

    #[test]
    fn hardware_profile_drift_reports_openrgb_sdk_rgb_drift() {
        let report = report_with_openrgb_sdk_snapshot(
            "Direct",
            [
                ("left_center", "#000000"),
                ("left_side", "#000000"),
                ("right_center", "#000000"),
                ("right_side", "#000000"),
            ],
        );
        let last_apply = rgb_sdk_apply_run(
            "active_mode=Breathing;colors=left_center:#00FF00,left_side:#FF0000,right_center:#0000FF,right_side:#FFFFFF",
        );

        let drift = hardware_profile_drift_report(&report, Some(&last_apply));

        assert_eq!(drift.status, "drifted");
        assert_eq!(drift.checked_count, 1);
        assert_eq!(drift.drifted_count, 1);
        assert_eq!(drift.items[0].action_id, "keyboard_rgb");
        assert_eq!(drift.items[0].status, "drifted");
        assert_eq!(
            drift.items[0].current_value.as_deref(),
            Some(
                "active_mode=Direct;colors=left_center:#000000,left_side:#000000,right_center:#000000,right_side:#000000"
            )
        );
    }

    #[test]
    fn fan_curve_drift_reports_live_snapshot_in_sync() {
        let saved = sample_fan_snapshot("legion_hwmon");
        let live = sample_fan_snapshot("legion_hwmon");

        let drift = fan_curve_drift_report(Some(&saved), Some(&live));

        assert_eq!(drift.status, "in_sync");
        assert_eq!(drift.checked_count, 1);
        assert_eq!(drift.drifted_count, 0);
        assert_eq!(drift.items[0].status, "in_sync");
    }

    #[test]
    fn fan_curve_drift_reports_live_snapshot_drift() {
        let saved = sample_fan_snapshot("legion_hwmon");
        let mut live = sample_fan_snapshot("legion_hwmon");
        live.points[0].value = "43000".to_owned();

        let drift = fan_curve_drift_report(Some(&saved), Some(&live));

        assert_eq!(drift.status, "drifted");
        assert_eq!(drift.checked_count, 1);
        assert_eq!(drift.drifted_count, 1);
        assert_eq!(drift.items[0].status, "drifted");
        assert_eq!(drift.items[0].saved_value, "42000");
        assert_eq!(drift.items[0].live_value.as_deref(), Some("43000"));
    }

    #[test]
    fn diagnostics_runtime_state_includes_fan_curve_drift() {
        let saved = sample_fan_snapshot("legion_hwmon");
        let mut live = sample_fan_snapshot("legion_hwmon");
        live.points[0].value = "43000".to_owned();

        let diagnostics = DiagnosticsBundle::from_report(CapabilityRegistry::default(), None)
            .with_runtime_state(DiagnosticsRuntimeState {
                last_known_good_fan_curve: Some(saved),
                live_fan_curve: Some(live),
                ..Default::default()
            });

        assert_eq!(diagnostics.fan_curve_drift.status, "drifted");
        assert_eq!(diagnostics.fan_curve_drift.drifted_count, 1);
    }

    #[test]
    fn runtime_refresh_notice_reports_recovery_profile_and_capability_drift() {
        let previous =
            sample_runtime_snapshot("balanced", "Standard", 2, 0, Some(sample_fan_snapshot("a")));
        let current = sample_runtime_snapshot(
            "performance",
            "Long_Life",
            1,
            1,
            Some(sample_fan_snapshot("b")),
        );

        let notice = runtime_refresh_notice(Some(&previous), &current, true).unwrap();

        assert!(notice.message.contains("Daemon communication recovered"));
        assert!(notice
            .message
            .contains("Platform profile changed from `balanced` to `performance`"));
        assert!(notice
            .message
            .contains("Battery charge type changed from `Standard` to `Long_Life`"));
        assert!(notice.message.contains(
            "Capability probe changed from 2 available/0 missing to 1 available/1 missing"
        ));
        assert!(notice
            .message
            .contains("Saved fan-curve snapshot changed from 1 point on a to 1 point on b"));
    }

    #[test]
    fn runtime_refresh_notice_reports_fan_curve_drift() {
        let previous = sample_runtime_snapshot("balanced", "Standard", 1, 0, None);
        let saved = sample_fan_snapshot("legion_hwmon");
        let mut live = sample_fan_snapshot("legion_hwmon");
        live.points[0].value = "43000".to_owned();
        let mut current =
            sample_runtime_snapshot("balanced", "Standard", 1, 0, Some(saved.clone()));
        current.diagnostics.fan_curve_drift = fan_curve_drift_report(Some(&saved), Some(&live));

        let notice = runtime_refresh_notice(Some(&previous), &current, false).unwrap();

        assert!(notice
            .message
            .contains("Live fan curve drifted from the saved snapshot"));
    }

    fn rgb_sdk_apply_run(readback: &str) -> HardwareProfileApplyRun {
        HardwareProfileApplyRun {
            profile_id: "rgb_profile".to_owned(),
            profile_label: "RGB profile".to_owned(),
            timestamp_unix_secs: 1,
            completed: true,
            message: "completed".to_owned(),
            results: vec![HardwareProfileApplyActionResult {
                action_id: "keyboard_rgb".to_owned(),
                result: WriteExecutionResult::applied(
                    WriteDryRunPlan {
                        method: "SetOpenRgbKeyboardRgbSdk".to_owned(),
                        capability_id: "keyboard_rgb_openrgb:sdk".to_owned(),
                        polkit_action: "org.ratvantage.LegionControl1.set-keyboard-rgb"
                            .to_owned(),
                        path: "openrgb-sdk:/usr/bin/openrgb".to_owned(),
                        previous_value: "SDK before snapshot".to_owned(),
                        requested_value: "effect=Breathing;brightness=100;speed=none;colors=left_center:#00FF00,left_side:#FF0000,right_center:#0000FF,right_side:#FFFFFF;sdk_packet=RGBCONTROLLER_UPDATELEDS;mode=Breathing;colors=FF0000,00FF00,0000FF,FFFFFF".to_owned(),
                        readback_required: true,
                        rollback_value: "SDK before snapshot".to_owned(),
                        rollback_instructions: Vec::new(),
                        reboot_required: false,
                        safety_notes: Vec::new(),
                        steps: Vec::new(),
                    },
                    "applied",
                    Some(readback.to_owned()),
                ),
            }],
        }
    }

    fn report_with_openrgb_sdk_snapshot<const N: usize>(
        active_mode: &str,
        colors: [(&str, &str); N],
    ) -> CapabilityRegistry {
        let sdk_colors = colors
            .into_iter()
            .map(|(zone, color)| (zone.to_owned(), color.to_owned()))
            .collect::<BTreeMap<_, _>>();
        CapabilityRegistry {
            keyboard_rgb_openrgb: Some(KeyboardRgbOpenRgbStatus {
                installed: true,
                path: Some("/usr/bin/openrgb".to_owned()),
                devices: vec![KeyboardRgbOpenRgbDevice {
                    index: 0,
                    name: "Lenovo 5 2023".to_owned(),
                    device_type: Some("Laptop".to_owned()),
                    description: Some("Lenovo 4-Zone device".to_owned()),
                    modes: vec!["Direct".to_owned(), "Breathing".to_owned()],
                    current_mode: Some(active_mode.to_owned()),
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
                backend_ready: true,
                write_support_claimed: true,
                sdk_helper_installed: true,
                sdk_helper_path: Some(
                    "/home/test/.local/bin/ratvantage-openrgb-keyboard-rgb-sdk-helper".to_owned(),
                ),
                sdk_server_running: true,
                sdk_snapshot_supported: true,
                sdk_active_mode: Some(active_mode.to_owned()),
                sdk_color_zones: sdk_colors.keys().cloned().collect(),
                sdk_colors,
            }),
            ..Default::default()
        }
    }

    #[test]
    fn runtime_refresh_notice_is_empty_for_steady_state() {
        let snapshot = sample_runtime_snapshot("balanced", "Standard", 2, 0, None);
        assert_eq!(
            runtime_refresh_notice(Some(&snapshot), &snapshot, false),
            None
        );
    }

    #[test]
    fn runtime_refresh_notice_reports_desktop_power_profile_drift() {
        let mut previous = sample_runtime_snapshot("balanced", "Standard", 2, 0, None);
        previous.diagnostics.raw_probe_report.power_profiles =
            Some(sample_power_profiles("power-saver"));
        let mut current = sample_runtime_snapshot("balanced", "Standard", 2, 0, None);
        current.diagnostics.raw_probe_report.power_profiles =
            Some(sample_power_profiles("balanced"));

        let notice = runtime_refresh_notice(Some(&previous), &current, false).unwrap();

        assert!(notice
            .message
            .contains("Desktop power profile changed from `power-saver` to `balanced`"));
    }

    fn sample_runtime_snapshot(
        profile: &str,
        charge_type: &str,
        available: usize,
        missing: usize,
        fan_snapshot: Option<FanCurveSnapshot>,
    ) -> RuntimeSnapshot {
        let hardware = HardwareSummary {
            sysfs_root: "/tmp/fixture".to_owned(),
            vendor: Some("LENOVO".to_owned()),
            product_name: Some("82WM".to_owned()),
            product_version: Some("Legion Pro 5 16ARX8".to_owned()),
            product_sku: None,
        };
        let capabilities = (0..available)
            .map(|index| Capability {
                id: format!("available-{index}"),
                label: format!("Available {index}"),
                status: CapabilityStatus::ProbeOnly,
                risk: RiskLevel::ReadOnly,
                evidence: vec![],
                details: serde_json::Value::Null,
            })
            .chain((0..missing).map(|index| Capability {
                id: format!("missing-{index}"),
                label: format!("Missing {index}"),
                status: CapabilityStatus::Missing,
                risk: RiskLevel::ReadOnly,
                evidence: vec![],
                details: serde_json::Value::Null,
            }))
            .collect::<Vec<_>>();
        let status = UiStatus::from_parts(hardware.clone(), capabilities.clone()).unwrap();
        let mut report = CapabilityRegistry {
            hardware,
            capabilities,
            ..Default::default()
        };
        report.platform_profile = Some(PlatformProfileCapability {
            current: Some(profile.to_owned()),
            choices: vec!["balanced".to_owned(), "performance".to_owned()],
            path: "/tmp/platform_profile".to_owned(),
            choices_path: "/tmp/platform_profile_choices".to_owned(),
            custom_profile_path: None,
            custom_profile_driver: None,
        });
        report.battery_charge_type = Some(legion_common::BatteryChargeTypeCapability {
            current: Some(charge_type.to_owned()),
            choices: vec![
                "Standard".to_owned(),
                "Long_Life".to_owned(),
                "Fast".to_owned(),
            ],
            path: "/tmp/charge_type".to_owned(),
            choices_path: "/tmp/charge_types".to_owned(),
        });
        let diagnostics = DiagnosticsBundle::from_report(report, Some("test-kernel".to_owned()))
            .with_runtime_state(DiagnosticsRuntimeState {
                last_known_good_fan_curve: fan_snapshot,
                ..Default::default()
            });

        RuntimeSnapshot {
            status,
            diagnostics,
        }
    }

    fn sample_fan_snapshot(curve_id: &str) -> FanCurveSnapshot {
        FanCurveSnapshot {
            curve_id: curve_id.to_owned(),
            path: Some("/tmp/hwmon".to_owned()),
            points: vec![FanCurvePointSnapshot {
                path: "/tmp/hwmon/pwm1_auto_point1_temp".to_owned(),
                value: "42000".to_owned(),
            }],
        }
    }

    fn sample_power_profiles(active_profile: &str) -> PowerProfilesCapability {
        PowerProfilesCapability {
            bus: "system".to_owned(),
            well_known_name: "org.freedesktop.UPower.PowerProfiles".to_owned(),
            unique_owner: Some(":1.42".to_owned()),
            active_profile: Some(active_profile.to_owned()),
            status: CapabilityStatus::ProbeOnly,
            detail: None,
        }
    }
}

fn push_path(paths: &mut Vec<String>, path: &str) {
    if !path.is_empty() {
        paths.push(path.to_owned());
    }
}

fn kernel_version() -> Option<String> {
    let output = Command::new("uname").arg("-r").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let version = String::from_utf8(output.stdout).ok()?;
    let version = version.trim();
    if version.is_empty() {
        None
    } else {
        Some(version.to_owned())
    }
}

fn recent_daemon_logs() -> Vec<String> {
    let output = Command::new("journalctl")
        .args([
            "-u",
            "legion-control-daemon.service",
            "-n",
            "40",
            "--no-pager",
            "--output=short-iso",
        ])
        .output();

    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    String::from_utf8(output.stdout)
        .ok()
        .map(|stdout| {
            stdout
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

impl LegionControlClient {
    pub fn system() -> Result<Self> {
        Ok(Self {
            connection: Connection::system()?,
        })
    }

    pub fn address(address: &str) -> Result<Self> {
        Ok(Self {
            connection: ConnectionBuilder::address(address)?.build()?,
        })
    }

    pub fn hardware_summary(&self) -> Result<HardwareSummary> {
        self.call_json("GetHardwareSummary")
    }

    pub fn capabilities(&self) -> Result<Vec<Capability>> {
        self.call_json("GetCapabilities")
    }

    pub fn refresh_capabilities(&self) -> Result<Vec<Capability>> {
        self.call_json("RefreshCapabilities")
    }

    pub fn telemetry(&self) -> Result<TelemetrySnapshot> {
        self.call_json("GetTelemetry")
    }

    pub fn raw_probe_report(&self) -> Result<CapabilityRegistry> {
        self.call_json("GetRawProbeReport")
    }

    pub fn diagnostics_bundle(&self) -> Result<DiagnosticsBundle> {
        Ok(DiagnosticsBundle::from_report_with_logs(
            self.raw_probe_report()?,
            kernel_version(),
            recent_daemon_logs(),
        )
        .with_runtime_state(DiagnosticsRuntimeState {
            gpu_mode_pending: self.gpu_mode_pending()?,
            last_known_good_fan_curve: self.last_known_good_fan_curve()?,
            live_fan_curve: self.live_fan_curve_readings().ok(),
            fan_preset_by_platform_profile: self.fan_preset_by_platform_profile()?,
            fan_preset_reapply_after_resume: self.fan_preset_reapply_after_resume()?,
            hardware_profiles: self.hardware_profiles()?,
            hardware_profile_triggers: self.hardware_profile_triggers()?,
            automation_rules: self.automation_rules()?,
            last_automation_rule_apply: self.last_automation_rule_apply()?,
            ryzen_backend_status: Some(self.ryzen_backend_status()?),
            last_hardware_profile_apply: self.last_hardware_profile_apply()?,
            recent_platform_profile_changes: self
                .recent_platform_profile_changes()
                .unwrap_or_default(),
            recent_desktop_power_profile_changes: self
                .recent_desktop_power_profile_changes()
                .unwrap_or_default(),
        }))
    }

    pub fn refresh_runtime_snapshot(&self) -> Result<RuntimeSnapshot> {
        let capabilities = self.refresh_capabilities()?;
        let report = self.raw_probe_report()?;
        let gpu_pending = self.gpu_mode_pending()?;
        let fan_snapshot = self.last_known_good_fan_curve()?;
        let live_fan_curve = self.live_fan_curve_readings().ok();
        let fan_preset_map = self.fan_preset_by_platform_profile()?;
        let hardware_profiles = self.hardware_profiles()?;
        let hardware_profile_triggers = self.hardware_profile_triggers()?;
        let automation_rules = self.automation_rules()?;
        let last_automation_rule_apply = self.last_automation_rule_apply()?;
        let ryzen_backend_status = Some(self.ryzen_backend_status()?);
        let last_hardware_profile_apply = self.last_hardware_profile_apply()?;
        let recent_platform_profile_changes =
            self.recent_platform_profile_changes().unwrap_or_default();
        let recent_desktop_power_profile_changes = self
            .recent_desktop_power_profile_changes()
            .unwrap_or_default();
        Ok(RuntimeSnapshot {
            status: UiStatus::from_parts(report.hardware.clone(), capabilities)?,
            diagnostics: DiagnosticsBundle::from_report_with_logs(
                report,
                kernel_version(),
                recent_daemon_logs(),
            )
            .with_runtime_state(DiagnosticsRuntimeState {
                gpu_mode_pending: gpu_pending,
                last_known_good_fan_curve: fan_snapshot,
                live_fan_curve,
                fan_preset_by_platform_profile: fan_preset_map,
                fan_preset_reapply_after_resume: self.fan_preset_reapply_after_resume()?,
                hardware_profiles,
                hardware_profile_triggers,
                automation_rules,
                last_automation_rule_apply,
                ryzen_backend_status,
                last_hardware_profile_apply,
                recent_platform_profile_changes,
                recent_desktop_power_profile_changes,
            }),
        })
    }

    pub fn plan_platform_profile_write(&self, requested: &str) -> Result<WriteDryRunPlan> {
        self.call_json_arg("PlanPlatformProfileWrite", requested)
    }

    pub fn plan_prepare_custom_thermal_mode(&self) -> Result<WriteDryRunPlan> {
        self.call_json("PlanPrepareCustomThermalMode")
    }

    pub fn set_platform_profile(
        &self,
        requested: &str,
    ) -> Result<legion_common::WriteExecutionResult> {
        self.call_json_arg("SetPlatformProfile", requested)
    }

    pub fn set_platform_and_desktop_profile(
        &self,
        requested: &str,
    ) -> Result<legion_common::WriteExecutionResult> {
        let diagnostics = self.diagnostics_bundle()?;
        let current_platform_profile = diagnostics
            .raw_probe_report
            .platform_profile
            .as_ref()
            .and_then(|profile| profile.current.clone());
        let mapped_profile_id = (current_platform_profile.as_deref() == Some(requested))
            .then(|| mapped_platform_profile(&diagnostics.automation_rules, requested))
            .flatten();
        let current_desktop_profile = diagnostics
            .raw_probe_report
            .power_profiles
            .as_ref()
            .map(|_| ppd_active_profile())
            .transpose()?;

        if let (Some(previous_platform_profile), Some(previous_desktop_profile)) = (
            current_platform_profile.as_deref(),
            current_desktop_profile.as_deref(),
        ) {
            if previous_platform_profile != requested
                && desktop_profile_for_platform_profile(requested).is_some()
            {
                return set_transitioning_platform_profile_with_desktop_sync(
                    requested,
                    previous_platform_profile,
                    previous_desktop_profile,
                    |profile| self.set_platform_profile(profile),
                    set_ppd_active_profile,
                    |result| result.applied,
                );
            }
        }

        set_platform_profile_with_desktop_sync(
            requested,
            current_desktop_profile.as_deref(),
            set_ppd_active_profile,
            |profile| {
                if let Some(profile_id) = mapped_profile_id {
                    let run = self.apply_hardware_profile(profile_id)?;
                    return run
                        .results
                        .last()
                        .map(|action| action.result.clone())
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "mapped hardware profile `{profile_id}` returned no action results"
                            )
                        });
                }
                self.set_platform_profile(profile)
            },
            |result| result.applied,
        )
    }

    pub fn plan_battery_charge_type_write(&self, requested: &str) -> Result<WriteDryRunPlan> {
        self.call_json_arg("PlanBatteryChargeTypeWrite", requested)
    }

    pub fn set_battery_charge_type(&self, requested: &str) -> Result<WriteExecutionResult> {
        self.call_json_arg("SetBatteryChargeType", requested)
    }

    pub fn plan_led_state_write(&self, led_id: &str, enabled: bool) -> Result<WriteDryRunPlan> {
        let proxy = Proxy::new(&self.connection, DBUS_INTERFACE, DBUS_PATH, DBUS_INTERFACE)?;
        let payload: String = proxy.call("PlanLedStateWrite", &(led_id, enabled))?;
        Ok(serde_json::from_str(&payload)?)
    }

    pub fn plan_keyboard_rgb_write(
        &self,
        request: &KeyboardRgbWriteRequest,
    ) -> Result<WriteDryRunPlan> {
        let proxy = Proxy::new(&self.connection, DBUS_INTERFACE, DBUS_PATH, DBUS_INTERFACE)?;
        let request_json = serde_json::to_string(request)?;
        let payload: String = proxy.call("PlanKeyboardRgbWrite", &(request_json,))?;
        Ok(serde_json::from_str(&payload)?)
    }

    pub fn plan_openrgb_keyboard_rgb_bridge(
        &self,
        request: &KeyboardRgbWriteRequest,
    ) -> Result<WriteDryRunPlan> {
        let proxy = Proxy::new(&self.connection, DBUS_INTERFACE, DBUS_PATH, DBUS_INTERFACE)?;
        let request_json = serde_json::to_string(request)?;
        let payload: String = proxy.call("PlanOpenRgbKeyboardRgbBridge", &(request_json,))?;
        Ok(serde_json::from_str(&payload)?)
    }

    pub fn plan_openrgb_keyboard_rgb_sdk_write(
        &self,
        request: &KeyboardRgbWriteRequest,
    ) -> Result<WriteDryRunPlan> {
        let proxy = Proxy::new(&self.connection, DBUS_INTERFACE, DBUS_PATH, DBUS_INTERFACE)?;
        let request_json = serde_json::to_string(request)?;
        let payload: String = proxy.call("PlanOpenRgbKeyboardRgbSdkWrite", &(request_json,))?;
        Ok(serde_json::from_str(&payload)?)
    }

    pub fn plan_openrgb_access_setup(&self, target_user: &str) -> Result<WriteDryRunPlan> {
        self.call_json_arg("PlanOpenRgbAccessSetup", target_user)
    }

    pub fn set_keyboard_rgb(
        &self,
        request: &KeyboardRgbWriteRequest,
    ) -> Result<WriteExecutionResult> {
        let proxy = Proxy::new(&self.connection, DBUS_INTERFACE, DBUS_PATH, DBUS_INTERFACE)?;
        let request_json = serde_json::to_string(request)?;
        let payload: String = proxy.call("SetKeyboardRgb", &(request_json,))?;
        Ok(serde_json::from_str(&payload)?)
    }

    pub fn setup_openrgb_access(&self, target_user: &str) -> Result<WriteExecutionResult> {
        self.call_json_arg("SetupOpenRgbAccess", target_user)
    }

    pub fn set_led_state(&self, led_id: &str, enabled: bool) -> Result<WriteExecutionResult> {
        let proxy = Proxy::new(&self.connection, DBUS_INTERFACE, DBUS_PATH, DBUS_INTERFACE)?;
        let payload: String = proxy.call("SetLedState", &(led_id, enabled))?;
        Ok(serde_json::from_str(&payload)?)
    }

    pub fn plan_ideapad_toggle_write(
        &self,
        toggle_id: &str,
        enabled: bool,
    ) -> Result<WriteDryRunPlan> {
        let proxy = Proxy::new(&self.connection, DBUS_INTERFACE, DBUS_PATH, DBUS_INTERFACE)?;
        let payload: String = proxy.call("PlanIdeapadToggleWrite", &(toggle_id, enabled))?;
        Ok(serde_json::from_str(&payload)?)
    }

    pub fn set_ideapad_toggle(
        &self,
        toggle_id: &str,
        enabled: bool,
    ) -> Result<WriteExecutionResult> {
        let proxy = Proxy::new(&self.connection, DBUS_INTERFACE, DBUS_PATH, DBUS_INTERFACE)?;
        let payload: String = proxy.call("SetIdeapadToggle", &(toggle_id, enabled))?;
        Ok(serde_json::from_str(&payload)?)
    }

    pub fn plan_cpu_governor_write(&self, requested: &str) -> Result<WriteDryRunPlan> {
        self.call_json_arg("PlanCpuGovernorWrite", requested)
    }

    pub fn set_cpu_governor(&self, requested: &str) -> Result<WriteExecutionResult> {
        self.call_json_arg("SetCpuGovernor", requested)
    }

    pub fn plan_cpu_epp_write(&self, requested: &str) -> Result<WriteDryRunPlan> {
        self.call_json_arg("PlanCpuEppWrite", requested)
    }

    pub fn set_cpu_epp(&self, requested: &str) -> Result<WriteExecutionResult> {
        self.call_json_arg("SetCpuEpp", requested)
    }

    pub fn plan_cpu_max_frequency_write(&self, requested_khz: i64) -> Result<WriteDryRunPlan> {
        self.call_json_i64_arg("PlanCpuMaxFrequencyWrite", requested_khz)
    }

    pub fn set_cpu_max_frequency(&self, requested_khz: i64) -> Result<WriteExecutionResult> {
        self.call_json_i64_arg("SetCpuMaxFrequency", requested_khz)
    }

    pub fn plan_cpu_boost_write(&self, requested: &str) -> Result<WriteDryRunPlan> {
        self.call_json_arg("PlanCpuBoostWrite", requested)
    }

    pub fn set_cpu_boost(&self, requested: &str) -> Result<WriteExecutionResult> {
        self.call_json_arg("SetCpuBoost", requested)
    }

    pub fn plan_curve_optimizer_all_core_write(&self, requested: &str) -> Result<WriteDryRunPlan> {
        self.call_json_arg("PlanCurveOptimizerAllCoreWrite", requested)
    }

    pub fn set_curve_optimizer_all_core(&self, requested: &str) -> Result<WriteExecutionResult> {
        self.call_json_arg("SetCurveOptimizerAllCore", requested)
    }

    pub fn last_curve_optimizer_all_core(&self) -> Result<Option<CurveOptimizerWriteState>> {
        self.call_json("GetLastCurveOptimizerAllCore")
    }

    pub fn ryzen_backend_status(&self) -> Result<RyzenBackendStatus> {
        self.call_json("GetRyzenBackendStatus")
    }

    pub fn plan_conservation_mode_write(&self, requested: &str) -> Result<WriteDryRunPlan> {
        self.call_json_arg("PlanConservationModeWrite", requested)
    }

    pub fn set_conservation_mode(&self, requested: &str) -> Result<WriteExecutionResult> {
        self.call_json_arg("SetConservationMode", requested)
    }

    pub fn plan_amd_gpu_dpm_force_level_write(&self, requested: &str) -> Result<WriteDryRunPlan> {
        self.call_json_arg("PlanAmdGpuDpmForceLevelWrite", requested)
    }

    pub fn set_amd_gpu_dpm_force_level(&self, requested: &str) -> Result<WriteExecutionResult> {
        self.call_json_arg("SetAmdGpuDpmForceLevel", requested)
    }

    pub fn plan_firmware_attribute_write(
        &self,
        attribute_id: &str,
        requested: &str,
    ) -> Result<WriteDryRunPlan> {
        self.call_json_two_args("PlanFirmwareAttributeWrite", attribute_id, requested)
    }

    pub fn plan_firmware_attribute_reset_write(
        &self,
        attribute_id: &str,
    ) -> Result<WriteDryRunPlan> {
        self.call_json_arg("PlanFirmwareAttributeResetWrite", attribute_id)
    }

    pub fn plan_custom_thermal_firmware_attribute_write(
        &self,
        attribute_id: &str,
        requested: &str,
    ) -> Result<CustomThermalPlanPreview> {
        self.call_json_two_args(
            "PlanCustomThermalFirmwareAttributeWrite",
            attribute_id,
            requested,
        )
    }

    pub fn plan_custom_thermal_firmware_ppt_preset_write(
        &self,
        preset_id: &str,
    ) -> Result<CustomThermalPlanPreview> {
        self.call_json_arg("PlanCustomThermalFirmwarePptPresetWrite", preset_id)
    }

    pub fn set_firmware_attribute(
        &self,
        attribute_id: &str,
        requested: &str,
    ) -> Result<WriteExecutionResult> {
        self.call_json_two_args("SetFirmwareAttribute", attribute_id, requested)
    }

    pub fn plan_gpu_mode_write(&self, requested: &str) -> Result<WriteDryRunPlan> {
        self.call_json_arg("PlanGpuModeWrite", requested)
    }

    pub fn plan_gpu_mode_runtime_write(&self, requested: &str) -> Result<WriteDryRunPlan> {
        self.call_json_arg("PlanGpuModeRuntimeWrite", requested)
    }

    pub fn set_gpu_mode(&self, requested: &str) -> Result<WriteExecutionResult> {
        self.call_json_arg("SetGpuMode", requested)
    }

    pub fn plan_fan_preset_write(&self, requested: &str) -> Result<WriteDryRunPlan> {
        self.call_json_arg("PlanFanPresetWrite", requested)
    }

    pub fn plan_custom_thermal_fan_preset_write(
        &self,
        requested: &str,
    ) -> Result<CustomThermalPlanPreview> {
        self.call_json_arg("PlanCustomThermalFanPresetWrite", requested)
    }

    pub fn plan_restore_auto_fan_write(&self) -> Result<WriteDryRunPlan> {
        self.call_json("PlanRestoreAutoFanWrite")
    }

    pub fn plan_custom_thermal_restore_auto_fan(&self) -> Result<CustomThermalPlanPreview> {
        self.call_json("PlanCustomThermalRestoreAutoFanWrite")
    }

    pub fn gpu_mode_pending(&self) -> Result<Option<GpuModePending>> {
        self.call_json("GetGpuModePending")
    }

    pub fn set_gpu_mode_pending(&self, requested: &str) -> Result<GpuModePending> {
        self.call_json_arg("SetGpuModePending", requested)
    }

    pub fn clear_gpu_mode_pending(&self) -> Result<Option<GpuModePending>> {
        self.call_json("ClearGpuModePending")
    }

    pub fn hardware_profiles(&self) -> Result<BTreeMap<String, HardwareProfile>> {
        self.call_json("GetHardwareProfiles")
    }

    pub fn hardware_profile_triggers(&self) -> Result<BTreeMap<String, String>> {
        self.call_json("GetHardwareProfileTriggers")
    }

    pub fn automation_rules(&self) -> Result<BTreeMap<String, AutomationRule>> {
        self.call_json("GetAutomationRules")
    }

    pub fn last_automation_rule_apply(&self) -> Result<BTreeMap<String, AutomationRuleApplyRun>> {
        self.call_json("GetLastAutomationRuleApply")
    }

    pub fn recent_platform_profile_changes(&self) -> Result<Vec<PlatformProfileChangeEvent>> {
        self.call_json("GetRecentPlatformProfileChanges")
    }

    pub fn recent_desktop_power_profile_changes(
        &self,
    ) -> Result<Vec<DesktopPowerProfileChangeEvent>> {
        self.call_json("GetRecentDesktopPowerProfileChanges")
    }

    pub fn automation_rule_preview(&self, rule_id: &str) -> Result<AutomationRuleEvaluation> {
        self.call_json_arg("GetAutomationRulePreview", rule_id)
    }

    pub fn apply_automation_rule(&self, rule_id: &str) -> Result<AutomationRuleApplyRun> {
        self.call_json_arg("ApplyAutomationRule", rule_id)
    }

    pub fn hardware_profile_apply_preview(
        &self,
        profile_id: &str,
    ) -> Result<HardwareProfileApplyPreview> {
        self.call_json_arg("GetHardwareProfileApplyPreview", profile_id)
    }

    pub fn hardware_profile_trigger_apply_preview(
        &self,
        trigger_id: &str,
    ) -> Result<HardwareProfileApplyPreview> {
        self.call_json_arg("GetHardwareProfileTriggerApplyPreview", trigger_id)
    }

    pub fn last_hardware_profile_apply(&self) -> Result<Option<HardwareProfileApplyRun>> {
        self.call_json("GetLastHardwareProfileApply")
    }

    pub fn apply_hardware_profile(&self, profile_id: &str) -> Result<HardwareProfileApplyRun> {
        self.call_json_arg("ApplyHardwareProfile", profile_id)
    }

    pub fn apply_hardware_profile_trigger(
        &self,
        trigger_id: &str,
    ) -> Result<HardwareProfileApplyRun> {
        self.call_json_arg("ApplyHardwareProfileTrigger", trigger_id)
    }

    pub fn set_hardware_profile(
        &self,
        profile_id: &str,
        profile_json: &str,
    ) -> Result<HardwareProfileApplyPreview> {
        self.call_json_two_args("SetHardwareProfile", profile_id, profile_json)
    }

    pub fn set_hardware_profile_trigger(
        &self,
        trigger_id: &str,
        profile_id: &str,
    ) -> Result<BTreeMap<String, String>> {
        self.call_json_two_args("SetHardwareProfileTrigger", trigger_id, profile_id)
    }

    pub fn set_automation_rule(
        &self,
        rule_id: &str,
        rule_json: &str,
    ) -> Result<BTreeMap<String, AutomationRule>> {
        self.call_json_two_args("SetAutomationRule", rule_id, rule_json)
    }

    pub fn remove_automation_rule(&self, rule_id: &str) -> Result<Option<AutomationRule>> {
        self.call_json_arg("RemoveAutomationRule", rule_id)
    }

    pub fn clear_automation_rules(&self) -> Result<BTreeMap<String, AutomationRule>> {
        self.call_json("ClearAutomationRules")
    }

    pub fn remove_hardware_profile_trigger(&self, trigger_id: &str) -> Result<Option<String>> {
        self.call_json_arg("RemoveHardwareProfileTrigger", trigger_id)
    }

    pub fn clear_hardware_profile_triggers(&self) -> Result<BTreeMap<String, String>> {
        self.call_json("ClearHardwareProfileTriggers")
    }

    pub fn remove_hardware_profile(&self, profile_id: &str) -> Result<Option<HardwareProfile>> {
        self.call_json_arg("RemoveHardwareProfile", profile_id)
    }

    pub fn clear_hardware_profiles(&self) -> Result<BTreeMap<String, HardwareProfile>> {
        self.call_json("ClearHardwareProfiles")
    }

    pub fn last_known_good_fan_curve(&self) -> Result<Option<FanCurveSnapshot>> {
        self.call_json("GetLastKnownGoodFanCurve")
    }

    /// Read current fan curve sysfs values without persisting the last-known-good snapshot.
    pub fn live_fan_curve_readings(&self) -> Result<FanCurveSnapshot> {
        self.call_json("GetLiveFanCurveReadings")
    }

    pub fn capture_last_known_good_fan_curve(&self) -> Result<FanCurveSnapshot> {
        self.call_json("CaptureLastKnownGoodFanCurve")
    }

    pub fn fan_preset_by_platform_profile(&self) -> Result<BTreeMap<String, String>> {
        self.call_json("GetFanPresetProfileMap")
    }

    pub fn set_fan_preset_profile_map_entry(
        &self,
        platform_profile: &str,
        fan_preset_id: &str,
    ) -> Result<BTreeMap<String, String>> {
        self.call_json_two_args(
            "SetFanPresetProfileMapEntry",
            platform_profile,
            fan_preset_id,
        )
    }

    pub fn remove_fan_preset_profile_map_entry(
        &self,
        platform_profile: &str,
    ) -> Result<BTreeMap<String, String>> {
        self.call_json_arg("RemoveFanPresetProfileMapEntry", platform_profile)
    }

    pub fn clear_fan_preset_profile_map(&self) -> Result<BTreeMap<String, String>> {
        self.call_json("ClearFanPresetProfileMap")
    }

    pub fn fan_preset_reapply_after_resume(&self) -> Result<bool> {
        self.call_json("GetFanPresetReapplyAfterResume")
    }

    pub fn set_fan_preset_reapply_after_resume(&self, enabled: bool) -> Result<bool> {
        let proxy = Proxy::new(&self.connection, DBUS_INTERFACE, DBUS_PATH, DBUS_INTERFACE)?;
        let payload: String = proxy.call("SetFanPresetReapplyAfterResume", &(enabled,))?;
        Ok(serde_json::from_str(&payload)?)
    }

    pub fn status(&self) -> Result<UiStatus> {
        UiStatus::from_client(self)
    }

    fn call_json<T>(&self, method: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let proxy = Proxy::new(&self.connection, DBUS_INTERFACE, DBUS_PATH, DBUS_INTERFACE)?;
        let payload: String = proxy.call(method, &())?;
        Ok(serde_json::from_str(&payload)?)
    }

    fn call_json_arg<T>(&self, method: &str, arg: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let proxy = Proxy::new(&self.connection, DBUS_INTERFACE, DBUS_PATH, DBUS_INTERFACE)?;
        let payload: String = proxy.call(method, &(arg,))?;
        Ok(serde_json::from_str(&payload)?)
    }

    fn call_json_i64_arg<T>(&self, method: &str, arg: i64) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let proxy = Proxy::new(&self.connection, DBUS_INTERFACE, DBUS_PATH, DBUS_INTERFACE)?;
        let payload: String = proxy.call(method, &(arg,))?;
        Ok(serde_json::from_str(&payload)?)
    }

    fn call_json_two_args<T>(&self, method: &str, first: &str, second: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let proxy = Proxy::new(&self.connection, DBUS_INTERFACE, DBUS_PATH, DBUS_INTERFACE)?;
        let payload: String = proxy.call(method, &(first, second))?;
        Ok(serde_json::from_str(&payload)?)
    }
}

const PPD_BUS_NAME: &str = "org.freedesktop.UPower.PowerProfiles";
const PPD_OBJECT_PATH: &str = "/org/freedesktop/UPower/PowerProfiles";
const PPD_INTERFACE: &str = "org.freedesktop.UPower.PowerProfiles";

/// Standard desktop power profile choices exposed by power-profiles-daemon.
pub const PPD_PROFILE_CHOICES: &[&str] = &["power-saver", "balanced", "performance"];

/// Map a firmware platform profile to its matching desktop power profile.
/// Custom firmware modes intentionally leave the desktop profile unchanged.
pub fn desktop_profile_for_platform_profile(profile: &str) -> Option<&'static str> {
    match profile {
        "low-power" => Some("power-saver"),
        "balanced" => Some("balanced"),
        "performance" | "max-power" => Some("performance"),
        _ => None,
    }
}

fn mapped_platform_profile<'a>(
    rules: &'a BTreeMap<String, AutomationRule>,
    requested: &str,
) -> Option<&'a str> {
    rules.values().find_map(|rule| {
        if !rule.enabled {
            return None;
        }
        let AutomationRuleKind::PlatformProfileRouter { mappings, .. } = &rule.kind else {
            return None;
        };
        mappings.get(requested).map(String::as_str)
    })
}

fn set_platform_profile_with_desktop_sync<T>(
    requested: &str,
    current_desktop_profile: Option<&str>,
    mut set_desktop_profile: impl FnMut(&str) -> Result<()>,
    set_platform_profile: impl FnOnce(&str) -> Result<T>,
    platform_profile_applied: impl FnOnce(&T) -> bool,
) -> Result<T> {
    let Some(requested_desktop_profile) = desktop_profile_for_platform_profile(requested) else {
        return set_platform_profile(requested);
    };
    let Some(previous_desktop_profile) = current_desktop_profile else {
        return set_platform_profile(requested);
    };

    let desktop_profile_changed = previous_desktop_profile != requested_desktop_profile;
    if desktop_profile_changed {
        if let Err(desktop_error) = set_desktop_profile(requested_desktop_profile) {
            if let Err(rollback_error) = set_desktop_profile(previous_desktop_profile) {
                return Err(anyhow::anyhow!(
                    "desktop profile change failed: {desktop_error}; rollback to `{previous_desktop_profile}` also failed: {rollback_error}"
                ));
            }
            return Err(desktop_error);
        }
    }

    match set_platform_profile(requested) {
        Ok(result) if platform_profile_applied(&result) => Ok(result),
        Ok(result) => {
            if desktop_profile_changed {
                set_desktop_profile(previous_desktop_profile).map_err(|rollback_error| {
                    anyhow::anyhow!(
                        "platform profile was not applied and desktop profile rollback to `{previous_desktop_profile}` failed: {rollback_error}"
                    )
                })?;
            }
            Ok(result)
        }
        Err(platform_error) => {
            if desktop_profile_changed {
                if let Err(rollback_error) = set_desktop_profile(previous_desktop_profile) {
                    return Err(anyhow::anyhow!(
                        "platform profile change failed: {platform_error}; desktop profile rollback to `{previous_desktop_profile}` also failed: {rollback_error}"
                    ));
                }
            }
            Err(platform_error)
        }
    }
}

fn set_transitioning_platform_profile_with_desktop_sync<T>(
    requested: &str,
    previous_platform_profile: &str,
    previous_desktop_profile: &str,
    mut set_platform_profile: impl FnMut(&str) -> Result<T>,
    mut set_desktop_profile: impl FnMut(&str) -> Result<()>,
    platform_profile_applied: impl Fn(&T) -> bool,
) -> Result<T> {
    let requested_desktop_profile = desktop_profile_for_platform_profile(requested)
        .ok_or_else(|| anyhow::anyhow!("firmware profile `{requested}` has no desktop mapping"))?;
    let result = set_platform_profile(requested)?;
    if !platform_profile_applied(&result) {
        return Ok(result);
    }
    if previous_desktop_profile == requested_desktop_profile {
        return Ok(result);
    }

    if let Err(desktop_error) = set_desktop_profile(requested_desktop_profile) {
        match set_platform_profile(previous_platform_profile) {
            Ok(rollback) if platform_profile_applied(&rollback) => {
                return Err(anyhow::anyhow!(
                    "desktop profile change failed after firmware transition: {desktop_error}; restored firmware profile `{previous_platform_profile}`"
                ));
            }
            Ok(_) => {
                return Err(anyhow::anyhow!(
                    "desktop profile change failed after firmware transition: {desktop_error}; firmware rollback to `{previous_platform_profile}` was not applied"
                ));
            }
            Err(rollback_error) => {
                return Err(anyhow::anyhow!(
                    "desktop profile change failed after firmware transition: {desktop_error}; firmware rollback to `{previous_platform_profile}` also failed: {rollback_error}"
                ));
            }
        }
    }

    Ok(result)
}

/// Read the active power-profiles-daemon profile from the system bus.
pub fn ppd_active_profile() -> Result<String> {
    let connection = Connection::system()?;
    let proxy = Proxy::new(&connection, PPD_BUS_NAME, PPD_OBJECT_PATH, PPD_INTERFACE)?;
    Ok(proxy.get_property("ActiveProfile")?)
}

/// Set the active power-profiles-daemon profile directly on the system bus.
/// PPD allows regular session users to change the active profile; no daemon proxy needed.
pub fn set_ppd_active_profile(profile: &str) -> Result<()> {
    use zbus::zvariant::Value;
    let connection = Connection::system()?;
    let proxy = Proxy::new(
        &connection,
        PPD_BUS_NAME,
        PPD_OBJECT_PATH,
        "org.freedesktop.DBus.Properties",
    )?;
    proxy.call::<_, _, ()>(
        "Set",
        &(
            PPD_INTERFACE,
            "ActiveProfile",
            Value::from(profile.to_owned()),
        ),
    )?;
    let active_profile = ppd_active_profile()?;
    if active_profile != profile {
        anyhow::bail!(
            "power-profiles-daemon read-back mismatch: requested `{profile}`, current `{active_profile}`"
        );
    }
    Ok(())
}
