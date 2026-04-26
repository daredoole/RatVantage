use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};

use anyhow::Result;
use legion_common::{
    plan_battery_charge_type_write as plan_battery_charge_type,
    plan_fan_preset_write as plan_fan_preset, plan_gpu_mode_write as plan_gpu_mode,
    plan_ideapad_toggle_write as plan_ideapad_toggle, plan_led_state_write as plan_led_state,
    plan_platform_profile_write as plan_platform_profile,
    plan_restore_auto_fan_write as plan_restore_auto_fan, validate_gpu_mode_choice,
    CapabilityRegistry, DaemonState, FanCurveSnapshot, FanPreset, GpuModePending,
    IdeapadToggleCapability, LedCapability, ValidationError, WriteDryRunPlan, WriteExecutionResult,
};
use legion_probe::{probe, ProbeOptions};
use serde::Serialize;
use zbus::{blocking::Connection, blocking::ConnectionBuilder, fdo, interface, message::Header};

pub const DBUS_INTERFACE: &str = "org.ratvantage.LegionControl1";
pub const DBUS_PATH: &str = "/org/ratvantage/LegionControl1";
pub const READ_ONLY_METHODS: &str = "GetHardwareSummary,GetCapabilities,RefreshCapabilities,GetTelemetry,GetRawProbeReport,GetGpuModePending,GetLastKnownGoodFanCurve,PlanPlatformProfileWrite,PlanBatteryChargeTypeWrite,PlanLedStateWrite,PlanIdeapadToggleWrite,PlanGpuModeWrite,PlanFanPresetWrite,PlanRestoreAutoFanWrite,SetGpuModePending,ClearGpuModePending,CaptureLastKnownGoodFanCurve";
pub const GATED_WRITE_METHODS: &str =
    "SetPlatformProfile,SetBatteryChargeType,SetLedState,SetIdeapadToggle";
pub const DEFAULT_STATE_PATH: &str = "/var/lib/legion-control/state.toml";

const PACKAGED_FAN_PRESETS: &[&str] = &[
    include_str!("../../../data/presets/quiet-office.toml"),
    include_str!("../../../data/presets/balanced-daily.toml"),
    include_str!("../../../data/presets/gaming.toml"),
    include_str!("../../../data/presets/max-safe.toml"),
];

const PLATFORM_PROFILE_WRITE_METHOD: &str = "SetPlatformProfile";
const BATTERY_CHARGE_TYPE_WRITE_METHOD: &str = "SetBatteryChargeType";
const LED_STATE_WRITE_METHOD: &str = "SetLedState";
const IDEAPAD_TOGGLE_WRITE_METHOD: &str = "SetIdeapadToggle";
const PKCHECK_MISSING: &str = "pkcheck is required for polkit authorization";

#[derive(Debug, Clone, Default)]
pub struct WriteAccessPolicy {
    pub platform_profile_enabled: bool,
    pub battery_charge_type_enabled: bool,
    pub led_state_enabled: bool,
    pub ideapad_toggle_enabled: bool,
    pub camera_power_enabled: bool,
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
        if self.ideapad_toggle_enabled || self.camera_power_enabled {
            methods.push(IDEAPAD_TOGGLE_WRITE_METHOD);
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

pub trait IdeapadToggleWriter: Send + Sync {
    fn write_ideapad_toggle(&self, path: &str, enabled: bool) -> std::result::Result<(), String>;
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
pub struct SysfsIdeapadToggleWriter;

impl IdeapadToggleWriter for SysfsIdeapadToggleWriter {
    fn write_ideapad_toggle(&self, path: &str, enabled: bool) -> std::result::Result<(), String> {
        fs::write(path, if enabled { "1" } else { "0" }).map_err(|error| error.to_string())
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
    ideapad_toggle_writer: Arc<dyn IdeapadToggleWriter>,
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
            ideapad_toggle_writer,
        }
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
        plan_fan_preset(&registry.fan_curves, &presets, requested)
            .map_err(PlanningError::Validation)
    }

    pub fn plan_restore_auto_fan_write(&self) -> Result<WriteDryRunPlan, PlanningError> {
        let registry = self.planning_snapshot()?;
        plan_restore_auto_fan(&registry.fan_curves).map_err(PlanningError::Validation)
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

    pub fn set_gpu_mode_pending(&self, requested: &str) -> fdo::Result<GpuModePending> {
        let registry = self.planning_snapshot().map_err(planning_to_fdo)?;
        validate_gpu_mode_choice(registry.gpu.as_ref(), requested).map_err(validation_to_fdo)?;
        let previous_mode = registry.gpu.and_then(|gpu| gpu.mode);
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

    pub fn last_known_good_fan_curve(&self) -> fdo::Result<Option<FanCurveSnapshot>> {
        self.state
            .lock()
            .map(|state| state.last_known_good_fan_curve.clone())
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))
    }

    pub fn capture_last_known_good_fan_curve(&self) -> fdo::Result<FanCurveSnapshot> {
        let registry = self.planning_snapshot().map_err(planning_to_fdo)?;
        let snapshot = capture_fan_curve_snapshot(&registry)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| fdo::Error::Failed("daemon state lock poisoned".to_owned()))?;
        state.last_known_good_fan_curve = Some(snapshot.clone());
        save_state(&self.state_path, &state)
            .map_err(|error| fdo::Error::Failed(format!("failed to save daemon state: {error}")))?;
        Ok(snapshot)
    }

    fn refresh(&self) -> fdo::Result<CapabilityRegistry> {
        let registry = probe(&self.options);
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

    fn GetLastKnownGoodFanCurve(&self) -> fdo::Result<String> {
        to_json(&self.last_known_good_fan_curve()?)
    }

    fn PlanPlatformProfileWrite(&self, requested: &str) -> fdo::Result<String> {
        to_plan_json(self.plan_platform_profile_write(requested))
    }

    fn PlanBatteryChargeTypeWrite(&self, requested: &str) -> fdo::Result<String> {
        to_plan_json(self.plan_battery_charge_type_write(requested))
    }

    fn PlanLedStateWrite(&self, led_id: &str, enabled: bool) -> fdo::Result<String> {
        to_plan_json(self.plan_led_state_write(led_id, enabled))
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

    fn PlanRestoreAutoFanWrite(&self) -> fdo::Result<String> {
        to_plan_json(self.plan_restore_auto_fan_write())
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

fn to_json<T: Serialize>(value: &T) -> fdo::Result<String> {
    serde_json::to_string(value).map_err(|error| fdo::Error::Failed(error.to_string()))
}

fn to_plan_json(result: Result<WriteDryRunPlan, PlanningError>) -> fdo::Result<String> {
    match result {
        Ok(plan) => to_json(&plan),
        Err(error) => Err(planning_to_fdo(error)),
    }
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

fn validation_to_fdo(error: ValidationError) -> fdo::Error {
    fdo::Error::InvalidArgs(serde_json::to_string(&error).unwrap_or_else(|_| format!("{error:?}")))
}

fn sender_from_header(header: &Header<'_>) -> fdo::Result<String> {
    header
        .sender()
        .map(ToString::to_string)
        .ok_or_else(|| fdo::Error::Failed("D-Bus caller sender is missing".to_owned()))
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
        "fn_lock" => "fn_lock writes are disabled by daemon policy".to_owned(),
        _ => "ideapad toggle writes are disabled by daemon policy".to_owned(),
    }
}

fn capture_fan_curve_snapshot(registry: &CapabilityRegistry) -> fdo::Result<FanCurveSnapshot> {
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
                ideapad_toggle_enabled: true,
                camera_power_enabled: true,
            }
            .enabled_methods(),
            [
                "SetPlatformProfile",
                "SetBatteryChargeType",
                "SetLedState",
                "SetIdeapadToggle"
            ]
        );
    }
}
