use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};

use anyhow::Result;
use legion_common::{
    encode_curve_optimizer_offset, format_gpu_mode_pending_summary,
    plan_amd_gpu_dpm_force_level_write as plan_amd_gpu_dpm_force_level,
    plan_battery_charge_type_write as plan_battery_charge_type,
    plan_conservation_mode_write as plan_conservation_mode, plan_cpu_boost_write as plan_cpu_boost,
    plan_cpu_epp_write as plan_cpu_epp, plan_cpu_governor_write as plan_cpu_governor,
    plan_curve_optimizer_all_core_write as plan_curve_optimizer_all_core,
    plan_fan_preset_write_with_platform_profile as plan_fan_preset,
    plan_firmware_attribute_reset_write_with_platform_profile as plan_firmware_attribute_reset,
    plan_firmware_attribute_write_with_platform_profile as plan_firmware_attribute,
    plan_firmware_ppt_preset_write_with_platform_profile as plan_firmware_ppt_preset,
    plan_gpu_mode_write as plan_gpu_mode,
    plan_hardware_profile_keyboard_rgb_write as plan_hardware_profile_keyboard_rgb,
    plan_ideapad_toggle_write as plan_ideapad_toggle, plan_keyboard_rgb_write as plan_keyboard_rgb,
    plan_led_state_write as plan_led_state, plan_openrgb_keyboard_rgb_bridge,
    plan_openrgb_keyboard_rgb_sdk_write as plan_openrgb_keyboard_rgb_sdk,
    plan_platform_profile_write as plan_platform_profile, plan_prepare_custom_thermal_mode,
    plan_restore_auto_fan_write_with_platform_profile as plan_restore_auto_fan,
    validate_automation_rule, validate_automation_rule_id,
    validate_curve_optimizer_all_core_offset, validate_fan_preset_platform_profile_entry,
    validate_gpu_mode_choice, validate_hardware_profile_id, validate_hardware_profile_trigger_id,
    AutomationRule, AutomationRuleApplyRun, AutomationRuleEvaluation, AutomationRuleKind,
    CapabilityRegistry, CurveOptimizerReadbackStatus, CurveOptimizerWriteState,
    CustomThermalPlanPreview, DaemonState, FanCurveSnapshot, FanPreset, GpuModePending,
    HardwareProfile, HardwareProfileActions, HardwareProfileApplyActionResult,
    HardwareProfileApplyPreview, HardwareProfileApplyRun, IdeapadToggleCapability,
    KeyboardRgbCapability, KeyboardRgbWriteRequest, LedCapability, PlatformProfileChangeEvent,
    RyzenAdjBackendStatus, RyzenBackendStatus, RyzenSmuBackendStatus, RyzenSmuSetupAssistant,
    ValidationError, WriteDryRunPlan, WriteExecutionResult, WritePlanStep,
};
use legion_probe::{probe, ProbeOptions};
use serde::{Deserialize, Serialize};
use zbus::{
    blocking::{Connection, ConnectionBuilder, MessageIterator},
    fdo, interface,
    message::{Header, Type},
    MatchRule,
};

pub const DBUS_INTERFACE: &str = "org.ratvantage.LegionControl1";
pub const DBUS_PATH: &str = "/org/ratvantage/LegionControl1";
pub const READ_ONLY_METHODS: &str = "CaptureLastKnownGoodFanCurve,ClearAutomationRules,ClearFanPresetProfileMap,ClearGpuModePending,ClearHardwareProfileTriggers,ClearHardwareProfiles,GetAutomationRulePreview,GetAutomationRules,GetCapabilities,GetFanPresetProfileMap,GetFanPresetReapplyAfterResume,GetGpuModePending,GetHardwareProfileApplyPreview,GetHardwareProfileTriggerApplyPreview,GetHardwareProfileTriggers,GetHardwareProfiles,GetHardwareSummary,GetLastAutomationRuleApply,GetLastCurveOptimizerAllCore,GetLastHardwareProfileApply,GetLastKnownGoodFanCurve,GetLiveFanCurveReadings,GetRawProbeReport,GetRecentPlatformProfileChanges,GetRyzenBackendStatus,GetTelemetry,PlanAmdGpuDpmForceLevelWrite,PlanBatteryChargeTypeWrite,PlanConservationModeWrite,PlanCpuBoostWrite,PlanCpuEppWrite,PlanCpuGovernorWrite,PlanCurveOptimizerAllCoreWrite,PlanCustomThermalFanPresetWrite,PlanCustomThermalFirmwareAttributeWrite,PlanCustomThermalFirmwarePptPresetWrite,PlanCustomThermalRestoreAutoFanWrite,PlanFanPresetWrite,PlanFirmwareAttributeResetWrite,PlanFirmwareAttributeWrite,PlanGpuModeWrite,PlanIdeapadToggleWrite,PlanKeyboardRgbWrite,PlanLedStateWrite,PlanOpenRgbAccessSetup,PlanOpenRgbKeyboardRgbBridge,PlanOpenRgbKeyboardRgbSdkWrite,PlanPlatformProfileWrite,PlanPrepareCustomThermalMode,PlanRestoreAutoFanWrite,RefreshCapabilities,RemoveAutomationRule,RemoveFanPresetProfileMapEntry,RemoveHardwareProfile,RemoveHardwareProfileTrigger,SetAutomationRule,SetFanPresetProfileMapEntry,SetFanPresetReapplyAfterResume,SetGpuModePending,SetHardwareProfile,SetHardwareProfileTrigger";
pub const GATED_WRITE_METHODS: &str =
    "SetPlatformProfile,SetBatteryChargeType,SetLedState,SetKeyboardRgb,SetIdeapadToggle,SetGpuMode,SetCpuGovernor,SetCpuEpp,SetFirmwareAttribute,SetCpuBoost,SetConservationMode,SetAmdGpuDpmForceLevel,SetCurveOptimizerAllCore,SetupOpenRgbAccess,ApplyHardwareProfile,ApplyHardwareProfileTrigger,ApplyAutomationRule";
pub const DEFAULT_STATE_PATH: &str = "/var/lib/legion-control/state.toml";
const AMD_GPU_POWER_PROFILE_SYNC_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);
const RECENT_PLATFORM_PROFILE_CHANGE_LIMIT: usize = 20;

const PACKAGED_FAN_PRESETS: &[&str] = &[
    include_str!("../../../data/presets/quiet-office.toml"),
    include_str!("../../../data/presets/balanced-daily.toml"),
    include_str!("../../../data/presets/gaming.toml"),
    include_str!("../../../data/presets/max-safe.toml"),
];

const PLATFORM_PROFILE_WRITE_METHOD: &str = "SetPlatformProfile";
const BATTERY_CHARGE_TYPE_WRITE_METHOD: &str = "SetBatteryChargeType";
const LED_STATE_WRITE_METHOD: &str = "SetLedState";
const KEYBOARD_RGB_WRITE_METHOD: &str = "SetKeyboardRgb";
const IDEAPAD_TOGGLE_WRITE_METHOD: &str = "SetIdeapadToggle";
const GPU_MODE_WRITE_METHOD: &str = "SetGpuMode";
const CPU_GOVERNOR_WRITE_METHOD: &str = "SetCpuGovernor";
const CPU_EPP_WRITE_METHOD: &str = "SetCpuEpp";
const FIRMWARE_ATTRIBUTE_WRITE_METHOD: &str = "SetFirmwareAttribute";
const CPU_BOOST_WRITE_METHOD: &str = "SetCpuBoost";
const CONSERVATION_MODE_WRITE_METHOD: &str = "SetConservationMode";
const AMD_GPU_DPM_FORCE_LEVEL_WRITE_METHOD: &str = "SetAmdGpuDpmForceLevel";
const CURVE_OPTIMIZER_ALL_CORE_WRITE_METHOD: &str = "SetCurveOptimizerAllCore";
const OPENRGB_ACCESS_SETUP_METHOD: &str = "SetupOpenRgbAccess";
const HARDWARE_PROFILE_APPLY_METHOD: &str = "ApplyHardwareProfile";
const HARDWARE_PROFILE_TRIGGER_APPLY_METHOD: &str = "ApplyHardwareProfileTrigger";
const PKCHECK_MISSING: &str = "pkcheck is required for polkit authorization";
const AUTOMATION_OBSERVER_SENDER: &str = "ratvantage.automation-observer";
const RESUME_OBSERVER_SENDER: &str = "ratvantage.resume-observer";
const AUTOMATION_OBSERVER_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);
const AUTOMATION_OBSERVER_COOLDOWN_SECS: u64 = 300;

#[derive(Debug, Clone, Default)]
pub struct WriteAccessPolicy {
    pub platform_profile_enabled: bool,
    pub battery_charge_type_enabled: bool,
    pub led_state_enabled: bool,
    pub keyboard_rgb_enabled: bool,
    pub ideapad_toggle_enabled: bool,
    pub camera_power_enabled: bool,
    pub usb_charging_enabled: bool,
    pub fan_mode_enabled: bool,
    pub gpu_mode_enabled: bool,
    pub cpu_governor_enabled: bool,
    pub cpu_epp_enabled: bool,
    pub firmware_attribute_enabled: bool,
    pub cpu_boost_enabled: bool,
    pub conservation_mode_enabled: bool,
    pub amd_gpu_dpm_enabled: bool,
    pub curve_optimizer_enabled: bool,
    pub openrgb_access_setup_enabled: bool,
    pub hardware_profile_apply_enabled: bool,
}

impl WriteAccessPolicy {
    pub fn enabled_methods(&self) -> Vec<&'static str> {
        let mut methods = Vec::new();
        if self.platform_profile_enabled {
            methods.push(PLATFORM_PROFILE_WRITE_METHOD);
        }
        if self.battery_charge_type_enabled {
            methods.push(BATTERY_CHARGE_TYPE_WRITE_METHOD);
        }
        if self.led_state_enabled {
            methods.push(LED_STATE_WRITE_METHOD);
        }
        if self.keyboard_rgb_enabled {
            methods.push(KEYBOARD_RGB_WRITE_METHOD);
        }
        if self.ideapad_toggle_enabled
            || self.camera_power_enabled
            || self.usb_charging_enabled
            || self.fan_mode_enabled
        {
            methods.push(IDEAPAD_TOGGLE_WRITE_METHOD);
        }
        if self.gpu_mode_enabled {
            methods.push(GPU_MODE_WRITE_METHOD);
        }
        if self.cpu_governor_enabled {
            methods.push(CPU_GOVERNOR_WRITE_METHOD);
        }
        if self.cpu_epp_enabled {
            methods.push(CPU_EPP_WRITE_METHOD);
        }
        if self.firmware_attribute_enabled {
            methods.push(FIRMWARE_ATTRIBUTE_WRITE_METHOD);
        }
        if self.cpu_boost_enabled {
            methods.push(CPU_BOOST_WRITE_METHOD);
        }
        if self.conservation_mode_enabled {
            methods.push(CONSERVATION_MODE_WRITE_METHOD);
        }
        if self.amd_gpu_dpm_enabled {
            methods.push(AMD_GPU_DPM_FORCE_LEVEL_WRITE_METHOD);
        }
        if self.curve_optimizer_enabled {
            methods.push(CURVE_OPTIMIZER_ALL_CORE_WRITE_METHOD);
        }
        if self.openrgb_access_setup_enabled {
            methods.push(OPENRGB_ACCESS_SETUP_METHOD);
        }
        if self.hardware_profile_apply_enabled {
            methods.push(HARDWARE_PROFILE_APPLY_METHOD);
            methods.push(HARDWARE_PROFILE_TRIGGER_APPLY_METHOD);
        }
        methods
    }
}

pub trait WriteAuthorizer: Send + Sync {
    fn authorize(&self, action: &str, sender: &str) -> std::result::Result<(), String>;
}

#[derive(Debug, Default)]
pub struct PkcheckAuthorizer;

impl WriteAuthorizer for PkcheckAuthorizer {
    fn authorize(&self, action: &str, sender: &str) -> std::result::Result<(), String> {
        let output = Command::new("pkcheck")
            .args(pkcheck_args(action, sender))
            .output()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    PKCHECK_MISSING.to_owned()
                } else {
                    format!("failed to run pkcheck: {error}")
                }
            })?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let detail = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("pkcheck exited with status {}", output.status)
        };
        Err(format!("polkit authorization failed: {detail}"))
    }
}

#[derive(Debug, Default)]
struct InternalAuthorizer;

impl WriteAuthorizer for InternalAuthorizer {
    fn authorize(&self, _action: &str, _sender: &str) -> std::result::Result<(), String> {
        Ok(())
    }
}

pub trait PlatformProfileWriter: Send + Sync {
    fn write_platform_profile(
        &self,
        path: &str,
        requested: &str,
    ) -> std::result::Result<(), String>;
}

pub trait BatteryChargeTypeWriter: Send + Sync {
    fn write_battery_charge_type(
        &self,
        path: &str,
        requested: &str,
    ) -> std::result::Result<(), String>;
}

pub trait LedStateWriter: Send + Sync {
    fn write_led_state(&self, path: &str, enabled: bool) -> std::result::Result<(), String>;
}

pub trait KeyboardRgbWriter: Send + Sync {
    fn write_keyboard_rgb(
        &self,
        path: &str,
        request: &KeyboardRgbWriteRequest,
    ) -> std::result::Result<(), String>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenRgbKeyboardRgbSdkSnapshot {
    pub active_mode: String,
    pub colors: BTreeMap<String, String>,
}

pub trait OpenRgbKeyboardRgbSdkWriter: Send + Sync {
    fn is_configured(&self) -> bool {
        true
    }

    fn read_keyboard_rgb_snapshot(
        &self,
        path: &str,
    ) -> std::result::Result<OpenRgbKeyboardRgbSdkSnapshot, String>;

    fn write_keyboard_rgb(
        &self,
        path: &str,
        request: &KeyboardRgbWriteRequest,
    ) -> std::result::Result<(), String>;

    fn restore_keyboard_rgb_snapshot(
        &self,
        path: &str,
        snapshot: &OpenRgbKeyboardRgbSdkSnapshot,
    ) -> std::result::Result<(), String>;
}

pub trait OpenRgbAccessSetupWriter: Send + Sync {
    fn setup_openrgb_access(&self, target_user: &str) -> std::result::Result<String, String>;
}

pub trait IdeapadToggleWriter: Send + Sync {
    fn write_ideapad_toggle(&self, path: &str, enabled: bool) -> std::result::Result<(), String>;
}

pub trait GpuModeWriter: Send + Sync {
    fn switch_gpu_mode(&self, requested: &str) -> std::result::Result<String, String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AmdGpuPowerProfileSyncOutcome {
    MissingAmdGpuDpmCapability,
    MissingFedoraPowerProfile,
    UnsupportedFedoraPowerProfile(String),
    AlreadyApplied {
        active_profile: String,
        force_level: String,
    },
    Applied {
        active_profile: String,
        previous_force_level: String,
        force_level: String,
        path: String,
    },
}

#[derive(Debug, Default)]
pub struct SysfsPlatformProfileWriter;

impl PlatformProfileWriter for SysfsPlatformProfileWriter {
    fn write_platform_profile(
        &self,
        path: &str,
        requested: &str,
    ) -> std::result::Result<(), String> {
        fs::write(path, requested).map_err(|error| error.to_string())
    }
}

#[derive(Debug, Default)]
pub struct SysfsBatteryChargeTypeWriter;

impl BatteryChargeTypeWriter for SysfsBatteryChargeTypeWriter {
    fn write_battery_charge_type(
        &self,
        path: &str,
        requested: &str,
    ) -> std::result::Result<(), String> {
        fs::write(path, requested).map_err(|error| error.to_string())
    }
}

#[derive(Debug, Default)]
pub struct SysfsLedStateWriter;

impl LedStateWriter for SysfsLedStateWriter {
    fn write_led_state(&self, path: &str, enabled: bool) -> std::result::Result<(), String> {
        fs::write(path, if enabled { "1" } else { "0" }).map_err(|error| error.to_string())
    }
}

#[derive(Debug, Default)]
pub struct UnsupportedKeyboardRgbWriter;

impl KeyboardRgbWriter for UnsupportedKeyboardRgbWriter {
    fn write_keyboard_rgb(
        &self,
        _path: &str,
        _request: &KeyboardRgbWriteRequest,
    ) -> std::result::Result<(), String> {
        Err("keyboard RGB execution backend is not configured".to_owned())
    }
}

#[derive(Debug, Default)]
pub struct UnsupportedOpenRgbKeyboardRgbSdkWriter;

impl OpenRgbKeyboardRgbSdkWriter for UnsupportedOpenRgbKeyboardRgbSdkWriter {
    fn is_configured(&self) -> bool {
        false
    }

    fn read_keyboard_rgb_snapshot(
        &self,
        _path: &str,
    ) -> std::result::Result<OpenRgbKeyboardRgbSdkSnapshot, String> {
        Err("OpenRGB SDK keyboard RGB execution backend is not configured".to_owned())
    }

    fn write_keyboard_rgb(
        &self,
        _path: &str,
        _request: &KeyboardRgbWriteRequest,
    ) -> std::result::Result<(), String> {
        Err("OpenRGB SDK keyboard RGB execution backend is not configured".to_owned())
    }

    fn restore_keyboard_rgb_snapshot(
        &self,
        _path: &str,
        _snapshot: &OpenRgbKeyboardRgbSdkSnapshot,
    ) -> std::result::Result<(), String> {
        Err("OpenRGB SDK keyboard RGB execution backend is not configured".to_owned())
    }
}

#[derive(Debug, Clone)]
pub struct CommandOpenRgbKeyboardRgbSdkWriter {
    helper_path: PathBuf,
}

impl CommandOpenRgbKeyboardRgbSdkWriter {
    pub fn new(helper_path: impl Into<PathBuf>) -> Self {
        Self {
            helper_path: helper_path.into(),
        }
    }

    fn run_helper(&self, args: &[String]) -> std::result::Result<String, String> {
        let output = Command::new(&self.helper_path)
            .args(args)
            .output()
            .map_err(|error| format!("failed to run OpenRGB SDK helper: {error}"))?;
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if output.status.success() {
            return Ok(stdout);
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(if stderr.is_empty() {
            format!("OpenRGB SDK helper exited with {}", output.status)
        } else {
            format!("OpenRGB SDK helper exited with {}: {stderr}", output.status)
        })
    }
}

impl OpenRgbKeyboardRgbSdkWriter for CommandOpenRgbKeyboardRgbSdkWriter {
    fn read_keyboard_rgb_snapshot(
        &self,
        path: &str,
    ) -> std::result::Result<OpenRgbKeyboardRgbSdkSnapshot, String> {
        let stdout = self.run_helper(&["snapshot".to_owned(), path.to_owned()])?;
        serde_json::from_str(&stdout)
            .map_err(|error| format!("OpenRGB SDK helper returned invalid snapshot JSON: {error}"))
    }

    fn write_keyboard_rgb(
        &self,
        path: &str,
        request: &KeyboardRgbWriteRequest,
    ) -> std::result::Result<(), String> {
        let request_json = serde_json::to_string(request).map_err(|error| error.to_string())?;
        self.run_helper(&["write".to_owned(), path.to_owned(), request_json])
            .map(|_| ())
    }

    fn restore_keyboard_rgb_snapshot(
        &self,
        path: &str,
        snapshot: &OpenRgbKeyboardRgbSdkSnapshot,
    ) -> std::result::Result<(), String> {
        let snapshot_json = serde_json::to_string(snapshot).map_err(|error| error.to_string())?;
        self.run_helper(&["restore".to_owned(), path.to_owned(), snapshot_json])
            .map(|_| ())
    }
}

#[derive(Debug, Default)]
pub struct SystemOpenRgbAccessSetupWriter;

impl OpenRgbAccessSetupWriter for SystemOpenRgbAccessSetupWriter {
    fn setup_openrgb_access(&self, target_user: &str) -> std::result::Result<String, String> {
        ensure_openrgb_target_user(target_user)?;
        if !system_group_exists("i2c")? {
            run_setup_command("groupadd", &["--system", "i2c"])?;
        }
        if !user_in_group(target_user, "i2c")? {
            run_setup_command("usermod", &["-aG", "i2c", target_user])?;
        }
        run_setup_command("modprobe", &["i2c-dev"])?;
        install_root_file(
            "/etc/modules-load.d/ratvantage-openrgb-i2c.conf",
            "i2c-dev\n",
        )?;
        install_root_file(
            "/etc/udev/rules.d/60-ratvantage-openrgb-i2c.rules",
            "KERNEL==\"i2c-[0-9]*\", GROUP=\"i2c\", MODE=\"0660\"\n",
        )?;
        run_setup_command("udevadm", &["control", "--reload-rules"])?;
        run_setup_command("udevadm", &["trigger", "--subsystem-match=i2c-dev"])?;
        Ok(format!(
            "OpenRGB access setup installed for {target_user}; log out and back in before relying on new group membership"
        ))
    }
}

#[derive(Debug, Default)]
pub struct SysfsIdeapadToggleWriter;

impl IdeapadToggleWriter for SysfsIdeapadToggleWriter {
    fn write_ideapad_toggle(&self, path: &str, enabled: bool) -> std::result::Result<(), String> {
        fs::write(path, if enabled { "1" } else { "0" }).map_err(|error| error.to_string())
    }
}

#[derive(Debug, Default)]
pub struct CommandGpuModeWriter;

impl GpuModeWriter for CommandGpuModeWriter {
    fn switch_gpu_mode(&self, requested: &str) -> std::result::Result<String, String> {
        let output = Command::new("envycontrol")
            .args(["-s", requested])
            .output()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    "envycontrol executable was not found".to_owned()
                } else {
                    format!("failed to run envycontrol: {error}")
                }
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if output.status.success() {
            Ok(if stdout.is_empty() { stderr } else { stdout })
        } else if stderr.is_empty() {
            Err(format!("envycontrol exited with status {}", output.status))
        } else {
            Err(stderr)
        }
    }
}

pub trait CpuGovernorWriter: Send + Sync {
    fn write_cpu_governor(&self, path: &str, requested: &str) -> std::result::Result<(), String>;
}

pub trait CpuEppWriter: Send + Sync {
    fn write_cpu_epp(&self, path: &str, requested: &str) -> std::result::Result<(), String>;
}

pub trait FirmwareAttributeWriter: Send + Sync {
    fn write_firmware_attribute(
        &self,
        path: &str,
        requested: &str,
    ) -> std::result::Result<(), String>;
}

pub trait CpuBoostWriter: Send + Sync {
    fn write_cpu_boost(&self, path: &str, requested: &str) -> std::result::Result<(), String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurveOptimizerCommandOutput {
    pub stdout: String,
    pub stderr: String,
}

pub trait CurveOptimizerAllCoreWriter: Send + Sync {
    fn set_curve_optimizer_all_core(
        &self,
        encoded_value: u32,
    ) -> std::result::Result<CurveOptimizerCommandOutput, String>;
}

#[derive(Debug, Default)]
pub struct SysfsCpuGovernorWriter;

impl CpuGovernorWriter for SysfsCpuGovernorWriter {
    fn write_cpu_governor(&self, path: &str, requested: &str) -> std::result::Result<(), String> {
        fs::write(path, requested).map_err(|error| error.to_string())
    }
}

#[derive(Debug, Default)]
pub struct SysfsCpuEppWriter;

impl CpuEppWriter for SysfsCpuEppWriter {
    fn write_cpu_epp(&self, path: &str, requested: &str) -> std::result::Result<(), String> {
        fs::write(path, requested).map_err(|error| error.to_string())
    }
}

#[derive(Debug, Default)]
pub struct SysfsFirmwareAttributeWriter;

impl FirmwareAttributeWriter for SysfsFirmwareAttributeWriter {
    fn write_firmware_attribute(
        &self,
        path: &str,
        requested: &str,
    ) -> std::result::Result<(), String> {
        fs::write(path, requested).map_err(|error| error.to_string())
    }
}

#[derive(Debug, Default)]
pub struct SysfsCpuBoostWriter;

impl CpuBoostWriter for SysfsCpuBoostWriter {
    fn write_cpu_boost(&self, path: &str, requested: &str) -> std::result::Result<(), String> {
        fs::write(path, requested).map_err(|error| error.to_string())
    }
}

#[derive(Debug, Default)]
pub struct RyzenAdjCurveOptimizerWriter;

impl CurveOptimizerAllCoreWriter for RyzenAdjCurveOptimizerWriter {
    fn set_curve_optimizer_all_core(
        &self,
        encoded_value: u32,
    ) -> std::result::Result<CurveOptimizerCommandOutput, String> {
        let output = Command::new("/usr/local/bin/ryzenadj")
            .arg(format!("--set-coall={encoded_value}"))
            .output()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    "ryzenadj executable was not found at /usr/local/bin/ryzenadj".to_owned()
                } else {
                    format!("failed to run ryzenadj: {error}")
                }
            })?;
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if output.status.success() {
            Ok(CurveOptimizerCommandOutput { stdout, stderr })
        } else {
            Err(format!(
                "ryzenadj exited with status {}; stdout: {}; stderr: {}",
                output.status, stdout, stderr
            ))
        }
    }
}

pub fn detect_ryzen_backend_status(root: &Path) -> RyzenBackendStatus {
    let ryzenadj_path = root.join("usr/local/bin/ryzenadj");
    let ryzenadj_metadata = fs::metadata(&ryzenadj_path).ok();
    let ryzenadj_available = ryzenadj_metadata.is_some();
    let ryzenadj_executable = ryzenadj_metadata
        .as_ref()
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false);
    let ryzenadj = RyzenAdjBackendStatus {
        path: ryzenadj_path.display().to_string(),
        available: ryzenadj_available,
        executable: ryzenadj_executable,
        supports_curve_optimizer: ryzenadj_executable,
        detail: if ryzenadj_executable {
            "RyzenAdj command backend is available; Curve Optimizer writes remain write-only without ryzen_smu read-back.".to_owned()
        } else if ryzenadj_available {
            "RyzenAdj exists but is not executable.".to_owned()
        } else {
            "RyzenAdj was not found at the expected path.".to_owned()
        },
    };

    let ryzen_smu_path = root.join("sys/kernel/ryzen_smu_drv");
    let pm_table_path = ryzen_smu_path.join("pm_table");
    let modules_path = root.join("proc/modules");
    let module_loaded = fs::read_to_string(&modules_path)
        .map(|modules| {
            modules
                .lines()
                .any(|line| line.split_whitespace().next() == Some("ryzen_smu"))
        })
        .unwrap_or_else(|_| ryzen_smu_path.exists());
    let sysfs_available = ryzen_smu_path.is_dir();
    let pm_table_available = pm_table_path.exists();
    let readback_available = sysfs_available && pm_table_available;
    let ryzen_smu = RyzenSmuBackendStatus {
        module_loaded,
        sysfs_path: ryzen_smu_path.display().to_string(),
        sysfs_available,
        pm_table_available,
        readback_available,
        detail: if readback_available {
            "ryzen_smu sysfs backend is present; future provider code can use it for read-back/validation.".to_owned()
        } else if module_loaded || sysfs_available {
            "ryzen_smu appears partially present, but pm_table read-back was not detected."
                .to_owned()
        } else {
            "ryzen_smu backend is not loaded or not exposing sysfs on this host.".to_owned()
        },
    };

    let curve_optimizer_readback_status = if readback_available {
        CurveOptimizerReadbackStatus::Verified
    } else {
        CurveOptimizerReadbackStatus::WriteOnly
    };
    let curve_optimizer_backend = if readback_available {
        "ryzen_smu".to_owned()
    } else if ryzenadj.supports_curve_optimizer {
        "ryzenadj_write_only".to_owned()
    } else {
        "unavailable".to_owned()
    };
    let setup_assistant = build_ryzen_smu_setup_assistant(&ryzenadj, &ryzen_smu);

    RyzenBackendStatus {
        ryzenadj,
        ryzen_smu,
        curve_optimizer_backend,
        curve_optimizer_readback_status,
        setup_assistant,
    }
}

fn build_ryzen_smu_setup_assistant(
    ryzenadj: &RyzenAdjBackendStatus,
    ryzen_smu: &RyzenSmuBackendStatus,
) -> RyzenSmuSetupAssistant {
    let recommended = ryzenadj.supports_curve_optimizer && !ryzen_smu.readback_available;
    RyzenSmuSetupAssistant {
        recommended,
        reason: if recommended {
            "Curve Optimizer writes are available through RyzenAdj, but read-back is missing. ryzen_smu may provide future validation/read-back support.".to_owned()
        } else if ryzen_smu.readback_available {
            "ryzen_smu read-back surface is already detected.".to_owned()
        } else {
            "Install RyzenAdj first if you want Curve Optimizer writes; ryzen_smu alone is only the read-back/setup side here.".to_owned()
        },
        commands: vec![
            "git clone https://github.com/amkillam/ryzen_smu.git".to_owned(),
            "cd ryzen_smu".to_owned(),
            "make".to_owned(),
            "sudo make install".to_owned(),
            "sudo modprobe ryzen_smu".to_owned(),
            "ls /sys/kernel/ryzen_smu_drv".to_owned(),
        ],
        notes: vec![
            "RatVantage does not install or load kernel modules automatically.".to_owned(),
            "Review the upstream README and code before building a kernel module.".to_owned(),
            "After loading ryzen_smu, refresh diagnostics and confirm /sys/kernel/ryzen_smu_drv is present.".to_owned(),
        ],
    }
}

pub struct LegionControl {
    options: ProbeOptions,
    registry: Mutex<CapabilityRegistry>,
    state_path: PathBuf,
    state: Mutex<DaemonState>,
    write_policy: WriteAccessPolicy,
    authorizer: Arc<dyn WriteAuthorizer>,
    platform_profile_writer: Arc<dyn PlatformProfileWriter>,
    battery_charge_type_writer: Arc<dyn BatteryChargeTypeWriter>,
    led_state_writer: Arc<dyn LedStateWriter>,
    keyboard_rgb_writer: Arc<dyn KeyboardRgbWriter>,
    openrgb_keyboard_rgb_sdk_writer: Arc<dyn OpenRgbKeyboardRgbSdkWriter>,
    openrgb_access_setup_writer: Arc<dyn OpenRgbAccessSetupWriter>,
    ideapad_toggle_writer: Arc<dyn IdeapadToggleWriter>,
    gpu_mode_writer: Arc<dyn GpuModeWriter>,
    cpu_governor_writer: Arc<dyn CpuGovernorWriter>,
    cpu_epp_writer: Arc<dyn CpuEppWriter>,
    firmware_attribute_writer: Arc<dyn FirmwareAttributeWriter>,
    cpu_boost_writer: Arc<dyn CpuBoostWriter>,
    curve_optimizer_writer: Arc<dyn CurveOptimizerAllCoreWriter>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanningError {
    RegistryUnavailable,
    PresetUnavailable(String),
    Validation(ValidationError),
}

impl LegionControl {
    pub fn new(options: ProbeOptions) -> Self {
        Self::new_with_state_path(options, DEFAULT_STATE_PATH)
    }

    pub fn new_with_state_path(options: ProbeOptions, state_path: impl Into<PathBuf>) -> Self {
        Self::new_with_runtime(
            options,
            state_path,
            WriteAccessPolicy::default(),
            Arc::new(PkcheckAuthorizer),
            Arc::new(SysfsPlatformProfileWriter),
            Arc::new(SysfsBatteryChargeTypeWriter),
            Arc::new(SysfsLedStateWriter),
            Arc::new(SysfsIdeapadToggleWriter),
            Arc::new(SysfsCpuGovernorWriter),
            Arc::new(SysfsCpuEppWriter),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_runtime(
        options: ProbeOptions,
        state_path: impl Into<PathBuf>,
        write_policy: WriteAccessPolicy,
        authorizer: Arc<dyn WriteAuthorizer>,
        platform_profile_writer: Arc<dyn PlatformProfileWriter>,
        battery_charge_type_writer: Arc<dyn BatteryChargeTypeWriter>,
        led_state_writer: Arc<dyn LedStateWriter>,
        ideapad_toggle_writer: Arc<dyn IdeapadToggleWriter>,
        cpu_governor_writer: Arc<dyn CpuGovernorWriter>,
        cpu_epp_writer: Arc<dyn CpuEppWriter>,
    ) -> Self {
        let state_path = state_path.into();
        let registry = probe(&options);
        let state = load_state(&state_path).unwrap_or_default();

        Self {
            options,
            registry: Mutex::new(registry),
            state_path,
            state: Mutex::new(state),
            write_policy,
            authorizer,
            platform_profile_writer,
            battery_charge_type_writer,
            led_state_writer,
            keyboard_rgb_writer: Arc::new(UnsupportedKeyboardRgbWriter),
            openrgb_keyboard_rgb_sdk_writer: Arc::new(UnsupportedOpenRgbKeyboardRgbSdkWriter),
            openrgb_access_setup_writer: Arc::new(SystemOpenRgbAccessSetupWriter),
            ideapad_toggle_writer,
            gpu_mode_writer: Arc::new(CommandGpuModeWriter),
            cpu_governor_writer,
            cpu_epp_writer,
            firmware_attribute_writer: Arc::new(SysfsFirmwareAttributeWriter),
            cpu_boost_writer: Arc::new(SysfsCpuBoostWriter),
            curve_optimizer_writer: Arc::new(RyzenAdjCurveOptimizerWriter),
        }
    }

    pub fn with_gpu_mode_writer(mut self, writer: Arc<dyn GpuModeWriter>) -> Self {
        self.gpu_mode_writer = writer;
        self
    }

    pub fn with_keyboard_rgb_writer(mut self, writer: Arc<dyn KeyboardRgbWriter>) -> Self {
        self.keyboard_rgb_writer = writer;
        self
    }

    pub fn with_openrgb_keyboard_rgb_sdk_writer(
        mut self,
        writer: Arc<dyn OpenRgbKeyboardRgbSdkWriter>,
    ) -> Self {
        self.openrgb_keyboard_rgb_sdk_writer = writer;
        if let Ok(mut registry) = self.registry.lock() {
            self.annotate_openrgb_keyboard_rgb_sdk_backend(&mut registry);
        }
        self
    }

    pub fn with_openrgb_access_setup_writer(
        mut self,
        writer: Arc<dyn OpenRgbAccessSetupWriter>,
    ) -> Self {
        self.openrgb_access_setup_writer = writer;
        self
    }

    pub fn with_curve_optimizer_writer(
        mut self,
        writer: Arc<dyn CurveOptimizerAllCoreWriter>,
    ) -> Self {
        self.curve_optimizer_writer = writer;
        self
    }

    pub fn snapshot(&self) -> fdo::Result<CapabilityRegistry> {
        self.registry
            .lock()
            .map(|registry| registry.clone())
            .map_err(|_| fdo::Error::Failed("capability registry lock poisoned".to_owned()))
    }

    pub fn plan_platform_profile_write(
        &self,
        requested: &str,
    ) -> Result<WriteDryRunPlan, PlanningError> {
        let registry = self.planning_snapshot()?;
        plan_platform_profile(registry.platform_profile.as_ref(), requested)
            .map_err(PlanningError::Validation)
    }

    pub fn plan_prepare_custom_thermal_mode(&self) -> Result<WriteDryRunPlan, PlanningError> {
        let registry = self.planning_snapshot()?;
        plan_prepare_custom_thermal_mode(registry.platform_profile.as_ref())
            .map_err(PlanningError::Validation)
    }

    pub fn plan_battery_charge_type_write(
        &self,
        requested: &str,
    ) -> Result<WriteDryRunPlan, PlanningError> {
        let registry = self.planning_snapshot()?;
        plan_battery_charge_type(registry.battery_charge_type.as_ref(), requested)
            .map_err(PlanningError::Validation)
    }

    pub fn plan_led_state_write(
        &self,
        led_id: &str,
        enabled: bool,
    ) -> Result<WriteDryRunPlan, PlanningError> {
        let registry = self.planning_snapshot()?;
        plan_led_state(&registry.leds, led_id, enabled).map_err(PlanningError::Validation)
    }

    pub fn plan_keyboard_rgb_write(
        &self,
        request: &KeyboardRgbWriteRequest,
    ) -> Result<WriteDryRunPlan, PlanningError> {
        let registry = self.planning_snapshot()?;
        plan_keyboard_rgb(registry.keyboard_rgb.as_ref(), request)
            .map_err(PlanningError::Validation)
    }

    pub fn plan_openrgb_keyboard_rgb_bridge(
        &self,
        request: &KeyboardRgbWriteRequest,
    ) -> Result<WriteDryRunPlan, PlanningError> {
        let registry = self.planning_snapshot()?;
        plan_openrgb_keyboard_rgb_bridge(registry.keyboard_rgb_openrgb.as_ref(), request)
            .map_err(PlanningError::Validation)
    }

    pub fn plan_openrgb_keyboard_rgb_sdk_write(
        &self,
        request: &KeyboardRgbWriteRequest,
    ) -> Result<WriteDryRunPlan, PlanningError> {
        let registry = self.planning_snapshot()?;
        plan_openrgb_keyboard_rgb_sdk(registry.keyboard_rgb_openrgb.as_ref(), request)
            .map_err(PlanningError::Validation)
    }

    pub fn plan_openrgb_access_setup(
        &self,
        target_user: &str,
    ) -> Result<WriteDryRunPlan, PlanningError> {
        validate_openrgb_target_user(target_user).map_err(PlanningError::Validation)?;
        let previous_value = match user_in_group(target_user, "i2c") {
            Ok(true) => "user_already_in_i2c".to_owned(),
            Ok(false) => "user_not_in_i2c".to_owned(),
            Err(error) => format!("unknown: {error}"),
        };
        Ok(WriteDryRunPlan {
            method: OPENRGB_ACCESS_SETUP_METHOD.to_owned(),
            capability_id: "keyboard_rgb_openrgb:access_setup".to_owned(),
            polkit_action: "org.ratvantage.LegionControl1.setup-openrgb-access".to_owned(),
            path: "/etc/modules-load.d/ratvantage-openrgb-i2c.conf;/etc/udev/rules.d/60-ratvantage-openrgb-i2c.rules".to_owned(),
            previous_value,
            requested_value: format!(
                "user={target_user};group=i2c;module=i2c-dev;udev=i2c group write access"
            ),
            readback_required: false,
            rollback_value: "remove i2c group membership and installed udev/module-load files manually if this setup must be reverted".to_owned(),
            rollback_instructions: vec![
                "remove the user from i2c only if no other OpenRGB/I2C workflow needs it".to_owned(),
                "remove /etc/modules-load.d/ratvantage-openrgb-i2c.conf if i2c-dev should not auto-load".to_owned(),
                "remove /etc/udev/rules.d/60-ratvantage-openrgb-i2c.rules and reload udev if the group rule should be reverted".to_owned(),
            ],
            reboot_required: false,
            safety_notes: vec![
                "this setup does not write keyboard RGB colors or HID payloads".to_owned(),
                "new group membership usually requires log out and log back in".to_owned(),
                "OpenRGB bridge execution remains blocked until live read-back/restore evidence passes".to_owned(),
            ],
            steps: vec![
                WritePlanStep::AuthorizeCaller,
                WritePlanStep::StorePreviousValue,
                WritePlanStep::WriteRequestedValue,
            ],
        })
    }

    pub fn plan_ideapad_toggle_write(
        &self,
        toggle_id: &str,
        enabled: bool,
    ) -> Result<WriteDryRunPlan, PlanningError> {
        let registry = self.planning_snapshot()?;
        plan_ideapad_toggle(
            &registry.ideapad_toggles,
            &registry.leds,
            toggle_id,
            enabled,
        )
        .map_err(PlanningError::Validation)
    }

    pub fn plan_gpu_mode_write(&self, requested: &str) -> Result<WriteDryRunPlan, PlanningError> {
        let registry = self.planning_snapshot()?;
        plan_gpu_mode(registry.gpu.as_ref(), requested).map_err(PlanningError::Validation)
    }

    pub fn plan_fan_preset_write(&self, requested: &str) -> Result<WriteDryRunPlan, PlanningError> {
        let registry = self.planning_snapshot()?;
        let presets = packaged_fan_presets()?;
        plan_fan_preset(
            &registry.fan_curves,
            &presets,
            registry.platform_profile.as_ref(),
            requested,
        )
        .map_err(PlanningError::Validation)
    }

    pub fn plan_restore_auto_fan_write(&self) -> Result<WriteDryRunPlan, PlanningError> {
        let registry = self.planning_snapshot()?;
        plan_restore_auto_fan(&registry.fan_curves, registry.platform_profile.as_ref())
            .map_err(PlanningError::Validation)
    }

    pub fn plan_custom_thermal_fan_preset_write(
        &self,
        requested: &str,
    ) -> Result<CustomThermalPlanPreview, PlanningError> {
        let registry = self.planning_snapshot()?;
        let presets = packaged_fan_presets()?;
        let prepare = plan_prepare_custom_thermal_mode(registry.platform_profile.as_ref())
            .map_err(PlanningError::Validation)?;
        let staged_profile = staged_custom_platform_profile(&registry)?;
        let dependent = plan_fan_preset(
            &registry.fan_curves,
            &presets,
            Some(&staged_profile),
            requested,
        )
        .map_err(PlanningError::Validation)?;
        Ok(custom_thermal_preview(
            "custom_thermal_fan_preset",
            format!("fan_preset:{requested}"),
            vec![prepare, dependent],
        ))
    }

    pub fn plan_custom_thermal_restore_auto_fan(
        &self,
    ) -> Result<CustomThermalPlanPreview, PlanningError> {
        let registry = self.planning_snapshot()?;
        let prepare = plan_prepare_custom_thermal_mode(registry.platform_profile.as_ref())
            .map_err(PlanningError::Validation)?;
        let staged_profile = staged_custom_platform_profile(&registry)?;
        let dependent = plan_restore_auto_fan(&registry.fan_curves, Some(&staged_profile))
            .map_err(PlanningError::Validation)?;
        Ok(custom_thermal_preview(
            "custom_thermal_restore_auto_fan",
            "restore_auto_fan".to_owned(),
            vec![prepare, dependent],
        ))
    }

    pub fn plan_cpu_governor_write(
        &self,
        requested: &str,
    ) -> Result<WriteDryRunPlan, PlanningError> {
        let registry = self.planning_snapshot()?;
        plan_cpu_governor(registry.cpu_power.as_ref(), requested).map_err(PlanningError::Validation)
    }

    pub fn plan_cpu_epp_write(&self, requested: &str) -> Result<WriteDryRunPlan, PlanningError> {
        let registry = self.planning_snapshot()?;
        plan_cpu_epp(registry.cpu_power.as_ref(), requested).map_err(PlanningError::Validation)
    }

    pub fn plan_cpu_boost_write(&self, requested: &str) -> Result<WriteDryRunPlan, PlanningError> {
        let registry = self.planning_snapshot()?;
        plan_cpu_boost(registry.cpu_power.as_ref(), requested).map_err(PlanningError::Validation)
    }

    pub fn plan_curve_optimizer_all_core_write(
        &self,
        requested: &str,
    ) -> Result<WriteDryRunPlan, PlanningError> {
        plan_curve_optimizer_all_core(requested).map_err(PlanningError::Validation)
    }

    pub fn hardware_profile_apply_preview(
        &self,
        profile_id: &str,
    ) -> fdo::Result<HardwareProfileApplyPreview> {
        validate_hardware_profile_id(profile_id).map_err(validation_to_fdo)?;
        let profile = self
            .state
            .lock()
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))?
            .hardware_profiles
            .get(profile_id)
            .cloned()
            .ok_or_else(|| {
                fdo::Error::InvalidArgs(format!("hardware profile `{profile_id}` is not stored"))
            })?;
        self.preview_hardware_profile(profile_id, &profile)
            .map_err(planning_to_fdo)
    }

    pub fn hardware_profile_trigger_apply_preview(
        &self,
        trigger_id: &str,
    ) -> fdo::Result<HardwareProfileApplyPreview> {
        validate_hardware_profile_trigger_id(trigger_id).map_err(validation_to_fdo)?;
        let profile_id = self
            .state
            .lock()
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))?
            .hardware_profile_triggers
            .get(trigger_id)
            .cloned()
            .ok_or_else(|| {
                fdo::Error::InvalidArgs(format!(
                    "hardware profile trigger `{trigger_id}` is not configured"
                ))
            })?;
        self.hardware_profile_apply_preview(&profile_id)
    }

    pub fn apply_hardware_profile(
        &self,
        profile_id: &str,
        sender: &str,
    ) -> fdo::Result<HardwareProfileApplyRun> {
        validate_hardware_profile_id(profile_id).map_err(validation_to_fdo)?;
        let profile = self
            .state
            .lock()
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))?
            .hardware_profiles
            .get(profile_id)
            .cloned()
            .ok_or_else(|| {
                fdo::Error::InvalidArgs(format!("hardware profile `{profile_id}` is not stored"))
            })?;
        let preview = self
            .preview_hardware_profile(profile_id, &profile)
            .map_err(planning_to_fdo)?;

        if !self.write_policy.hardware_profile_apply_enabled {
            return self.record_hardware_profile_apply_run(HardwareProfileApplyRun {
                profile_id: preview.profile_id,
                profile_label: preview.profile_label,
                timestamp_unix_secs: unix_timestamp_secs(),
                completed: false,
                message: "hardware profile apply is disabled by daemon policy".to_owned(),
                results: Vec::new(),
            });
        }
        if let Err(reason) = self.authorizer.authorize(
            "org.ratvantage.LegionControl1.apply-hardware-profile",
            sender,
        ) {
            return self.record_hardware_profile_apply_run(HardwareProfileApplyRun {
                profile_id: preview.profile_id,
                profile_label: preview.profile_label,
                timestamp_unix_secs: unix_timestamp_secs(),
                completed: false,
                message: format!("hardware profile apply blocked by authorization: {reason}"),
                results: Vec::new(),
            });
        }

        let mut results = Vec::new();
        let mut plan_index = 0usize;
        let mut completed = true;
        macro_rules! run_action {
            ($action_id:expr, $call:expr) => {{
                let fallback_plan = preview.plans.get(plan_index).cloned().ok_or_else(|| {
                    fdo::Error::Failed("hardware profile plan/action mismatch".to_owned())
                })?;
                plan_index += 1;
                let result = match $call {
                    Ok(result) => result,
                    Err(error) => WriteExecutionResult::failed(
                        fallback_plan,
                        format!("hardware profile action failed: {error}"),
                        None,
                    ),
                };
                let applied = result.applied;
                results.push(HardwareProfileApplyActionResult {
                    action_id: $action_id.to_owned(),
                    result,
                });
                if !applied {
                    completed = false;
                }
            }};
        }

        if completed {
            if let Some(value) = &profile.actions.platform_profile {
                run_action!("platform_profile", self.set_platform_profile(value, sender));
            }
        }
        if completed {
            if let Some(value) = &profile.actions.battery_charge_type {
                run_action!(
                    "battery_charge_type",
                    self.set_battery_charge_type(value, sender)
                );
            }
        }
        if completed {
            if let Some(value) = &profile.actions.gpu_mode {
                run_action!("gpu_mode", self.set_gpu_mode(value, sender));
            }
        }
        if completed {
            if let Some(request) = &profile.actions.keyboard_rgb {
                run_action!(
                    "keyboard_rgb",
                    self.apply_hardware_profile_keyboard_rgb(request, sender)
                );
            }
        }
        if completed {
            if let Some(value) = &profile.actions.cpu_governor {
                run_action!("cpu_governor", self.set_cpu_governor(value, sender));
            }
        }
        if completed {
            if let Some(value) = &profile.actions.cpu_epp {
                run_action!("cpu_epp", self.set_cpu_epp(value, sender));
            }
        }
        if completed {
            if let Some(value) = &profile.actions.cpu_boost {
                run_action!("cpu_boost", self.set_cpu_boost(value, sender));
            }
        }
        if completed {
            if let Some(value) = &profile.actions.conservation_mode {
                run_action!(
                    "conservation_mode",
                    self.set_conservation_mode(value, sender)
                );
            }
        }
        if completed {
            if let Some(value) = &profile.actions.amd_gpu_dpm_force_level {
                run_action!(
                    "amd_gpu_dpm_force_level",
                    self.set_amd_gpu_dpm_force_level(value, sender)
                );
            }
        }
        if completed {
            if let Some(value) = &profile.actions.curve_optimizer_all_core {
                run_action!(
                    "curve_optimizer_all_core",
                    self.set_curve_optimizer_all_core(value, sender)
                );
            }
        }
        if completed {
            for (attribute_id, value) in &profile.actions.firmware_attributes {
                run_action!(
                    &format!("firmware_attribute:{attribute_id}"),
                    self.set_firmware_attribute(attribute_id, value, sender)
                );
                if !completed {
                    break;
                }
            }
        }

        let message = if completed {
            "hardware profile applied".to_owned()
        } else {
            "hardware profile apply stopped after first non-applied action".to_owned()
        };
        self.record_hardware_profile_apply_run(HardwareProfileApplyRun {
            profile_id: preview.profile_id,
            profile_label: preview.profile_label,
            timestamp_unix_secs: unix_timestamp_secs(),
            completed,
            message,
            results,
        })
    }

    pub fn plan_conservation_mode_write(
        &self,
        requested: &str,
    ) -> Result<WriteDryRunPlan, PlanningError> {
        let registry = self.planning_snapshot()?;
        plan_conservation_mode(&registry.ideapad_toggles, requested)
            .map_err(PlanningError::Validation)
    }

    pub fn plan_amd_gpu_dpm_force_level_write(
        &self,
        requested: &str,
    ) -> Result<WriteDryRunPlan, PlanningError> {
        let registry = self.planning_snapshot()?;
        plan_amd_gpu_dpm_force_level(registry.amd_gpu_power_dpm.as_ref(), requested)
            .map_err(PlanningError::Validation)
    }

    pub fn plan_firmware_attribute_write(
        &self,
        attribute_id: &str,
        requested: &str,
    ) -> Result<WriteDryRunPlan, PlanningError> {
        let registry = self.planning_snapshot()?;
        plan_firmware_attribute(
            &registry.firmware_attributes,
            registry.platform_profile.as_ref(),
            attribute_id,
            requested,
        )
        .map_err(PlanningError::Validation)
    }

    pub fn plan_firmware_attribute_reset_write(
        &self,
        attribute_id: &str,
    ) -> Result<WriteDryRunPlan, PlanningError> {
        let registry = self.planning_snapshot()?;
        plan_firmware_attribute_reset(
            &registry.firmware_attributes,
            registry.platform_profile.as_ref(),
            attribute_id,
        )
        .map_err(PlanningError::Validation)
    }

    pub fn plan_custom_thermal_firmware_attribute_write(
        &self,
        attribute_id: &str,
        requested: &str,
    ) -> Result<CustomThermalPlanPreview, PlanningError> {
        let registry = self.planning_snapshot()?;
        let prepare = plan_prepare_custom_thermal_mode(registry.platform_profile.as_ref())
            .map_err(PlanningError::Validation)?;
        let staged_profile = staged_custom_platform_profile(&registry)?;
        let dependent = plan_firmware_attribute(
            &registry.firmware_attributes,
            Some(&staged_profile),
            attribute_id,
            requested,
        )
        .map_err(PlanningError::Validation)?;
        Ok(custom_thermal_preview(
            "custom_thermal_firmware_attribute",
            format!("firmware_attribute:{attribute_id}"),
            vec![prepare, dependent],
        ))
    }

    pub fn plan_custom_thermal_firmware_ppt_preset_write(
        &self,
        preset_id: &str,
    ) -> Result<CustomThermalPlanPreview, PlanningError> {
        let registry = self.planning_snapshot()?;
        let prepare = plan_prepare_custom_thermal_mode(registry.platform_profile.as_ref())
            .map_err(PlanningError::Validation)?;
        let staged_profile = staged_custom_platform_profile(&registry)?;
        let mut plans = vec![prepare];
        plans.extend(
            plan_firmware_ppt_preset(
                &registry.firmware_attributes,
                Some(&staged_profile),
                preset_id,
            )
            .map_err(PlanningError::Validation)?,
        );
        Ok(custom_thermal_preview(
            "custom_thermal_firmware_ppt_preset",
            format!("firmware_ppt_preset:{preset_id}"),
            plans,
        ))
    }

    fn preview_hardware_profile(
        &self,
        profile_id: &str,
        profile: &HardwareProfile,
    ) -> Result<HardwareProfileApplyPreview, PlanningError> {
        validate_hardware_profile_id(profile_id).map_err(PlanningError::Validation)?;
        let plans = self.plan_hardware_profile_actions(&profile.actions)?;
        Ok(HardwareProfileApplyPreview {
            profile_id: profile_id.to_owned(),
            profile_label: profile.label.clone(),
            plans,
        })
    }

    fn plan_hardware_profile_actions(
        &self,
        actions: &HardwareProfileActions,
    ) -> Result<Vec<WriteDryRunPlan>, PlanningError> {
        let registry = self.planning_snapshot()?;
        if actions.battery_charge_type.is_some() && actions.conservation_mode.is_some() {
            return Err(PlanningError::Validation(
                legion_common::ValidationError::BlockedChoice {
                    capability_id: "hardware_profiles".to_owned(),
                    requested: "battery_charge_type+conservation_mode".to_owned(),
                    reason: "battery charge type and conservation_mode overlap on Lenovo firmware; split them into separate profile/automation steps with read-back between them".to_owned(),
                },
            ));
        }
        let mut plans = Vec::new();
        if let Some(value) = &actions.platform_profile {
            plans.push(
                plan_platform_profile(registry.platform_profile.as_ref(), value)
                    .map_err(PlanningError::Validation)?,
            );
        }
        if let Some(value) = &actions.battery_charge_type {
            plans.push(
                plan_battery_charge_type(registry.battery_charge_type.as_ref(), value)
                    .map_err(PlanningError::Validation)?,
            );
        }
        if let Some(value) = &actions.gpu_mode {
            plans.push(
                plan_gpu_mode(registry.gpu.as_ref(), value).map_err(PlanningError::Validation)?,
            );
        }
        if let Some(request) = &actions.keyboard_rgb {
            plans.push(
                plan_hardware_profile_keyboard_rgb(
                    registry.keyboard_rgb.as_ref(),
                    registry.keyboard_rgb_openrgb.as_ref(),
                    request,
                )
                .map_err(PlanningError::Validation)?,
            );
        }
        if let Some(value) = &actions.cpu_governor {
            plans.push(
                plan_cpu_governor(registry.cpu_power.as_ref(), value)
                    .map_err(PlanningError::Validation)?,
            );
        }
        if let Some(value) = &actions.cpu_epp {
            plans.push(
                plan_cpu_epp(registry.cpu_power.as_ref(), value)
                    .map_err(PlanningError::Validation)?,
            );
        }
        if let Some(value) = &actions.cpu_boost {
            plans.push(
                plan_cpu_boost(registry.cpu_power.as_ref(), value)
                    .map_err(PlanningError::Validation)?,
            );
        }
        if let Some(value) = &actions.conservation_mode {
            plans.push(
                plan_conservation_mode(&registry.ideapad_toggles, value)
                    .map_err(PlanningError::Validation)?,
            );
        }
        if let Some(value) = &actions.amd_gpu_dpm_force_level {
            plans.push(
                plan_amd_gpu_dpm_force_level(registry.amd_gpu_power_dpm.as_ref(), value)
                    .map_err(PlanningError::Validation)?,
            );
        }
        if let Some(value) = &actions.curve_optimizer_all_core {
            plans.push(plan_curve_optimizer_all_core(value).map_err(PlanningError::Validation)?);
        }
        for (attribute_id, value) in &actions.firmware_attributes {
            plans.push(
                plan_firmware_attribute(
                    &registry.firmware_attributes,
                    registry.platform_profile.as_ref(),
                    attribute_id,
                    value,
                )
                .map_err(PlanningError::Validation)?,
            );
        }
        Ok(plans)
    }

    pub fn set_firmware_attribute(
        &self,
        attribute_id: &str,
        requested: &str,
        sender: &str,
    ) -> fdo::Result<WriteExecutionResult> {
        let plan = self
            .plan_firmware_attribute_write(attribute_id, requested)
            .map_err(planning_to_fdo)?;
        if !self.write_policy.firmware_attribute_enabled {
            return Ok(WriteExecutionResult::blocked_by_policy(
                plan,
                "firmware attribute writes are disabled by daemon policy",
            ));
        }
        let plan = enabled_write_plan(plan);
        if let Err(reason) = self.authorizer.authorize(&plan.polkit_action, sender) {
            return Ok(WriteExecutionResult::blocked_by_authorization(plan, reason));
        }

        let path = plan.path.clone();
        let previous_value = plan.previous_value.clone();
        if let Err(error) = self
            .firmware_attribute_writer
            .write_firmware_attribute(&path, requested)
        {
            return Ok(WriteExecutionResult::failed(
                plan,
                format!("failed to write firmware attribute: {error}"),
                None,
            ));
        }

        let readback = self.refresh_firmware_attribute(attribute_id)?;
        if readback == requested {
            return Ok(WriteExecutionResult::applied(
                plan,
                "firmware attribute write applied and read back successfully",
                Some(readback),
            ));
        }

        match self
            .firmware_attribute_writer
            .write_firmware_attribute(&path, &previous_value)
        {
            Ok(()) => {
                let rollback_readback = self.refresh_firmware_attribute(attribute_id)?;
                Ok(WriteExecutionResult::failed(
                    plan,
                    format!(
                        "firmware attribute read-back mismatch after write; restored previous value `{previous_value}`"
                    ),
                    Some(rollback_readback),
                ))
            }
            Err(rollback_error) => Ok(WriteExecutionResult::failed(
                plan,
                format!(
                    "firmware attribute read-back mismatch after write and rollback failed: expected `{requested}` got `{readback}`; rollback error: {rollback_error}"
                ),
                Some(readback),
            )),
        }
    }

    pub fn set_cpu_boost(
        &self,
        requested: &str,
        sender: &str,
    ) -> fdo::Result<WriteExecutionResult> {
        let plan = self
            .plan_cpu_boost_write(requested)
            .map_err(planning_to_fdo)?;
        if !self.write_policy.cpu_boost_enabled {
            return Ok(WriteExecutionResult::blocked_by_policy(
                plan,
                "CPU boost writes are disabled by daemon policy",
            ));
        }
        let plan = enabled_write_plan(plan);
        if let Err(reason) = self.authorizer.authorize(&plan.polkit_action, sender) {
            return Ok(WriteExecutionResult::blocked_by_authorization(plan, reason));
        }
        let path = plan.path.clone();
        let previous_value = plan.previous_value.clone();
        if let Err(error) = self.cpu_boost_writer.write_cpu_boost(&path, requested) {
            return Ok(WriteExecutionResult::failed(
                plan,
                format!("failed to write CPU boost: {error}"),
                None,
            ));
        }
        let readback = self.refresh_cpu_boost()?;
        if readback == requested {
            return Ok(WriteExecutionResult::applied(
                plan,
                "CPU boost write applied and read back successfully",
                Some(readback),
            ));
        }
        match self.cpu_boost_writer.write_cpu_boost(&path, &previous_value) {
            Ok(()) => {
                let rollback_readback = self.refresh_cpu_boost()?;
                Ok(WriteExecutionResult::failed(
                    plan,
                    format!(
                        "CPU boost read-back mismatch after write; restored previous value `{previous_value}`"
                    ),
                    Some(rollback_readback),
                ))
            }
            Err(rollback_error) => Ok(WriteExecutionResult::failed(
                plan,
                format!(
                    "CPU boost read-back mismatch after write and rollback failed: expected `{requested}` got `{readback}`; rollback error: {rollback_error}"
                ),
                Some(readback),
            )),
        }
    }

    pub fn set_curve_optimizer_all_core(
        &self,
        requested: &str,
        sender: &str,
    ) -> fdo::Result<WriteExecutionResult> {
        let plan = self
            .plan_curve_optimizer_all_core_write(requested)
            .map_err(planning_to_fdo)?;
        if !self.write_policy.curve_optimizer_enabled {
            return Ok(WriteExecutionResult::blocked_by_policy(
                plan,
                "Curve Optimizer writes are disabled by daemon policy",
            ));
        }
        let plan = enabled_write_plan(plan);
        if let Err(reason) = self.authorizer.authorize(&plan.polkit_action, sender) {
            return Ok(WriteExecutionResult::blocked_by_authorization(plan, reason));
        }

        let offset =
            validate_curve_optimizer_all_core_offset(requested).map_err(validation_to_fdo)?;
        let encoded = encode_curve_optimizer_offset(offset).map_err(validation_to_fdo)?;
        let output = match self
            .curve_optimizer_writer
            .set_curve_optimizer_all_core(encoded)
        {
            Ok(output) => output,
            Err(error) => {
                return Ok(WriteExecutionResult::failed(
                    plan,
                    format!("failed to execute RyzenAdj Curve Optimizer write: {error}"),
                    None,
                ));
            }
        };

        if !output.stdout.contains("Successfully set coall")
            && !output.stderr.contains("Successfully set coall")
        {
            return Ok(WriteExecutionResult::failed(
                plan,
                "RyzenAdj completed but did not report `Successfully set coall`",
                Some(format!(
                    "stdout: {}; stderr: {}",
                    output.stdout, output.stderr
                )),
            ));
        }

        let state = CurveOptimizerWriteState {
            signed_offset: offset,
            encoded_value: encoded,
            backend: "ryzenadj".to_owned(),
            readback_status: CurveOptimizerReadbackStatus::WriteOnly,
            timestamp_unix_secs: unix_timestamp_secs(),
            stdout: (!output.stdout.is_empty()).then_some(output.stdout),
            stderr: (!output.stderr.is_empty()).then_some(output.stderr),
        };
        self.record_curve_optimizer_all_core(state)?;

        Ok(WriteExecutionResult::applied(
            plan,
            "Curve Optimizer all-core write accepted by RyzenAdj; read-back is unavailable on this backend",
            Some(format!("offset={offset} encoded={encoded} readback=write_only")),
        ))
    }

    pub fn set_conservation_mode(
        &self,
        requested: &str,
        sender: &str,
    ) -> fdo::Result<WriteExecutionResult> {
        let plan = self
            .plan_conservation_mode_write(requested)
            .map_err(planning_to_fdo)?;
        if !self.write_policy.conservation_mode_enabled {
            return Ok(WriteExecutionResult::blocked_by_policy(
                plan,
                "conservation mode writes are disabled by daemon policy",
            ));
        }
        let plan = enabled_write_plan(plan);
        if let Err(reason) = self.authorizer.authorize(&plan.polkit_action, sender) {
            return Ok(WriteExecutionResult::blocked_by_authorization(plan, reason));
        }
        let path = plan.path.clone();
        let previous_enabled = plan.previous_value == "1";
        let enabled = requested == "1";
        if let Err(error) = self
            .ideapad_toggle_writer
            .write_ideapad_toggle(&path, enabled)
        {
            return Ok(WriteExecutionResult::failed(
                plan,
                format!("failed to write conservation mode: {error}"),
                None,
            ));
        }
        let (readback, _) = self.refresh_ideapad_toggle_state("conservation_mode")?;
        if readback == requested {
            return Ok(WriteExecutionResult::applied(
                plan,
                "conservation mode write applied and read back successfully",
                Some(readback),
            ));
        }
        match self
            .ideapad_toggle_writer
            .write_ideapad_toggle(&path, previous_enabled)
        {
            Ok(()) => {
                let (rollback_readback, _) =
                    self.refresh_ideapad_toggle_state("conservation_mode")?;
                Ok(WriteExecutionResult::failed(
                    plan,
                    format!(
                        "conservation mode read-back mismatch after write; restored previous value `{}`",
                        if previous_enabled { "1" } else { "0" }
                    ),
                    Some(rollback_readback),
                ))
            }
            Err(rollback_error) => Ok(WriteExecutionResult::failed(
                plan,
                format!(
                    "conservation mode read-back mismatch after write and rollback failed: expected `{requested}` got `{readback}`; rollback error: {rollback_error}"
                ),
                Some(readback),
            )),
        }
    }

    pub fn set_amd_gpu_dpm_force_level(
        &self,
        requested: &str,
        sender: &str,
    ) -> fdo::Result<WriteExecutionResult> {
        let plan = self
            .plan_amd_gpu_dpm_force_level_write(requested)
            .map_err(planning_to_fdo)?;
        if !self.write_policy.amd_gpu_dpm_enabled {
            return Ok(WriteExecutionResult::blocked_by_policy(
                plan,
                "AMD GPU DPM writes are disabled by daemon policy",
            ));
        }
        let plan = enabled_write_plan(plan);
        if let Err(reason) = self.authorizer.authorize(&plan.polkit_action, sender) {
            return Ok(WriteExecutionResult::blocked_by_authorization(plan, reason));
        }

        let path = plan.path.clone();
        let previous_value = plan.previous_value.clone();
        if let Err(error) = fs::write(&path, requested) {
            return Ok(WriteExecutionResult::failed(
                plan,
                format!("failed to write AMD GPU DPM force level: {error}"),
                None,
            ));
        }
        let readback = self.refresh_amd_gpu_dpm_force_level()?;
        if readback == requested {
            return Ok(WriteExecutionResult::applied(
                plan,
                "AMD GPU DPM force level write applied and read back successfully",
                Some(readback),
            ));
        }

        match fs::write(&path, &previous_value) {
            Ok(()) => {
                let rollback_readback = self.refresh_amd_gpu_dpm_force_level()?;
                Ok(WriteExecutionResult::failed(
                    plan,
                    format!(
                        "AMD GPU DPM force level read-back mismatch after write; restored previous value `{previous_value}`"
                    ),
                    Some(rollback_readback),
                ))
            }
            Err(rollback_error) => Ok(WriteExecutionResult::failed(
                plan,
                format!(
                    "AMD GPU DPM force level read-back mismatch after write and rollback failed: expected `{requested}` got `{readback}`; rollback error: {rollback_error}"
                ),
                Some(readback),
            )),
        }
    }

    pub fn set_cpu_governor(
        &self,
        requested: &str,
        sender: &str,
    ) -> fdo::Result<WriteExecutionResult> {
        let plan = self
            .plan_cpu_governor_write(requested)
            .map_err(planning_to_fdo)?;
        if !self.write_policy.cpu_governor_enabled {
            return Ok(WriteExecutionResult::blocked_by_policy(
                plan,
                "CPU governor writes are disabled by daemon policy",
            ));
        }
        let plan = enabled_write_plan(plan);
        if let Err(reason) = self.authorizer.authorize(&plan.polkit_action, sender) {
            return Ok(WriteExecutionResult::blocked_by_authorization(plan, reason));
        }

        let path = plan.path.clone();
        let previous_value = plan.previous_value.clone();
        if let Err(error) = self
            .cpu_governor_writer
            .write_cpu_governor(&path, requested)
        {
            return Ok(WriteExecutionResult::failed(
                plan,
                format!("failed to write CPU governor: {error}"),
                None,
            ));
        }

        let readback = self.refresh_cpu_governor()?;
        if readback == requested {
            return Ok(WriteExecutionResult::applied(
                plan,
                "CPU governor write applied and read back successfully",
                Some(readback),
            ));
        }

        match self.cpu_governor_writer.write_cpu_governor(&path, &previous_value) {
            Ok(()) => {
                let rollback_readback = self.refresh_cpu_governor()?;
                Ok(WriteExecutionResult::failed(
                    plan,
                    format!(
                        "CPU governor read-back mismatch after write; restored previous value `{previous_value}`"
                    ),
                    Some(rollback_readback),
                ))
            }
            Err(rollback_error) => Ok(WriteExecutionResult::failed(
                plan,
                format!(
                    "CPU governor read-back mismatch after write and rollback failed: expected `{requested}` got `{readback}`; rollback error: {rollback_error}"
                ),
                Some(readback),
            )),
        }
    }

    pub fn set_cpu_epp(&self, requested: &str, sender: &str) -> fdo::Result<WriteExecutionResult> {
        let plan = self
            .plan_cpu_epp_write(requested)
            .map_err(planning_to_fdo)?;
        if !self.write_policy.cpu_epp_enabled {
            return Ok(WriteExecutionResult::blocked_by_policy(
                plan,
                "CPU EPP writes are disabled by daemon policy",
            ));
        }
        let plan = enabled_write_plan(plan);
        if let Err(reason) = self.authorizer.authorize(&plan.polkit_action, sender) {
            return Ok(WriteExecutionResult::blocked_by_authorization(plan, reason));
        }

        let path = plan.path.clone();
        let previous_value = plan.previous_value.clone();
        if let Err(error) = self.cpu_epp_writer.write_cpu_epp(&path, requested) {
            return Ok(WriteExecutionResult::failed(
                plan,
                format!("failed to write CPU EPP: {error}"),
                None,
            ));
        }

        let readback = self.refresh_cpu_epp()?;
        if readback == requested {
            return Ok(WriteExecutionResult::applied(
                plan,
                "CPU EPP write applied and read back successfully",
                Some(readback),
            ));
        }

        match self.cpu_epp_writer.write_cpu_epp(&path, &previous_value) {
            Ok(()) => {
                let rollback_readback = self.refresh_cpu_epp()?;
                Ok(WriteExecutionResult::failed(
                    plan,
                    format!(
                        "CPU EPP read-back mismatch after write; restored previous value `{previous_value}`"
                    ),
                    Some(rollback_readback),
                ))
            }
            Err(rollback_error) => Ok(WriteExecutionResult::failed(
                plan,
                format!(
                    "CPU EPP read-back mismatch after write and rollback failed: expected `{requested}` got `{readback}`; rollback error: {rollback_error}"
                ),
                Some(readback),
            )),
        }
    }

    pub fn set_platform_profile(
        &self,
        requested: &str,
        sender: &str,
    ) -> fdo::Result<WriteExecutionResult> {
        let plan = self
            .plan_platform_profile_write(requested)
            .map_err(planning_to_fdo)?;
        if !self.write_policy.platform_profile_enabled {
            return Ok(WriteExecutionResult::blocked_by_policy(
                plan,
                "platform profile writes are disabled by daemon policy",
            ));
        }
        let plan = enabled_write_plan(plan);
        if let Err(reason) = self.authorizer.authorize(&plan.polkit_action, sender) {
            return Ok(WriteExecutionResult::blocked_by_authorization(plan, reason));
        }

        let path = plan.path.clone();
        let previous_value = plan.previous_value.clone();
        if let Err(error) = self
            .platform_profile_writer
            .write_platform_profile(&path, requested)
        {
            return Ok(WriteExecutionResult::failed(
                plan,
                format!("failed to write platform profile: {error}"),
                None,
            ));
        }

        let readback = self.refresh_platform_profile()?;
        if readback == requested {
            return Ok(WriteExecutionResult::applied(
                plan,
                "platform profile write applied and read back successfully",
                Some(readback),
            ));
        }

        match self
            .platform_profile_writer
            .write_platform_profile(&path, &previous_value)
        {
            Ok(()) => {
                let rollback_readback = self.refresh_platform_profile()?;
                Ok(WriteExecutionResult::failed(
                    plan,
                    format!(
                        "platform profile read-back mismatch after write; restored previous value `{previous_value}`"
                    ),
                    Some(rollback_readback),
                ))
            }
            Err(rollback_error) => Ok(WriteExecutionResult::failed(
                plan,
                format!(
                    "platform profile read-back mismatch after write and rollback failed: expected `{requested}` got `{readback}`; rollback error: {rollback_error}"
                ),
                Some(readback),
            )),
        }
    }

    pub fn set_battery_charge_type(
        &self,
        requested: &str,
        sender: &str,
    ) -> fdo::Result<WriteExecutionResult> {
        let plan = self
            .plan_battery_charge_type_write(requested)
            .map_err(planning_to_fdo)?;
        if !self.write_policy.battery_charge_type_enabled {
            return Ok(WriteExecutionResult::blocked_by_policy(
                plan,
                "battery charge type writes are disabled by daemon policy",
            ));
        }
        let plan = enabled_write_plan(plan);
        if let Err(reason) = self.authorizer.authorize(&plan.polkit_action, sender) {
            return Ok(WriteExecutionResult::blocked_by_authorization(plan, reason));
        }

        let path = plan.path.clone();
        let previous_value = plan.previous_value.clone();
        if let Err(error) = self
            .battery_charge_type_writer
            .write_battery_charge_type(&path, requested)
        {
            return Ok(WriteExecutionResult::failed(
                plan,
                format!("failed to write battery charge type: {error}"),
                None,
            ));
        }

        let readback = self.refresh_battery_charge_type()?;
        if readback == requested {
            return Ok(WriteExecutionResult::applied(
                plan,
                "battery charge type write applied and read back successfully",
                Some(readback),
            ));
        }

        match self
            .battery_charge_type_writer
            .write_battery_charge_type(&path, &previous_value)
        {
            Ok(()) => {
                let rollback_readback = self.refresh_battery_charge_type()?;
                Ok(WriteExecutionResult::failed(
                    plan,
                    format!(
                        "battery charge type read-back mismatch after write; restored previous value `{previous_value}`"
                    ),
                    Some(rollback_readback),
                ))
            }
            Err(rollback_error) => Ok(WriteExecutionResult::failed(
                plan,
                format!(
                    "battery charge type read-back mismatch after write and rollback failed: expected `{requested}` got `{readback}`; rollback error: {rollback_error}"
                ),
                Some(readback),
            )),
        }
    }

    pub fn set_led_state(
        &self,
        led_id: &str,
        enabled: bool,
        sender: &str,
    ) -> fdo::Result<WriteExecutionResult> {
        let plan = self
            .plan_led_state_write(led_id, enabled)
            .map_err(planning_to_fdo)?;
        if !self.write_policy.led_state_enabled {
            return Ok(WriteExecutionResult::blocked_by_policy(
                plan,
                "LED writes are disabled by daemon policy",
            ));
        }
        let plan = enabled_write_plan(plan);
        if let Err(reason) = self.authorizer.authorize(&plan.polkit_action, sender) {
            return Ok(WriteExecutionResult::blocked_by_authorization(plan, reason));
        }

        let path = plan.path.clone();
        let previous_enabled = plan.previous_value == "1";
        if let Err(error) = self.led_state_writer.write_led_state(&path, enabled) {
            return Ok(WriteExecutionResult::failed(
                plan,
                format!("failed to write LED state: {error}"),
                None,
            ));
        }

        let readback = self.refresh_led_state(led_id)?;
        let requested_value = if enabled { "1" } else { "0" };
        if readback == requested_value {
            return Ok(WriteExecutionResult::applied(
                plan,
                "LED state write applied and read back successfully",
                Some(readback),
            ));
        }

        match self
            .led_state_writer
            .write_led_state(&path, previous_enabled)
        {
            Ok(()) => {
                let rollback_readback = self.refresh_led_state(led_id)?;
                Ok(WriteExecutionResult::failed(
                    plan,
                    format!(
                        "LED state read-back mismatch after write; restored previous value `{}`",
                        if previous_enabled { "1" } else { "0" }
                    ),
                    Some(rollback_readback),
                ))
            }
            Err(rollback_error) => Ok(WriteExecutionResult::failed(
                plan,
                format!(
                    "LED state read-back mismatch after write and rollback failed: expected `{requested_value}` got `{readback}`; rollback error: {rollback_error}"
                ),
                Some(readback),
            )),
        }
    }

    pub fn set_keyboard_rgb(
        &self,
        request: &KeyboardRgbWriteRequest,
        sender: &str,
    ) -> fdo::Result<WriteExecutionResult> {
        let registry = self.planning_snapshot().map_err(planning_to_fdo)?;
        if registry.keyboard_rgb.is_none() {
            return self.set_openrgb_keyboard_rgb_sdk(request, sender);
        }
        let capability = registry.keyboard_rgb.as_ref();
        let plan = plan_keyboard_rgb(capability, request).map_err(validation_to_fdo)?;
        if !self.write_policy.keyboard_rgb_enabled {
            return Ok(WriteExecutionResult::blocked_by_policy(
                plan,
                "keyboard RGB writes are disabled by daemon policy",
            ));
        }
        let previous_request = keyboard_rgb_current_request(capability.ok_or_else(|| {
            fdo::Error::Failed("keyboard RGB capability disappeared before write".to_owned())
        })?)?;
        let plan = enabled_write_plan(plan);
        if let Err(reason) = self.authorizer.authorize(&plan.polkit_action, sender) {
            return Ok(WriteExecutionResult::blocked_by_authorization(plan, reason));
        }

        let path = plan.path.clone();
        if let Err(error) = self.keyboard_rgb_writer.write_keyboard_rgb(&path, request) {
            return Ok(WriteExecutionResult::failed(
                plan,
                format!("failed to write keyboard RGB state: {error}"),
                None,
            ));
        }

        let readback = self.refresh_keyboard_rgb_state()?;
        let requested_value = plan.requested_value.clone();
        if readback == requested_value {
            return Ok(WriteExecutionResult::applied(
                plan,
                "keyboard RGB write applied and read back successfully",
                Some(readback),
            ));
        }

        match self
            .keyboard_rgb_writer
            .write_keyboard_rgb(&path, &previous_request)
        {
            Ok(()) => {
                let rollback_readback = self.refresh_keyboard_rgb_state()?;
                Ok(WriteExecutionResult::failed(
                    plan,
                    format!(
                        "keyboard RGB read-back mismatch after write; restored previous value `{}`",
                        keyboard_rgb_request_summary(&previous_request)
                    ),
                    Some(rollback_readback),
                ))
            }
            Err(rollback_error) => Ok(WriteExecutionResult::failed(
                plan,
                format!(
                    "keyboard RGB read-back mismatch after write and rollback failed: expected `{}` got `{readback}`; rollback error: {rollback_error}",
                    requested_value
                ),
                Some(readback),
            )),
        }
    }

    fn apply_hardware_profile_keyboard_rgb(
        &self,
        request: &KeyboardRgbWriteRequest,
        sender: &str,
    ) -> fdo::Result<WriteExecutionResult> {
        let registry = self.planning_snapshot().map_err(planning_to_fdo)?;
        if registry.keyboard_rgb.is_some() {
            return self.set_keyboard_rgb(request, sender);
        }

        self.set_openrgb_keyboard_rgb_sdk(request, sender)
    }

    fn set_openrgb_keyboard_rgb_sdk(
        &self,
        request: &KeyboardRgbWriteRequest,
        sender: &str,
    ) -> fdo::Result<WriteExecutionResult> {
        let registry = self.planning_snapshot().map_err(planning_to_fdo)?;
        let plan = plan_openrgb_keyboard_rgb_sdk(registry.keyboard_rgb_openrgb.as_ref(), request)
            .map_err(validation_to_fdo)?;
        if !self.write_policy.keyboard_rgb_enabled {
            return Ok(WriteExecutionResult::blocked_by_policy(
                plan,
                "OpenRGB SDK keyboard RGB writes are disabled by daemon policy",
            ));
        }
        let plan = enabled_write_plan(plan);
        if let Err(reason) = self.authorizer.authorize(&plan.polkit_action, sender) {
            return Ok(WriteExecutionResult::blocked_by_authorization(plan, reason));
        }

        let path = plan.path.clone();
        let previous_snapshot = match self
            .openrgb_keyboard_rgb_sdk_writer
            .read_keyboard_rgb_snapshot(&path)
        {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return Ok(WriteExecutionResult::failed(
                    plan,
                    format!("failed to read OpenRGB SDK keyboard RGB before-snapshot: {error}"),
                    None,
                ));
            }
        };

        if let Err(error) = self
            .openrgb_keyboard_rgb_sdk_writer
            .write_keyboard_rgb(&path, request)
        {
            return Ok(WriteExecutionResult::failed(
                plan,
                format!("failed to write OpenRGB SDK keyboard RGB state: {error}"),
                Some(openrgb_sdk_snapshot_summary(&previous_snapshot)),
            ));
        }

        let readback = match self
            .openrgb_keyboard_rgb_sdk_writer
            .read_keyboard_rgb_snapshot(&path)
        {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return Ok(WriteExecutionResult::failed(
                    plan,
                    format!("failed to read OpenRGB SDK keyboard RGB after write: {error}"),
                    Some(openrgb_sdk_snapshot_summary(&previous_snapshot)),
                ));
            }
        };
        if openrgb_sdk_snapshot_matches_request(&readback, request) {
            return Ok(WriteExecutionResult::applied(
                plan,
                "OpenRGB SDK keyboard RGB write applied and read back successfully",
                Some(openrgb_sdk_snapshot_summary(&readback)),
            ));
        }

        match self
            .openrgb_keyboard_rgb_sdk_writer
            .restore_keyboard_rgb_snapshot(&path, &previous_snapshot)
        {
            Ok(()) => {
                let rollback_readback = self
                    .openrgb_keyboard_rgb_sdk_writer
                    .read_keyboard_rgb_snapshot(&path)
                    .map(|snapshot| openrgb_sdk_snapshot_summary(&snapshot))
                    .unwrap_or_else(|error| format!("rollback read-back failed: {error}"));
                Ok(WriteExecutionResult::failed(
                    plan,
                    format!(
                        "OpenRGB SDK keyboard RGB read-back mismatch after write; restored previous snapshot `{}`",
                        openrgb_sdk_snapshot_summary(&previous_snapshot)
                    ),
                    Some(rollback_readback),
                ))
            }
            Err(rollback_error) => Ok(WriteExecutionResult::failed(
                plan,
                format!(
                    "OpenRGB SDK keyboard RGB read-back mismatch after write and rollback failed: expected `{}` got `{}`; rollback error: {rollback_error}",
                    keyboard_rgb_request_summary(request),
                    openrgb_sdk_snapshot_summary(&readback)
                ),
                Some(openrgb_sdk_snapshot_summary(&readback)),
            )),
        }
    }

    pub fn setup_openrgb_access(
        &self,
        target_user: &str,
        sender: &str,
    ) -> fdo::Result<WriteExecutionResult> {
        let plan = self
            .plan_openrgb_access_setup(target_user)
            .map_err(planning_to_fdo)?;
        if !self.write_policy.openrgb_access_setup_enabled {
            return Ok(WriteExecutionResult::blocked_by_policy(
                plan,
                "OpenRGB access setup is disabled by daemon policy",
            ));
        }
        let plan = enabled_write_plan(plan);
        if let Err(reason) = self.authorizer.authorize(&plan.polkit_action, sender) {
            return Ok(WriteExecutionResult::blocked_by_authorization(plan, reason));
        }

        match self
            .openrgb_access_setup_writer
            .setup_openrgb_access(target_user)
        {
            Ok(message) => Ok(WriteExecutionResult::applied(
                plan,
                message,
                Some(format!("user={target_user};i2c_group_configured=true")),
            )),
            Err(error) => Ok(WriteExecutionResult::failed(
                plan,
                format!("OpenRGB access setup failed: {error}"),
                None,
            )),
        }
    }

    pub fn set_ideapad_toggle(
        &self,
        toggle_id: &str,
        enabled: bool,
        sender: &str,
    ) -> fdo::Result<WriteExecutionResult> {
        let plan = self
            .plan_ideapad_toggle_write(toggle_id, enabled)
            .map_err(planning_to_fdo)?;
        if !self.ideapad_toggle_write_enabled(toggle_id) {
            return Ok(WriteExecutionResult::blocked_by_policy(
                plan,
                ideapad_toggle_policy_message(toggle_id),
            ));
        }
        let plan = enabled_write_plan(plan);
        if let Err(reason) = self.authorizer.authorize(&plan.polkit_action, sender) {
            return Ok(WriteExecutionResult::blocked_by_authorization(plan, reason));
        }

        let path = plan.path.clone();
        let previous_enabled = plan.previous_value == "1";
        if let Err(error) = self
            .ideapad_toggle_writer
            .write_ideapad_toggle(&path, enabled)
        {
            return Ok(WriteExecutionResult::failed(
                plan,
                format!("failed to write ideapad toggle: {error}"),
                None,
            ));
        }

        let requested_value = if enabled { "1" } else { "0" };
        let (toggle_readback, indicator_readback) = self.refresh_ideapad_toggle_state(toggle_id)?;
        if toggle_readback == requested_value
            && (toggle_id != "fn_lock" || indicator_readback.as_deref() == Some(requested_value))
        {
            return Ok(WriteExecutionResult::applied(
                plan,
                "ideapad toggle write applied and read back successfully",
                Some(toggle_readback),
            ));
        }

        match self
            .ideapad_toggle_writer
            .write_ideapad_toggle(&path, previous_enabled)
        {
            Ok(()) => {
                let (rollback_toggle, rollback_indicator) =
                    self.refresh_ideapad_toggle_state(toggle_id)?;
                let mut detail = format!(
                    "ideapad toggle read-back mismatch after write; restored previous value `{}`",
                    if previous_enabled { "1" } else { "0" }
                );
                if toggle_id == "fn_lock" {
                    detail.push_str(&format!(
                        " (toggle read-back `{toggle_readback}`, indicator `{}`)",
                        indicator_readback.as_deref().unwrap_or("missing")
                    ));
                    detail.push_str(&format!(
                        "; rollback read-back toggle `{rollback_toggle}`, indicator `{}`",
                        rollback_indicator.as_deref().unwrap_or("missing")
                    ));
                }
                Ok(WriteExecutionResult::failed(
                    plan,
                    detail,
                    Some(rollback_toggle),
                ))
            }
            Err(rollback_error) => Ok(WriteExecutionResult::failed(
                plan,
                format!(
                    "ideapad toggle read-back mismatch after write and rollback failed: expected `{requested_value}` got `{toggle_readback}`; indicator `{}`; rollback error: {rollback_error}",
                    indicator_readback.as_deref().unwrap_or("missing")
                ),
                Some(toggle_readback),
            )),
        }
    }

    pub fn gpu_mode_pending(&self) -> fdo::Result<Option<GpuModePending>> {
        self.state
            .lock()
            .map(|state| state.gpu_mode_pending.clone())
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))
    }

    pub fn last_curve_optimizer_all_core(&self) -> fdo::Result<Option<CurveOptimizerWriteState>> {
        self.state
            .lock()
            .map(|state| state.last_curve_optimizer_all_core.clone())
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))
    }

    pub fn ryzen_backend_status(&self) -> fdo::Result<RyzenBackendStatus> {
        Ok(detect_ryzen_backend_status(&self.options.sysfs_root))
    }

    pub fn last_hardware_profile_apply(&self) -> fdo::Result<Option<HardwareProfileApplyRun>> {
        self.state
            .lock()
            .map(|state| state.last_hardware_profile_apply.clone())
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))
    }

    pub fn recent_platform_profile_changes(&self) -> fdo::Result<Vec<PlatformProfileChangeEvent>> {
        self.state
            .lock()
            .map(|state| state.recent_platform_profile_changes.clone())
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))
    }

    fn record_curve_optimizer_all_core(
        &self,
        write_state: CurveOptimizerWriteState,
    ) -> fdo::Result<CurveOptimizerWriteState> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))?;
        state.last_curve_optimizer_all_core = Some(write_state.clone());
        save_state(&self.state_path, &state)
            .map_err(|error| fdo::Error::Failed(format!("failed to save daemon state: {error}")))?;
        Ok(write_state)
    }

    fn record_hardware_profile_apply_run(
        &self,
        run: HardwareProfileApplyRun,
    ) -> fdo::Result<HardwareProfileApplyRun> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))?;
        state.last_hardware_profile_apply = Some(run.clone());
        save_state(&self.state_path, &state)
            .map_err(|error| fdo::Error::Failed(format!("failed to save daemon state: {error}")))?;
        Ok(run)
    }

    fn observe_platform_profile_change(&self) -> fdo::Result<Option<(String, String)>> {
        let registry = self.refresh()?;
        let current = registry
            .platform_profile
            .and_then(|profile| profile.current)
            .ok_or_else(|| {
                fdo::Error::Failed(
                    "platform_profile current value unavailable for observer".to_owned(),
                )
            })?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))?;
        let previous = state.last_observed_platform_profile.clone();
        state.last_observed_platform_profile = Some(current.clone());
        let change = previous.as_ref().and_then(|previous| {
            if previous == &current {
                None
            } else {
                Some((previous.clone(), current.clone()))
            }
        });
        if let Some((previous, current)) = &change {
            state
                .recent_platform_profile_changes
                .push(PlatformProfileChangeEvent {
                    timestamp_unix_secs: unix_timestamp_secs(),
                    previous_profile: previous.clone(),
                    current_profile: current.clone(),
                    source: "platform_profile_observer".to_owned(),
                });
            let overflow = state
                .recent_platform_profile_changes
                .len()
                .saturating_sub(RECENT_PLATFORM_PROFILE_CHANGE_LIMIT);
            if overflow > 0 {
                state.recent_platform_profile_changes.drain(0..overflow);
            }
        }
        save_state(&self.state_path, &state)
            .map_err(|error| fdo::Error::Failed(format!("failed to save daemon state: {error}")))?;
        Ok(change)
    }

    fn record_current_platform_profile_observed(&self) -> fdo::Result<()> {
        let registry = self.refresh()?;
        let current = registry
            .platform_profile
            .and_then(|profile| profile.current);
        if let Some(current) = current {
            let mut state = self
                .state
                .lock()
                .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))?;
            state.last_observed_platform_profile = Some(current);
            save_state(&self.state_path, &state).map_err(|error| {
                fdo::Error::Failed(format!("failed to save daemon state: {error}"))
            })?;
        }
        Ok(())
    }

    pub fn set_gpu_mode(&self, requested: &str, sender: &str) -> fdo::Result<WriteExecutionResult> {
        let plan = self
            .plan_gpu_mode_write(requested)
            .map_err(planning_to_fdo)?;
        if !self.write_policy.gpu_mode_enabled {
            return Ok(WriteExecutionResult::blocked_by_policy(
                plan,
                "GPU mode writes are disabled by daemon policy",
            ));
        }
        let plan = enabled_write_plan(plan);
        if let Err(reason) = self.authorizer.authorize(&plan.polkit_action, sender) {
            return Ok(WriteExecutionResult::blocked_by_authorization(plan, reason));
        }

        let previous_mode = plan.previous_value.clone();
        if let Err(error) = self.gpu_mode_writer.switch_gpu_mode(requested) {
            return Ok(WriteExecutionResult::failed(
                plan,
                format!("failed to execute envycontrol GPU mode switch: {error}"),
                Some(previous_mode),
            ));
        }

        let pending = self.record_gpu_mode_pending(requested, Some(previous_mode))?;
        Ok(WriteExecutionResult::applied(
            plan,
            "envycontrol GPU mode switch command completed; reboot is required before read-back verification",
            Some(format_gpu_mode_pending_summary(Some(&pending))),
        ))
    }

    pub fn set_gpu_mode_pending(&self, requested: &str) -> fdo::Result<GpuModePending> {
        let registry = self.planning_snapshot().map_err(planning_to_fdo)?;
        validate_gpu_mode_choice(registry.gpu.as_ref(), requested).map_err(validation_to_fdo)?;
        let previous_mode = registry.gpu.and_then(|gpu| gpu.mode);
        self.record_gpu_mode_pending(requested, previous_mode)
    }

    fn record_gpu_mode_pending(
        &self,
        requested: &str,
        previous_mode: Option<String>,
    ) -> fdo::Result<GpuModePending> {
        let pending = GpuModePending {
            requested_mode: requested.to_owned(),
            previous_mode,
            reboot_required: true,
        };

        let mut state = self
            .state
            .lock()
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))?;
        state.gpu_mode_pending = Some(pending.clone());
        save_state(&self.state_path, &state)
            .map_err(|error| fdo::Error::Failed(format!("failed to save daemon state: {error}")))?;
        Ok(pending)
    }

    pub fn clear_gpu_mode_pending(&self) -> fdo::Result<Option<GpuModePending>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))?;
        let previous = state.gpu_mode_pending.take();
        save_state(&self.state_path, &state)
            .map_err(|error| fdo::Error::Failed(format!("failed to save daemon state: {error}")))?;
        Ok(previous)
    }

    pub fn hardware_profiles(&self) -> fdo::Result<BTreeMap<String, HardwareProfile>> {
        self.state
            .lock()
            .map(|state| state.hardware_profiles.clone())
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))
    }

    pub fn set_hardware_profile(
        &self,
        profile_id: &str,
        profile_json: &str,
    ) -> fdo::Result<HardwareProfileApplyPreview> {
        validate_hardware_profile_id(profile_id).map_err(validation_to_fdo)?;
        let profile: HardwareProfile = serde_json::from_str(profile_json).map_err(|error| {
            fdo::Error::InvalidArgs(format!("invalid hardware profile JSON: {error}"))
        })?;
        let preview = self
            .preview_hardware_profile(profile_id, &profile)
            .map_err(planning_to_fdo)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))?;
        state
            .hardware_profiles
            .insert(profile_id.to_owned(), profile);
        save_state(&self.state_path, &state)
            .map_err(|error| fdo::Error::Failed(format!("failed to save daemon state: {error}")))?;
        Ok(preview)
    }

    pub fn remove_hardware_profile(
        &self,
        profile_id: &str,
    ) -> fdo::Result<Option<HardwareProfile>> {
        validate_hardware_profile_id(profile_id).map_err(validation_to_fdo)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))?;
        let previous = state.hardware_profiles.remove(profile_id);
        if previous.is_some() {
            state
                .hardware_profile_triggers
                .retain(|_, mapped_profile_id| mapped_profile_id != profile_id);
        }
        save_state(&self.state_path, &state)
            .map_err(|error| fdo::Error::Failed(format!("failed to save daemon state: {error}")))?;
        Ok(previous)
    }

    pub fn clear_hardware_profiles(&self) -> fdo::Result<BTreeMap<String, HardwareProfile>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))?;
        state.hardware_profiles.clear();
        state.hardware_profile_triggers.clear();
        state.automation_rules.clear();
        state.last_automation_rule_apply.clear();
        save_state(&self.state_path, &state)
            .map_err(|error| fdo::Error::Failed(format!("failed to save daemon state: {error}")))?;
        Ok(state.hardware_profiles.clone())
    }

    pub fn hardware_profile_triggers(&self) -> fdo::Result<BTreeMap<String, String>> {
        self.state
            .lock()
            .map(|state| state.hardware_profile_triggers.clone())
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))
    }

    pub fn set_hardware_profile_trigger(
        &self,
        trigger_id: &str,
        profile_id: &str,
    ) -> fdo::Result<BTreeMap<String, String>> {
        validate_hardware_profile_trigger_id(trigger_id).map_err(validation_to_fdo)?;
        validate_hardware_profile_id(profile_id).map_err(validation_to_fdo)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))?;
        if !state.hardware_profiles.contains_key(profile_id) {
            return Err(fdo::Error::InvalidArgs(format!(
                "hardware profile `{profile_id}` is not stored"
            )));
        }
        state
            .hardware_profile_triggers
            .insert(trigger_id.to_owned(), profile_id.to_owned());
        save_state(&self.state_path, &state)
            .map_err(|error| fdo::Error::Failed(format!("failed to save daemon state: {error}")))?;
        Ok(state.hardware_profile_triggers.clone())
    }

    pub fn remove_hardware_profile_trigger(&self, trigger_id: &str) -> fdo::Result<Option<String>> {
        validate_hardware_profile_trigger_id(trigger_id).map_err(validation_to_fdo)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))?;
        let previous = state.hardware_profile_triggers.remove(trigger_id);
        save_state(&self.state_path, &state)
            .map_err(|error| fdo::Error::Failed(format!("failed to save daemon state: {error}")))?;
        Ok(previous)
    }

    pub fn clear_hardware_profile_triggers(&self) -> fdo::Result<BTreeMap<String, String>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))?;
        state.hardware_profile_triggers.clear();
        save_state(&self.state_path, &state)
            .map_err(|error| fdo::Error::Failed(format!("failed to save daemon state: {error}")))?;
        Ok(state.hardware_profile_triggers.clone())
    }

    pub fn apply_hardware_profile_trigger(
        &self,
        trigger_id: &str,
        sender: &str,
    ) -> fdo::Result<HardwareProfileApplyRun> {
        validate_hardware_profile_trigger_id(trigger_id).map_err(validation_to_fdo)?;
        let profile_id = self
            .state
            .lock()
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))?
            .hardware_profile_triggers
            .get(trigger_id)
            .cloned()
            .ok_or_else(|| {
                fdo::Error::InvalidArgs(format!(
                    "hardware profile trigger `{trigger_id}` is not configured"
                ))
            })?;
        self.apply_hardware_profile(&profile_id, sender)
    }

    pub fn automation_rules(&self) -> fdo::Result<BTreeMap<String, AutomationRule>> {
        self.state
            .lock()
            .map(|state| state.automation_rules.clone())
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))
    }

    pub fn last_automation_rule_apply(
        &self,
    ) -> fdo::Result<BTreeMap<String, AutomationRuleApplyRun>> {
        self.state
            .lock()
            .map(|state| state.last_automation_rule_apply.clone())
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))
    }

    pub fn set_automation_rule(
        &self,
        rule_id: &str,
        rule_json: &str,
    ) -> fdo::Result<BTreeMap<String, AutomationRule>> {
        validate_automation_rule_id(rule_id).map_err(validation_to_fdo)?;
        let rule: AutomationRule = serde_json::from_str(rule_json)
            .map_err(|error| fdo::Error::InvalidArgs(error.to_string()))?;
        validate_automation_rule(&rule).map_err(validation_to_fdo)?;

        let mut state = self
            .state
            .lock()
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))?;
        match &rule.kind {
            AutomationRuleKind::FastChargeUntilThreshold {
                fast_charge_profile_id,
                protect_profile_id,
                ..
            } => {
                for profile_id in [fast_charge_profile_id, protect_profile_id] {
                    if !state.hardware_profiles.contains_key(profile_id) {
                        return Err(fdo::Error::InvalidArgs(format!(
                            "hardware profile `{profile_id}` is not stored"
                        )));
                    }
                }
            }
            AutomationRuleKind::AcProfileRouter {
                ac_profile_id,
                battery_profile_id,
                ..
            } => {
                for profile_id in [ac_profile_id, battery_profile_id] {
                    if !state.hardware_profiles.contains_key(profile_id) {
                        return Err(fdo::Error::InvalidArgs(format!(
                            "hardware profile `{profile_id}` is not stored"
                        )));
                    }
                }
            }
            AutomationRuleKind::BatteryProfileThreshold { profile_id, .. } => {
                if !state.hardware_profiles.contains_key(profile_id) {
                    return Err(fdo::Error::InvalidArgs(format!(
                        "hardware profile `{profile_id}` is not stored"
                    )));
                }
            }
        }
        state.automation_rules.insert(rule_id.to_owned(), rule);
        save_state(&self.state_path, &state)
            .map_err(|error| fdo::Error::Failed(format!("failed to save daemon state: {error}")))?;
        Ok(state.automation_rules.clone())
    }

    pub fn remove_automation_rule(&self, rule_id: &str) -> fdo::Result<Option<AutomationRule>> {
        validate_automation_rule_id(rule_id).map_err(validation_to_fdo)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))?;
        let previous = state.automation_rules.remove(rule_id);
        state.last_automation_rule_apply.remove(rule_id);
        save_state(&self.state_path, &state)
            .map_err(|error| fdo::Error::Failed(format!("failed to save daemon state: {error}")))?;
        Ok(previous)
    }

    pub fn clear_automation_rules(&self) -> fdo::Result<BTreeMap<String, AutomationRule>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))?;
        state.automation_rules.clear();
        state.last_automation_rule_apply.clear();
        save_state(&self.state_path, &state)
            .map_err(|error| fdo::Error::Failed(format!("failed to save daemon state: {error}")))?;
        Ok(state.automation_rules.clone())
    }

    pub fn automation_rule_preview(&self, rule_id: &str) -> fdo::Result<AutomationRuleEvaluation> {
        validate_automation_rule_id(rule_id).map_err(validation_to_fdo)?;
        let rule = self
            .state
            .lock()
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))?
            .automation_rules
            .get(rule_id)
            .cloned()
            .ok_or_else(|| {
                fdo::Error::InvalidArgs(format!("automation rule `{rule_id}` is not stored"))
            })?;
        self.evaluate_automation_rule(rule_id, &rule)
    }

    pub fn apply_automation_rule(
        &self,
        rule_id: &str,
        sender: &str,
    ) -> fdo::Result<AutomationRuleApplyRun> {
        self.apply_automation_rule_with_cooldown(rule_id, sender, None)
    }

    fn apply_automation_rule_with_cooldown(
        &self,
        rule_id: &str,
        sender: &str,
        cooldown_secs: Option<u64>,
    ) -> fdo::Result<AutomationRuleApplyRun> {
        let evaluation = self.automation_rule_preview(rule_id)?;
        let now = unix_timestamp_secs();
        if let Some(cooldown_secs) = cooldown_secs {
            if let Some(selected_profile_id) = evaluation.selected_profile_id.clone() {
                let last = self
                    .state
                    .lock()
                    .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))?
                    .last_automation_rule_apply
                    .get(rule_id)
                    .cloned();
                if let Some(last) = last {
                    let same_profile = last.evaluation.selected_profile_id.as_deref()
                        == Some(selected_profile_id.as_str());
                    let within_cooldown =
                        now.saturating_sub(last.timestamp_unix_secs) < cooldown_secs;
                    if same_profile && within_cooldown {
                        let mut skipped = evaluation;
                        skipped.matched = false;
                        skipped.reason = format!(
                            "selected profile `{selected_profile_id}` is still inside automation cooldown"
                        );
                        let run = AutomationRuleApplyRun {
                            timestamp_unix_secs: now,
                            evaluation: skipped,
                            profile_run: None,
                        };
                        self.record_automation_rule_apply_run(rule_id, run.clone())?;
                        return Ok(run);
                    }
                }
            }
        }
        let profile_run = match evaluation.selected_profile_id.as_deref() {
            Some(profile_id) if evaluation.matched => {
                Some(self.apply_hardware_profile(profile_id, sender)?)
            }
            _ => None,
        };
        let run = AutomationRuleApplyRun {
            timestamp_unix_secs: now,
            evaluation,
            profile_run,
        };
        self.record_automation_rule_apply_run(rule_id, run.clone())?;
        Ok(run)
    }

    fn record_automation_rule_apply_run(
        &self,
        rule_id: &str,
        run: AutomationRuleApplyRun,
    ) -> fdo::Result<AutomationRuleApplyRun> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))?;
        state
            .last_automation_rule_apply
            .insert(rule_id.to_owned(), run.clone());
        save_state(&self.state_path, &state)
            .map_err(|error| fdo::Error::Failed(format!("failed to save daemon state: {error}")))?;
        Ok(run)
    }

    fn evaluate_automation_rule(
        &self,
        rule_id: &str,
        rule: &AutomationRule,
    ) -> fdo::Result<AutomationRuleEvaluation> {
        validate_automation_rule(rule).map_err(validation_to_fdo)?;
        let registry = self.refresh()?;
        let battery_capacity_percent = registry
            .telemetry
            .battery
            .as_ref()
            .and_then(|battery| battery.capacity_percent);
        let ac_online = if registry.telemetry.ac_adapters.is_empty() {
            None
        } else {
            Some(
                registry
                    .telemetry
                    .ac_adapters
                    .iter()
                    .any(|adapter| adapter.online == Some(true)),
            )
        };

        let mut evaluation = AutomationRuleEvaluation {
            rule_id: rule_id.to_owned(),
            rule_label: rule.label.clone(),
            enabled: rule.enabled,
            matched: false,
            reason: String::new(),
            battery_capacity_percent,
            ac_online,
            selected_profile_id: None,
            profile_preview: None,
        };

        if !rule.enabled {
            evaluation.reason = "rule is disabled".to_owned();
            return Ok(evaluation);
        }

        match &rule.kind {
            AutomationRuleKind::FastChargeUntilThreshold {
                threshold_percent,
                fast_charge_profile_id,
                protect_profile_id,
                require_ac,
                ..
            } => {
                if *require_ac && ac_online != Some(true) {
                    evaluation.reason = "AC adapter is not online".to_owned();
                    return Ok(evaluation);
                }
                let Some(capacity) = battery_capacity_percent else {
                    evaluation.reason = "battery capacity telemetry is unavailable".to_owned();
                    return Ok(evaluation);
                };
                let selected = if capacity < i64::from(*threshold_percent) {
                    fast_charge_profile_id
                } else {
                    protect_profile_id
                };
                evaluation.matched = true;
                evaluation.reason = if selected == fast_charge_profile_id {
                    format!(
                        "battery {capacity}% is below threshold {threshold_percent}%; selecting fast-charge profile"
                    )
                } else {
                    format!(
                        "battery {capacity}% is at or above threshold {threshold_percent}%; selecting protect profile"
                    )
                };
                evaluation.selected_profile_id = Some(selected.clone());
                evaluation.profile_preview = Some(self.hardware_profile_apply_preview(selected)?);
            }
            AutomationRuleKind::AcProfileRouter {
                ac_profile_id,
                battery_profile_id,
                ..
            } => {
                let Some(ac_online) = ac_online else {
                    evaluation.reason = "AC adapter telemetry is unavailable".to_owned();
                    return Ok(evaluation);
                };
                let selected = if ac_online {
                    ac_profile_id
                } else {
                    battery_profile_id
                };
                evaluation.matched = true;
                evaluation.reason = if ac_online {
                    "AC adapter is online; selecting AC profile".to_owned()
                } else {
                    "AC adapter is offline; selecting battery profile".to_owned()
                };
                evaluation.selected_profile_id = Some(selected.clone());
                evaluation.profile_preview = Some(self.hardware_profile_apply_preview(selected)?);
            }
            AutomationRuleKind::BatteryProfileThreshold {
                threshold_percent,
                profile_id,
                when_below_or_equal,
                require_ac,
                ..
            } => {
                if let Some(required_ac) = require_ac {
                    if ac_online != Some(*required_ac) {
                        evaluation.reason = if *required_ac {
                            "AC adapter is not online".to_owned()
                        } else {
                            "AC adapter is online".to_owned()
                        };
                        return Ok(evaluation);
                    }
                }
                let Some(capacity) = battery_capacity_percent else {
                    evaluation.reason = "battery capacity telemetry is unavailable".to_owned();
                    return Ok(evaluation);
                };
                let threshold = i64::from(*threshold_percent);
                let matched = if *when_below_or_equal {
                    capacity <= threshold
                } else {
                    capacity >= threshold
                };
                if !matched {
                    evaluation.reason = if *when_below_or_equal {
                        format!(
                            "battery {capacity}% is above threshold {threshold_percent}%; skipping profile"
                        )
                    } else {
                        format!(
                            "battery {capacity}% is below threshold {threshold_percent}%; skipping profile"
                        )
                    };
                    return Ok(evaluation);
                }
                evaluation.matched = true;
                evaluation.reason = if *when_below_or_equal {
                    format!(
                        "battery {capacity}% is at or below threshold {threshold_percent}%; selecting profile"
                    )
                } else {
                    format!(
                        "battery {capacity}% is at or above threshold {threshold_percent}%; selecting profile"
                    )
                };
                evaluation.selected_profile_id = Some(profile_id.clone());
                evaluation.profile_preview = Some(self.hardware_profile_apply_preview(profile_id)?);
            }
        }

        Ok(evaluation)
    }

    pub fn last_known_good_fan_curve(&self) -> fdo::Result<Option<FanCurveSnapshot>> {
        self.state
            .lock()
            .map(|state| state.last_known_good_fan_curve.clone())
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))
    }

    pub fn live_fan_curve_readings(&self) -> fdo::Result<FanCurveSnapshot> {
        let registry = self.planning_snapshot().map_err(planning_to_fdo)?;
        read_fan_curve_snapshot(&registry)
    }

    pub fn capture_last_known_good_fan_curve(&self) -> fdo::Result<FanCurveSnapshot> {
        let registry = self.planning_snapshot().map_err(planning_to_fdo)?;
        let snapshot = read_fan_curve_snapshot(&registry)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))?;
        state.last_known_good_fan_curve = Some(snapshot.clone());
        save_state(&self.state_path, &state)
            .map_err(|error| fdo::Error::Failed(format!("failed to save daemon state: {error}")))?;
        Ok(snapshot)
    }

    pub fn fan_preset_by_platform_profile(&self) -> fdo::Result<BTreeMap<String, String>> {
        self.state
            .lock()
            .map(|state| state.fan_preset_by_platform_profile.clone())
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))
    }

    pub fn set_fan_preset_profile_map_entry(
        &self,
        platform_profile: &str,
        fan_preset_id: &str,
    ) -> fdo::Result<BTreeMap<String, String>> {
        let registry = self.planning_snapshot().map_err(planning_to_fdo)?;
        let presets = packaged_fan_presets().map_err(planning_to_fdo)?;
        validate_fan_preset_platform_profile_entry(
            registry.platform_profile.as_ref(),
            &registry.fan_curves,
            &presets,
            platform_profile,
            fan_preset_id,
        )
        .map_err(validation_to_fdo)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))?;
        state
            .fan_preset_by_platform_profile
            .insert(platform_profile.to_owned(), fan_preset_id.to_owned());
        save_state(&self.state_path, &state)
            .map_err(|error| fdo::Error::Failed(format!("failed to save daemon state: {error}")))?;
        Ok(state.fan_preset_by_platform_profile.clone())
    }

    pub fn remove_fan_preset_profile_map_entry(
        &self,
        platform_profile: &str,
    ) -> fdo::Result<BTreeMap<String, String>> {
        if platform_profile.trim().is_empty() {
            return Err(fdo::Error::InvalidArgs(
                "platform_profile must be non-empty".to_owned(),
            ));
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))?;
        state
            .fan_preset_by_platform_profile
            .remove(platform_profile);
        save_state(&self.state_path, &state)
            .map_err(|error| fdo::Error::Failed(format!("failed to save daemon state: {error}")))?;
        Ok(state.fan_preset_by_platform_profile.clone())
    }

    pub fn clear_fan_preset_profile_map(&self) -> fdo::Result<BTreeMap<String, String>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))?;
        state.fan_preset_by_platform_profile.clear();
        save_state(&self.state_path, &state)
            .map_err(|error| fdo::Error::Failed(format!("failed to save daemon state: {error}")))?;
        Ok(state.fan_preset_by_platform_profile.clone())
    }

    pub fn fan_preset_reapply_after_resume(&self) -> fdo::Result<bool> {
        self.state
            .lock()
            .map(|state| state.fan_preset_reapply_after_resume)
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))
    }

    pub fn set_fan_preset_reapply_after_resume(&self, enabled: bool) -> fdo::Result<bool> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))?;
        state.fan_preset_reapply_after_resume = enabled;
        save_state(&self.state_path, &state)
            .map_err(|error| fdo::Error::Failed(format!("failed to save daemon state: {error}")))?;
        Ok(state.fan_preset_reapply_after_resume)
    }

    fn refresh(&self) -> fdo::Result<CapabilityRegistry> {
        let mut registry = probe(&self.options);
        self.annotate_openrgb_keyboard_rgb_sdk_backend(&mut registry);
        let mut cached = self
            .registry
            .lock()
            .map_err(|_| fdo::Error::Failed("capability registry lock poisoned".to_owned()))?;
        *cached = registry.clone();
        Ok(registry)
    }

    fn planning_snapshot(&self) -> Result<CapabilityRegistry, PlanningError> {
        self.registry
            .lock()
            .map(|registry| registry.clone())
            .map_err(|_| PlanningError::RegistryUnavailable)
    }

    fn annotate_openrgb_keyboard_rgb_sdk_backend(&self, registry: &mut CapabilityRegistry) {
        let Some(openrgb) = registry.keyboard_rgb_openrgb.as_mut() else {
            return;
        };
        if self.openrgb_keyboard_rgb_sdk_writer.is_configured() {
            openrgb.sdk_helper_installed = true;
        }
        if !self.write_policy.keyboard_rgb_enabled
            || !self.openrgb_keyboard_rgb_sdk_writer.is_configured()
            || !openrgb.installed
            || openrgb.devices.is_empty()
        {
            return;
        }

        let path = openrgb
            .path
            .as_deref()
            .map(|path| format!("openrgb-sdk:{path}"))
            .unwrap_or_else(|| "openrgb-sdk:openrgb".to_owned());
        let Ok(snapshot) = self
            .openrgb_keyboard_rgb_sdk_writer
            .read_keyboard_rgb_snapshot(&path)
        else {
            openrgb.sdk_server_running = false;
            openrgb.sdk_snapshot_supported = false;
            openrgb.backend_ready = false;
            openrgb.write_support_claimed = false;
            return;
        };

        openrgb.sdk_server_running = true;
        openrgb.sdk_snapshot_supported = true;
        openrgb.sdk_active_mode =
            (!snapshot.active_mode.is_empty()).then_some(snapshot.active_mode);
        openrgb.sdk_color_zones = snapshot.colors.keys().cloned().collect();
        openrgb.sdk_colors = snapshot.colors;
        openrgb.backend_ready = true;
        openrgb.write_support_claimed = true;
    }

    fn refresh_platform_profile(&self) -> fdo::Result<String> {
        let refreshed = self.refresh()?;
        refreshed
            .platform_profile
            .and_then(|profile| profile.current)
            .ok_or_else(|| {
                fdo::Error::Failed(
                    "platform_profile current value missing after write/read-back".to_owned(),
                )
            })
    }

    fn refresh_battery_charge_type(&self) -> fdo::Result<String> {
        let refreshed = self.refresh()?;
        refreshed
            .battery_charge_type
            .and_then(|charge_type| charge_type.current)
            .ok_or_else(|| {
                fdo::Error::Failed(
                    "battery_charge_type current value missing after write/read-back".to_owned(),
                )
            })
    }

    fn refresh_cpu_governor(&self) -> fdo::Result<String> {
        let refreshed = self.refresh()?;
        refreshed
            .cpu_power
            .and_then(|cpu| cpu.governor)
            .ok_or_else(|| {
                fdo::Error::Failed("cpu_power governor missing after write/read-back".to_owned())
            })
    }

    fn refresh_cpu_epp(&self) -> fdo::Result<String> {
        let refreshed = self.refresh()?;
        refreshed.cpu_power.and_then(|cpu| cpu.epp).ok_or_else(|| {
            fdo::Error::Failed("cpu_power epp missing after write/read-back".to_owned())
        })
    }

    fn refresh_cpu_boost(&self) -> fdo::Result<String> {
        let refreshed = self.refresh()?;
        refreshed
            .cpu_power
            .and_then(|cpu| cpu.boost)
            .map(|boost| if boost { "1" } else { "0" }.to_owned())
            .ok_or_else(|| {
                fdo::Error::Failed("cpu_power boost missing after write/read-back".to_owned())
            })
    }

    fn refresh_firmware_attribute(&self, attribute_id: &str) -> fdo::Result<String> {
        let refreshed = self.refresh()?;
        refreshed
            .firmware_attributes
            .into_iter()
            .find(|attribute| attribute.name == attribute_id)
            .and_then(|attribute| attribute.current_value)
            .ok_or_else(|| {
                fdo::Error::Failed(format!(
                    "firmware attribute current value missing after write/read-back for {attribute_id}"
                ))
            })
    }

    fn refresh_amd_gpu_dpm_force_level(&self) -> fdo::Result<String> {
        let refreshed = self.refresh()?;
        refreshed
            .amd_gpu_power_dpm
            .and_then(|dpm| dpm.current_force_performance_level)
            .ok_or_else(|| {
                fdo::Error::Failed(
                    "AMD GPU DPM force level missing after write/read-back".to_owned(),
                )
            })
    }

    fn refresh_led_state(&self, led_id: &str) -> fdo::Result<String> {
        let refreshed = self.refresh()?;
        refreshed_led(&refreshed.leds, led_id)
            .and_then(|led| led.brightness)
            .map(|brightness| brightness.to_string())
            .ok_or_else(|| {
                fdo::Error::Failed(format!(
                    "LED current value missing after write/read-back for {led_id}"
                ))
            })
    }

    fn refresh_keyboard_rgb_state(&self) -> fdo::Result<String> {
        let refreshed = self.refresh()?;
        let capability = refreshed.keyboard_rgb.as_ref().ok_or_else(|| {
            fdo::Error::Failed("keyboard RGB capability missing after write/read-back".to_owned())
        })?;
        let request = keyboard_rgb_current_request(capability)?;
        Ok(keyboard_rgb_request_summary(&request))
    }

    fn refresh_ideapad_toggle_state(
        &self,
        toggle_id: &str,
    ) -> fdo::Result<(String, Option<String>)> {
        let refreshed = self.refresh()?;
        let toggle_value = refreshed_ideapad_toggle(&refreshed.ideapad_toggles, toggle_id)
            .and_then(|toggle| toggle.current_value.clone())
            .ok_or_else(|| {
                fdo::Error::Failed(format!(
                    "ideapad toggle current value missing after write/read-back for {toggle_id}"
                ))
            })?;
        let indicator = if toggle_id == "fn_lock" {
            Some(
                refreshed_led(&refreshed.leds, "platform::fnlock")
                    .and_then(|led| led.brightness)
                    .map(|brightness| brightness.to_string())
                    .ok_or_else(|| {
                        fdo::Error::Failed(
                            "paired platform::fnlock LED missing after write/read-back".to_owned(),
                        )
                    })?,
            )
        } else {
            None
        };
        Ok((toggle_value, indicator))
    }

    fn ideapad_toggle_write_enabled(&self, toggle_id: &str) -> bool {
        match toggle_id {
            "fn_lock" => self.write_policy.ideapad_toggle_enabled,
            "camera_power" => self.write_policy.camera_power_enabled,
            "usb_charging" => self.write_policy.usb_charging_enabled,
            "fan_mode" => self.write_policy.fan_mode_enabled,
            _ => false,
        }
    }
}

#[allow(non_snake_case)]
#[interface(name = "org.ratvantage.LegionControl1")]
impl LegionControl {
    fn GetHardwareSummary(&self) -> fdo::Result<String> {
        to_json(&self.snapshot()?.hardware)
    }

    fn GetCapabilities(&self) -> fdo::Result<String> {
        to_json(&self.snapshot()?.capabilities)
    }

    fn RefreshCapabilities(&self) -> fdo::Result<String> {
        to_json(&self.refresh()?.capabilities)
    }

    fn GetTelemetry(&self) -> fdo::Result<String> {
        to_json(&self.snapshot()?.telemetry)
    }

    fn GetRawProbeReport(&self) -> fdo::Result<String> {
        to_json(&self.snapshot()?)
    }

    fn GetGpuModePending(&self) -> fdo::Result<String> {
        to_json(&self.gpu_mode_pending()?)
    }

    fn GetHardwareProfiles(&self) -> fdo::Result<String> {
        to_json(&self.hardware_profiles()?)
    }

    fn GetHardwareProfileTriggers(&self) -> fdo::Result<String> {
        to_json(&self.hardware_profile_triggers()?)
    }

    fn GetAutomationRules(&self) -> fdo::Result<String> {
        to_json(&self.automation_rules()?)
    }

    fn GetLastAutomationRuleApply(&self) -> fdo::Result<String> {
        to_json(&self.last_automation_rule_apply()?)
    }

    fn GetAutomationRulePreview(&self, rule_id: &str) -> fdo::Result<String> {
        to_json(&self.automation_rule_preview(rule_id)?)
    }

    fn GetHardwareProfileApplyPreview(&self, profile_id: &str) -> fdo::Result<String> {
        to_json(&self.hardware_profile_apply_preview(profile_id)?)
    }

    fn GetHardwareProfileTriggerApplyPreview(&self, trigger_id: &str) -> fdo::Result<String> {
        to_json(&self.hardware_profile_trigger_apply_preview(trigger_id)?)
    }

    fn GetLastHardwareProfileApply(&self) -> fdo::Result<String> {
        to_json(&self.last_hardware_profile_apply()?)
    }

    fn GetRecentPlatformProfileChanges(&self) -> fdo::Result<String> {
        to_json(&self.recent_platform_profile_changes()?)
    }

    fn GetLastCurveOptimizerAllCore(&self) -> fdo::Result<String> {
        to_json(&self.last_curve_optimizer_all_core()?)
    }

    fn GetRyzenBackendStatus(&self) -> fdo::Result<String> {
        to_json(&self.ryzen_backend_status()?)
    }

    fn GetLastKnownGoodFanCurve(&self) -> fdo::Result<String> {
        to_json(&self.last_known_good_fan_curve()?)
    }

    fn GetLiveFanCurveReadings(&self) -> fdo::Result<String> {
        to_json(&self.live_fan_curve_readings()?)
    }

    fn GetFanPresetProfileMap(&self) -> fdo::Result<String> {
        to_json(&self.fan_preset_by_platform_profile()?)
    }

    fn SetFanPresetProfileMapEntry(
        &self,
        platform_profile: &str,
        fan_preset_id: &str,
    ) -> fdo::Result<String> {
        to_json(&self.set_fan_preset_profile_map_entry(platform_profile, fan_preset_id)?)
    }

    fn RemoveFanPresetProfileMapEntry(&self, platform_profile: &str) -> fdo::Result<String> {
        to_json(&self.remove_fan_preset_profile_map_entry(platform_profile)?)
    }

    fn ClearFanPresetProfileMap(&self) -> fdo::Result<String> {
        to_json(&self.clear_fan_preset_profile_map()?)
    }

    fn GetFanPresetReapplyAfterResume(&self) -> fdo::Result<String> {
        to_json(&self.fan_preset_reapply_after_resume()?)
    }

    fn SetFanPresetReapplyAfterResume(&self, enabled: bool) -> fdo::Result<String> {
        to_json(&self.set_fan_preset_reapply_after_resume(enabled)?)
    }

    fn PlanPlatformProfileWrite(&self, requested: &str) -> fdo::Result<String> {
        to_plan_json(self.plan_platform_profile_write(requested))
    }

    fn PlanPrepareCustomThermalMode(&self) -> fdo::Result<String> {
        to_plan_json(self.plan_prepare_custom_thermal_mode())
    }

    fn PlanBatteryChargeTypeWrite(&self, requested: &str) -> fdo::Result<String> {
        to_plan_json(self.plan_battery_charge_type_write(requested))
    }

    fn PlanLedStateWrite(&self, led_id: &str, enabled: bool) -> fdo::Result<String> {
        to_plan_json(self.plan_led_state_write(led_id, enabled))
    }

    fn PlanKeyboardRgbWrite(&self, request_json: &str) -> fdo::Result<String> {
        let request: KeyboardRgbWriteRequest =
            serde_json::from_str(request_json).map_err(|error| {
                fdo::Error::InvalidArgs(format!("invalid keyboard RGB request JSON: {error}"))
            })?;
        to_plan_json(self.plan_keyboard_rgb_write(&request))
    }

    fn PlanOpenRgbKeyboardRgbBridge(&self, request_json: &str) -> fdo::Result<String> {
        let request: KeyboardRgbWriteRequest =
            serde_json::from_str(request_json).map_err(|error| {
                fdo::Error::InvalidArgs(format!(
                    "invalid OpenRGB keyboard RGB request JSON: {error}"
                ))
            })?;
        to_plan_json(self.plan_openrgb_keyboard_rgb_bridge(&request))
    }

    fn PlanOpenRgbKeyboardRgbSdkWrite(&self, request_json: &str) -> fdo::Result<String> {
        let request: KeyboardRgbWriteRequest =
            serde_json::from_str(request_json).map_err(|error| {
                fdo::Error::InvalidArgs(format!(
                    "invalid OpenRGB SDK keyboard RGB request JSON: {error}"
                ))
            })?;
        to_plan_json(self.plan_openrgb_keyboard_rgb_sdk_write(&request))
    }

    fn PlanOpenRgbAccessSetup(&self, target_user: &str) -> fdo::Result<String> {
        to_plan_json(self.plan_openrgb_access_setup(target_user))
    }

    fn PlanIdeapadToggleWrite(&self, toggle_id: &str, enabled: bool) -> fdo::Result<String> {
        to_plan_json(self.plan_ideapad_toggle_write(toggle_id, enabled))
    }

    fn PlanGpuModeWrite(&self, requested: &str) -> fdo::Result<String> {
        to_plan_json(self.plan_gpu_mode_write(requested))
    }

    fn PlanFanPresetWrite(&self, requested: &str) -> fdo::Result<String> {
        to_plan_json(self.plan_fan_preset_write(requested))
    }

    fn PlanCustomThermalFanPresetWrite(&self, requested: &str) -> fdo::Result<String> {
        to_custom_thermal_preview_json(self.plan_custom_thermal_fan_preset_write(requested))
    }

    fn PlanRestoreAutoFanWrite(&self) -> fdo::Result<String> {
        to_plan_json(self.plan_restore_auto_fan_write())
    }

    fn PlanCustomThermalRestoreAutoFanWrite(&self) -> fdo::Result<String> {
        to_custom_thermal_preview_json(self.plan_custom_thermal_restore_auto_fan())
    }

    fn PlanCpuGovernorWrite(&self, requested: &str) -> fdo::Result<String> {
        to_plan_json(self.plan_cpu_governor_write(requested))
    }

    fn PlanCpuEppWrite(&self, requested: &str) -> fdo::Result<String> {
        to_plan_json(self.plan_cpu_epp_write(requested))
    }

    fn PlanCpuBoostWrite(&self, requested: &str) -> fdo::Result<String> {
        to_plan_json(self.plan_cpu_boost_write(requested))
    }

    fn PlanCurveOptimizerAllCoreWrite(&self, requested: &str) -> fdo::Result<String> {
        to_plan_json(self.plan_curve_optimizer_all_core_write(requested))
    }

    fn PlanConservationModeWrite(&self, requested: &str) -> fdo::Result<String> {
        to_plan_json(self.plan_conservation_mode_write(requested))
    }

    fn PlanAmdGpuDpmForceLevelWrite(&self, requested: &str) -> fdo::Result<String> {
        to_plan_json(self.plan_amd_gpu_dpm_force_level_write(requested))
    }

    fn PlanFirmwareAttributeWrite(
        &self,
        attribute_id: &str,
        requested: &str,
    ) -> fdo::Result<String> {
        to_plan_json(self.plan_firmware_attribute_write(attribute_id, requested))
    }

    fn PlanFirmwareAttributeResetWrite(&self, attribute_id: &str) -> fdo::Result<String> {
        to_plan_json(self.plan_firmware_attribute_reset_write(attribute_id))
    }

    fn PlanCustomThermalFirmwareAttributeWrite(
        &self,
        attribute_id: &str,
        requested: &str,
    ) -> fdo::Result<String> {
        to_custom_thermal_preview_json(
            self.plan_custom_thermal_firmware_attribute_write(attribute_id, requested),
        )
    }

    fn PlanCustomThermalFirmwarePptPresetWrite(&self, preset_id: &str) -> fdo::Result<String> {
        to_custom_thermal_preview_json(
            self.plan_custom_thermal_firmware_ppt_preset_write(preset_id),
        )
    }

    fn SetCpuGovernor(
        &self,
        requested: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let sender = sender_from_header(&header)?;
        to_json(&self.set_cpu_governor(requested, &sender)?)
    }

    fn SetCpuEpp(
        &self,
        requested: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let sender = sender_from_header(&header)?;
        to_json(&self.set_cpu_epp(requested, &sender)?)
    }

    fn SetCpuBoost(
        &self,
        requested: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let sender = sender_from_header(&header)?;
        to_json(&self.set_cpu_boost(requested, &sender)?)
    }

    fn SetCurveOptimizerAllCore(
        &self,
        requested: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let sender = sender_from_header(&header)?;
        to_json(&self.set_curve_optimizer_all_core(requested, &sender)?)
    }

    fn SetConservationMode(
        &self,
        requested: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let sender = sender_from_header(&header)?;
        to_json(&self.set_conservation_mode(requested, &sender)?)
    }

    fn SetAmdGpuDpmForceLevel(
        &self,
        requested: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let sender = sender_from_header(&header)?;
        to_json(&self.set_amd_gpu_dpm_force_level(requested, &sender)?)
    }

    fn SetGpuMode(
        &self,
        requested: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let sender = sender_from_header(&header)?;
        to_json(&self.set_gpu_mode(requested, &sender)?)
    }

    fn SetFirmwareAttribute(
        &self,
        attribute_id: &str,
        requested: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let sender = sender_from_header(&header)?;
        to_json(&self.set_firmware_attribute(attribute_id, requested, &sender)?)
    }

    fn SetPlatformProfile(
        &self,
        requested: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let sender = sender_from_header(&header)?;
        to_json(&self.set_platform_profile(requested, &sender)?)
    }

    fn SetBatteryChargeType(
        &self,
        requested: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let sender = sender_from_header(&header)?;
        to_json(&self.set_battery_charge_type(requested, &sender)?)
    }

    fn SetLedState(
        &self,
        led_id: &str,
        enabled: bool,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let sender = sender_from_header(&header)?;
        to_json(&self.set_led_state(led_id, enabled, &sender)?)
    }

    fn SetKeyboardRgb(
        &self,
        request_json: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let sender = sender_from_header(&header)?;
        let request: KeyboardRgbWriteRequest =
            serde_json::from_str(request_json).map_err(|error| {
                fdo::Error::InvalidArgs(format!("invalid keyboard RGB request JSON: {error}"))
            })?;
        to_json(&self.set_keyboard_rgb(&request, &sender)?)
    }

    fn SetupOpenRgbAccess(
        &self,
        target_user: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let sender = sender_from_header(&header)?;
        to_json(&self.setup_openrgb_access(target_user, &sender)?)
    }

    fn SetIdeapadToggle(
        &self,
        toggle_id: &str,
        enabled: bool,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let sender = sender_from_header(&header)?;
        to_json(&self.set_ideapad_toggle(toggle_id, enabled, &sender)?)
    }

    fn SetGpuModePending(&self, requested: &str) -> fdo::Result<String> {
        to_json(&self.set_gpu_mode_pending(requested)?)
    }

    fn ClearGpuModePending(&self) -> fdo::Result<String> {
        to_json(&self.clear_gpu_mode_pending()?)
    }

    fn SetHardwareProfile(&self, profile_id: &str, profile_json: &str) -> fdo::Result<String> {
        to_json(&self.set_hardware_profile(profile_id, profile_json)?)
    }

    fn ApplyHardwareProfile(
        &self,
        profile_id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let sender = sender_from_header(&header)?;
        to_json(&self.apply_hardware_profile(profile_id, &sender)?)
    }

    fn ApplyHardwareProfileTrigger(
        &self,
        trigger_id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let sender = sender_from_header(&header)?;
        to_json(&self.apply_hardware_profile_trigger(trigger_id, &sender)?)
    }

    fn ApplyAutomationRule(
        &self,
        rule_id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let sender = sender_from_header(&header)?;
        to_json(&self.apply_automation_rule(rule_id, &sender)?)
    }

    fn SetHardwareProfileTrigger(&self, trigger_id: &str, profile_id: &str) -> fdo::Result<String> {
        to_json(&self.set_hardware_profile_trigger(trigger_id, profile_id)?)
    }

    fn SetAutomationRule(&self, rule_id: &str, rule_json: &str) -> fdo::Result<String> {
        to_json(&self.set_automation_rule(rule_id, rule_json)?)
    }

    fn RemoveAutomationRule(&self, rule_id: &str) -> fdo::Result<String> {
        to_json(&self.remove_automation_rule(rule_id)?)
    }

    fn ClearAutomationRules(&self) -> fdo::Result<String> {
        to_json(&self.clear_automation_rules()?)
    }

    fn RemoveHardwareProfileTrigger(&self, trigger_id: &str) -> fdo::Result<String> {
        to_json(&self.remove_hardware_profile_trigger(trigger_id)?)
    }

    fn ClearHardwareProfileTriggers(&self) -> fdo::Result<String> {
        to_json(&self.clear_hardware_profile_triggers()?)
    }

    fn RemoveHardwareProfile(&self, profile_id: &str) -> fdo::Result<String> {
        to_json(&self.remove_hardware_profile(profile_id)?)
    }

    fn ClearHardwareProfiles(&self) -> fdo::Result<String> {
        to_json(&self.clear_hardware_profiles()?)
    }

    fn CaptureLastKnownGoodFanCurve(&self) -> fdo::Result<String> {
        to_json(&self.capture_last_known_good_fan_curve()?)
    }
}

pub fn system_connection(service: LegionControl) -> Result<Connection> {
    Ok(ConnectionBuilder::system()?
        .name(DBUS_INTERFACE)?
        .serve_at(DBUS_PATH, service)?
        .build()?)
}

pub fn session_connection(service: LegionControl) -> Result<Connection> {
    Ok(ConnectionBuilder::session()?
        .name(DBUS_INTERFACE)?
        .serve_at(DBUS_PATH, service)?
        .build()?)
}

/// Subscribes to systemd-logind `PrepareForSleep` on the system bus and, after resume:
/// - refreshes the probe cache and prints a **dry-run** fan preset plan when fan re-apply is enabled
/// - applies a mapped `resume` hardware-profile trigger through normal daemon write gates
pub fn spawn_fan_preset_resume_observer(
    state_path: PathBuf,
    options: ProbeOptions,
    write_policy: WriteAccessPolicy,
) {
    let _ = std::thread::Builder::new()
        .name("ratvantage-fan-resume".to_owned())
        .spawn(move || {
            if let Err(error) = run_fan_preset_resume_observer(state_path, options, write_policy) {
                eprintln!("legion-control-daemon: fan resume observer exit: {error}");
            }
        });
}

/// Polls Fedora's PowerProfiles D-Bus state and mirrors it to amdgpu
/// `power_dpm_force_performance_level` when the daemon was started with the opt-in flag.
pub fn spawn_amd_gpu_power_profile_sync_observer(options: ProbeOptions) {
    let _ = std::thread::Builder::new()
        .name("ratvantage-amdgpu-power-sync".to_owned())
        .spawn(move || loop {
            match handle_amd_gpu_power_profile_sync_tick(&options) {
                Ok(AmdGpuPowerProfileSyncOutcome::Applied {
                    active_profile,
                    previous_force_level,
                    force_level,
                    path,
                }) => {
                    eprintln!(
                        "legion-control-daemon: AMD GPU power sync: Fedora profile `{active_profile}` mapped {path} from `{previous_force_level}` to `{force_level}`"
                    );
                }
                Ok(_) => {}
                Err(error) => {
                    eprintln!("legion-control-daemon: AMD GPU power sync failed: {error}");
                }
            }
            std::thread::sleep(AMD_GPU_POWER_PROFILE_SYNC_INTERVAL);
        });
}

/// Periodically evaluates persisted automation rules and applies the selected hardware profile
/// through daemon validators/read-back/rollback. This observer is opt-in from daemon startup.
pub fn spawn_automation_observer(
    state_path: PathBuf,
    options: ProbeOptions,
    write_policy: WriteAccessPolicy,
) {
    let _ = std::thread::Builder::new()
        .name("ratvantage-automation".to_owned())
        .spawn(move || loop {
            match handle_automation_observer_tick(
                &state_path,
                &options,
                write_policy.clone(),
                AUTOMATION_OBSERVER_COOLDOWN_SECS,
            ) {
                Ok(runs) => {
                    for run in runs {
                        if run.profile_run.is_some() {
                            eprintln!(
                                "legion-control-daemon: automation `{}` applied: {}",
                                run.evaluation.rule_id, run.evaluation.reason
                            );
                        }
                    }
                }
                Err(error) => {
                    eprintln!("legion-control-daemon: automation observer failed: {error}");
                }
            }
            match handle_platform_profile_change_observer_tick(
                &state_path,
                &options,
                write_policy.clone(),
            ) {
                Ok(Some(run)) => {
                    eprintln!(
                        "legion-control-daemon: platform profile changed trigger applied: profile={} completed={}",
                        run.profile_id, run.completed
                    );
                }
                Ok(None) => {}
                Err(error) => {
                    eprintln!(
                        "legion-control-daemon: platform profile change observer failed: {error}"
                    );
                }
            }
            match handle_gpu_mode_reboot_completion_observer_tick(
                &state_path,
                &options,
                write_policy.clone(),
            ) {
                Ok(Some(run)) => {
                    eprintln!(
                        "legion-control-daemon: GPU reboot completion trigger applied: profile={} completed={}",
                        run.profile_id, run.completed
                    );
                }
                Ok(None) => {}
                Err(error) => {
                    eprintln!(
                        "legion-control-daemon: GPU reboot completion observer failed: {error}"
                    );
                }
            }
            std::thread::sleep(AUTOMATION_OBSERVER_INTERVAL);
        });
}

pub fn handle_automation_observer_tick(
    state_path: &Path,
    options: &ProbeOptions,
    write_policy: WriteAccessPolicy,
    _default_cooldown_secs: u64,
) -> Result<Vec<AutomationRuleApplyRun>, String> {
    if !write_policy.hardware_profile_apply_enabled {
        return Err("automation observer requires hardware profile apply policy".to_owned());
    }
    let ctl = LegionControl::new_with_runtime(
        options.clone(),
        state_path.to_path_buf(),
        write_policy,
        Arc::new(InternalAuthorizer),
        Arc::new(SysfsPlatformProfileWriter),
        Arc::new(SysfsBatteryChargeTypeWriter),
        Arc::new(SysfsLedStateWriter),
        Arc::new(SysfsIdeapadToggleWriter),
        Arc::new(SysfsCpuGovernorWriter),
        Arc::new(SysfsCpuEppWriter),
    );
    let rules = ctl
        .automation_rules()
        .map_err(|e| format!("automation rules unavailable: {e}"))?;
    let mut runs = Vec::new();
    for (rule_id, rule) in &rules {
        let rule_cooldown_secs = match &rule.kind {
            AutomationRuleKind::FastChargeUntilThreshold { cooldown_secs, .. } => *cooldown_secs,
            AutomationRuleKind::AcProfileRouter { cooldown_secs, .. } => *cooldown_secs,
            AutomationRuleKind::BatteryProfileThreshold { cooldown_secs, .. } => *cooldown_secs,
        };
        let run = ctl
            .apply_automation_rule_with_cooldown(
                rule_id,
                AUTOMATION_OBSERVER_SENDER,
                Some(rule_cooldown_secs),
            )
            .map_err(|e| format!("automation rule `{rule_id}` failed: {e}"))?;
        runs.push(run);
    }
    Ok(runs)
}

pub fn handle_platform_profile_change_observer_tick(
    state_path: &Path,
    options: &ProbeOptions,
    write_policy: WriteAccessPolicy,
) -> Result<Option<HardwareProfileApplyRun>, String> {
    let ctl = LegionControl::new_with_runtime(
        options.clone(),
        state_path.to_path_buf(),
        write_policy,
        Arc::new(InternalAuthorizer),
        Arc::new(SysfsPlatformProfileWriter),
        Arc::new(SysfsBatteryChargeTypeWriter),
        Arc::new(SysfsLedStateWriter),
        Arc::new(SysfsIdeapadToggleWriter),
        Arc::new(SysfsCpuGovernorWriter),
        Arc::new(SysfsCpuEppWriter),
    );
    let Some((previous, current)) = ctl
        .observe_platform_profile_change()
        .map_err(|e| format!("platform profile observation failed: {e}"))?
    else {
        return Ok(None);
    };
    let triggers = ctl
        .hardware_profile_triggers()
        .map_err(|e| format!("hardware profile triggers unavailable: {e}"))?;
    if !triggers.contains_key("platform_profile_changed") {
        return Ok(None);
    }

    let run = ctl
        .apply_hardware_profile_trigger("platform_profile_changed", AUTOMATION_OBSERVER_SENDER)
        .map_err(|e| {
            format!(
                "platform profile changed trigger failed after `{previous}` -> `{current}`: {e}"
            )
        })?;
    ctl.record_current_platform_profile_observed()
        .map_err(|e| format!("failed to record post-trigger platform profile: {e}"))?;
    Ok(Some(run))
}

pub fn handle_gpu_mode_reboot_completion_observer_tick(
    state_path: &Path,
    options: &ProbeOptions,
    write_policy: WriteAccessPolicy,
) -> Result<Option<HardwareProfileApplyRun>, String> {
    if !write_policy.hardware_profile_apply_enabled {
        return Err(
            "GPU mode reboot completion trigger requires hardware profile apply policy".to_owned(),
        );
    }
    let ctl = LegionControl::new_with_runtime(
        options.clone(),
        state_path.to_path_buf(),
        write_policy,
        Arc::new(InternalAuthorizer),
        Arc::new(SysfsPlatformProfileWriter),
        Arc::new(SysfsBatteryChargeTypeWriter),
        Arc::new(SysfsLedStateWriter),
        Arc::new(SysfsIdeapadToggleWriter),
        Arc::new(SysfsCpuGovernorWriter),
        Arc::new(SysfsCpuEppWriter),
    );
    let Some(pending) = ctl
        .gpu_mode_pending()
        .map_err(|e| format!("GPU mode pending state unavailable: {e}"))?
    else {
        return Ok(None);
    };
    let registry = ctl
        .refresh()
        .map_err(|e| format!("GPU mode reboot completion probe failed: {e}"))?;
    let current_mode = registry.gpu.as_ref().and_then(|gpu| gpu.mode.as_deref());
    if current_mode != Some(pending.requested_mode.as_str()) {
        return Ok(None);
    }

    ctl.clear_gpu_mode_pending()
        .map_err(|e| format!("failed to clear completed GPU pending state: {e}"))?;
    let triggers = ctl
        .hardware_profile_triggers()
        .map_err(|e| format!("hardware profile triggers unavailable: {e}"))?;
    if !triggers.contains_key("gpu_mode_reboot_completed") {
        return Ok(None);
    }

    ctl.apply_hardware_profile_trigger("gpu_mode_reboot_completed", AUTOMATION_OBSERVER_SENDER)
        .map(Some)
        .map_err(|e| format!("GPU mode reboot completed trigger failed: {e}"))
}

pub fn handle_amd_gpu_power_profile_sync_tick(
    options: &ProbeOptions,
) -> Result<AmdGpuPowerProfileSyncOutcome, String> {
    let registry = probe(options);
    let Some(active_profile) = registry
        .power_profiles
        .as_ref()
        .and_then(|profile| profile.active_profile.as_deref())
    else {
        return Ok(AmdGpuPowerProfileSyncOutcome::MissingFedoraPowerProfile);
    };
    apply_amd_gpu_force_level_for_fedora_power_profile(&registry, active_profile)
}

pub fn amd_gpu_force_level_for_fedora_power_profile(profile: &str) -> Option<&'static str> {
    match profile {
        "power-saver" | "low-power" | "quiet" => Some("low"),
        "balanced" | "performance" | "balanced-performance" => Some("auto"),
        _ => None,
    }
}

pub fn apply_amd_gpu_force_level_for_fedora_power_profile(
    registry: &CapabilityRegistry,
    active_profile: &str,
) -> Result<AmdGpuPowerProfileSyncOutcome, String> {
    let Some(target) = amd_gpu_force_level_for_fedora_power_profile(active_profile) else {
        return Ok(
            AmdGpuPowerProfileSyncOutcome::UnsupportedFedoraPowerProfile(active_profile.to_owned()),
        );
    };
    let Some(capability) = registry.amd_gpu_power_dpm.as_ref() else {
        return Ok(AmdGpuPowerProfileSyncOutcome::MissingAmdGpuDpmCapability);
    };
    if !capability.choices.iter().any(|choice| choice == target) {
        return Err(format!(
            "AMD GPU DPM force-level target `{target}` is not listed by capability choices {:?}",
            capability.choices
        ));
    }
    let previous = capability
        .current_force_performance_level
        .clone()
        .unwrap_or_default();
    if previous == target {
        return Ok(AmdGpuPowerProfileSyncOutcome::AlreadyApplied {
            active_profile: active_profile.to_owned(),
            force_level: target.to_owned(),
        });
    }

    let path = capability.force_performance_level_path.clone();
    fs::write(&path, target)
        .map_err(|error| format!("failed to write AMD GPU DPM force level {path}: {error}"))?;
    let readback = read_trimmed_path(&path)
        .ok_or_else(|| format!("AMD GPU DPM force level read-back missing after writing {path}"))?;
    if readback == target {
        return Ok(AmdGpuPowerProfileSyncOutcome::Applied {
            active_profile: active_profile.to_owned(),
            previous_force_level: previous,
            force_level: target.to_owned(),
            path,
        });
    }

    if !previous.is_empty() && capability.choices.iter().any(|choice| choice == &previous) {
        let _ = fs::write(&path, &previous);
    }
    Err(format!(
        "AMD GPU DPM force level read-back mismatch after write: expected `{target}`, got `{readback}`"
    ))
}

fn run_fan_preset_resume_observer(
    state_path: PathBuf,
    options: ProbeOptions,
    write_policy: WriteAccessPolicy,
) -> Result<(), String> {
    let conn = Connection::system().map_err(|e| e.to_string())?;
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .path("/org/freedesktop/login1")
        .map_err(|e: zbus::Error| e.to_string())?
        .interface("org.freedesktop.login1.Manager")
        .map_err(|e: zbus::Error| e.to_string())?
        .member("PrepareForSleep")
        .map_err(|e: zbus::Error| e.to_string())?
        .build();
    let iter = MessageIterator::for_match_rule(rule, &conn, Some(32)).map_err(|e| e.to_string())?;
    for msg in iter {
        let msg = msg.map_err(|e| e.to_string())?;
        if msg.message_type() != Type::Signal {
            continue;
        }
        let start = match msg.body().deserialize::<bool>() {
            Ok(value) => value,
            Err(_) => continue,
        };
        if start {
            continue;
        }
        match handle_login1_resume_tick(&state_path, &options, write_policy.clone()) {
            Ok(Some(run)) => {
                eprintln!(
                    "legion-control-daemon: resume hardware profile trigger applied: profile={} completed={}",
                    run.profile_id, run.completed
                );
            }
            Ok(None) => {}
            Err(error) => {
                eprintln!("legion-control-daemon: resume policy tick: {error}");
            }
        }
    }
    Ok(())
}

pub fn handle_login1_resume_tick(
    state_path: &Path,
    options: &ProbeOptions,
    write_policy: WriteAccessPolicy,
) -> Result<Option<HardwareProfileApplyRun>, String> {
    handle_login1_resume_fan_tick(state_path, options)?;
    handle_login1_resume_hardware_profile_trigger_tick(state_path, options, write_policy)
}

fn handle_login1_resume_fan_tick(state_path: &Path, options: &ProbeOptions) -> Result<(), String> {
    let ctl = LegionControl::new_with_state_path(options.clone(), state_path.to_path_buf());
    ctl.refresh()
        .map_err(|e| format!("resume fan reapply: probe refresh failed: {e}"))?;
    let enabled = ctl
        .state
        .lock()
        .map_err(|_| "daemon state lock poisoned".to_owned())?
        .fan_preset_reapply_after_resume;
    if !enabled {
        return Ok(());
    }
    let map = ctl
        .fan_preset_by_platform_profile()
        .map_err(|e| format!("resume fan reapply: {e}"))?;
    let profile = ctl
        .snapshot()
        .map_err(|e| format!("resume fan reapply: {e}"))?
        .platform_profile
        .as_ref()
        .and_then(|p| p.current.clone())
        .ok_or_else(|| "resume fan reapply: platform profile unknown".to_owned())?;
    let Some(preset_id) = map.get(&profile) else {
        eprintln!(
            "legion-control-daemon: resume fan reapply enabled but no preset mapping for profile `{profile}`"
        );
        return Ok(());
    };
    match ctl.plan_fan_preset_write(preset_id.as_str()) {
        Ok(plan) => {
            eprintln!(
                "legion-control-daemon: resume fan reapply (dry-run): profile={profile} preset={preset_id} plan_method={} requested={}",
                plan.method, plan.requested_value
            );
        }
        Err(error) => {
            eprintln!(
                "legion-control-daemon: resume fan reapply plan failed: profile={profile} preset={preset_id}: {error:?}"
            );
        }
    }
    Ok(())
}

fn handle_login1_resume_hardware_profile_trigger_tick(
    state_path: &Path,
    options: &ProbeOptions,
    write_policy: WriteAccessPolicy,
) -> Result<Option<HardwareProfileApplyRun>, String> {
    let ctl = LegionControl::new_with_runtime(
        options.clone(),
        state_path.to_path_buf(),
        write_policy,
        Arc::new(InternalAuthorizer),
        Arc::new(SysfsPlatformProfileWriter),
        Arc::new(SysfsBatteryChargeTypeWriter),
        Arc::new(SysfsLedStateWriter),
        Arc::new(SysfsIdeapadToggleWriter),
        Arc::new(SysfsCpuGovernorWriter),
        Arc::new(SysfsCpuEppWriter),
    );
    let triggers = ctl
        .hardware_profile_triggers()
        .map_err(|e| format!("resume hardware profile trigger unavailable: {e}"))?;
    if !triggers.contains_key("resume") {
        return Ok(None);
    }
    ctl.apply_hardware_profile_trigger("resume", RESUME_OBSERVER_SENDER)
        .map(Some)
        .map_err(|e| format!("resume hardware profile trigger failed: {e}"))
}

fn to_json<T: Serialize>(value: &T) -> fdo::Result<String> {
    serde_json::to_string(value).map_err(|error| fdo::Error::Failed(error.to_string()))
}

fn to_plan_json(result: Result<WriteDryRunPlan, PlanningError>) -> fdo::Result<String> {
    match result {
        Ok(plan) => to_json(&plan),
        Err(error) => Err(planning_to_fdo(error)),
    }
}

fn to_custom_thermal_preview_json(
    result: Result<CustomThermalPlanPreview, PlanningError>,
) -> fdo::Result<String> {
    match result {
        Ok(preview) => to_json(&preview),
        Err(error) => Err(planning_to_fdo(error)),
    }
}

fn staged_custom_platform_profile(
    registry: &CapabilityRegistry,
) -> Result<legion_common::PlatformProfileCapability, PlanningError> {
    let mut profile = registry.platform_profile.clone().ok_or_else(|| {
        PlanningError::Validation(ValidationError::MissingCapability {
            capability_id: "platform_profile".to_owned(),
        })
    })?;
    profile.current = Some("custom".to_owned());
    Ok(profile)
}

fn custom_thermal_preview(
    sequence_id: &str,
    target: String,
    plans: Vec<WriteDryRunPlan>,
) -> CustomThermalPlanPreview {
    let rollback_order = plans
        .iter()
        .rev()
        .map(|plan| {
            format!(
                "{} rollback: {}",
                plan.method,
                plan.rollback_instructions.join("; ")
            )
        })
        .collect();
    CustomThermalPlanPreview {
        sequence_id: sequence_id.to_owned(),
        target,
        plans,
        rollback_order,
        safety_notes: vec![
            "preview only; execute no hardware writes until each step has live evidence".to_owned(),
            "run plans in listed order and roll back in rollback_order if a later step fails"
                .to_owned(),
        ],
    }
}

fn enabled_write_plan(mut plan: WriteDryRunPlan) -> WriteDryRunPlan {
    plan.safety_notes
        .retain(|note| !note.to_ascii_lowercase().contains("dry-run planning only"));
    plan.safety_notes.push(
        "write method enabled by daemon policy; applied writes still require polkit and read-back verification"
            .to_owned(),
    );
    plan
}

fn unix_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn planning_to_fdo(error: PlanningError) -> fdo::Error {
    match error {
        PlanningError::RegistryUnavailable => {
            fdo::Error::Failed("capability registry unavailable".to_owned())
        }
        PlanningError::PresetUnavailable(error) => fdo::Error::Failed(error),
        PlanningError::Validation(error) => validation_to_fdo(error),
    }
}

fn refreshed_led<'a>(leds: &'a [LedCapability], led_id: &str) -> Option<&'a LedCapability> {
    leds.iter().find(|led| led.name == led_id)
}

fn refreshed_ideapad_toggle<'a>(
    toggles: &'a [IdeapadToggleCapability],
    toggle_id: &str,
) -> Option<&'a IdeapadToggleCapability> {
    toggles.iter().find(|toggle| toggle.name == toggle_id)
}

fn keyboard_rgb_current_request(
    capability: &KeyboardRgbCapability,
) -> fdo::Result<KeyboardRgbWriteRequest> {
    Ok(KeyboardRgbWriteRequest {
        effect: capability.current_effect.clone().ok_or_else(|| {
            fdo::Error::Failed("keyboard RGB current effect missing for read-back".to_owned())
        })?,
        colors: capability.current_colors.clone(),
        brightness: capability.current_brightness.ok_or_else(|| {
            fdo::Error::Failed("keyboard RGB current brightness missing for read-back".to_owned())
        })?,
        speed: capability.current_speed,
    })
}

fn keyboard_rgb_request_summary(request: &KeyboardRgbWriteRequest) -> String {
    format!(
        "effect={};brightness={};speed={};colors={}",
        request.effect,
        request.brightness,
        request
            .speed
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_owned()),
        request
            .colors
            .iter()
            .map(|(zone, color)| format!("{zone}:{color}"))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn openrgb_sdk_snapshot_matches_request(
    snapshot: &OpenRgbKeyboardRgbSdkSnapshot,
    request: &KeyboardRgbWriteRequest,
) -> bool {
    snapshot.active_mode.eq_ignore_ascii_case(&request.effect)
        && request.colors.iter().all(|(zone, requested)| {
            snapshot
                .colors
                .get(zone)
                .map(|readback| normalize_rgb_hex(readback) == normalize_rgb_hex(requested))
                .unwrap_or(false)
        })
}

fn openrgb_sdk_snapshot_summary(snapshot: &OpenRgbKeyboardRgbSdkSnapshot) -> String {
    format!(
        "active_mode={};colors={}",
        snapshot.active_mode,
        snapshot
            .colors
            .iter()
            .map(|(zone, color)| format!("{zone}:{}", normalize_rgb_hex(color)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn normalize_rgb_hex(color: &str) -> String {
    let trimmed = color.trim();
    if trimmed.starts_with('#') {
        trimmed.to_ascii_uppercase()
    } else {
        format!("#{}", trimmed.to_ascii_uppercase())
    }
}

fn validation_to_fdo(error: ValidationError) -> fdo::Error {
    fdo::Error::InvalidArgs(serde_json::to_string(&error).unwrap_or_else(|_| format!("{error:?}")))
}

fn sender_from_header(header: &Header<'_>) -> fdo::Result<String> {
    header
        .sender()
        .map(ToString::to_string)
        .ok_or_else(|| fdo::Error::Failed("D-Bus caller sender is missing".to_owned()))
}

fn validate_openrgb_target_user(target_user: &str) -> Result<(), ValidationError> {
    if target_user.trim().is_empty() {
        return Err(ValidationError::EmptyValue {
            field: "target_user".to_owned(),
        });
    }
    if target_user == "root" {
        return Err(ValidationError::BlockedChoice {
            capability_id: "keyboard_rgb_openrgb:access_setup:user".to_owned(),
            requested: target_user.to_owned(),
            reason: "OpenRGB access setup must target a non-root desktop user".to_owned(),
        });
    }
    if target_user
        .chars()
        .any(|ch| !(ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.')))
    {
        return Err(ValidationError::BlockedChoice {
            capability_id: "keyboard_rgb_openrgb:access_setup:user".to_owned(),
            requested: target_user.to_owned(),
            reason: "target user contains unsupported characters".to_owned(),
        });
    }
    Ok(())
}

fn ensure_openrgb_target_user(target_user: &str) -> std::result::Result<(), String> {
    validate_openrgb_target_user(target_user).map_err(|error| format!("{error:?}"))?;
    let output = Command::new("id")
        .arg(target_user)
        .output()
        .map_err(|error| format!("failed to inspect target user: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!("target user does not exist: {target_user}"))
    }
}

fn system_group_exists(group: &str) -> std::result::Result<bool, String> {
    let output = Command::new("getent")
        .args(["group", group])
        .output()
        .map_err(|error| format!("failed to inspect group {group}: {error}"))?;
    Ok(output.status.success())
}

fn user_in_group(target_user: &str, group: &str) -> std::result::Result<bool, String> {
    let output = Command::new("id")
        .args(["-nG", target_user])
        .output()
        .map_err(|error| format!("failed to inspect groups for {target_user}: {error}"))?;
    if !output.status.success() {
        return Err(format!("failed to inspect groups for {target_user}"));
    }
    let groups = String::from_utf8_lossy(&output.stdout);
    Ok(groups
        .split_whitespace()
        .any(|candidate| candidate == group))
}

fn run_setup_command(command: &str, args: &[&str]) -> std::result::Result<(), String> {
    let output = Command::new(command)
        .args(args)
        .output()
        .map_err(|error| format!("failed to run {command}: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("{command} exited with status {}", output.status)
    };
    Err(detail)
}

fn install_root_file(path: &str, content: &str) -> std::result::Result<(), String> {
    let path = Path::new(path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    fs::write(path, content)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o644))
        .map_err(|error| format!("failed to chmod {}: {error}", path.display()))
}

fn pkcheck_args(action: &str, sender: &str) -> Vec<String> {
    vec![
        "--action-id".to_owned(),
        action.to_owned(),
        "--system-bus-name".to_owned(),
        sender.to_owned(),
        "--allow-user-interaction".to_owned(),
    ]
}

fn ideapad_toggle_policy_message(toggle_id: &str) -> String {
    match toggle_id {
        "camera_power" => "camera power writes are disabled by daemon policy".to_owned(),
        "usb_charging" => "usb charging writes are disabled by daemon policy".to_owned(),
        "fn_lock" => "fn_lock writes are disabled by daemon policy".to_owned(),
        _ => "ideapad toggle writes are disabled by daemon policy".to_owned(),
    }
}

fn read_fan_curve_snapshot(registry: &CapabilityRegistry) -> fdo::Result<FanCurveSnapshot> {
    let curve = registry
        .fan_curves
        .first()
        .ok_or_else(|| fdo::Error::InvalidArgs("fan_curves capability is missing".to_owned()))?;
    let mut points = Vec::with_capacity(curve.point_paths.len());
    for path in &curve.point_paths {
        let value = fs::read_to_string(path)
            .map_err(|error| {
                fdo::Error::Failed(format!("failed to read fan curve point {path}: {error}"))
            })?
            .trim()
            .to_owned();
        points.push(legion_common::FanCurvePointSnapshot {
            path: path.clone(),
            value,
        });
    }

    Ok(FanCurveSnapshot {
        curve_id: curve.id.clone(),
        path: curve.path.clone(),
        points,
    })
}

fn read_trimmed_path(path: &str) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn load_state(path: &Path) -> Result<DaemonState> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(toml::from_str(&contents)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(DaemonState::default()),
        Err(error) => Err(error.into()),
    }
}

fn save_state(path: &Path, state: &DaemonState) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, toml::to_string_pretty(state)?)?;
    Ok(())
}

fn packaged_fan_presets() -> Result<Vec<FanPreset>, PlanningError> {
    PACKAGED_FAN_PRESETS
        .iter()
        .map(|preset| {
            toml::from_str::<FanPreset>(preset)
                .map_err(|error| PlanningError::PresetUnavailable(error.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkcheck_args_use_system_bus_subject_and_interaction_flag() {
        assert_eq!(
            pkcheck_args(
                "org.ratvantage.LegionControl1.set-platform-profile",
                ":1.42"
            ),
            [
                "--action-id",
                "org.ratvantage.LegionControl1.set-platform-profile",
                "--system-bus-name",
                ":1.42",
                "--allow-user-interaction",
            ]
        );
    }

    #[test]
    fn write_access_policy_lists_enabled_gated_methods() {
        assert_eq!(
            WriteAccessPolicy {
                platform_profile_enabled: true,
                battery_charge_type_enabled: true,
                led_state_enabled: true,
                keyboard_rgb_enabled: true,
                ideapad_toggle_enabled: true,
                camera_power_enabled: true,
                usb_charging_enabled: true,
                fan_mode_enabled: true,
                gpu_mode_enabled: false,
                cpu_governor_enabled: true,
                cpu_epp_enabled: true,
                firmware_attribute_enabled: true,
                cpu_boost_enabled: true,
                conservation_mode_enabled: true,
                amd_gpu_dpm_enabled: true,
                curve_optimizer_enabled: true,
                openrgb_access_setup_enabled: true,
                hardware_profile_apply_enabled: true,
            }
            .enabled_methods(),
            [
                "SetPlatformProfile",
                "SetBatteryChargeType",
                "SetLedState",
                "SetKeyboardRgb",
                "SetIdeapadToggle",
                "SetCpuGovernor",
                "SetCpuEpp",
                "SetFirmwareAttribute",
                "SetCpuBoost",
                "SetConservationMode",
                "SetAmdGpuDpmForceLevel",
                "SetCurveOptimizerAllCore",
                "SetupOpenRgbAccess",
                "ApplyHardwareProfile",
                "ApplyHardwareProfileTrigger"
            ]
        );
    }

    #[test]
    fn enabled_write_plan_replaces_dry_run_safety_note() {
        let plan = WriteDryRunPlan {
            method: "SetPlatformProfile".to_owned(),
            capability_id: "platform_profile".to_owned(),
            polkit_action: "org.ratvantage.LegionControl1.set-platform-profile".to_owned(),
            path: "/tmp/platform_profile".to_owned(),
            previous_value: "quiet".to_owned(),
            requested_value: "balanced".to_owned(),
            readback_required: true,
            rollback_value: "quiet".to_owned(),
            rollback_instructions: Vec::new(),
            reboot_required: false,
            safety_notes: vec!["write method remains disabled; dry-run planning only".to_owned()],
            steps: Vec::new(),
        };

        let plan = enabled_write_plan(plan);

        assert!(!plan
            .safety_notes
            .iter()
            .any(|note| note.contains("dry-run planning only")));
        assert!(plan
            .safety_notes
            .iter()
            .any(|note| note.contains("write method enabled by daemon policy")));
    }

    #[test]
    fn fedora_power_profiles_map_to_amd_gpu_force_levels() {
        assert_eq!(
            amd_gpu_force_level_for_fedora_power_profile("power-saver"),
            Some("low")
        );
        assert_eq!(
            amd_gpu_force_level_for_fedora_power_profile("balanced"),
            Some("auto")
        );
        assert_eq!(
            amd_gpu_force_level_for_fedora_power_profile("performance"),
            Some("auto")
        );
        assert_eq!(
            amd_gpu_force_level_for_fedora_power_profile("unknown"),
            None
        );
    }

    #[test]
    fn amd_gpu_power_profile_sync_writes_low_for_power_saver() {
        let root =
            std::env::temp_dir().join(format!("ratvantage-amdgpu-sync-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let force_path = root.join("power_dpm_force_performance_level");
        fs::write(&force_path, "auto\n").unwrap();
        let registry = CapabilityRegistry {
            amd_gpu_power_dpm: Some(legion_common::AmdGpuPowerDpmCapability {
                card: "card7".to_owned(),
                status: legion_common::CapabilityStatus::ProbeOnly,
                vendor: "0x1002".to_owned(),
                force_performance_level_path: force_path.display().to_string(),
                current_force_performance_level: Some("auto".to_owned()),
                power_dpm_state: Some("performance".to_owned()),
                current_sclk: Some("2: 2200Mhz *".to_owned()),
                current_mclk: Some("1: 1600Mhz *".to_owned()),
                choices: vec!["auto".to_owned(), "low".to_owned()],
            }),
            ..Default::default()
        };

        let outcome =
            apply_amd_gpu_force_level_for_fedora_power_profile(&registry, "power-saver").unwrap();

        assert!(matches!(
            outcome,
            AmdGpuPowerProfileSyncOutcome::Applied {
                previous_force_level,
                force_level,
                ..
            } if previous_force_level == "auto" && force_level == "low"
        ));
        assert_eq!(fs::read_to_string(&force_path).unwrap(), "low");
        let _ = fs::remove_dir_all(root);
    }
}
