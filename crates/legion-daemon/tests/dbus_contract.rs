use std::{collections::BTreeMap, env, fs, os::unix::fs::PermissionsExt, sync::Arc};

use legion_common::{
    AutomationRuleKind, Capability, CapabilityRegistry, CurveOptimizerWriteState, FanCurveSnapshot,
    GpuModePending, HardwareSummary, TelemetrySnapshot, WriteDryRunPlan, WriteExecutionResult,
    WriteExecutionStatus,
};
use legion_control_daemon::{
    BatteryChargeTypeWriter, CpuEppWriter, CpuGovernorWriter, CurveOptimizerAllCoreWriter,
    CurveOptimizerCommandOutput, IdeapadToggleWriter, LedStateWriter, LegionControl,
    PlatformProfileWriter, WriteAccessPolicy, WriteAuthorizer, DBUS_INTERFACE, DBUS_PATH,
};
use legion_probe::ProbeOptions;
use ratvantage_test_support::{
    call_json, copied_fixture_root, fixture_root, introspected_methods, PrivateBus,
};
use zbus::blocking::{ConnectionBuilder, Proxy};

static ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn read_only_methods_return_expected_json_contracts() {
    let (_bus, _service_connection, proxy) = test_proxy();

    let hardware: HardwareSummary = call_json(&proxy, "GetHardwareSummary");
    assert_eq!(hardware.vendor.as_deref(), Some("LENOVO"));
    assert_eq!(hardware.product_name.as_deref(), Some("82WM"));
    assert_eq!(
        hardware.product_version.as_deref(),
        Some("Legion Pro 5 16ARX8")
    );

    let capabilities: Vec<Capability> = call_json(&proxy, "GetCapabilities");
    assert!(capabilities
        .iter()
        .any(|capability| capability.id == "platform_profile"));
    assert!(capabilities
        .iter()
        .any(|capability| capability.id == "leds"));

    let telemetry: TelemetrySnapshot = call_json(&proxy, "GetTelemetry");
    assert!(telemetry
        .sensors
        .iter()
        .any(|sensor| sensor.label.as_deref() == Some("CPU Fan")));
    assert_eq!(
        telemetry
            .battery
            .as_ref()
            .and_then(|battery| battery.capacity_percent),
        Some(79)
    );
    assert_eq!(
        telemetry
            .battery
            .as_ref()
            .and_then(|battery| battery.status.as_deref()),
        Some("Charging")
    );
    assert_eq!(
        telemetry
            .battery
            .as_ref()
            .and_then(|battery| battery.health.as_deref()),
        Some("Good")
    );

    let raw: CapabilityRegistry = call_json(&proxy, "GetRawProbeReport");
    assert_eq!(raw.hardware, hardware);
    assert_eq!(raw.capabilities, capabilities);
    assert_eq!(raw.telemetry, telemetry);
    assert!(raw
        .fan_curves
        .iter()
        .any(|curve| curve.id == "legion-hwmon"));

    let refreshed: Vec<Capability> = call_json(&proxy, "RefreshCapabilities");
    assert_eq!(refreshed, capabilities);

    let live: FanCurveSnapshot = call_json(&proxy, "GetLiveFanCurveReadings");
    assert_eq!(live.curve_id, "legion-hwmon");
    assert!(
        live.points
            .iter()
            .any(|point| point.path.contains("pwm1_auto")),
        "expected pwm auto point paths in live readings: {:?}",
        live.points
    );

    let fan_map: BTreeMap<String, String> = call_json(&proxy, "GetFanPresetProfileMap");
    assert!(fan_map.is_empty());

    let reapply: bool = call_json(&proxy, "GetFanPresetReapplyAfterResume");
    assert!(!reapply);

    let payload: String = proxy
        .call("PlanPlatformProfileWrite", &("performance",))
        .unwrap();
    let platform_plan: WriteDryRunPlan = serde_json::from_str(&payload).unwrap();
    assert_eq!(platform_plan.method, "SetPlatformProfile");
    assert_eq!(platform_plan.requested_value, "performance");
    assert_eq!(platform_plan.previous_value, "balanced");

    let payload: String = proxy
        .call("PlanBatteryChargeTypeWrite", &("Conservation",))
        .unwrap();
    let battery_plan: WriteDryRunPlan = serde_json::from_str(&payload).unwrap();
    assert_eq!(battery_plan.method, "SetBatteryChargeType");
    assert_eq!(battery_plan.requested_value, "Conservation");
    assert_eq!(battery_plan.previous_value, "Standard");

    let payload: String = proxy
        .call("PlanLedStateWrite", &("platform::ylogo", false))
        .unwrap();
    let led_plan: WriteDryRunPlan = serde_json::from_str(&payload).unwrap();
    assert_eq!(led_plan.method, "SetLedState");
    assert_eq!(led_plan.requested_value, "0");
    assert_eq!(led_plan.previous_value, "1");

    let payload: String = proxy
        .call("PlanIdeapadToggleWrite", &("fn_lock", true))
        .unwrap();
    let toggle_plan: WriteDryRunPlan = serde_json::from_str(&payload).unwrap();
    assert_eq!(toggle_plan.method, "SetIdeapadToggle");
    assert_eq!(toggle_plan.requested_value, "1");
    assert_eq!(toggle_plan.previous_value, "0");

    let payload: String = proxy
        .call("PlanIdeapadToggleWrite", &("camera_power", false))
        .unwrap();
    let toggle_plan: WriteDryRunPlan = serde_json::from_str(&payload).unwrap();
    assert_eq!(toggle_plan.method, "SetIdeapadToggle");
    assert_eq!(toggle_plan.requested_value, "0");
    assert_eq!(toggle_plan.previous_value, "1");

    let payload: String = proxy
        .call("PlanFirmwareAttributeWrite", &("ppt_pl1_spl", "75"))
        .unwrap();
    let firmware_plan: WriteDryRunPlan = serde_json::from_str(&payload).unwrap();
    assert_eq!(firmware_plan.method, "SetFirmwareAttribute");
    assert_eq!(firmware_plan.requested_value, "75");
    assert_eq!(firmware_plan.previous_value, "70");

    let payload: String = proxy.call("SetPlatformProfile", &("performance",)).unwrap();
    let execution: WriteExecutionResult = serde_json::from_str(&payload).unwrap();
    assert_eq!(execution.status, WriteExecutionStatus::BlockedByPolicy);
    assert!(!execution.applied);
    assert_eq!(execution.plan.method, "SetPlatformProfile");
    assert_eq!(execution.plan.requested_value, "performance");

    let payload: String = proxy
        .call("SetBatteryChargeType", &("Conservation",))
        .unwrap();
    let execution: WriteExecutionResult = serde_json::from_str(&payload).unwrap();
    assert_eq!(execution.status, WriteExecutionStatus::BlockedByPolicy);
    assert!(!execution.applied);
    assert_eq!(execution.plan.method, "SetBatteryChargeType");
    assert_eq!(execution.plan.requested_value, "Conservation");

    let payload: String = proxy
        .call("SetLedState", &("platform::ylogo", false))
        .unwrap();
    let execution: WriteExecutionResult = serde_json::from_str(&payload).unwrap();
    assert_eq!(execution.status, WriteExecutionStatus::BlockedByPolicy);
    assert!(!execution.applied);
    assert_eq!(execution.plan.method, "SetLedState");
    assert_eq!(execution.plan.requested_value, "0");

    let payload: String = proxy.call("SetIdeapadToggle", &("fn_lock", true)).unwrap();
    let execution: WriteExecutionResult = serde_json::from_str(&payload).unwrap();
    assert_eq!(execution.status, WriteExecutionStatus::BlockedByPolicy);
    assert!(!execution.applied);
    assert_eq!(execution.plan.method, "SetIdeapadToggle");
    assert_eq!(execution.plan.requested_value, "1");

    let payload: String = proxy
        .call("SetFirmwareAttribute", &("ppt_pl1_spl", "75"))
        .unwrap();
    let execution: WriteExecutionResult = serde_json::from_str(&payload).unwrap();
    assert_eq!(execution.status, WriteExecutionStatus::BlockedByPolicy);
    assert!(!execution.applied);
    assert_eq!(execution.plan.method, "SetFirmwareAttribute");
    assert_eq!(execution.plan.requested_value, "75");
}

#[test]
fn introspection_exposes_gated_reversible_write_methods_only() {
    let (_bus, _service_connection, proxy) = test_proxy();
    let xml = proxy.introspect().unwrap();
    let mut methods = introspected_methods(&xml, DBUS_INTERFACE);
    methods.sort_unstable();

    assert_eq!(
        methods,
        [
            "ApplyAutomationRule",
            "ApplyHardwareProfile",
            "ApplyHardwareProfileTrigger",
            "CaptureLastKnownGoodFanCurve",
            "ClearAutomationRules",
            "ClearFanPresetProfileMap",
            "ClearGpuModePending",
            "ClearHardwareProfileTriggers",
            "ClearHardwareProfiles",
            "GetAutomationRulePreview",
            "GetAutomationRules",
            "GetCapabilities",
            "GetFanPresetProfileMap",
            "GetFanPresetReapplyAfterResume",
            "GetGpuModePending",
            "GetHardwareProfileApplyPreview",
            "GetHardwareProfileTriggerApplyPreview",
            "GetHardwareProfileTriggers",
            "GetHardwareProfiles",
            "GetHardwareSummary",
            "GetLastAutomationRuleApply",
            "GetLastCurveOptimizerAllCore",
            "GetLastHardwareProfileApply",
            "GetLastKnownGoodFanCurve",
            "GetLiveFanCurveReadings",
            "GetRawProbeReport",
            "GetTelemetry",
            "PlanAmdGpuDpmForceLevelWrite",
            "PlanBatteryChargeTypeWrite",
            "PlanConservationModeWrite",
            "PlanCpuBoostWrite",
            "PlanCpuEppWrite",
            "PlanCpuGovernorWrite",
            "PlanCurveOptimizerAllCoreWrite",
            "PlanFanPresetWrite",
            "PlanFirmwareAttributeWrite",
            "PlanGpuModeWrite",
            "PlanIdeapadToggleWrite",
            "PlanLedStateWrite",
            "PlanPlatformProfileWrite",
            "PlanRestoreAutoFanWrite",
            "RefreshCapabilities",
            "RemoveAutomationRule",
            "RemoveFanPresetProfileMapEntry",
            "RemoveHardwareProfile",
            "RemoveHardwareProfileTrigger",
            "SetAmdGpuDpmForceLevel",
            "SetAutomationRule",
            "SetBatteryChargeType",
            "SetConservationMode",
            "SetCpuBoost",
            "SetCpuEpp",
            "SetCpuGovernor",
            "SetCurveOptimizerAllCore",
            "SetFanPresetProfileMapEntry",
            "SetFanPresetReapplyAfterResume",
            "SetFirmwareAttribute",
            "SetGpuMode",
            "SetGpuModePending",
            "SetHardwareProfile",
            "SetHardwareProfileTrigger",
            "SetIdeapadToggle",
            "SetLedState",
            "SetPlatformProfile",
        ]
    );
    assert!(!methods
        .iter()
        .any(|method| matches!(method.as_str(), "ApplyFanPreset" | "RestoreAutoFan")));
}

#[test]
fn daemon_builds_dry_run_plans_without_other_dbus_write_methods() {
    let service = LegionControl::new(ProbeOptions {
        sysfs_root: fixture_root(),
    });

    let platform_plan = service.plan_platform_profile_write("performance").unwrap();
    assert_eq!(platform_plan.method, "SetPlatformProfile");
    assert_eq!(platform_plan.capability_id, "platform_profile");
    assert_eq!(platform_plan.previous_value, "balanced");
    assert_eq!(platform_plan.requested_value, "performance");
    assert_eq!(platform_plan.rollback_value, "balanced");
    assert!(platform_plan.readback_required);

    let battery_plan = service
        .plan_battery_charge_type_write("Conservation")
        .unwrap();
    assert_eq!(battery_plan.method, "SetBatteryChargeType");
    assert_eq!(battery_plan.capability_id, "battery_charge_type");
    assert_eq!(battery_plan.previous_value, "Standard");
    assert_eq!(battery_plan.requested_value, "Conservation");
    assert_eq!(battery_plan.rollback_value, "Standard");
    assert!(battery_plan.readback_required);

    let led_plan = service
        .plan_led_state_write("platform::ylogo", false)
        .unwrap();
    assert_eq!(led_plan.method, "SetLedState");
    assert_eq!(led_plan.capability_id, "leds");
    assert_eq!(led_plan.previous_value, "1");
    assert_eq!(led_plan.requested_value, "0");
    assert_eq!(led_plan.rollback_value, "1");
    assert!(led_plan.readback_required);

    let toggle_plan = service.plan_ideapad_toggle_write("fn_lock", true).unwrap();
    assert_eq!(toggle_plan.method, "SetIdeapadToggle");
    assert_eq!(toggle_plan.capability_id, "ideapad_toggles");
    assert_eq!(toggle_plan.previous_value, "0");
    assert_eq!(toggle_plan.requested_value, "1");
    assert_eq!(toggle_plan.rollback_value, "0");
    assert!(toggle_plan.readback_required);

    let camera_plan = service
        .plan_ideapad_toggle_write("camera_power", false)
        .unwrap();
    assert_eq!(camera_plan.previous_value, "1");
    assert_eq!(camera_plan.requested_value, "0");

    let firmware_plan = service
        .plan_firmware_attribute_write("ppt_pl2_sppt", "90")
        .unwrap();
    assert_eq!(firmware_plan.method, "SetFirmwareAttribute");
    assert_eq!(firmware_plan.capability_id, "firmware_attributes");
    assert_eq!(firmware_plan.previous_value, "85");
    assert_eq!(firmware_plan.requested_value, "90");
    assert_eq!(firmware_plan.rollback_value, "85");
    assert!(firmware_plan.readback_required);

    let boost_plan = service.plan_cpu_boost_write("0").unwrap();
    assert_eq!(boost_plan.method, "SetCpuBoost");
    assert_eq!(boost_plan.capability_id, "cpu_power");
    assert_eq!(boost_plan.previous_value, "1");
    assert_eq!(boost_plan.requested_value, "0");

    let conservation_plan = service.plan_conservation_mode_write("0").unwrap();
    assert_eq!(conservation_plan.method, "SetConservationMode");
    assert_eq!(conservation_plan.capability_id, "ideapad_toggles");
    assert_eq!(conservation_plan.previous_value, "1");
    assert_eq!(conservation_plan.requested_value, "0");

    let dpm_plan = service.plan_amd_gpu_dpm_force_level_write("low").unwrap();
    assert_eq!(dpm_plan.method, "SetAmdGpuDpmForceLevel");
    assert_eq!(dpm_plan.capability_id, "amd_gpu_power_dpm");
    assert_eq!(dpm_plan.previous_value, "auto");
    assert_eq!(dpm_plan.requested_value, "low");

    let co_plan = service.plan_curve_optimizer_all_core_write("-20").unwrap();
    assert_eq!(co_plan.method, "SetCurveOptimizerAllCore");
    assert_eq!(co_plan.capability_id, "curve_optimizer_all_core");
    assert_eq!(co_plan.requested_value, "-20 (encoded 4294967276)");
    assert!(!co_plan.readback_required);

    assert!(service.plan_platform_profile_write("custom").is_err());
    assert!(service.plan_battery_charge_type_write("Invalid").is_err());
    assert!(service
        .plan_led_state_write("platform::fnlock", true)
        .is_err());
    assert!(service
        .plan_ideapad_toggle_write("touchpad", false)
        .is_err());
    assert!(service
        .plan_ideapad_toggle_write("conservation_mode", false)
        .is_err());
    let fan_mode_plan = service.plan_ideapad_toggle_write("fan_mode", true).unwrap();
    assert_eq!(fan_mode_plan.method, "SetIdeapadToggle");
    assert_eq!(fan_mode_plan.previous_value, "0");
    assert_eq!(fan_mode_plan.requested_value, "1");
    assert!(service.plan_gpu_mode_write("hybrid").is_err());
    assert!(service
        .plan_firmware_attribute_write("ppt_pl2_sppt", "999")
        .is_err());
    assert!(service.plan_cpu_boost_write("yes").is_err());
    assert!(service.plan_conservation_mode_write("yes").is_err());
    assert!(service
        .plan_amd_gpu_dpm_force_level_write("manual")
        .is_err());
    assert!(service.plan_curve_optimizer_all_core_write("-40").is_err());

    let fan_plan = service.plan_fan_preset_write("balanced-daily").unwrap();
    assert_eq!(fan_plan.method, "ApplyFanPreset");
    assert_eq!(fan_plan.capability_id, "fan_curves");
    assert_eq!(fan_plan.requested_value, "balanced-daily");
    assert_eq!(fan_plan.previous_value, "current fan curve snapshot");
    assert!(fan_plan.readback_required);

    let restore_plan = service.plan_restore_auto_fan_write().unwrap();
    assert_eq!(restore_plan.method, "RestoreAutoFan");
    assert_eq!(restore_plan.capability_id, "fan_curves");
    assert_eq!(restore_plan.requested_value, "auto/default fan control");
}

#[test]
fn set_gpu_mode_executes_envycontrol_and_records_pending_state() {
    let _guard = ENV_MUTEX.lock().unwrap();
    let state_path = unique_state_path("gpu-mode-exec");
    let fake_dir = unique_temp_dir("fake-envycontrol");
    let fake_bin = fake_dir.join("envycontrol");
    let log_path = fake_dir.join("envycontrol.log");
    fs::create_dir_all(&fake_dir).unwrap();
    fs::write(
        &fake_bin,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nif [ \"$1\" = \"--query\" ]; then echo 'Current GPU mode: integrated'; exit 0; fi\nif [ \"$1\" = \"-s\" ]; then echo \"switched $2\"; exit 0; fi\necho unexpected args >&2\nexit 2\n",
            log_path.display()
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_bin).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_bin, permissions).unwrap();

    let old_path = env::var_os("PATH");
    let new_path = match &old_path {
        Some(path) => format!("{}:{}", fake_dir.display(), path.to_string_lossy()),
        None => fake_dir.display().to_string(),
    };
    env::set_var("PATH", new_path);

    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: "/".into(),
        },
        &state_path,
        WriteAccessPolicy {
            gpu_mode_enabled: true,
            ..Default::default()
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
        Arc::new(NoOpCpuGovernorWriter),
        Arc::new(NoOpCpuEppWriter),
    );
    let (_bus, _service_connection, proxy) = test_proxy_with_service(service);
    let payload: String = proxy.call("SetGpuMode", &("hybrid",)).unwrap();
    let result: WriteExecutionResult = serde_json::from_str(&payload).unwrap();

    if let Some(path) = old_path {
        env::set_var("PATH", path);
    } else {
        env::remove_var("PATH");
    }

    assert_eq!(result.status, WriteExecutionStatus::Applied);
    assert!(result.applied);
    assert!(result.message.contains("reboot is required"));
    assert_eq!(result.plan.method, "SetGpuMode");
    assert_eq!(result.plan.previous_value, "integrated");
    assert_eq!(result.plan.requested_value, "hybrid");
    assert_eq!(
        result.readback_value.as_deref(),
        Some("hybrid pending (was integrated); reboot required")
    );
    let state = fs::read_to_string(&state_path).unwrap();
    assert!(state.contains("requested_mode = \"hybrid\""));
    assert!(state.contains("previous_mode = \"integrated\""));
    let log = fs::read_to_string(&log_path).unwrap();
    assert!(log.lines().any(|line| line == "--query"));
    assert!(log.lines().any(|line| line == "-s hybrid"));

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fake_dir);
}

#[test]
fn set_curve_optimizer_executes_fake_backend_and_records_write_only_state() {
    let state_path = unique_state_path("curve-optimizer");
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture_root(),
        },
        &state_path,
        WriteAccessPolicy {
            curve_optimizer_enabled: true,
            hardware_profile_apply_enabled: false,
            ..Default::default()
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
        Arc::new(NoOpCpuGovernorWriter),
        Arc::new(NoOpCpuEppWriter),
    )
    .with_curve_optimizer_writer(Arc::new(RecordingCurveOptimizerWriter {
        calls: calls.clone(),
    }));
    let (_bus, _service_connection, proxy) = test_proxy_with_service(service);
    let payload: String = proxy.call("SetCurveOptimizerAllCore", &("-20",)).unwrap();
    let result: WriteExecutionResult = serde_json::from_str(&payload).unwrap();

    assert_eq!(result.status, WriteExecutionStatus::Applied);
    assert!(result.applied);
    assert_eq!(result.plan.method, "SetCurveOptimizerAllCore");
    assert_eq!(result.plan.requested_value, "-20 (encoded 4294967276)");
    assert_eq!(calls.lock().unwrap().as_slice(), &[4_294_967_276]);
    assert_eq!(
        result.readback_value.as_deref(),
        Some("offset=-20 encoded=4294967276 readback=write_only")
    );

    let payload: String = proxy.call("GetLastCurveOptimizerAllCore", &()).unwrap();
    let state: Option<CurveOptimizerWriteState> = serde_json::from_str(&payload).unwrap();
    let state = state.unwrap();
    assert_eq!(state.signed_offset, -20);
    assert_eq!(state.encoded_value, 4_294_967_276);
    assert_eq!(state.backend, "ryzenadj");

    let _ = fs::remove_file(state_path);
}

#[test]
fn set_curve_optimizer_requires_ryzenadj_success_marker() {
    let state_path = unique_state_path("curve-optimizer-missing-marker");
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture_root(),
        },
        &state_path,
        WriteAccessPolicy {
            curve_optimizer_enabled: true,
            hardware_profile_apply_enabled: false,
            ..Default::default()
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
        Arc::new(NoOpCpuGovernorWriter),
        Arc::new(NoOpCpuEppWriter),
    )
    .with_curve_optimizer_writer(Arc::new(MissingMarkerCurveOptimizerWriter));
    let (_bus, _service_connection, proxy) = test_proxy_with_service(service);
    let payload: String = proxy.call("SetCurveOptimizerAllCore", &("-20",)).unwrap();
    let result: WriteExecutionResult = serde_json::from_str(&payload).unwrap();

    assert_eq!(result.status, WriteExecutionStatus::Failed);
    assert!(!result.applied);
    assert_eq!(result.plan.method, "SetCurveOptimizerAllCore");
    assert!(result
        .message
        .contains("did not report `Successfully set coall`"));
    assert_eq!(
        result.readback_value.as_deref(),
        Some("stdout: Curve Optimizer command completed; stderr: ")
    );

    let payload: String = proxy.call("GetLastCurveOptimizerAllCore", &()).unwrap();
    let state: Option<CurveOptimizerWriteState> = serde_json::from_str(&payload).unwrap();
    assert_eq!(state, None);

    let _ = fs::remove_file(state_path);
}

#[test]
fn platform_profile_write_applies_when_policy_and_authorizer_allow_it() {
    let fixture = copied_fixture_root("platform-profile-write-success");
    let state_path = unique_state_path("platform-profile-write-success");
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
        WriteAccessPolicy {
            platform_profile_enabled: true,
            battery_charge_type_enabled: false,
            led_state_enabled: false,
            ideapad_toggle_enabled: false,
            camera_power_enabled: false,
            usb_charging_enabled: false,
            fan_mode_enabled: false,
            gpu_mode_enabled: false,
            cpu_governor_enabled: false,
            cpu_epp_enabled: false,
            firmware_attribute_enabled: false,
            cpu_boost_enabled: false,
            conservation_mode_enabled: false,
            amd_gpu_dpm_enabled: false,
            curve_optimizer_enabled: false,
            hardware_profile_apply_enabled: false,
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
        Arc::new(NoOpCpuGovernorWriter),
        Arc::new(NoOpCpuEppWriter),
    );

    let result = service
        .set_platform_profile("performance", ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::Applied);
    assert!(result.applied);
    assert_eq!(result.readback_value.as_deref(), Some("performance"));
    assert_eq!(
        fs::read_to_string(fixture.join("sys/firmware/acpi/platform_profile"))
            .unwrap()
            .trim(),
        "performance"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn platform_profile_write_rejects_invalid_choice_before_write() {
    let fixture = copied_fixture_root("platform-profile-write-invalid");
    let state_path = unique_state_path("platform-profile-write-invalid");
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
        WriteAccessPolicy {
            platform_profile_enabled: true,
            battery_charge_type_enabled: false,
            led_state_enabled: false,
            ideapad_toggle_enabled: false,
            camera_power_enabled: false,
            usb_charging_enabled: false,
            fan_mode_enabled: false,
            gpu_mode_enabled: false,
            cpu_governor_enabled: false,
            cpu_epp_enabled: false,
            firmware_attribute_enabled: false,
            cpu_boost_enabled: false,
            conservation_mode_enabled: false,
            amd_gpu_dpm_enabled: false,
            curve_optimizer_enabled: false,
            hardware_profile_apply_enabled: false,
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
        Arc::new(NoOpCpuGovernorWriter),
        Arc::new(NoOpCpuEppWriter),
    );

    assert!(service.set_platform_profile("custom", ":1.99").is_err());
    assert_eq!(
        fs::read_to_string(fixture.join("sys/firmware/acpi/platform_profile"))
            .unwrap()
            .trim(),
        "balanced"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn platform_profile_write_reports_write_failure_without_changing_value() {
    let fixture = copied_fixture_root("platform-profile-write-failure");
    let state_path = unique_state_path("platform-profile-write-failure");
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
        WriteAccessPolicy {
            platform_profile_enabled: true,
            battery_charge_type_enabled: false,
            led_state_enabled: false,
            ideapad_toggle_enabled: false,
            camera_power_enabled: false,
            usb_charging_enabled: false,
            fan_mode_enabled: false,
            gpu_mode_enabled: false,
            cpu_governor_enabled: false,
            cpu_epp_enabled: false,
            firmware_attribute_enabled: false,
            cpu_boost_enabled: false,
            conservation_mode_enabled: false,
            amd_gpu_dpm_enabled: false,
            curve_optimizer_enabled: false,
            hardware_profile_apply_enabled: false,
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(FailingPlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
        Arc::new(NoOpCpuGovernorWriter),
        Arc::new(NoOpCpuEppWriter),
    );

    let result = service
        .set_platform_profile("performance", ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::Failed);
    assert!(!result.applied);
    assert!(result.message.contains("failed to write platform profile"));
    assert_eq!(
        fs::read_to_string(fixture.join("sys/firmware/acpi/platform_profile"))
            .unwrap()
            .trim(),
        "balanced"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn platform_profile_write_rolls_back_after_readback_mismatch() {
    let fixture = copied_fixture_root("platform-profile-write-rollback");
    let state_path = unique_state_path("platform-profile-write-rollback");
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
        WriteAccessPolicy {
            platform_profile_enabled: true,
            battery_charge_type_enabled: false,
            led_state_enabled: false,
            ideapad_toggle_enabled: false,
            camera_power_enabled: false,
            usb_charging_enabled: false,
            fan_mode_enabled: false,
            gpu_mode_enabled: false,
            cpu_governor_enabled: false,
            cpu_epp_enabled: false,
            firmware_attribute_enabled: false,
            cpu_boost_enabled: false,
            conservation_mode_enabled: false,
            amd_gpu_dpm_enabled: false,
            curve_optimizer_enabled: false,
            hardware_profile_apply_enabled: false,
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(MismatchingPlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
        Arc::new(NoOpCpuGovernorWriter),
        Arc::new(NoOpCpuEppWriter),
    );

    let result = service
        .set_platform_profile("performance", ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::Failed);
    assert!(!result.applied);
    assert!(result
        .message
        .contains("restored previous value `balanced`"));
    assert_eq!(result.readback_value.as_deref(), Some("balanced"));
    assert_eq!(
        fs::read_to_string(fixture.join("sys/firmware/acpi/platform_profile"))
            .unwrap()
            .trim(),
        "balanced"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn battery_charge_type_write_applies_when_policy_and_authorizer_allow_it() {
    let fixture = copied_fixture_root("battery-charge-type-write-success");
    let state_path = unique_state_path("battery-charge-type-write-success");
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
        WriteAccessPolicy {
            platform_profile_enabled: false,
            battery_charge_type_enabled: true,
            led_state_enabled: false,
            ideapad_toggle_enabled: false,
            camera_power_enabled: false,
            usb_charging_enabled: false,
            fan_mode_enabled: false,
            gpu_mode_enabled: false,
            cpu_governor_enabled: false,
            cpu_epp_enabled: false,
            firmware_attribute_enabled: false,
            cpu_boost_enabled: false,
            conservation_mode_enabled: false,
            amd_gpu_dpm_enabled: false,
            curve_optimizer_enabled: false,
            hardware_profile_apply_enabled: false,
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
        Arc::new(NoOpCpuGovernorWriter),
        Arc::new(NoOpCpuEppWriter),
    );

    let result = service
        .set_battery_charge_type("Conservation", ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::Applied);
    assert!(result.applied);
    assert_eq!(result.readback_value.as_deref(), Some("Conservation"));
    assert_eq!(
        fs::read_to_string(fixture.join("sys/class/power_supply/BAT0/charge_type"))
            .unwrap()
            .trim(),
        "Conservation"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn battery_charge_type_write_rolls_back_after_readback_mismatch() {
    let fixture = copied_fixture_root("battery-charge-type-write-rollback");
    let state_path = unique_state_path("battery-charge-type-write-rollback");
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
        WriteAccessPolicy {
            platform_profile_enabled: false,
            battery_charge_type_enabled: true,
            led_state_enabled: false,
            ideapad_toggle_enabled: false,
            camera_power_enabled: false,
            usb_charging_enabled: false,
            fan_mode_enabled: false,
            gpu_mode_enabled: false,
            cpu_governor_enabled: false,
            cpu_epp_enabled: false,
            firmware_attribute_enabled: false,
            cpu_boost_enabled: false,
            conservation_mode_enabled: false,
            amd_gpu_dpm_enabled: false,
            curve_optimizer_enabled: false,
            hardware_profile_apply_enabled: false,
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(MismatchingBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
        Arc::new(NoOpCpuGovernorWriter),
        Arc::new(NoOpCpuEppWriter),
    );

    let result = service
        .set_battery_charge_type("Conservation", ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::Failed);
    assert!(!result.applied);
    assert!(result
        .message
        .contains("restored previous value `Standard`"));
    assert_eq!(result.readback_value.as_deref(), Some("Standard"));
    assert_eq!(
        fs::read_to_string(fixture.join("sys/class/power_supply/BAT0/charge_type"))
            .unwrap()
            .trim(),
        "Standard"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn firmware_attribute_write_applies_when_policy_and_authorizer_allow_it() {
    let fixture = copied_fixture_root("firmware-attribute-write-success");
    let state_path = unique_state_path("firmware-attribute-write-success");
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
        WriteAccessPolicy {
            platform_profile_enabled: false,
            battery_charge_type_enabled: false,
            led_state_enabled: false,
            ideapad_toggle_enabled: false,
            camera_power_enabled: false,
            usb_charging_enabled: false,
            fan_mode_enabled: false,
            gpu_mode_enabled: false,
            cpu_governor_enabled: false,
            cpu_epp_enabled: false,
            firmware_attribute_enabled: true,
            cpu_boost_enabled: false,
            conservation_mode_enabled: false,
            amd_gpu_dpm_enabled: false,
            curve_optimizer_enabled: false,
            hardware_profile_apply_enabled: false,
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
        Arc::new(NoOpCpuGovernorWriter),
        Arc::new(NoOpCpuEppWriter),
    );

    let result = service
        .set_firmware_attribute("ppt_pl1_spl", "75", ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::Applied);
    assert!(result.applied);
    assert_eq!(result.readback_value.as_deref(), Some("75"));
    assert_eq!(
        fs::read_to_string(
            fixture.join(
                "sys/class/firmware-attributes/thinklmi/attributes/ppt_pl1_spl/current_value"
            )
        )
        .unwrap()
        .trim(),
        "75"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn firmware_attribute_write_rejects_out_of_range_before_write() {
    let fixture = copied_fixture_root("firmware-attribute-write-invalid");
    let state_path = unique_state_path("firmware-attribute-write-invalid");
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
        WriteAccessPolicy {
            platform_profile_enabled: false,
            battery_charge_type_enabled: false,
            led_state_enabled: false,
            ideapad_toggle_enabled: false,
            camera_power_enabled: false,
            usb_charging_enabled: false,
            fan_mode_enabled: false,
            gpu_mode_enabled: false,
            cpu_governor_enabled: false,
            cpu_epp_enabled: false,
            firmware_attribute_enabled: true,
            cpu_boost_enabled: false,
            conservation_mode_enabled: false,
            amd_gpu_dpm_enabled: false,
            curve_optimizer_enabled: false,
            hardware_profile_apply_enabled: false,
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
        Arc::new(NoOpCpuGovernorWriter),
        Arc::new(NoOpCpuEppWriter),
    );

    assert!(service
        .set_firmware_attribute("ppt_pl1_spl", "100", ":1.99")
        .is_err());
    assert_eq!(
        fs::read_to_string(
            fixture.join(
                "sys/class/firmware-attributes/thinklmi/attributes/ppt_pl1_spl/current_value"
            )
        )
        .unwrap()
        .trim(),
        "70"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn led_state_write_applies_when_policy_and_authorizer_allow_it() {
    let fixture = copied_fixture_root("led-state-write-success");
    let state_path = unique_state_path("led-state-write-success");
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
        WriteAccessPolicy {
            platform_profile_enabled: false,
            battery_charge_type_enabled: false,
            led_state_enabled: true,
            ideapad_toggle_enabled: false,
            camera_power_enabled: false,
            usb_charging_enabled: false,
            fan_mode_enabled: false,
            gpu_mode_enabled: false,
            cpu_governor_enabled: false,
            cpu_epp_enabled: false,
            firmware_attribute_enabled: false,
            cpu_boost_enabled: false,
            conservation_mode_enabled: false,
            amd_gpu_dpm_enabled: false,
            curve_optimizer_enabled: false,
            hardware_profile_apply_enabled: false,
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
        Arc::new(NoOpCpuGovernorWriter),
        Arc::new(NoOpCpuEppWriter),
    );

    let result = service
        .set_led_state("platform::ylogo", false, ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::Applied);
    assert!(result.applied);
    assert_eq!(result.readback_value.as_deref(), Some("0"));
    assert_eq!(
        fs::read_to_string(fixture.join("sys/class/leds/platform::ylogo/brightness"))
            .unwrap()
            .trim(),
        "0"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn led_state_write_rolls_back_after_readback_mismatch() {
    let fixture = copied_fixture_root("led-state-write-rollback");
    let state_path = unique_state_path("led-state-write-rollback");
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
        WriteAccessPolicy {
            platform_profile_enabled: false,
            battery_charge_type_enabled: false,
            led_state_enabled: true,
            ideapad_toggle_enabled: false,
            camera_power_enabled: false,
            usb_charging_enabled: false,
            fan_mode_enabled: false,
            gpu_mode_enabled: false,
            cpu_governor_enabled: false,
            cpu_epp_enabled: false,
            firmware_attribute_enabled: false,
            cpu_boost_enabled: false,
            conservation_mode_enabled: false,
            amd_gpu_dpm_enabled: false,
            curve_optimizer_enabled: false,
            hardware_profile_apply_enabled: false,
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(MismatchingLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
        Arc::new(NoOpCpuGovernorWriter),
        Arc::new(NoOpCpuEppWriter),
    );

    let result = service
        .set_led_state("platform::ylogo", false, ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::Failed);
    assert!(!result.applied);
    assert!(result.message.contains("restored previous value `1`"));
    assert_eq!(result.readback_value.as_deref(), Some("1"));
    assert_eq!(
        fs::read_to_string(fixture.join("sys/class/leds/platform::ylogo/brightness"))
            .unwrap()
            .trim(),
        "1"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn ideapad_toggle_write_applies_when_policy_and_authorizer_allow_it() {
    let fixture = copied_fixture_root("ideapad-toggle-write-success");
    let state_path = unique_state_path("ideapad-toggle-write-success");
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
        WriteAccessPolicy {
            platform_profile_enabled: false,
            battery_charge_type_enabled: false,
            led_state_enabled: false,
            ideapad_toggle_enabled: true,
            camera_power_enabled: false,
            usb_charging_enabled: false,
            fan_mode_enabled: false,
            gpu_mode_enabled: false,
            cpu_governor_enabled: false,
            cpu_epp_enabled: false,
            firmware_attribute_enabled: false,
            cpu_boost_enabled: false,
            conservation_mode_enabled: false,
            amd_gpu_dpm_enabled: false,
            curve_optimizer_enabled: false,
            hardware_profile_apply_enabled: false,
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
        Arc::new(NoOpCpuGovernorWriter),
        Arc::new(NoOpCpuEppWriter),
    );

    let result = service
        .set_ideapad_toggle("fn_lock", true, ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::Applied);
    assert!(result.applied);
    assert_eq!(result.readback_value.as_deref(), Some("1"));
    assert_eq!(
        fs::read_to_string(
            fixture.join("sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/fn_lock")
        )
        .unwrap()
        .trim(),
        "1"
    );
    assert_eq!(
        fs::read_to_string(fixture.join("sys/class/leds/platform::fnlock/brightness"))
            .unwrap()
            .trim(),
        "1"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn ideapad_toggle_write_rolls_back_after_readback_mismatch() {
    let fixture = copied_fixture_root("ideapad-toggle-write-rollback");
    let state_path = unique_state_path("ideapad-toggle-write-rollback");
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
        WriteAccessPolicy {
            platform_profile_enabled: false,
            battery_charge_type_enabled: false,
            led_state_enabled: false,
            ideapad_toggle_enabled: true,
            camera_power_enabled: false,
            usb_charging_enabled: false,
            fan_mode_enabled: false,
            gpu_mode_enabled: false,
            cpu_governor_enabled: false,
            cpu_epp_enabled: false,
            firmware_attribute_enabled: false,
            cpu_boost_enabled: false,
            conservation_mode_enabled: false,
            amd_gpu_dpm_enabled: false,
            curve_optimizer_enabled: false,
            hardware_profile_apply_enabled: false,
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(MismatchingIdeapadToggleWriter),
        Arc::new(NoOpCpuGovernorWriter),
        Arc::new(NoOpCpuEppWriter),
    );

    let result = service
        .set_ideapad_toggle("fn_lock", true, ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::Failed);
    assert!(!result.applied);
    assert!(result.message.contains("restored previous value `0`"));
    assert_eq!(result.readback_value.as_deref(), Some("0"));
    assert_eq!(
        fs::read_to_string(
            fixture.join("sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/fn_lock")
        )
        .unwrap()
        .trim(),
        "0"
    );
    assert_eq!(
        fs::read_to_string(fixture.join("sys/class/leds/platform::fnlock/brightness"))
            .unwrap()
            .trim(),
        "0"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn camera_power_write_applies_when_policy_and_authorizer_allow_it() {
    let fixture = copied_fixture_root("camera-power-write-success");
    let state_path = unique_state_path("camera-power-write-success");
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
        WriteAccessPolicy {
            platform_profile_enabled: false,
            battery_charge_type_enabled: false,
            led_state_enabled: false,
            ideapad_toggle_enabled: false,
            camera_power_enabled: true,
            usb_charging_enabled: false,
            fan_mode_enabled: false,
            gpu_mode_enabled: false,
            cpu_governor_enabled: false,
            cpu_epp_enabled: false,
            firmware_attribute_enabled: false,
            cpu_boost_enabled: false,
            conservation_mode_enabled: false,
            amd_gpu_dpm_enabled: false,
            curve_optimizer_enabled: false,
            hardware_profile_apply_enabled: false,
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
        Arc::new(NoOpCpuGovernorWriter),
        Arc::new(NoOpCpuEppWriter),
    );

    let result = service
        .set_ideapad_toggle("camera_power", false, ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::Applied);
    assert!(result.applied);
    assert_eq!(result.readback_value.as_deref(), Some("0"));
    assert_eq!(
        fs::read_to_string(
            fixture.join("sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/camera_power")
        )
        .unwrap()
        .trim(),
        "0"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn camera_power_write_rolls_back_after_readback_mismatch() {
    let fixture = copied_fixture_root("camera-power-write-rollback");
    let state_path = unique_state_path("camera-power-write-rollback");
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
        WriteAccessPolicy {
            platform_profile_enabled: false,
            battery_charge_type_enabled: false,
            led_state_enabled: false,
            ideapad_toggle_enabled: false,
            camera_power_enabled: true,
            usb_charging_enabled: false,
            fan_mode_enabled: false,
            gpu_mode_enabled: false,
            cpu_governor_enabled: false,
            cpu_epp_enabled: false,
            firmware_attribute_enabled: false,
            cpu_boost_enabled: false,
            conservation_mode_enabled: false,
            amd_gpu_dpm_enabled: false,
            curve_optimizer_enabled: false,
            hardware_profile_apply_enabled: false,
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(MismatchingCameraPowerWriter),
        Arc::new(NoOpCpuGovernorWriter),
        Arc::new(NoOpCpuEppWriter),
    );

    let result = service
        .set_ideapad_toggle("camera_power", false, ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::Failed);
    assert!(!result.applied);
    assert!(result.message.contains("restored previous value `1`"));
    assert_eq!(result.readback_value.as_deref(), Some("1"));
    assert_eq!(
        fs::read_to_string(
            fixture.join("sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/camera_power")
        )
        .unwrap()
        .trim(),
        "1"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn usb_charging_write_reports_policy_block_when_write_is_disabled() {
    let fixture = copied_runtime_fixture_root("usb-charging-write-blocked");
    seed_usb_charging_toggle(&fixture, "1");
    let state_path = unique_state_path("usb-charging-write-blocked");
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
        WriteAccessPolicy {
            platform_profile_enabled: false,
            battery_charge_type_enabled: false,
            led_state_enabled: false,
            ideapad_toggle_enabled: false,
            camera_power_enabled: false,
            usb_charging_enabled: false,
            fan_mode_enabled: false,
            gpu_mode_enabled: false,
            cpu_governor_enabled: false,
            cpu_epp_enabled: false,
            firmware_attribute_enabled: false,
            cpu_boost_enabled: false,
            conservation_mode_enabled: false,
            amd_gpu_dpm_enabled: false,
            curve_optimizer_enabled: false,
            hardware_profile_apply_enabled: false,
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
        Arc::new(NoOpCpuGovernorWriter),
        Arc::new(NoOpCpuEppWriter),
    );

    let plan = service
        .plan_ideapad_toggle_write("usb_charging", false)
        .unwrap();
    assert_eq!(plan.previous_value, "1");
    assert_eq!(plan.requested_value, "0");

    let result = service
        .set_ideapad_toggle("usb_charging", false, ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::BlockedByPolicy);
    assert!(!result.applied);
    assert!(result.message.contains("usb charging writes are disabled"));
    assert_eq!(
        fs::read_to_string(
            fixture.join("sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/usb_charging")
        )
        .unwrap()
        .trim(),
        "1"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn usb_charging_write_applies_when_policy_and_authorizer_allow_it() {
    let fixture = copied_runtime_fixture_root("usb-charging-write-success");
    seed_usb_charging_toggle(&fixture, "1");
    let state_path = unique_state_path("usb-charging-write-success");
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
        WriteAccessPolicy {
            platform_profile_enabled: false,
            battery_charge_type_enabled: false,
            led_state_enabled: false,
            ideapad_toggle_enabled: false,
            camera_power_enabled: false,
            usb_charging_enabled: true,
            fan_mode_enabled: false,
            gpu_mode_enabled: false,
            cpu_governor_enabled: false,
            cpu_epp_enabled: false,
            firmware_attribute_enabled: false,
            cpu_boost_enabled: false,
            conservation_mode_enabled: false,
            amd_gpu_dpm_enabled: false,
            curve_optimizer_enabled: false,
            hardware_profile_apply_enabled: false,
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
        Arc::new(NoOpCpuGovernorWriter),
        Arc::new(NoOpCpuEppWriter),
    );

    let result = service
        .set_ideapad_toggle("usb_charging", false, ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::Applied);
    assert!(result.applied);
    assert_eq!(result.readback_value.as_deref(), Some("0"));
    assert_eq!(
        fs::read_to_string(
            fixture.join("sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/usb_charging")
        )
        .unwrap()
        .trim(),
        "0"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn usb_charging_write_rolls_back_after_readback_mismatch() {
    let fixture = copied_runtime_fixture_root("usb-charging-write-rollback");
    seed_usb_charging_toggle(&fixture, "1");
    let state_path = unique_state_path("usb-charging-write-rollback");
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
        WriteAccessPolicy {
            platform_profile_enabled: false,
            battery_charge_type_enabled: false,
            led_state_enabled: false,
            ideapad_toggle_enabled: false,
            camera_power_enabled: false,
            usb_charging_enabled: true,
            fan_mode_enabled: false,
            gpu_mode_enabled: false,
            cpu_governor_enabled: false,
            cpu_epp_enabled: false,
            firmware_attribute_enabled: false,
            cpu_boost_enabled: false,
            conservation_mode_enabled: false,
            amd_gpu_dpm_enabled: false,
            curve_optimizer_enabled: false,
            hardware_profile_apply_enabled: false,
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(MismatchingUsbChargingWriter),
        Arc::new(NoOpCpuGovernorWriter),
        Arc::new(NoOpCpuEppWriter),
    );

    let result = service
        .set_ideapad_toggle("usb_charging", false, ":1.99")
        .unwrap();
    assert_eq!(result.status, WriteExecutionStatus::Failed);
    assert!(!result.applied);
    assert!(result.message.contains("restored previous value `1`"));
    assert_eq!(result.readback_value.as_deref(), Some("1"));
    assert_eq!(
        fs::read_to_string(
            fixture.join("sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/usb_charging")
        )
        .unwrap()
        .trim(),
        "1"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn daemon_builds_fan_preset_plan_from_runtime_fixture() {
    let state_path = unique_state_path("runtime-fan-curve");
    let service = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: runtime_fixture_root(),
        },
        &state_path,
    );

    let plan = service.plan_fan_preset_write("balanced-daily").unwrap();
    assert_eq!(plan.method, "ApplyFanPreset");
    assert_eq!(plan.capability_id, "fan_curves");
    assert_eq!(plan.requested_value, "balanced-daily");
    assert_eq!(plan.previous_value, "current fan curve snapshot");
    assert!(plan.readback_required);
    assert!(!plan.reboot_required);
    assert!(plan
        .safety_notes
        .iter()
        .any(|note| note.contains("Middle-ground fan ramp")));

    let restore_plan = service.plan_restore_auto_fan_write().unwrap();
    assert_eq!(restore_plan.method, "RestoreAutoFan");
    assert_eq!(restore_plan.capability_id, "fan_curves");
    assert_eq!(restore_plan.requested_value, "auto/default fan control");
    assert!(restore_plan.readback_required);
    assert!(!restore_plan.reboot_required);

    assert_eq!(service.last_known_good_fan_curve().unwrap(), None);
    let captured = service.capture_last_known_good_fan_curve().unwrap();
    assert_eq!(captured.curve_id, "legion_hwmon");
    assert!(captured.points.len() >= 20);
    assert!(captured
        .points
        .iter()
        .any(|point| point.path.ends_with("pwm1_auto_point1_temp") && !point.value.is_empty()));
    assert_eq!(service.last_known_good_fan_curve().unwrap(), Some(captured));
    let _ = fs::remove_file(state_path);
}

#[test]
fn fan_preset_profile_map_round_trips_through_state_file() {
    let state_path = unique_state_path("fan-profile-map");
    let service = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: runtime_fixture_root(),
        },
        &state_path,
    );
    assert!(service.fan_preset_by_platform_profile().unwrap().is_empty());
    let updated = service
        .set_fan_preset_profile_map_entry("performance", "gaming")
        .unwrap();
    assert_eq!(
        updated.get("performance").map(String::as_str),
        Some("gaming")
    );
    let reloaded = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: runtime_fixture_root(),
        },
        &state_path,
    );
    assert_eq!(
        reloaded
            .fan_preset_by_platform_profile()
            .unwrap()
            .get("performance")
            .map(String::as_str),
        Some("gaming")
    );
    let after_remove = reloaded
        .remove_fan_preset_profile_map_entry("performance")
        .unwrap();
    assert!(!after_remove.contains_key("performance"));
    let persisted = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: runtime_fixture_root(),
        },
        &state_path,
    );
    assert!(persisted
        .fan_preset_by_platform_profile()
        .unwrap()
        .is_empty());
    let _ = fs::remove_file(state_path);
}

#[test]
fn hardware_profile_store_validates_and_previews_supported_actions() {
    let state_path = unique_state_path("hardware-profile-store");
    let service = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: fixture_root(),
        },
        &state_path,
    );
    let profile_json = serde_json::json!({
        "schema_version": 1,
        "label": "Balanced tuned",
        "actions": {
            "platform_profile": "performance",
            "battery_charge_type": "Fast",
            "cpu_boost": "0",
            "curve_optimizer_all_core": "-20",
            "firmware_attributes": {
                "ppt_pl1_spl": "75"
            }
        }
    })
    .to_string();

    let preview = service
        .set_hardware_profile("balanced_tuned", &profile_json)
        .unwrap();
    assert_eq!(preview.profile_id, "balanced_tuned");
    assert_eq!(preview.profile_label, "Balanced tuned");
    assert_eq!(
        preview
            .plans
            .iter()
            .map(|plan| plan.method.as_str())
            .collect::<Vec<_>>(),
        [
            "SetPlatformProfile",
            "SetBatteryChargeType",
            "SetCpuBoost",
            "SetCurveOptimizerAllCore",
            "SetFirmwareAttribute"
        ]
    );

    let reloaded = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: fixture_root(),
        },
        &state_path,
    );
    assert!(reloaded
        .hardware_profiles()
        .unwrap()
        .contains_key("balanced_tuned"));
    let preview = reloaded
        .hardware_profile_apply_preview("balanced_tuned")
        .unwrap();
    assert_eq!(preview.plans.len(), 5);
    let removed = reloaded.remove_hardware_profile("balanced_tuned").unwrap();
    assert!(removed.is_some());
    assert!(reloaded.hardware_profiles().unwrap().is_empty());

    let _ = fs::remove_file(state_path);
}

#[test]
fn hardware_profile_store_rejects_unknown_or_invalid_actions() {
    let state_path = unique_state_path("hardware-profile-invalid");
    let service = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: fixture_root(),
        },
        &state_path,
    );

    let unknown_json = serde_json::json!({
        "schema_version": 1,
        "label": "Bad",
        "actions": {
            "raw_sysfs": "/sys/nope"
        }
    })
    .to_string();
    assert!(service
        .set_hardware_profile("bad_unknown", &unknown_json)
        .is_err());

    let invalid_json = serde_json::json!({
        "schema_version": 1,
        "label": "Bad CO",
        "actions": {
            "curve_optimizer_all_core": "-40"
        }
    })
    .to_string();
    assert!(service
        .set_hardware_profile("bad_co", &invalid_json)
        .is_err());
    assert!(service.hardware_profiles().unwrap().is_empty());

    let _ = fs::remove_file(state_path);
}

#[test]
fn hardware_profile_store_rejects_overlapping_charge_type_and_conservation_mode() {
    let state_path = unique_state_path("hardware-profile-charge-conflict");
    let service = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: fixture_root(),
        },
        &state_path,
    );
    let profile_json = serde_json::json!({
        "schema_version": 1,
        "label": "Conflicting charge behavior",
        "actions": {
            "battery_charge_type": "Fast",
            "conservation_mode": "1"
        }
    })
    .to_string();

    let error = service
        .set_hardware_profile("bad_charge", &profile_json)
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("battery charge type and conservation_mode overlap"));
    assert!(service.hardware_profiles().unwrap().is_empty());

    let _ = fs::remove_file(state_path);
}

#[test]
fn apply_hardware_profile_executes_actions_and_records_last_run() {
    let state_path = unique_state_path("hardware-profile-apply");
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture_root(),
        },
        &state_path,
        WriteAccessPolicy {
            curve_optimizer_enabled: true,
            hardware_profile_apply_enabled: true,
            ..Default::default()
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
        Arc::new(NoOpCpuGovernorWriter),
        Arc::new(NoOpCpuEppWriter),
    )
    .with_curve_optimizer_writer(Arc::new(RecordingCurveOptimizerWriter {
        calls: calls.clone(),
    }));
    let profile_json = serde_json::json!({
        "schema_version": 1,
        "label": "CO test",
        "actions": {
            "curve_optimizer_all_core": "-20"
        }
    })
    .to_string();

    service
        .set_hardware_profile("co_test", &profile_json)
        .unwrap();
    let run = service
        .apply_hardware_profile("co_test", "test.sender")
        .unwrap();

    assert!(run.completed);
    assert_eq!(run.profile_id, "co_test");
    assert_eq!(run.results.len(), 1);
    assert_eq!(run.results[0].action_id, "curve_optimizer_all_core");
    assert_eq!(run.results[0].result.status, WriteExecutionStatus::Applied);
    assert_eq!(calls.lock().unwrap().as_slice(), &[4_294_967_276]);
    assert_eq!(service.last_hardware_profile_apply().unwrap(), Some(run));

    let _ = fs::remove_file(state_path);
}

#[test]
fn apply_hardware_profile_executes_battery_charge_type_action() {
    let fixture = copied_fixture_root("hardware-profile-battery-charge-type");
    let state_path = unique_state_path("hardware-profile-battery-charge-type");
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
        WriteAccessPolicy {
            battery_charge_type_enabled: true,
            hardware_profile_apply_enabled: true,
            ..Default::default()
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
        Arc::new(NoOpCpuGovernorWriter),
        Arc::new(NoOpCpuEppWriter),
    );
    let profile_json = serde_json::json!({
        "schema_version": 1,
        "label": "Fast charge",
        "actions": {
            "battery_charge_type": "Fast"
        }
    })
    .to_string();

    service
        .set_hardware_profile("fast_charge", &profile_json)
        .unwrap();
    let run = service
        .apply_hardware_profile("fast_charge", "test.sender")
        .unwrap();

    assert!(run.completed);
    assert_eq!(run.results.len(), 1);
    assert_eq!(run.results[0].action_id, "battery_charge_type");
    assert_eq!(run.results[0].result.status, WriteExecutionStatus::Applied);
    assert_eq!(
        run.results[0].result.readback_value.as_deref(),
        Some("Fast")
    );
    assert_eq!(
        fs::read_to_string(fixture.join("sys/class/power_supply/BAT0/charge_type"))
            .unwrap()
            .trim(),
        "Fast"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn fast_charge_threshold_automation_selects_and_applies_profile() {
    let fixture = copied_fixture_root("automation-fast-charge-threshold");
    let state_path = unique_state_path("automation-fast-charge-threshold");
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
        WriteAccessPolicy {
            battery_charge_type_enabled: true,
            hardware_profile_apply_enabled: true,
            ..Default::default()
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
        Arc::new(NoOpCpuGovernorWriter),
        Arc::new(NoOpCpuEppWriter),
    );
    let fast_charge = serde_json::json!({
        "schema_version": 1,
        "label": "Fast charge",
        "actions": {
            "battery_charge_type": "Fast"
        }
    })
    .to_string();
    let protect = serde_json::json!({
        "schema_version": 1,
        "label": "Protect battery",
        "actions": {
            "battery_charge_type": "Conservation"
        }
    })
    .to_string();
    service
        .set_hardware_profile("fast_charge", &fast_charge)
        .unwrap();
    service
        .set_hardware_profile("battery_protect", &protect)
        .unwrap();
    let rule = serde_json::json!({
        "schema_version": 1,
        "label": "Fast charge until 80%",
        "enabled": true,
        "kind": "fast_charge_until_threshold",
        "threshold_percent": 80,
        "fast_charge_profile_id": "fast_charge",
        "protect_profile_id": "battery_protect",
        "require_ac": true,
        "cooldown_secs": 42
    })
    .to_string();

    service
        .set_automation_rule("fast_charge_until_80", &rule)
        .unwrap();
    let rules = service.automation_rules().unwrap();
    let AutomationRuleKind::FastChargeUntilThreshold { cooldown_secs, .. } =
        &rules["fast_charge_until_80"].kind;
    assert_eq!(*cooldown_secs, 42);
    let preview = service
        .automation_rule_preview("fast_charge_until_80")
        .unwrap();
    assert!(preview.matched);
    assert_eq!(preview.battery_capacity_percent, Some(79));
    assert_eq!(preview.ac_online, Some(true));
    assert_eq!(preview.selected_profile_id.as_deref(), Some("fast_charge"));
    assert!(preview.reason.contains("below threshold 80%"));
    assert_eq!(
        preview
            .profile_preview
            .as_ref()
            .unwrap()
            .plans
            .first()
            .unwrap()
            .method,
        "SetBatteryChargeType"
    );

    let run = service
        .apply_automation_rule("fast_charge_until_80", "test.sender")
        .unwrap();
    assert!(run.profile_run.as_ref().unwrap().completed);
    assert_eq!(
        service
            .last_automation_rule_apply()
            .unwrap()
            .get("fast_charge_until_80"),
        Some(&run)
    );
    assert_eq!(
        fs::read_to_string(fixture.join("sys/class/power_supply/BAT0/charge_type"))
            .unwrap()
            .trim(),
        "Fast"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn automation_observer_tick_applies_saved_rule_once_per_cooldown() {
    let fixture = copied_fixture_root("automation-observer-tick");
    let state_path = unique_state_path("automation-observer-tick");
    let service = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
    );
    let fast_charge = serde_json::json!({
        "schema_version": 1,
        "label": "Fast charge",
        "actions": {
            "battery_charge_type": "Fast"
        }
    })
    .to_string();
    let protect = serde_json::json!({
        "schema_version": 1,
        "label": "Protect battery",
        "actions": {
            "battery_charge_type": "Conservation"
        }
    })
    .to_string();
    service
        .set_hardware_profile("fast_charge", &fast_charge)
        .unwrap();
    service
        .set_hardware_profile("battery_protect", &protect)
        .unwrap();
    let rule = serde_json::json!({
        "schema_version": 1,
        "label": "Fast charge until 80%",
        "enabled": true,
        "kind": "fast_charge_until_threshold",
        "threshold_percent": 80,
        "fast_charge_profile_id": "fast_charge",
        "protect_profile_id": "battery_protect",
        "require_ac": true,
        "cooldown_secs": 300
    })
    .to_string();
    service
        .set_automation_rule("fast_charge_until_80", &rule)
        .unwrap();

    let policy = WriteAccessPolicy {
        battery_charge_type_enabled: true,
        hardware_profile_apply_enabled: true,
        ..Default::default()
    };
    let runs = legion_control_daemon::handle_automation_observer_tick(
        &state_path,
        &ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        policy.clone(),
        300,
    )
    .unwrap();
    assert_eq!(runs.len(), 1);
    assert!(runs[0].profile_run.as_ref().unwrap().completed);
    assert_eq!(
        fs::read_to_string(fixture.join("sys/class/power_supply/BAT0/charge_type"))
            .unwrap()
            .trim(),
        "Fast"
    );

    let runs = legion_control_daemon::handle_automation_observer_tick(
        &state_path,
        &ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        policy,
        300,
    )
    .unwrap();
    assert_eq!(runs.len(), 1);
    assert!(runs[0].profile_run.is_none());
    assert!(runs[0].evaluation.reason.contains("cooldown"));

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn apply_hardware_profile_records_policy_block_without_running_actions() {
    let state_path = unique_state_path("hardware-profile-apply-blocked");
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture_root(),
        },
        &state_path,
        WriteAccessPolicy {
            curve_optimizer_enabled: true,
            hardware_profile_apply_enabled: false,
            ..Default::default()
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
        Arc::new(NoOpCpuGovernorWriter),
        Arc::new(NoOpCpuEppWriter),
    )
    .with_curve_optimizer_writer(Arc::new(RecordingCurveOptimizerWriter {
        calls: calls.clone(),
    }));
    let profile_json = serde_json::json!({
        "schema_version": 1,
        "label": "CO blocked",
        "actions": {
            "curve_optimizer_all_core": "-20"
        }
    })
    .to_string();

    service
        .set_hardware_profile("co_blocked", &profile_json)
        .unwrap();
    let run = service
        .apply_hardware_profile("co_blocked", "test.sender")
        .unwrap();

    assert!(!run.completed);
    assert!(run.results.is_empty());
    assert!(run.message.contains("disabled by daemon policy"));
    assert!(calls.lock().unwrap().is_empty());
    assert_eq!(service.last_hardware_profile_apply().unwrap(), Some(run));

    let _ = fs::remove_file(state_path);
}

#[test]
fn hardware_profile_triggers_validate_persist_and_apply_mapped_profile() {
    let state_path = unique_state_path("hardware-profile-trigger");
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture_root(),
        },
        &state_path,
        WriteAccessPolicy {
            curve_optimizer_enabled: true,
            hardware_profile_apply_enabled: true,
            ..Default::default()
        },
        Arc::new(AllowAllAuthorizer),
        Arc::new(RealFixturePlatformProfileWriter),
        Arc::new(RealFixtureBatteryChargeTypeWriter),
        Arc::new(RealFixtureLedStateWriter),
        Arc::new(RealFixtureIdeapadToggleWriter),
        Arc::new(NoOpCpuGovernorWriter),
        Arc::new(NoOpCpuEppWriter),
    )
    .with_curve_optimizer_writer(Arc::new(RecordingCurveOptimizerWriter {
        calls: calls.clone(),
    }));
    let profile_json = serde_json::json!({
        "schema_version": 1,
        "label": "CO test",
        "actions": {
            "curve_optimizer_all_core": "-20"
        }
    })
    .to_string();

    service
        .set_hardware_profile("co_test", &profile_json)
        .unwrap();
    let triggers = service
        .set_hardware_profile_trigger("ac_connected", "co_test")
        .unwrap();
    assert_eq!(
        triggers.get("ac_connected").map(String::as_str),
        Some("co_test")
    );
    let preview = service
        .hardware_profile_trigger_apply_preview("ac_connected")
        .unwrap();
    assert_eq!(preview.profile_id, "co_test");
    assert_eq!(preview.plans.len(), 1);
    assert_eq!(preview.plans[0].method, "SetCurveOptimizerAllCore");

    let reloaded = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: fixture_root(),
        },
        &state_path,
    );
    assert_eq!(
        reloaded
            .hardware_profile_triggers()
            .unwrap()
            .get("ac_connected")
            .map(String::as_str),
        Some("co_test")
    );

    let run = service
        .apply_hardware_profile_trigger("ac_connected", "test.sender")
        .unwrap();
    assert!(run.completed);
    assert_eq!(calls.lock().unwrap().as_slice(), &[4_294_967_276]);

    assert!(service
        .set_hardware_profile_trigger("lid_open", "co_test")
        .is_err());
    assert!(service
        .set_hardware_profile_trigger("resume", "missing")
        .is_err());
    assert_eq!(
        service
            .remove_hardware_profile_trigger("ac_connected")
            .unwrap(),
        Some("co_test".to_owned())
    );
    assert!(service.hardware_profile_triggers().unwrap().is_empty());

    let _ = fs::remove_file(state_path);
}

#[test]
fn fan_preset_reapply_after_resume_round_trips_over_dbus_and_state_file() {
    let state_path = unique_state_path("fan-reapply-dbus");
    let service = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: fixture_root(),
        },
        &state_path,
    );
    let (_bus, _service_connection, proxy) = test_proxy_with_service(service);

    let reapply: bool = call_json(&proxy, "GetFanPresetReapplyAfterResume");
    assert!(!reapply);
    let payload: String = proxy
        .call("SetFanPresetReapplyAfterResume", &(true,))
        .unwrap();
    let reapply: bool = serde_json::from_str(&payload).unwrap();
    assert!(reapply);
    let reapply: bool = call_json(&proxy, "GetFanPresetReapplyAfterResume");
    assert!(reapply);
    let payload: String = proxy
        .call("SetFanPresetReapplyAfterResume", &(false,))
        .unwrap();
    let reapply: bool = serde_json::from_str(&payload).unwrap();
    assert!(!reapply);

    drop(proxy);
    drop(_service_connection);
    let reloaded = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: fixture_root(),
        },
        &state_path,
    );
    assert!(!reloaded.fan_preset_reapply_after_resume().unwrap());
    let _ = fs::remove_file(state_path);
}

#[test]
fn daemon_loads_clears_and_ignores_invalid_state_files() {
    let state_path = unique_state_path("pending-gpu");
    fs::write(
        &state_path,
        r#"schema_version = 1

[gpu_mode_pending]
requested_mode = "hybrid"
previous_mode = "nvidia"
reboot_required = true
"#,
    )
    .unwrap();

    let service = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: fixture_root(),
        },
        &state_path,
    );
    let pending = service.gpu_mode_pending().unwrap().unwrap();
    assert_eq!(
        pending,
        GpuModePending {
            requested_mode: "hybrid".to_owned(),
            previous_mode: Some("nvidia".to_owned()),
            reboot_required: true,
        }
    );

    let cleared = service.clear_gpu_mode_pending().unwrap().unwrap();
    assert_eq!(cleared.requested_mode, "hybrid");
    let reloaded = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: fixture_root(),
        },
        &state_path,
    );
    assert_eq!(reloaded.gpu_mode_pending().unwrap(), None);

    fs::write(&state_path, "not valid toml =").unwrap();
    let corrupt = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: fixture_root(),
        },
        &state_path,
    );
    assert_eq!(corrupt.gpu_mode_pending().unwrap(), None);
    let _ = fs::remove_file(state_path);
}

fn test_proxy() -> (PrivateBus, zbus::blocking::Connection, Proxy<'static>) {
    test_proxy_with_service(LegionControl::new(ProbeOptions {
        sysfs_root: fixture_root(),
    }))
}

fn test_proxy_with_service(
    service: LegionControl,
) -> (PrivateBus, zbus::blocking::Connection, Proxy<'static>) {
    let bus = PrivateBus::start();
    let service_connection = ConnectionBuilder::address(bus.address())
        .unwrap()
        .name(DBUS_INTERFACE)
        .unwrap()
        .serve_at(DBUS_PATH, service)
        .unwrap()
        .build()
        .unwrap();
    let client_connection = ConnectionBuilder::address(bus.address())
        .unwrap()
        .build()
        .unwrap();
    let proxy =
        Proxy::new_owned(client_connection, DBUS_INTERFACE, DBUS_PATH, DBUS_INTERFACE).unwrap();

    (bus, service_connection, proxy)
}

fn runtime_fixture_root() -> std::path::PathBuf {
    fixture_root()
        .parent()
        .expect("fixture root must have parent")
        .join("sysfs-82wm-runtime-capture")
}

fn copied_runtime_fixture_root(label: &str) -> std::path::PathBuf {
    let destination = std::path::PathBuf::from("/tmp").join(format!(
        "ratvantage-{label}-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let status = std::process::Command::new("cp")
        .args([
            "-a",
            runtime_fixture_root().to_str().unwrap(),
            destination.to_str().unwrap(),
        ])
        .status()
        .expect("cp must be available for runtime fixture copy tests");
    assert!(status.success(), "cp -a runtime fixture copy must succeed");
    destination
}

fn seed_usb_charging_toggle(root: &std::path::Path, value: &str) {
    let path = root.join("sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/usb_charging");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, value).unwrap();
}

fn unique_state_path(label: &str) -> std::path::PathBuf {
    std::path::PathBuf::from("/tmp").join(format!(
        "ratvantage-{label}-{}-{}.toml",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ))
}

fn unique_temp_dir(label: &str) -> std::path::PathBuf {
    std::path::PathBuf::from("/tmp").join(format!(
        "ratvantage-{label}-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ))
}

struct AllowAllAuthorizer;

impl WriteAuthorizer for AllowAllAuthorizer {
    fn authorize(&self, _action: &str, _sender: &str) -> std::result::Result<(), String> {
        Ok(())
    }
}

struct RecordingCurveOptimizerWriter {
    calls: Arc<std::sync::Mutex<Vec<u32>>>,
}

impl CurveOptimizerAllCoreWriter for RecordingCurveOptimizerWriter {
    fn set_curve_optimizer_all_core(
        &self,
        encoded_value: u32,
    ) -> std::result::Result<CurveOptimizerCommandOutput, String> {
        self.calls.lock().unwrap().push(encoded_value);
        Ok(CurveOptimizerCommandOutput {
            stdout: "Successfully set coall".to_owned(),
            stderr: "no compatible ryzen_smu kernel module found, fallback to /dev/mem".to_owned(),
        })
    }
}

struct MissingMarkerCurveOptimizerWriter;

impl CurveOptimizerAllCoreWriter for MissingMarkerCurveOptimizerWriter {
    fn set_curve_optimizer_all_core(
        &self,
        _encoded_value: u32,
    ) -> std::result::Result<CurveOptimizerCommandOutput, String> {
        Ok(CurveOptimizerCommandOutput {
            stdout: "Curve Optimizer command completed".to_owned(),
            stderr: String::new(),
        })
    }
}

struct RealFixturePlatformProfileWriter;

impl PlatformProfileWriter for RealFixturePlatformProfileWriter {
    fn write_platform_profile(
        &self,
        path: &str,
        requested: &str,
    ) -> std::result::Result<(), String> {
        fs::write(path, requested).map_err(|error| error.to_string())
    }
}

struct RealFixtureBatteryChargeTypeWriter;

impl BatteryChargeTypeWriter for RealFixtureBatteryChargeTypeWriter {
    fn write_battery_charge_type(
        &self,
        path: &str,
        requested: &str,
    ) -> std::result::Result<(), String> {
        fs::write(path, requested).map_err(|error| error.to_string())
    }
}

struct RealFixtureLedStateWriter;

impl LedStateWriter for RealFixtureLedStateWriter {
    fn write_led_state(&self, path: &str, enabled: bool) -> std::result::Result<(), String> {
        fs::write(path, if enabled { "1" } else { "0" }).map_err(|error| error.to_string())
    }
}

struct RealFixtureIdeapadToggleWriter;

impl IdeapadToggleWriter for RealFixtureIdeapadToggleWriter {
    fn write_ideapad_toggle(&self, path: &str, enabled: bool) -> std::result::Result<(), String> {
        fs::write(path, if enabled { "1" } else { "0" }).map_err(|error| error.to_string())?;
        if path.ends_with("/fn_lock") {
            let indicator = path.replace(
                "sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/fn_lock",
                "sys/class/leds/platform::fnlock/brightness",
            );
            fs::write(indicator, if enabled { "1" } else { "0" })
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

struct FailingPlatformProfileWriter;

impl PlatformProfileWriter for FailingPlatformProfileWriter {
    fn write_platform_profile(
        &self,
        _path: &str,
        _requested: &str,
    ) -> std::result::Result<(), String> {
        Err("injected write failure".to_owned())
    }
}

struct MismatchingPlatformProfileWriter;

impl PlatformProfileWriter for MismatchingPlatformProfileWriter {
    fn write_platform_profile(
        &self,
        path: &str,
        requested: &str,
    ) -> std::result::Result<(), String> {
        let value = if requested == "performance" {
            "balanced"
        } else {
            requested
        };
        fs::write(path, value).map_err(|error| error.to_string())
    }
}

struct MismatchingBatteryChargeTypeWriter;

impl BatteryChargeTypeWriter for MismatchingBatteryChargeTypeWriter {
    fn write_battery_charge_type(
        &self,
        path: &str,
        requested: &str,
    ) -> std::result::Result<(), String> {
        let value = if requested == "Conservation" {
            "Standard"
        } else {
            requested
        };
        fs::write(path, value).map_err(|error| error.to_string())
    }
}

struct MismatchingLedStateWriter;

impl LedStateWriter for MismatchingLedStateWriter {
    fn write_led_state(&self, path: &str, _enabled: bool) -> std::result::Result<(), String> {
        fs::write(path, "1").map_err(|error| error.to_string())
    }
}

struct MismatchingIdeapadToggleWriter;

impl IdeapadToggleWriter for MismatchingIdeapadToggleWriter {
    fn write_ideapad_toggle(&self, path: &str, _enabled: bool) -> std::result::Result<(), String> {
        fs::write(path, "0").map_err(|error| error.to_string())?;
        if path.ends_with("/fn_lock") {
            let indicator = path.replace(
                "sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/fn_lock",
                "sys/class/leds/platform::fnlock/brightness",
            );
            fs::write(indicator, "0").map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

struct NoOpCpuGovernorWriter;

impl CpuGovernorWriter for NoOpCpuGovernorWriter {
    fn write_cpu_governor(&self, _path: &str, _requested: &str) -> std::result::Result<(), String> {
        Ok(())
    }
}

struct NoOpCpuEppWriter;

impl CpuEppWriter for NoOpCpuEppWriter {
    fn write_cpu_epp(&self, _path: &str, _requested: &str) -> std::result::Result<(), String> {
        Ok(())
    }
}

struct MismatchingCameraPowerWriter;

impl IdeapadToggleWriter for MismatchingCameraPowerWriter {
    fn write_ideapad_toggle(&self, path: &str, _enabled: bool) -> std::result::Result<(), String> {
        fs::write(path, "1").map_err(|error| error.to_string())
    }
}

struct MismatchingUsbChargingWriter;

impl IdeapadToggleWriter for MismatchingUsbChargingWriter {
    fn write_ideapad_toggle(&self, path: &str, _enabled: bool) -> std::result::Result<(), String> {
        fs::write(path, "1").map_err(|error| error.to_string())
    }
}
