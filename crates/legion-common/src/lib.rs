use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CapabilityRegistry {
    pub hardware: HardwareSummary,
    pub capabilities: Vec<Capability>,
    pub platform_profile: Option<PlatformProfileCapability>,
    pub battery_charge_type: Option<BatteryChargeTypeCapability>,
    pub hwmon_sensors: Vec<HwmonSensor>,
    pub fan_curves: Vec<FanCurveCapability>,
    pub leds: Vec<LedCapability>,
    pub firmware_attributes: Vec<FirmwareAttributeCapability>,
    pub ideapad_toggles: Vec<IdeapadToggleCapability>,
    pub gpu: Option<GpuCapability>,
    pub telemetry: TelemetrySnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonState {
    pub schema_version: u32,
    #[serde(default)]
    pub gpu_mode_pending: Option<GpuModePending>,
    #[serde(default)]
    pub last_known_good_fan_curve: Option<FanCurveSnapshot>,
}

impl Default for DaemonState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            gpu_mode_pending: None,
            last_known_good_fan_curve: None,
        }
    }
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BatteryTelemetry {
    pub name: String,
    pub path: String,
    pub capacity_percent: Option<i64>,
    pub status: Option<String>,
    pub health: Option<String>,
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
pub struct FirmwareAttributeCapability {
    pub name: String,
    pub current_value: Option<String>,
    pub display_name: Option<String>,
    pub path: String,
}

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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TelemetrySnapshot {
    pub sensors: Vec<HwmonSensor>,
    pub battery: Option<BatteryTelemetry>,
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
        method: "SetIdeapadToggle",
        capability_id: "ideapad_toggles",
        polkit_action: "org.ratvantage.LegionControl1.set-ideapad-toggle",
        request_type: r#"{"toggle_id":"fn_lock|camera_power","enabled":"bool"}"#,
        risk: RiskLevel::ReversibleWrite,
        enabled: false,
        reboot_required: false,
        preconditions: &[
            "ideapad toggle capability is detected",
            "daemon has read current toggle state and sysfs path",
        ],
        validators: &[
            "requested toggle exactly matches one detected ideapad toggle id",
            "only fn_lock and camera_power are enabled for reversible ideapad toggle writes right now",
            "fn_lock requires paired platform::fnlock LED corroboration before and after write",
            "camera_power requires binary current value and explicit UI warning/confirmation before frontend exposure",
            "post-write toggle read-back matches requested state",
        ],
        rollback: &[
            "store previous toggle value before write",
            "restore previous toggle value if toggle or corroborating LED read-back fails",
        ],
        safety_notes: &[
            "write method remains disabled; dry-run planning only",
            "current rollout is restricted to fn_lock and camera_power with per-toggle safety rules",
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

    if toggle.name != "fn_lock" && toggle.name != "camera_power" {
        return Err(ValidationError::BlockedChoice {
            capability_id: "ideapad_toggles".to_owned(),
            requested: toggle_id.to_owned(),
            reason:
                "only fn_lock and camera_power are enabled for reversible ideapad toggle writes right now"
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

    if toggle.name == "camera_power" {
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
    plan_write(
        write_contract("SetGpuMode"),
        "envycontrol",
        capability
            .mode
            .as_deref()
            .expect("validated GPU mode current value must exist"),
        requested,
    )
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
    validate_fan_preset_choice(fan_curves, presets, requested)?;
    let preset = find_fan_preset(presets, requested).expect("validated fan preset must exist");
    let curve = select_fan_curve(fan_curves).expect("validated fan curve capability must exist");
    let mut plan = plan_write(
        write_contract("ApplyFanPreset"),
        curve.path.as_deref().unwrap_or("fan_curves"),
        "current fan curve snapshot",
        &preset.id,
    )?;
    plan.safety_notes.push(preset.safety_note.clone());
    Ok(plan)
}

pub fn plan_restore_auto_fan_write(
    fan_curves: &[FanCurveCapability],
) -> Result<WriteDryRunPlan, ValidationError> {
    let curve = select_fan_curve(fan_curves).ok_or_else(|| ValidationError::MissingCapability {
        capability_id: "fan_curves".to_owned(),
    })?;
    plan_write(
        write_contract("RestoreAutoFan"),
        curve.path.as_deref().unwrap_or("fan_curves"),
        "current fan-control state",
        "auto/default fan control",
    )
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

fn require_current(capability_id: &str, current: Option<&str>) -> Result<(), ValidationError> {
    match current {
        Some(value) if !value.is_empty() => Ok(()),
        _ => Err(ValidationError::MissingCurrentValue {
            capability_id: capability_id.to_owned(),
        }),
    }
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
    fn write_contracts_are_drafted_but_disabled() {
        let methods = WRITE_METHOD_CONTRACTS
            .iter()
            .map(|contract| contract.method)
            .collect::<Vec<_>>();

        assert_eq!(
            methods,
            [
                "SetPlatformProfile",
                "SetBatteryChargeType",
                "SetLedState",
                "SetIdeapadToggle",
                "SetGpuMode",
                "ApplyFanPreset",
                "RestoreAutoFan"
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
    fn gpu_mode_validator_rejects_missing_unsupported_and_non_exact_choices() {
        let capability = GpuCapability {
            provider: "envycontrol".to_owned(),
            status: CapabilityStatus::ProbeOnly,
            mode: Some("hybrid".to_owned()),
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
            .any(|note| note.contains("fn_lock and camera_power")));
        assert!(plan.readback_required);
    }

    #[test]
    fn gpu_mode_dry_run_plan_uses_validator_and_reboot_contract_metadata() {
        let capability = GpuCapability {
            provider: "envycontrol".to_owned(),
            status: CapabilityStatus::ProbeOnly,
            mode: Some("hybrid".to_owned()),
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
