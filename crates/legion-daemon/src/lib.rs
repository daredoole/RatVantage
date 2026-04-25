use std::sync::Mutex;

use anyhow::Result;
use legion_common::{
    plan_battery_charge_type_write as plan_battery_charge_type,
    plan_fan_preset_write as plan_fan_preset, plan_gpu_mode_write as plan_gpu_mode,
    plan_platform_profile_write as plan_platform_profile, CapabilityRegistry, FanPreset,
    ValidationError, WriteDryRunPlan,
};
use legion_probe::{probe, ProbeOptions};
use serde::Serialize;
use zbus::{blocking::Connection, blocking::ConnectionBuilder, fdo, interface};

pub const DBUS_INTERFACE: &str = "org.ratvantage.LegionControl1";
pub const DBUS_PATH: &str = "/org/ratvantage/LegionControl1";
pub const READ_ONLY_METHODS: &str = "GetHardwareSummary,GetCapabilities,RefreshCapabilities,GetTelemetry,GetRawProbeReport,PlanPlatformProfileWrite,PlanBatteryChargeTypeWrite,PlanGpuModeWrite,PlanFanPresetWrite";

const PACKAGED_FAN_PRESETS: &[&str] = &[
    include_str!("../../../data/presets/quiet-office.toml"),
    include_str!("../../../data/presets/balanced-daily.toml"),
    include_str!("../../../data/presets/gaming.toml"),
    include_str!("../../../data/presets/max-safe.toml"),
];

pub struct LegionControl {
    options: ProbeOptions,
    registry: Mutex<CapabilityRegistry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanningError {
    RegistryUnavailable,
    PresetUnavailable(String),
    Validation(ValidationError),
}

impl LegionControl {
    pub fn new(options: ProbeOptions) -> Self {
        let registry = probe(&options);

        Self {
            options,
            registry: Mutex::new(registry),
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
        Err(PlanningError::RegistryUnavailable) => Err(fdo::Error::Failed(
            "capability registry unavailable".to_owned(),
        )),
        Err(PlanningError::PresetUnavailable(error)) => Err(fdo::Error::Failed(error)),
        Err(PlanningError::Validation(error)) => Err(fdo::Error::InvalidArgs(
            serde_json::to_string(&error).unwrap_or_else(|_| format!("{error:?}")),
        )),
    }
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
