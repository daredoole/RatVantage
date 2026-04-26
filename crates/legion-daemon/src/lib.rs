use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use anyhow::Result;
use legion_common::{
    plan_battery_charge_type_write as plan_battery_charge_type,
    plan_fan_preset_write as plan_fan_preset, plan_gpu_mode_write as plan_gpu_mode,
    plan_platform_profile_write as plan_platform_profile,
    plan_restore_auto_fan_write as plan_restore_auto_fan, validate_gpu_mode_choice,
    CapabilityRegistry, DaemonState, FanPreset, GpuModePending, ValidationError, WriteDryRunPlan,
};
use legion_probe::{probe, ProbeOptions};
use serde::Serialize;
use zbus::{blocking::Connection, blocking::ConnectionBuilder, fdo, interface};

pub const DBUS_INTERFACE: &str = "org.ratvantage.LegionControl1";
pub const DBUS_PATH: &str = "/org/ratvantage/LegionControl1";
pub const READ_ONLY_METHODS: &str = "GetHardwareSummary,GetCapabilities,RefreshCapabilities,GetTelemetry,GetRawProbeReport,GetGpuModePending,PlanPlatformProfileWrite,PlanBatteryChargeTypeWrite,PlanGpuModeWrite,PlanFanPresetWrite,PlanRestoreAutoFanWrite,SetGpuModePending,ClearGpuModePending";
pub const DEFAULT_STATE_PATH: &str = "/var/lib/legion-control/state.toml";

const PACKAGED_FAN_PRESETS: &[&str] = &[
    include_str!("../../../data/presets/quiet-office.toml"),
    include_str!("../../../data/presets/balanced-daily.toml"),
    include_str!("../../../data/presets/gaming.toml"),
    include_str!("../../../data/presets/max-safe.toml"),
];

pub struct LegionControl {
    options: ProbeOptions,
    registry: Mutex<CapabilityRegistry>,
    state_path: PathBuf,
    state: Mutex<DaemonState>,
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
        let state_path = state_path.into();
        let registry = probe(&options);
        let state = load_state(&state_path).unwrap_or_default();

        Self {
            options,
            registry: Mutex::new(registry),
            state_path,
            state: Mutex::new(state),
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

    fn PlanPlatformProfileWrite(&self, requested: &str) -> fdo::Result<String> {
        to_plan_json(self.plan_platform_profile_write(requested))
    }

    fn PlanBatteryChargeTypeWrite(&self, requested: &str) -> fdo::Result<String> {
        to_plan_json(self.plan_battery_charge_type_write(requested))
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

    fn SetGpuModePending(&self, requested: &str) -> fdo::Result<String> {
        to_json(&self.set_gpu_mode_pending(requested)?)
    }

    fn ClearGpuModePending(&self) -> fdo::Result<String> {
        to_json(&self.clear_gpu_mode_pending()?)
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

fn validation_to_fdo(error: ValidationError) -> fdo::Error {
    fdo::Error::InvalidArgs(serde_json::to_string(&error).unwrap_or_else(|_| format!("{error:?}")))
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
