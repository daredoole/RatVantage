use std::{collections::BTreeMap, env, fs, os::unix::fs::PermissionsExt, sync::Arc};

use legion_common::{
    AutomationRuleKind, Capability, CapabilityRegistry, CurveOptimizerWriteState,
    CustomThermalPlanPreview, DesktopPowerProfileChangeEvent, FanCurveSnapshot, GpuModePending,
    HardwareSummary, KeyboardRgbOpenRgbDevice, KeyboardRgbOpenRgbStatus, KeyboardRgbWriteRequest,
    PlatformProfileChangeEvent, RyzenBackendStatus, TelemetrySnapshot, WriteDryRunPlan,
    WriteExecutionResult, WriteExecutionStatus,
};
use legion_control_daemon::{
    BatteryChargeTypeWriter, CommandOpenRgbKeyboardRgbSdkWriter, CpuEppWriter, CpuGovernorWriter,
    CurveOptimizerAllCoreWriter, CurveOptimizerCommandOutput, IdeapadToggleWriter,
    KeyboardRgbWriter, LedStateWriter, LegionControl, OpenRgbAccessSetupWriter,
    OpenRgbKeyboardRgbSdkSnapshot, OpenRgbKeyboardRgbSdkWriter, PlatformProfileWriter,
    WriteAccessPolicy, WriteAuthorizer, DBUS_INTERFACE, DBUS_PATH,
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

    let ryzen_status: RyzenBackendStatus = call_json(&proxy, "GetRyzenBackendStatus");
    assert_eq!(
        ryzen_status.curve_optimizer_readback_status,
        legion_common::CurveOptimizerReadbackStatus::WriteOnly
    );
    assert!(!ryzen_status.setup_assistant.commands.is_empty());

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

    let prepare_error = proxy
        .call::<_, _, String>("PlanPrepareCustomThermalMode", &())
        .unwrap_err()
        .to_string();
    assert!(prepare_error.contains("unsupported_choice"));

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
    assert!(firmware_plan.safety_notes.iter().any(
        |note| note.contains("custom thermal prerequisite unavailable for firmware PPT writes")
    ));

    let payload: String = proxy
        .call("PlanFirmwareAttributeResetWrite", &("ppt_pl1_spl",))
        .unwrap();
    let firmware_reset_plan: WriteDryRunPlan = serde_json::from_str(&payload).unwrap();
    assert_eq!(firmware_reset_plan.method, "SetFirmwareAttribute");
    assert_eq!(firmware_reset_plan.requested_value, "70");

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
            "GetRecentDesktopPowerProfileChanges",
            "GetRecentPlatformProfileChanges",
            "GetRyzenBackendStatus",
            "GetTelemetry",
            "PlanAmdGpuDpmForceLevelWrite",
            "PlanBatteryChargeTypeWrite",
            "PlanConservationModeWrite",
            "PlanCpuBoostWrite",
            "PlanCpuEppWrite",
            "PlanCpuGovernorWrite",
            "PlanCurveOptimizerAllCoreWrite",
            "PlanCustomThermalFanPresetWrite",
            "PlanCustomThermalFirmwareAttributeWrite",
            "PlanCustomThermalFirmwarePptPresetWrite",
            "PlanCustomThermalRestoreAutoFanWrite",
            "PlanFanPresetWrite",
            "PlanFirmwareAttributeResetWrite",
            "PlanFirmwareAttributeWrite",
            "PlanGpuModeRuntimeWrite",
            "PlanGpuModeWrite",
            "PlanIdeapadToggleWrite",
            "PlanKeyboardRgbWrite",
            "PlanLedStateWrite",
            "PlanOpenRgbAccessSetup",
            "PlanOpenRgbKeyboardRgbBridge",
            "PlanOpenRgbKeyboardRgbSdkWrite",
            "PlanPlatformProfileWrite",
            "PlanPrepareCustomThermalMode",
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
            "SetKeyboardRgb",
            "SetLedState",
            "SetPlatformProfile",
            "SetupOpenRgbAccess",
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
    assert!(matches!(
        service.plan_prepare_custom_thermal_mode(),
        Err(legion_control_daemon::PlanningError::Validation(
            legion_common::ValidationError::UnsupportedChoice { .. }
        ))
    ));

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
    assert!(firmware_plan.safety_notes.iter().any(
        |note| note.contains("custom thermal prerequisite unavailable for firmware PPT writes")
    ));

    let firmware_reset_plan = service
        .plan_firmware_attribute_reset_write("ppt_pl2_sppt")
        .unwrap();
    assert_eq!(firmware_reset_plan.method, "SetFirmwareAttribute");
    assert_eq!(firmware_reset_plan.previous_value, "85");
    assert_eq!(firmware_reset_plan.requested_value, "85");
    assert!(firmware_reset_plan
        .safety_notes
        .iter()
        .any(|note| { note.contains("reset-to-default plan uses firmware default_value `85`") }));

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

    let openrgb_setup_plan = service
        .plan_openrgb_access_setup("ratvantage-test")
        .unwrap();
    assert_eq!(openrgb_setup_plan.method, "SetupOpenRgbAccess");
    assert_eq!(
        openrgb_setup_plan.capability_id,
        "keyboard_rgb_openrgb:access_setup"
    );
    assert!(openrgb_setup_plan
        .requested_value
        .contains("group=i2c;module=i2c-dev"));
    assert!(!openrgb_setup_plan.readback_required);
    assert!(service.plan_openrgb_access_setup("root").is_err());

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
    assert!(service.plan_gpu_mode_runtime_write("hybrid").is_err());
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
    assert!(
        fan_plan
            .safety_notes
            .iter()
            .any(|note| note
                .contains("custom thermal prerequisite unavailable for fan preset writes"))
    );

    let restore_plan = service.plan_restore_auto_fan_write().unwrap();
    assert_eq!(restore_plan.method, "RestoreAutoFan");
    assert_eq!(restore_plan.capability_id, "fan_curves");
    assert_eq!(restore_plan.requested_value, "auto/default fan control");
    assert!(restore_plan.safety_notes.iter().any(|note| note
        .contains("custom thermal prerequisite unavailable for fan restore/default writes")));
}

#[test]
fn daemon_prepares_custom_thermal_mode_when_custom_profile_is_listed() {
    let fixture = copied_fixture_root("prepare-custom-thermal");
    fs::write(
        fixture.join("sys/firmware/acpi/platform_profile_choices"),
        "low-power balanced performance max-power custom\n",
    )
    .unwrap();
    fs::write(
        fixture.join("sys/firmware/acpi/platform_profile"),
        "low-power\n",
    )
    .unwrap();

    let service = LegionControl::new(ProbeOptions {
        sysfs_root: fixture.clone(),
    });

    assert!(service.plan_platform_profile_write("custom").is_err());

    let plan = service.plan_prepare_custom_thermal_mode().unwrap();
    assert_eq!(plan.method, "PrepareCustomThermalMode");
    assert_eq!(plan.capability_id, "platform_profile");
    assert_eq!(plan.previous_value, "low-power");
    assert_eq!(plan.requested_value, "custom");
    assert_eq!(plan.rollback_value, "low-power");
    assert_eq!(
        plan.polkit_action,
        "org.ratvantage.LegionControl1.set-platform-profile"
    );
    assert!(plan.readback_required);
    assert!(plan
        .safety_notes
        .iter()
        .any(|note| note.contains("custom thermal mode preparation required")));

    let (bus, connection, proxy) = test_proxy_with_service(service);
    let payload: String = proxy.call("PlanPrepareCustomThermalMode", &()).unwrap();
    let dbus_plan: WriteDryRunPlan = serde_json::from_str(&payload).unwrap();
    assert_eq!(dbus_plan.method, "PrepareCustomThermalMode");
    assert_eq!(dbus_plan.previous_value, "low-power");

    drop(proxy);
    drop(connection);
    drop(bus);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn openrgb_access_setup_is_policy_gated_and_uses_setup_writer() {
    let service = LegionControl::new(ProbeOptions {
        sysfs_root: fixture_root(),
    });
    let blocked = service
        .setup_openrgb_access("ratvantage-test", "test.sender")
        .unwrap();
    assert_eq!(blocked.status, WriteExecutionStatus::BlockedByPolicy);
    assert!(!blocked.applied);
    assert_eq!(blocked.plan.method, "SetupOpenRgbAccess");

    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture_root(),
        },
        unique_state_path("openrgb-access-setup"),
        WriteAccessPolicy {
            openrgb_access_setup_enabled: true,
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
    .with_openrgb_access_setup_writer(Arc::new(RecordingOpenRgbAccessSetupWriter {
        calls: Arc::clone(&calls),
    }));
    let applied = service
        .setup_openrgb_access("ratvantage-test", "test.sender")
        .unwrap();
    assert_eq!(applied.status, WriteExecutionStatus::Applied);
    assert!(applied.applied);
    assert_eq!(calls.lock().unwrap().as_slice(), ["ratvantage-test"]);
    assert!(applied.message.contains("configured ratvantage-test"));
    assert_eq!(
        applied.readback_value.as_deref(),
        Some("user=ratvantage-test;i2c_group_configured=true")
    );
}

#[test]
fn daemon_builds_custom_thermal_firmware_sequence_preview() {
    let fixture = copied_fixture_root("custom-thermal-firmware-sequence");
    fs::write(
        fixture.join("sys/firmware/acpi/platform_profile_choices"),
        "low-power balanced performance max-power custom\n",
    )
    .unwrap();
    fs::write(
        fixture.join("sys/firmware/acpi/platform_profile"),
        "low-power\n",
    )
    .unwrap();

    let service = LegionControl::new(ProbeOptions {
        sysfs_root: fixture.clone(),
    });

    let preview = service
        .plan_custom_thermal_firmware_attribute_write("ppt_pl1_spl", "75")
        .unwrap();
    assert_eq!(preview.sequence_id, "custom_thermal_firmware_attribute");
    assert_eq!(preview.target, "firmware_attribute:ppt_pl1_spl");
    assert_eq!(preview.plans.len(), 2);
    assert_eq!(preview.plans[0].method, "PrepareCustomThermalMode");
    assert_eq!(preview.plans[0].previous_value, "low-power");
    assert_eq!(preview.plans[0].requested_value, "custom");
    assert_eq!(preview.plans[1].method, "SetFirmwareAttribute");
    assert_eq!(preview.plans[1].requested_value, "75");
    assert!(preview.plans[1].safety_notes.iter().any(|note| {
        note.contains("custom thermal prerequisite satisfied for firmware PPT writes")
    }));
    assert!(preview.rollback_order[0].contains("SetFirmwareAttribute rollback"));
    assert!(preview.rollback_order[1].contains("PrepareCustomThermalMode rollback"));

    let (bus, connection, proxy) = test_proxy_with_service(service);
    let payload: String = proxy
        .call(
            "PlanCustomThermalFirmwareAttributeWrite",
            &("ppt_pl1_spl", "75"),
        )
        .unwrap();
    let dbus_preview: CustomThermalPlanPreview = serde_json::from_str(&payload).unwrap();
    assert_eq!(dbus_preview.plans[0].method, "PrepareCustomThermalMode");
    assert_eq!(dbus_preview.plans[1].method, "SetFirmwareAttribute");

    drop(proxy);
    drop(connection);
    drop(bus);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn daemon_builds_custom_thermal_firmware_ppt_preset_sequence_preview() {
    let fixture = copied_fixture_root("custom-thermal-firmware-ppt-preset-sequence");
    fs::write(
        fixture.join("sys/firmware/acpi/platform_profile_choices"),
        "low-power balanced performance max-power custom\n",
    )
    .unwrap();
    fs::write(
        fixture.join("sys/firmware/acpi/platform_profile"),
        "low-power\n",
    )
    .unwrap();

    let service = LegionControl::new(ProbeOptions {
        sysfs_root: fixture.clone(),
    });

    let preview = service
        .plan_custom_thermal_firmware_ppt_preset_write("performance-custom")
        .unwrap();
    assert_eq!(preview.sequence_id, "custom_thermal_firmware_ppt_preset");
    assert_eq!(preview.target, "firmware_ppt_preset:performance-custom");
    assert_eq!(preview.plans.len(), 4);
    assert_eq!(preview.plans[0].method, "PrepareCustomThermalMode");
    assert_eq!(preview.plans[0].previous_value, "low-power");
    assert_eq!(preview.plans[0].requested_value, "custom");
    assert_eq!(preview.plans[1].method, "SetFirmwareAttribute");
    assert_eq!(preview.plans[1].requested_value, "80");
    assert_eq!(preview.plans[2].requested_value, "115");
    assert_eq!(preview.plans[3].requested_value, "135");
    assert!(preview.plans[3].safety_notes.iter().any(|note| {
        note.contains("performance custom PPT preset needs live thermal validation")
    }));
    assert!(preview.rollback_order[0].contains("SetFirmwareAttribute rollback"));
    assert!(preview
        .rollback_order
        .last()
        .unwrap()
        .contains("PrepareCustomThermalMode rollback"));

    let reset_preview = service
        .plan_custom_thermal_firmware_ppt_preset_write("reset-defaults")
        .unwrap();
    assert_eq!(reset_preview.plans[1].requested_value, "70");
    assert_eq!(reset_preview.plans[2].requested_value, "85");
    assert_eq!(reset_preview.plans[3].requested_value, "102");

    let (bus, connection, proxy) = test_proxy_with_service(service);
    let payload: String = proxy
        .call(
            "PlanCustomThermalFirmwarePptPresetWrite",
            &("performance-custom",),
        )
        .unwrap();
    let dbus_preview: CustomThermalPlanPreview = serde_json::from_str(&payload).unwrap();
    assert_eq!(dbus_preview.plans[0].method, "PrepareCustomThermalMode");
    assert_eq!(dbus_preview.plans[1].requested_value, "80");
    assert_eq!(dbus_preview.plans[3].requested_value, "135");

    drop(proxy);
    drop(connection);
    drop(bus);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn daemon_builds_custom_thermal_fan_sequence_preview() {
    let fixture = copied_runtime_fixture_root("custom-thermal-fan-sequence");
    fs::write(
        fixture.join("sys/firmware/acpi/platform_profile_choices"),
        "low-power balanced performance max-power custom\n",
    )
    .unwrap();
    fs::write(
        fixture.join("sys/firmware/acpi/platform_profile"),
        "balanced\n",
    )
    .unwrap();

    let service = LegionControl::new(ProbeOptions {
        sysfs_root: fixture.clone(),
    });

    let preview = service
        .plan_custom_thermal_fan_preset_write("balanced-daily")
        .unwrap();
    assert_eq!(preview.sequence_id, "custom_thermal_fan_preset");
    assert_eq!(preview.plans.len(), 2);
    assert_eq!(preview.plans[0].method, "PrepareCustomThermalMode");
    assert_eq!(preview.plans[1].method, "ApplyFanPreset");
    assert!(preview.plans[1].safety_notes.iter().any(|note| {
        note.contains("custom thermal prerequisite satisfied for fan preset writes")
    }));

    let restore_preview = service.plan_custom_thermal_restore_auto_fan().unwrap();
    assert_eq!(
        restore_preview.sequence_id,
        "custom_thermal_restore_auto_fan"
    );
    assert_eq!(restore_preview.plans[0].method, "PrepareCustomThermalMode");
    assert_eq!(restore_preview.plans[1].method, "RestoreAutoFan");

    let _ = fs::remove_dir_all(fixture);
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
    let profile_json = serde_json::json!({
        "schema_version": 1,
        "label": "Integrated GPU on battery",
        "actions": {
            "gpu_mode": "hybrid"
        }
    })
    .to_string();
    let preview = service
        .set_hardware_profile("integrated_gpu_on_battery", &profile_json)
        .unwrap();
    assert_eq!(
        preview
            .plans
            .iter()
            .map(|plan| plan.method.as_str())
            .collect::<Vec<_>>(),
        ["SetGpuMode"]
    );
    assert_eq!(preview.plans[0].previous_value, "integrated");
    assert_eq!(preview.plans[0].requested_value, "hybrid");

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
            openrgb_access_setup_enabled: false,
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
            openrgb_access_setup_enabled: false,
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
            keyboard_rgb_enabled: false,
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
            openrgb_access_setup_enabled: false,
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
            keyboard_rgb_enabled: false,
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
            openrgb_access_setup_enabled: false,
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
            keyboard_rgb_enabled: false,
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
            openrgb_access_setup_enabled: false,
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
            keyboard_rgb_enabled: false,
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
            openrgb_access_setup_enabled: false,
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
            keyboard_rgb_enabled: false,
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
            openrgb_access_setup_enabled: false,
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
            keyboard_rgb_enabled: false,
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
            openrgb_access_setup_enabled: false,
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
            keyboard_rgb_enabled: false,
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
            openrgb_access_setup_enabled: false,
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
            keyboard_rgb_enabled: false,
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
            openrgb_access_setup_enabled: false,
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
            keyboard_rgb_enabled: false,
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
            openrgb_access_setup_enabled: false,
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
            keyboard_rgb_enabled: false,
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
            openrgb_access_setup_enabled: false,
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
fn keyboard_rgb_write_applies_with_fake_backend_and_readback() {
    let fixture = keyboard_rgb_fixture_root("keyboard-rgb-write-apply");
    let state_path = unique_state_path("keyboard-rgb-write-apply");
    let service = keyboard_rgb_service(
        fixture.clone(),
        &state_path,
        FixtureKeyboardRgbWriter {
            force_mismatch: false,
            fail_rollback: false,
        },
    );
    let (_bus, _service_connection, proxy) = test_proxy_with_service(service);

    let payload: String = proxy
        .call("SetKeyboardRgb", &(keyboard_rgb_request_json(),))
        .unwrap();
    let result: WriteExecutionResult = serde_json::from_str(&payload).unwrap();

    assert_eq!(result.status, WriteExecutionStatus::Applied);
    assert!(result.applied);
    assert_eq!(result.plan.method, "SetKeyboardRgb");
    assert_eq!(result.plan.capability_id, "keyboard_rgb");
    assert_eq!(
        result.readback_value.as_deref(),
        Some("effect=breath;brightness=80;speed=30;colors=left:#333333,right:#444444")
    );
    assert_eq!(
        keyboard_rgb_metadata_current_effect(&fixture).as_deref(),
        Some("breath")
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn keyboard_rgb_write_rolls_back_after_readback_mismatch() {
    let fixture = keyboard_rgb_fixture_root("keyboard-rgb-write-rollback");
    let state_path = unique_state_path("keyboard-rgb-write-rollback");
    let service = keyboard_rgb_service(
        fixture.clone(),
        &state_path,
        FixtureKeyboardRgbWriter {
            force_mismatch: true,
            fail_rollback: false,
        },
    );
    let (_bus, _service_connection, proxy) = test_proxy_with_service(service);

    let payload: String = proxy
        .call("SetKeyboardRgb", &(keyboard_rgb_request_json(),))
        .unwrap();
    let result: WriteExecutionResult = serde_json::from_str(&payload).unwrap();

    assert_eq!(result.status, WriteExecutionStatus::Failed);
    assert!(!result.applied);
    assert!(result.message.contains("restored previous value"));
    assert_eq!(
        result.readback_value.as_deref(),
        Some("effect=static;brightness=40;speed=10;colors=left:#111111,right:#222222")
    );
    assert_eq!(
        keyboard_rgb_metadata_current_effect(&fixture).as_deref(),
        Some("static")
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn keyboard_rgb_write_reports_failed_rollback_after_readback_mismatch() {
    let fixture = keyboard_rgb_fixture_root("keyboard-rgb-write-rollback-failed");
    let state_path = unique_state_path("keyboard-rgb-write-rollback-failed");
    let service = keyboard_rgb_service(
        fixture.clone(),
        &state_path,
        FixtureKeyboardRgbWriter {
            force_mismatch: true,
            fail_rollback: true,
        },
    );
    let (_bus, _service_connection, proxy) = test_proxy_with_service(service);

    let payload: String = proxy
        .call("SetKeyboardRgb", &(keyboard_rgb_request_json(),))
        .unwrap();
    let result: WriteExecutionResult = serde_json::from_str(&payload).unwrap();

    assert_eq!(result.status, WriteExecutionStatus::Failed);
    assert!(!result.applied);
    assert!(result.message.contains("rollback failed"));
    assert_eq!(
        result.readback_value.as_deref(),
        Some("effect=static;brightness=40;speed=10;colors=left:#111111,right:#222222")
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
            keyboard_rgb_enabled: false,
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
            openrgb_access_setup_enabled: false,
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
            keyboard_rgb_enabled: false,
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
            openrgb_access_setup_enabled: false,
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
            keyboard_rgb_enabled: false,
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
            openrgb_access_setup_enabled: false,
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
            keyboard_rgb_enabled: false,
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
            openrgb_access_setup_enabled: false,
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
            keyboard_rgb_enabled: false,
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
            openrgb_access_setup_enabled: false,
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
            keyboard_rgb_enabled: false,
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
            openrgb_access_setup_enabled: false,
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
            keyboard_rgb_enabled: false,
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
            openrgb_access_setup_enabled: false,
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
    assert!(
        plan.safety_notes
            .iter()
            .any(|note| note
                .contains("custom thermal prerequisite unavailable for fan preset writes"))
    );

    let restore_plan = service.plan_restore_auto_fan_write().unwrap();
    assert_eq!(restore_plan.method, "RestoreAutoFan");
    assert_eq!(restore_plan.capability_id, "fan_curves");
    assert_eq!(restore_plan.requested_value, "auto/default fan control");
    assert!(restore_plan.readback_required);
    assert!(!restore_plan.reboot_required);
    assert!(restore_plan.safety_notes.iter().any(|note| note
        .contains("custom thermal prerequisite unavailable for fan restore/default writes")));

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
fn hardware_profile_store_validates_keyboard_rgb_action() {
    let fixture = keyboard_rgb_fixture_root("hardware-profile-keyboard-rgb");
    let state_path = unique_state_path("hardware-profile-keyboard-rgb");
    let service = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
    );
    let profile_json = serde_json::json!({
        "schema_version": 1,
        "label": "Blue breathing",
        "actions": {
            "keyboard_rgb": {
                "effect": "breath",
                "colors": {
                    "left": "#333333",
                    "right": "#444444"
                },
                "brightness": 80,
                "speed": 30
            }
        }
    })
    .to_string();

    let preview = service
        .set_hardware_profile("blue_breathing", &profile_json)
        .unwrap();

    assert_eq!(preview.profile_id, "blue_breathing");
    assert_eq!(preview.profile_label, "Blue breathing");
    assert_eq!(preview.plans.len(), 1);
    assert_eq!(preview.plans[0].method, "SetKeyboardRgb");
    assert_eq!(preview.plans[0].capability_id, "keyboard_rgb");
    assert_eq!(
        preview.plans[0].requested_value,
        "effect=breath;brightness=80;speed=30;colors=left:#333333,right:#444444"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
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
fn apply_hardware_profile_stops_at_keyboard_rgb_policy_gate() {
    let fixture = keyboard_rgb_fixture_root("hardware-profile-keyboard-rgb-policy");
    let state_path = unique_state_path("hardware-profile-keyboard-rgb-policy");
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
        WriteAccessPolicy {
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
        "label": "Blue breathing",
        "actions": {
            "keyboard_rgb": {
                "effect": "breath",
                "colors": {
                    "left": "#333333",
                    "right": "#444444"
                },
                "brightness": 80,
                "speed": 30
            }
        }
    })
    .to_string();

    service
        .set_hardware_profile("blue_breathing", &profile_json)
        .unwrap();
    let run = service
        .apply_hardware_profile("blue_breathing", "test.sender")
        .unwrap();

    assert!(!run.completed);
    assert_eq!(run.profile_id, "blue_breathing");
    assert_eq!(
        run.message,
        "hardware profile apply stopped after first non-applied action"
    );
    assert_eq!(run.results.len(), 1);
    assert_eq!(run.results[0].action_id, "keyboard_rgb");
    assert_eq!(
        run.results[0].result.status,
        WriteExecutionStatus::BlockedByPolicy
    );
    assert_eq!(run.results[0].result.plan.method, "SetKeyboardRgb");
    assert_eq!(service.last_hardware_profile_apply().unwrap(), Some(run));

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn apply_hardware_profile_executes_openrgb_sdk_keyboard_rgb_with_fake_backend() {
    let fixture = openrgb_keyboard_rgb_fixture_root("hardware-profile-openrgb-sdk-apply");
    let state_path = unique_state_path("hardware-profile-openrgb-sdk-apply");
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let service = openrgb_keyboard_rgb_service(
        fixture.clone(),
        &state_path,
        FixtureOpenRgbKeyboardRgbSdkWriter {
            snapshot: Arc::new(std::sync::Mutex::new(openrgb_sdk_before_snapshot())),
            calls: calls.clone(),
            force_mismatch: false,
            fail_restore: false,
        },
    );
    let readiness = service
        .snapshot()
        .unwrap()
        .keyboard_rgb_openrgb
        .expect("OpenRGB fixture must be detected");
    assert!(readiness.backend_ready);
    assert!(readiness.write_support_claimed);
    assert!(readiness.sdk_helper_installed);
    assert!(readiness.sdk_server_running);
    assert!(readiness.sdk_snapshot_supported);
    assert_eq!(readiness.sdk_active_mode.as_deref(), Some("Direct"));
    assert_eq!(
        readiness.sdk_color_zones,
        vec![
            "left_center".to_owned(),
            "left_side".to_owned(),
            "right_center".to_owned(),
            "right_side".to_owned()
        ]
    );

    service
        .set_hardware_profile("openrgb_breathing", &openrgb_keyboard_rgb_profile_json())
        .unwrap();
    let run = service
        .apply_hardware_profile("openrgb_breathing", "test.sender")
        .unwrap();

    assert!(run.completed);
    assert_eq!(run.results.len(), 1);
    assert_eq!(run.results[0].action_id, "keyboard_rgb");
    assert_eq!(run.results[0].result.status, WriteExecutionStatus::Applied);
    assert_eq!(
        run.results[0].result.plan.method,
        "SetOpenRgbKeyboardRgbSdk"
    );
    assert_eq!(
        run.results[0].result.plan.capability_id,
        "keyboard_rgb_openrgb:sdk"
    );
    assert_eq!(
        run.results[0].result.readback_value.as_deref(),
        Some("active_mode=Breathing;colors=left_center:#00FF00,left_side:#FF0000,right_center:#0000FF,right_side:#FFFFFF")
    );
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        &["read", "read", "write:Breathing", "read"]
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn set_keyboard_rgb_falls_back_to_openrgb_sdk_when_native_backend_is_absent() {
    let fixture = openrgb_keyboard_rgb_fixture_root("set-keyboard-rgb-openrgb-sdk");
    let state_path = unique_state_path("set-keyboard-rgb-openrgb-sdk");
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let service = openrgb_keyboard_rgb_service(
        fixture.clone(),
        &state_path,
        FixtureOpenRgbKeyboardRgbSdkWriter {
            snapshot: Arc::new(std::sync::Mutex::new(openrgb_sdk_before_snapshot())),
            calls: calls.clone(),
            force_mismatch: false,
            fail_restore: false,
        },
    );
    let request = openrgb_keyboard_rgb_request();

    let result = service.set_keyboard_rgb(&request, "test.sender").unwrap();

    assert_eq!(result.status, WriteExecutionStatus::Applied);
    assert_eq!(result.plan.method, "SetOpenRgbKeyboardRgbSdk");
    assert_eq!(result.plan.capability_id, "keyboard_rgb_openrgb:sdk");
    assert_eq!(
        result.readback_value.as_deref(),
        Some("active_mode=Breathing;colors=left_center:#00FF00,left_side:#FF0000,right_center:#0000FF,right_side:#FFFFFF")
    );
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        &["read", "read", "write:Breathing", "read"]
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn apply_hardware_profile_openrgb_sdk_uses_command_helper_boundary() {
    let fixture = openrgb_keyboard_rgb_fixture_root("hardware-profile-openrgb-sdk-command");
    let state_path = unique_state_path("hardware-profile-openrgb-sdk-command");
    let helper = FakeOpenRgbSdkCommandHelper::new("openrgb-sdk-command-helper", false);
    let service = openrgb_keyboard_rgb_service_with_writer(
        fixture.clone(),
        &state_path,
        Arc::new(CommandOpenRgbKeyboardRgbSdkWriter::new(
            helper.executable_path(),
        )),
    );
    helper.clear_calls();

    service
        .set_hardware_profile("openrgb_breathing", &openrgb_keyboard_rgb_profile_json())
        .unwrap();
    let run = service
        .apply_hardware_profile("openrgb_breathing", "test.sender")
        .unwrap();

    assert!(run.completed);
    assert_eq!(run.results.len(), 1);
    assert_eq!(run.results[0].result.status, WriteExecutionStatus::Applied);
    assert_eq!(
        run.results[0].result.plan.method,
        "SetOpenRgbKeyboardRgbSdk"
    );
    assert_eq!(
        run.results[0].result.readback_value.as_deref(),
        Some("active_mode=Breathing;colors=left_center:#00FF00,left_side:#FF0000,right_center:#0000FF,right_side:#FFFFFF")
    );
    helper.assert_calls_exact(&["snapshot", "write", "snapshot"]);

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn apply_hardware_profile_openrgb_sdk_rolls_back_after_readback_mismatch() {
    let fixture = openrgb_keyboard_rgb_fixture_root("hardware-profile-openrgb-sdk-rollback");
    let state_path = unique_state_path("hardware-profile-openrgb-sdk-rollback");
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let service = openrgb_keyboard_rgb_service(
        fixture.clone(),
        &state_path,
        FixtureOpenRgbKeyboardRgbSdkWriter {
            snapshot: Arc::new(std::sync::Mutex::new(openrgb_sdk_before_snapshot())),
            calls: calls.clone(),
            force_mismatch: true,
            fail_restore: false,
        },
    );

    service
        .set_hardware_profile("openrgb_breathing", &openrgb_keyboard_rgb_profile_json())
        .unwrap();
    let run = service
        .apply_hardware_profile("openrgb_breathing", "test.sender")
        .unwrap();

    assert!(!run.completed);
    assert_eq!(run.results[0].result.status, WriteExecutionStatus::Failed);
    assert!(run.results[0]
        .result
        .message
        .contains("restored previous snapshot"));
    assert_eq!(
        run.results[0].result.readback_value.as_deref(),
        Some("active_mode=Direct;colors=left_center:#000000,left_side:#000000,right_center:#000000,right_side:#000000")
    );
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        &[
            "read",
            "read",
            "write:Breathing",
            "read",
            "restore:Direct",
            "read"
        ]
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn apply_hardware_profile_openrgb_sdk_reports_failed_rollback() {
    let fixture = openrgb_keyboard_rgb_fixture_root("hardware-profile-openrgb-sdk-rollback-failed");
    let state_path = unique_state_path("hardware-profile-openrgb-sdk-rollback-failed");
    let service = openrgb_keyboard_rgb_service(
        fixture.clone(),
        &state_path,
        FixtureOpenRgbKeyboardRgbSdkWriter {
            snapshot: Arc::new(std::sync::Mutex::new(openrgb_sdk_before_snapshot())),
            calls: Arc::new(std::sync::Mutex::new(Vec::new())),
            force_mismatch: true,
            fail_restore: true,
        },
    );

    service
        .set_hardware_profile("openrgb_breathing", &openrgb_keyboard_rgb_profile_json())
        .unwrap();
    let run = service
        .apply_hardware_profile("openrgb_breathing", "test.sender")
        .unwrap();

    assert!(!run.completed);
    assert_eq!(run.results[0].result.status, WriteExecutionStatus::Failed);
    assert!(run.results[0].result.message.contains("rollback failed"));
    assert_eq!(
        run.results[0].result.readback_value.as_deref(),
        Some("active_mode=Direct;colors=left_center:#00FF00,left_side:#FF0000,right_center:#0000FF,right_side:#FFFFFF")
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
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
            openrgb_access_setup_enabled: false,
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
fn mixed_daily_hardware_profile_preview_combines_safe_actions_and_staged_rgb() {
    let fixture = copied_fixture_root("hardware-profile-mixed-daily-preview");
    let device = fixture.join("sys/class/hidraw/hidraw9/device");
    fs::create_dir_all(&device).unwrap();
    fs::write(
        device.join("uevent"),
        "DRIVER=hid-generic\nHID_ID=0003:0000048D:0000C985\nHID_NAME=ITE Tech. Inc. ITE Device(8295)\nMODALIAS=hid:b0003g0001v0000048Dp0000C985\n",
    )
    .unwrap();
    let capability = serde_json::json!({
        "backend": "hidraw",
        "device_id": "hidraw9",
        "path": fixture.join("sys/class/hidraw/hidraw9").display().to_string(),
        "zones": [
            { "id": "left", "label": "Left" },
            { "id": "right", "label": "Right" }
        ],
        "effects": ["static", "breath"],
        "current_effect": "static",
        "current_colors": {
            "left": "#111111",
            "right": "#222222"
        },
        "current_brightness": 40,
        "min_brightness": 0,
        "max_brightness": 100,
        "current_speed": 10,
        "min_speed": 0,
        "max_speed": 100
    });
    fs::write(
        device.join("ratvantage-keyboard-rgb.json"),
        serde_json::to_string(&capability).unwrap(),
    )
    .unwrap();
    let state_path = unique_state_path("hardware-profile-mixed-daily-preview");
    let service = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
    );
    let profile_json = serde_json::json!({
        "schema_version": 1,
        "label": "Balanced daily mixed",
        "actions": {
            "platform_profile": "balanced",
            "battery_charge_type": "Standard",
            "cpu_governor": "powersave",
            "cpu_epp": "balance_performance",
            "cpu_boost": "1",
            "amd_gpu_dpm_force_level": "auto",
            "keyboard_rgb": {
                "effect": "breath",
                "colors": {
                    "left": "#333333",
                    "right": "#333333"
                },
                "brightness": 40,
                "speed": 30
            }
        }
    })
    .to_string();

    service
        .set_hardware_profile("balanced_daily_mixed", &profile_json)
        .unwrap();
    let preview = service
        .hardware_profile_apply_preview("balanced_daily_mixed")
        .unwrap();
    let methods: Vec<&str> = preview
        .plans
        .iter()
        .map(|plan| plan.method.as_str())
        .collect();

    assert_eq!(
        methods,
        vec![
            "SetPlatformProfile",
            "SetBatteryChargeType",
            "SetKeyboardRgb",
            "SetCpuGovernor",
            "SetCpuEpp",
            "SetCpuBoost",
            "SetAmdGpuDpmForceLevel"
        ]
    );
    assert_eq!(preview.plans[2].capability_id, "keyboard_rgb");
    assert_eq!(
        preview.plans[2].requested_value,
        "effect=breath;brightness=40;speed=30;colors=left:#333333,right:#333333"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn quiet_battery_mixed_profile_preview_combines_low_power_actions_and_staged_rgb() {
    let fixture = copied_fixture_root("hardware-profile-quiet-battery-preview");
    let device = fixture.join("sys/class/hidraw/hidraw9/device");
    fs::create_dir_all(&device).unwrap();
    fs::write(
        device.join("uevent"),
        "DRIVER=hid-generic\nHID_ID=0003:0000048D:0000C985\nHID_NAME=ITE Tech. Inc. ITE Device(8295)\nMODALIAS=hid:b0003g0001v0000048Dp0000C985\n",
    )
    .unwrap();
    let capability = serde_json::json!({
        "backend": "hidraw",
        "device_id": "hidraw9",
        "path": fixture.join("sys/class/hidraw/hidraw9").display().to_string(),
        "zones": [
            { "id": "left", "label": "Left" },
            { "id": "right", "label": "Right" }
        ],
        "effects": ["static", "breath"],
        "current_effect": "static",
        "current_colors": {
            "left": "#111111",
            "right": "#222222"
        },
        "current_brightness": 40,
        "min_brightness": 0,
        "max_brightness": 100,
        "current_speed": 10,
        "min_speed": 0,
        "max_speed": 100
    });
    fs::write(
        device.join("ratvantage-keyboard-rgb.json"),
        serde_json::to_string(&capability).unwrap(),
    )
    .unwrap();
    let state_path = unique_state_path("hardware-profile-quiet-battery-preview");
    let service = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
    );
    let profile_json = serde_json::json!({
        "schema_version": 1,
        "label": "Quiet battery mixed",
        "actions": {
            "platform_profile": "low-power",
            "battery_charge_type": "Conservation",
            "cpu_governor": "powersave",
            "cpu_epp": "power",
            "cpu_boost": "0",
            "amd_gpu_dpm_force_level": "low",
            "keyboard_rgb": {
                "effect": "breath",
                "colors": {
                    "left": "#222222",
                    "right": "#222222"
                },
                "brightness": 20,
                "speed": 20
            }
        }
    })
    .to_string();

    service
        .set_hardware_profile("quiet_battery_mixed", &profile_json)
        .unwrap();
    let preview = service
        .hardware_profile_apply_preview("quiet_battery_mixed")
        .unwrap();
    let methods: Vec<&str> = preview
        .plans
        .iter()
        .map(|plan| plan.method.as_str())
        .collect();

    assert_eq!(
        methods,
        vec![
            "SetPlatformProfile",
            "SetBatteryChargeType",
            "SetKeyboardRgb",
            "SetCpuGovernor",
            "SetCpuEpp",
            "SetCpuBoost",
            "SetAmdGpuDpmForceLevel"
        ]
    );
    assert_eq!(preview.plans[1].requested_value, "Conservation");
    assert_eq!(
        preview.plans[2].requested_value,
        "effect=breath;brightness=20;speed=20;colors=left:#222222,right:#222222"
    );
    assert_eq!(preview.plans[6].requested_value, "low");

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn performance_ac_mixed_profile_preview_combines_performance_actions_and_staged_rgb() {
    let fixture = copied_fixture_root("hardware-profile-performance-ac-preview");
    let device = fixture.join("sys/class/hidraw/hidraw9/device");
    fs::create_dir_all(&device).unwrap();
    fs::write(
        device.join("uevent"),
        "DRIVER=hid-generic\nHID_ID=0003:0000048D:0000C985\nHID_NAME=ITE Tech. Inc. ITE Device(8295)\nMODALIAS=hid:b0003g0001v0000048Dp0000C985\n",
    )
    .unwrap();
    let capability = serde_json::json!({
        "backend": "hidraw",
        "device_id": "hidraw9",
        "path": fixture.join("sys/class/hidraw/hidraw9").display().to_string(),
        "zones": [
            { "id": "left", "label": "Left" },
            { "id": "right", "label": "Right" }
        ],
        "effects": ["static", "breath"],
        "current_effect": "static",
        "current_colors": {
            "left": "#111111",
            "right": "#222222"
        },
        "current_brightness": 40,
        "min_brightness": 0,
        "max_brightness": 100,
        "current_speed": 10,
        "min_speed": 0,
        "max_speed": 100
    });
    fs::write(
        device.join("ratvantage-keyboard-rgb.json"),
        serde_json::to_string(&capability).unwrap(),
    )
    .unwrap();
    let state_path = unique_state_path("hardware-profile-performance-ac-preview");
    let service = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
    );
    let profile_json = serde_json::json!({
        "schema_version": 1,
        "label": "Performance AC mixed",
        "actions": {
            "platform_profile": "performance",
            "battery_charge_type": "Standard",
            "cpu_governor": "performance",
            "cpu_epp": "performance",
            "cpu_boost": "1",
            "amd_gpu_dpm_force_level": "auto",
            "keyboard_rgb": {
                "effect": "breath",
                "colors": {
                    "left": "#555555",
                    "right": "#555555"
                },
                "brightness": 60,
                "speed": 40
            }
        }
    })
    .to_string();

    service
        .set_hardware_profile("performance_ac_mixed", &profile_json)
        .unwrap();
    let preview = service
        .hardware_profile_apply_preview("performance_ac_mixed")
        .unwrap();
    let methods: Vec<&str> = preview
        .plans
        .iter()
        .map(|plan| plan.method.as_str())
        .collect();

    assert_eq!(
        methods,
        vec![
            "SetPlatformProfile",
            "SetBatteryChargeType",
            "SetKeyboardRgb",
            "SetCpuGovernor",
            "SetCpuEpp",
            "SetCpuBoost",
            "SetAmdGpuDpmForceLevel"
        ]
    );
    assert_eq!(preview.plans[0].requested_value, "performance");
    assert_eq!(preview.plans[4].requested_value, "performance");
    assert_eq!(
        preview.plans[2].requested_value,
        "effect=breath;brightness=60;speed=40;colors=left:#555555,right:#555555"
    );
    assert_eq!(preview.plans[6].requested_value, "auto");

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
    let cooldown_secs = match &rules["fast_charge_until_80"].kind {
        AutomationRuleKind::FastChargeUntilThreshold { cooldown_secs, .. } => cooldown_secs,
        other => panic!("unexpected automation rule kind: {other:?}"),
    };
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
fn battery_threshold_automation_selects_profile() {
    let fixture = copied_fixture_root("automation-battery-threshold");
    let state_path = unique_state_path("automation-battery-threshold");
    fs::write(fixture.join("sys/class/power_supply/ADP0/online"), "0\n").unwrap();
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
        WriteAccessPolicy {
            platform_profile_enabled: true,
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
    let low_power = serde_json::json!({
        "schema_version": 1,
        "label": "Low battery quiet",
        "actions": {
            "platform_profile": "low-power"
        }
    })
    .to_string();
    service
        .set_hardware_profile("low_battery_quiet", &low_power)
        .unwrap();
    let rule = serde_json::json!({
        "schema_version": 1,
        "label": "Quiet below 80%",
        "enabled": true,
        "kind": "battery_profile_threshold",
        "threshold_percent": 80,
        "profile_id": "low_battery_quiet",
        "when_below_or_equal": true,
        "require_ac": false,
        "cooldown_secs": 60
    })
    .to_string();

    service
        .set_automation_rule("quiet_below_80", &rule)
        .unwrap();
    let preview = service.automation_rule_preview("quiet_below_80").unwrap();
    assert!(preview.matched);
    assert_eq!(preview.battery_capacity_percent, Some(79));
    assert_eq!(preview.ac_online, Some(false));
    assert_eq!(
        preview.selected_profile_id.as_deref(),
        Some("low_battery_quiet")
    );
    assert!(preview.reason.contains("at or below threshold 80%"));
    assert_eq!(
        preview
            .profile_preview
            .as_ref()
            .unwrap()
            .plans
            .first()
            .unwrap()
            .method,
        "SetPlatformProfile"
    );

    let run = service
        .apply_automation_rule("quiet_below_80", "test.sender")
        .unwrap();
    assert!(run.profile_run.as_ref().unwrap().completed);
    assert_eq!(
        fs::read_to_string(fixture.join("sys/firmware/acpi/platform_profile"))
            .unwrap()
            .trim(),
        "low-power"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn periodic_idle_automation_reapplies_profile_with_cooldown() {
    let fixture = copied_fixture_root("automation-periodic-idle");
    let state_path = unique_state_path("automation-periodic-idle");
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
        WriteAccessPolicy {
            platform_profile_enabled: true,
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
    let repair_profile = serde_json::json!({
        "schema_version": 1,
        "label": "Periodic repair",
        "actions": {
            "platform_profile": "low-power"
        }
    })
    .to_string();
    service
        .set_hardware_profile("periodic_repair", &repair_profile)
        .unwrap();
    let rule = serde_json::json!({
        "schema_version": 1,
        "label": "Periodic idle correction",
        "enabled": true,
        "kind": "periodic_idle",
        "profile_id": "periodic_repair",
        "cooldown_secs": 300
    })
    .to_string();
    service
        .set_automation_rule("periodic_idle_correction", &rule)
        .unwrap();

    let preview = service
        .automation_rule_preview("periodic_idle_correction")
        .unwrap();
    assert!(preview.matched);
    assert_eq!(
        preview.selected_profile_id.as_deref(),
        Some("periodic_repair")
    );
    assert!(preview.reason.contains("periodic idle correction"));

    let runs = legion_control_daemon::handle_automation_observer_tick(
        &state_path,
        &ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        WriteAccessPolicy {
            platform_profile_enabled: true,
            hardware_profile_apply_enabled: true,
            ..Default::default()
        },
        300,
    )
    .unwrap();
    assert_eq!(runs.len(), 1);
    assert!(runs[0].profile_run.as_ref().unwrap().completed);
    assert_eq!(
        fs::read_to_string(fixture.join("sys/firmware/acpi/platform_profile"))
            .unwrap()
            .trim(),
        "low-power"
    );

    let runs = legion_control_daemon::handle_automation_observer_tick(
        &state_path,
        &ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        WriteAccessPolicy {
            platform_profile_enabled: true,
            hardware_profile_apply_enabled: true,
            ..Default::default()
        },
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
fn ac_profile_router_rule_selects_profile_from_ac_telemetry() {
    let fixture = copied_fixture_root("automation-ac-profile-router");
    let state_path = unique_state_path("automation-ac-profile-router");
    let service = LegionControl::new_with_runtime(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
        WriteAccessPolicy {
            platform_profile_enabled: true,
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
    let ac_profile = serde_json::json!({
        "schema_version": 1,
        "label": "Plugged in",
        "actions": {
            "platform_profile": "performance"
        }
    })
    .to_string();
    let battery_profile = serde_json::json!({
        "schema_version": 1,
        "label": "On battery",
        "actions": {
            "platform_profile": "low-power"
        }
    })
    .to_string();
    service
        .set_hardware_profile("plugged_in", &ac_profile)
        .unwrap();
    service
        .set_hardware_profile("on_battery", &battery_profile)
        .unwrap();
    let rule = serde_json::json!({
        "schema_version": 1,
        "label": "AC profile router",
        "enabled": true,
        "kind": "ac_profile_router",
        "ac_profile_id": "plugged_in",
        "battery_profile_id": "on_battery",
        "cooldown_secs": 120
    })
    .to_string();
    service.set_automation_rule("ac_router", &rule).unwrap();

    let preview = service.automation_rule_preview("ac_router").unwrap();
    assert!(preview.matched);
    assert_eq!(preview.ac_online, Some(true));
    assert_eq!(preview.selected_profile_id.as_deref(), Some("plugged_in"));
    assert!(preview.reason.contains("AC adapter is online"));
    assert_eq!(
        preview.profile_preview.unwrap().plans[0].method,
        "SetPlatformProfile"
    );

    let run = service
        .apply_automation_rule("ac_router", "test.sender")
        .unwrap();
    assert!(run.profile_run.as_ref().unwrap().completed);
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
fn ac_profile_router_preview_includes_cpu_tuning_actions() {
    let fixture = copied_fixture_root("automation-ac-cpu-profile-router");
    let state_path = unique_state_path("automation-ac-cpu-profile-router");
    let service = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
    );
    let ac_profile = serde_json::json!({
        "schema_version": 1,
        "label": "AC CPU performance",
        "actions": {
            "platform_profile": "performance",
            "cpu_governor": "performance",
            "cpu_epp": "performance",
            "cpu_boost": "1"
        }
    })
    .to_string();
    let battery_profile = serde_json::json!({
        "schema_version": 1,
        "label": "Battery CPU efficiency",
        "actions": {
            "platform_profile": "low-power",
            "cpu_governor": "powersave",
            "cpu_epp": "power",
            "cpu_boost": "0"
        }
    })
    .to_string();
    service
        .set_hardware_profile("ac_cpu_performance", &ac_profile)
        .unwrap();
    service
        .set_hardware_profile("battery_cpu_efficiency", &battery_profile)
        .unwrap();
    let rule = serde_json::json!({
        "schema_version": 1,
        "label": "AC CPU performance router",
        "enabled": true,
        "kind": "ac_profile_router",
        "ac_profile_id": "ac_cpu_performance",
        "battery_profile_id": "battery_cpu_efficiency",
        "cooldown_secs": 300
    })
    .to_string();
    service.set_automation_rule("ac_cpu_router", &rule).unwrap();

    let preview = service.automation_rule_preview("ac_cpu_router").unwrap();
    assert!(preview.matched);
    assert_eq!(preview.ac_online, Some(true));
    assert_eq!(
        preview.selected_profile_id.as_deref(),
        Some("ac_cpu_performance")
    );
    let methods: Vec<&str> = preview
        .profile_preview
        .as_ref()
        .unwrap()
        .plans
        .iter()
        .map(|plan| plan.method.as_str())
        .collect();
    assert_eq!(
        methods,
        vec![
            "SetPlatformProfile",
            "SetCpuGovernor",
            "SetCpuEpp",
            "SetCpuBoost"
        ]
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn platform_profile_change_observer_applies_mapped_trigger_once() {
    let fixture = copied_fixture_root("platform-profile-change-observer");
    let state_path = unique_state_path("platform-profile-change-observer");
    let service = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
    );
    let profile_json = serde_json::json!({
        "schema_version": 1,
        "label": "Profile changed battery response",
        "actions": {
            "battery_charge_type": "Fast"
        }
    })
    .to_string();
    service
        .set_hardware_profile("profile_changed_response", &profile_json)
        .unwrap();
    service
        .set_hardware_profile_trigger("platform_profile_changed", "profile_changed_response")
        .unwrap();

    let policy = WriteAccessPolicy {
        battery_charge_type_enabled: true,
        hardware_profile_apply_enabled: true,
        ..Default::default()
    };
    let first = legion_control_daemon::handle_platform_profile_change_observer_tick(
        &state_path,
        &ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        policy.clone(),
    )
    .unwrap();
    assert!(first.is_none());

    fs::write(
        fixture.join("sys/firmware/acpi/platform_profile"),
        "performance\n",
    )
    .unwrap();
    let second = legion_control_daemon::handle_platform_profile_change_observer_tick(
        &state_path,
        &ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        policy.clone(),
    )
    .unwrap()
    .unwrap();
    assert!(second.completed);
    assert_eq!(second.profile_id, "profile_changed_response");
    assert_eq!(
        fs::read_to_string(fixture.join("sys/class/power_supply/BAT0/charge_type"))
            .unwrap()
            .trim(),
        "Fast"
    );
    let history = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
    )
    .recent_platform_profile_changes()
    .unwrap();
    assert_eq!(
        history,
        vec![PlatformProfileChangeEvent {
            timestamp_unix_secs: history[0].timestamp_unix_secs,
            previous_profile: "balanced".to_owned(),
            current_profile: "performance".to_owned(),
            source: "platform_profile_observer".to_owned(),
        }]
    );

    let third = legion_control_daemon::handle_platform_profile_change_observer_tick(
        &state_path,
        &ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        policy,
    )
    .unwrap();
    assert!(third.is_none());
    assert_eq!(
        LegionControl::new_with_state_path(
            ProbeOptions {
                sysfs_root: fixture.clone(),
            },
            &state_path,
        )
        .recent_platform_profile_changes()
        .unwrap()
        .len(),
        1
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn desktop_power_profile_change_observer_applies_mapped_trigger_once() {
    let fixture = copied_fixture_root("desktop-power-profile-change-observer");
    let state_path = unique_state_path("desktop-power-profile-change-observer");
    let service = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
    );
    let profile_json = serde_json::json!({
        "schema_version": 1,
        "label": "Desktop power profile response",
        "actions": {
            "battery_charge_type": "Fast"
        }
    })
    .to_string();
    service
        .set_hardware_profile("desktop_power_response", &profile_json)
        .unwrap();
    service
        .set_hardware_profile_trigger("desktop_power_profile_changed", "desktop_power_response")
        .unwrap();

    let policy = WriteAccessPolicy {
        battery_charge_type_enabled: true,
        hardware_profile_apply_enabled: true,
        ..Default::default()
    };
    let first =
        legion_control_daemon::handle_desktop_power_profile_change_observer_tick_with_current(
            &state_path,
            &ProbeOptions {
                sysfs_root: fixture.clone(),
            },
            policy.clone(),
            "balanced".to_owned(),
        )
        .unwrap();
    assert!(first.is_none());

    let second =
        legion_control_daemon::handle_desktop_power_profile_change_observer_tick_with_current(
            &state_path,
            &ProbeOptions {
                sysfs_root: fixture.clone(),
            },
            policy.clone(),
            "power-saver".to_owned(),
        )
        .unwrap()
        .unwrap();
    assert!(second.completed);
    assert_eq!(second.profile_id, "desktop_power_response");
    assert_eq!(
        fs::read_to_string(fixture.join("sys/class/power_supply/BAT0/charge_type"))
            .unwrap()
            .trim(),
        "Fast"
    );
    let history = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
    )
    .recent_desktop_power_profile_changes()
    .unwrap();
    assert_eq!(
        history,
        vec![DesktopPowerProfileChangeEvent {
            timestamp_unix_secs: history[0].timestamp_unix_secs,
            previous_profile: "balanced".to_owned(),
            current_profile: "power-saver".to_owned(),
            source: "desktop_power_profile_observer".to_owned(),
        }]
    );

    let third =
        legion_control_daemon::handle_desktop_power_profile_change_observer_tick_with_current(
            &state_path,
            &ProbeOptions {
                sysfs_root: fixture.clone(),
            },
            policy,
            "power-saver".to_owned(),
        )
        .unwrap();
    assert!(third.is_none());
    assert_eq!(
        LegionControl::new_with_state_path(
            ProbeOptions {
                sysfs_root: fixture.clone(),
            },
            &state_path,
        )
        .recent_desktop_power_profile_changes()
        .unwrap()
        .len(),
        1
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fixture);
}

#[test]
fn resume_observer_applies_mapped_hardware_profile_trigger() {
    let fixture = copied_fixture_root("resume-hardware-profile-trigger");
    let state_path = unique_state_path("resume-hardware-profile-trigger");
    let service = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        &state_path,
    );
    let profile_json = serde_json::json!({
        "schema_version": 1,
        "label": "Resume battery response",
        "actions": {
            "battery_charge_type": "Fast"
        }
    })
    .to_string();
    service
        .set_hardware_profile("resume_fast", &profile_json)
        .unwrap();
    service
        .set_hardware_profile_trigger("resume", "resume_fast")
        .unwrap();

    let run = legion_control_daemon::handle_login1_resume_tick(
        &state_path,
        &ProbeOptions {
            sysfs_root: fixture.clone(),
        },
        WriteAccessPolicy {
            battery_charge_type_enabled: true,
            hardware_profile_apply_enabled: true,
            ..Default::default()
        },
    )
    .unwrap()
    .unwrap();

    assert!(run.completed);
    assert_eq!(run.profile_id, "resume_fast");
    assert_eq!(
        fs::read_to_string(fixture.join("sys/class/power_supply/BAT0/charge_type"))
            .unwrap()
            .trim(),
        "Fast"
    );
    assert_eq!(
        LegionControl::new_with_state_path(
            ProbeOptions {
                sysfs_root: fixture.clone(),
            },
            &state_path,
        )
        .last_hardware_profile_apply()
        .unwrap()
        .as_ref()
        .map(|run| run.profile_id.as_str()),
        Some("resume_fast")
    );

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
            openrgb_access_setup_enabled: false,
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
            openrgb_access_setup_enabled: false,
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

#[test]
fn gpu_mode_reboot_completion_observer_clears_pending_and_applies_mapped_trigger() {
    let _guard = ENV_MUTEX.lock().unwrap();
    let state_path = unique_state_path("gpu-reboot-completed-trigger");
    let fake_dir = unique_temp_dir("gpu-reboot-completed-envycontrol");
    let fake_bin = fake_dir.join("envycontrol");
    fs::create_dir_all(&fake_dir).unwrap();
    fs::write(
        &fake_bin,
        "#!/bin/sh\nif [ \"$1\" = \"--query\" ]; then echo 'Current GPU mode: hybrid'; exit 0; fi\necho unexpected args >&2\nexit 2\n",
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
            sysfs_root: "/".into(),
        },
        &state_path,
    );
    service
        .set_hardware_profile(
            "gpu_reboot_repair",
            &serde_json::json!({
                "schema_version": 1,
                "label": "GPU reboot repair",
                "actions": {}
            })
            .to_string(),
        )
        .unwrap();
    service
        .set_hardware_profile_trigger("gpu_mode_reboot_completed", "gpu_reboot_repair")
        .unwrap();

    let run = legion_control_daemon::handle_gpu_mode_reboot_completion_observer_tick(
        &state_path,
        &ProbeOptions {
            sysfs_root: "/".into(),
        },
        WriteAccessPolicy {
            hardware_profile_apply_enabled: true,
            ..Default::default()
        },
    )
    .unwrap()
    .unwrap();
    assert!(run.completed);
    assert_eq!(run.profile_id, "gpu_reboot_repair");

    let reloaded = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: "/".into(),
        },
        &state_path,
    );
    assert_eq!(reloaded.gpu_mode_pending().unwrap(), None);
    assert_eq!(
        reloaded
            .last_hardware_profile_apply()
            .unwrap()
            .as_ref()
            .map(|run| run.profile_id.as_str()),
        Some("gpu_reboot_repair")
    );

    match old_path {
        Some(path) => env::set_var("PATH", path),
        None => env::remove_var("PATH"),
    }
    let _ = fs::remove_file(state_path);
    let _ = fs::remove_dir_all(fake_dir);
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

fn keyboard_rgb_fixture_root(label: &str) -> std::path::PathBuf {
    let root = unique_temp_dir(label);
    let _ = fs::remove_dir_all(&root);
    let device = root.join("sys/class/hidraw/hidraw9/device");
    fs::create_dir_all(&device).unwrap();
    fs::write(
        device.join("uevent"),
        "DRIVER=hid-generic\nHID_ID=0003:0000048D:0000C985\nHID_NAME=ITE Tech. Inc. ITE Device(8295)\nMODALIAS=hid:b0003g0001v0000048Dp0000C985\n",
    )
    .unwrap();
    let capability = serde_json::json!({
        "backend": "hidraw",
        "device_id": "hidraw9",
        "path": root.join("sys/class/hidraw/hidraw9").display().to_string(),
        "zones": [
            { "id": "left", "label": "Left" },
            { "id": "right", "label": "Right" }
        ],
        "effects": ["static", "breath"],
        "current_effect": "static",
        "current_colors": {
            "left": "#111111",
            "right": "#222222"
        },
        "current_brightness": 40,
        "min_brightness": 0,
        "max_brightness": 100,
        "current_speed": 10,
        "min_speed": 0,
        "max_speed": 100
    });
    fs::write(
        keyboard_rgb_metadata_path(&root),
        serde_json::to_string(&capability).unwrap(),
    )
    .unwrap();
    root
}

fn keyboard_rgb_metadata_path(root: &std::path::Path) -> std::path::PathBuf {
    root.join("sys/class/hidraw/hidraw9/device/ratvantage-keyboard-rgb.json")
}

fn keyboard_rgb_metadata_current_effect(root: &std::path::Path) -> Option<String> {
    let raw = fs::read_to_string(keyboard_rgb_metadata_path(root)).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    value["current_effect"].as_str().map(str::to_owned)
}

fn keyboard_rgb_request_json() -> &'static str {
    r##"{"effect":"breath","colors":{"left":"#333333","right":"#444444"},"brightness":80,"speed":30}"##
}

fn keyboard_rgb_service(
    root: std::path::PathBuf,
    state_path: &std::path::Path,
    writer: FixtureKeyboardRgbWriter,
) -> LegionControl {
    LegionControl::new_with_runtime(
        ProbeOptions { sysfs_root: root },
        state_path,
        WriteAccessPolicy {
            keyboard_rgb_enabled: true,
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
    .with_keyboard_rgb_writer(Arc::new(writer))
}

fn openrgb_keyboard_rgb_fixture_root(label: &str) -> std::path::PathBuf {
    let root = unique_temp_dir(label);
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let status = KeyboardRgbOpenRgbStatus {
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
        user_in_i2c_group: false,
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
    };
    fs::write(
        root.join("ratvantage-keyboard-rgb-openrgb.json"),
        serde_json::to_string(&status).unwrap(),
    )
    .unwrap();
    root
}

fn openrgb_keyboard_rgb_profile_json() -> String {
    serde_json::json!({
        "schema_version": 1,
        "label": "OpenRGB breathing",
        "actions": {
            "keyboard_rgb": {
                "effect": "Breathing",
                "colors": {
                    "left_side": "#ff0000",
                    "left_center": "#00ff00",
                    "right_center": "#0000ff",
                    "right_side": "#ffffff"
                },
                "brightness": 75,
                "speed": 30
            }
        }
    })
    .to_string()
}

fn openrgb_keyboard_rgb_request() -> KeyboardRgbWriteRequest {
    KeyboardRgbWriteRequest {
        effect: "Breathing".to_owned(),
        colors: BTreeMap::from([
            ("left_side".to_owned(), "#ff0000".to_owned()),
            ("left_center".to_owned(), "#00ff00".to_owned()),
            ("right_center".to_owned(), "#0000ff".to_owned()),
            ("right_side".to_owned(), "#ffffff".to_owned()),
        ]),
        brightness: 75,
        speed: Some(30),
    }
}

fn openrgb_keyboard_rgb_service(
    root: std::path::PathBuf,
    state_path: &std::path::Path,
    writer: FixtureOpenRgbKeyboardRgbSdkWriter,
) -> LegionControl {
    openrgb_keyboard_rgb_service_with_writer(root, state_path, Arc::new(writer))
}

fn openrgb_keyboard_rgb_service_with_writer(
    root: std::path::PathBuf,
    state_path: &std::path::Path,
    writer: Arc<dyn OpenRgbKeyboardRgbSdkWriter>,
) -> LegionControl {
    LegionControl::new_with_runtime(
        ProbeOptions { sysfs_root: root },
        state_path,
        WriteAccessPolicy {
            keyboard_rgb_enabled: true,
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
    .with_openrgb_keyboard_rgb_sdk_writer(writer)
}

fn openrgb_sdk_before_snapshot() -> OpenRgbKeyboardRgbSdkSnapshot {
    OpenRgbKeyboardRgbSdkSnapshot {
        active_mode: "Direct".to_owned(),
        colors: BTreeMap::from([
            ("left_side".to_owned(), "#000000".to_owned()),
            ("left_center".to_owned(), "#000000".to_owned()),
            ("right_center".to_owned(), "#000000".to_owned()),
            ("right_side".to_owned(), "#000000".to_owned()),
        ]),
    }
}

fn openrgb_sdk_command_helper(root: &std::path::Path, force_mismatch: bool) -> std::path::PathBuf {
    let _ = fs::remove_dir_all(root);
    fs::create_dir_all(root).unwrap();
    let state_path = root.join("state.json");
    let calls_path = root.join("calls.log");
    fs::write(
        &state_path,
        serde_json::to_string(&openrgb_sdk_before_snapshot()).unwrap(),
    )
    .unwrap();
    let helper_path = root.join("openrgb-sdk-helper.py");
    let script = format!(
        r#"#!/usr/bin/env python3
import json
import pathlib
import sys

state_path = pathlib.Path({state_path:?})
calls_path = pathlib.Path({calls_path:?})
force_mismatch = {force_mismatch}
action = sys.argv[1]

def log(value):
    with calls_path.open("a", encoding="utf-8") as handle:
        handle.write(value + "\n")

if action == "snapshot":
    log("snapshot")
    print(state_path.read_text())
elif action == "write":
    log("write")
    request = json.loads(sys.argv[3])
    state = {{
        "active_mode": "Direct" if force_mismatch else request["effect"],
        "colors": request["colors"],
    }}
    state_path.write_text(json.dumps(state))
elif action == "restore":
    log("restore")
    state_path.write_text(sys.argv[3])
else:
    raise SystemExit(f"unexpected action {{action}}")
"#,
        state_path = state_path.display().to_string(),
        calls_path = calls_path.display().to_string(),
        force_mismatch = if force_mismatch { "True" } else { "False" }
    );
    fs::write(&helper_path, script).unwrap();
    let mut permissions = fs::metadata(&helper_path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&helper_path, permissions).unwrap();
    helper_path
}

struct FakeOpenRgbSdkCommandHelper {
    root: std::path::PathBuf,
    executable_path: std::path::PathBuf,
    calls_log: std::path::PathBuf,
}

impl FakeOpenRgbSdkCommandHelper {
    fn new(label: &str, force_mismatch: bool) -> Self {
        let root = unique_temp_dir(label);
        let executable_path = openrgb_sdk_command_helper(&root, force_mismatch);
        let calls_log = root.join("calls.log");
        Self {
            root,
            executable_path,
            calls_log,
        }
    }

    fn executable_path(&self) -> std::path::PathBuf {
        self.executable_path.clone()
    }

    fn clear_calls(&self) {
        fs::write(&self.calls_log, "").unwrap();
    }

    fn read_calls(&self) -> Vec<String> {
        fs::read_to_string(&self.calls_log)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn assert_calls_exact(&self, expected: &[&str]) {
        let actual = self.read_calls();
        let expected = expected
            .iter()
            .map(|call| (*call).to_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            actual, expected,
            "OpenRGB SDK command helper calls did not match expected boundary"
        );
    }
}

impl Drop for FakeOpenRgbSdkCommandHelper {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct AllowAllAuthorizer;

impl WriteAuthorizer for AllowAllAuthorizer {
    fn authorize(&self, _action: &str, _sender: &str) -> std::result::Result<(), String> {
        Ok(())
    }
}

struct RecordingOpenRgbAccessSetupWriter {
    calls: Arc<std::sync::Mutex<Vec<String>>>,
}

impl OpenRgbAccessSetupWriter for RecordingOpenRgbAccessSetupWriter {
    fn setup_openrgb_access(&self, target_user: &str) -> std::result::Result<String, String> {
        self.calls.lock().unwrap().push(target_user.to_owned());
        Ok(format!("configured {target_user}"))
    }
}

struct FixtureKeyboardRgbWriter {
    force_mismatch: bool,
    fail_rollback: bool,
}

impl KeyboardRgbWriter for FixtureKeyboardRgbWriter {
    fn write_keyboard_rgb(
        &self,
        path: &str,
        request: &KeyboardRgbWriteRequest,
    ) -> std::result::Result<(), String> {
        if self.force_mismatch && request.effect != "static" {
            return Ok(());
        }
        if self.fail_rollback && request.effect == "static" {
            return Err("simulated rollback failure".to_owned());
        }

        let metadata_path = std::path::Path::new(path)
            .join("device")
            .join("ratvantage-keyboard-rgb.json");
        let raw = fs::read_to_string(&metadata_path).map_err(|error| error.to_string())?;
        let mut value: serde_json::Value =
            serde_json::from_str(&raw).map_err(|error| error.to_string())?;
        value["current_effect"] = serde_json::json!(request.effect);
        value["current_colors"] =
            serde_json::to_value(&request.colors).map_err(|error| error.to_string())?;
        value["current_brightness"] = serde_json::json!(request.brightness);
        value["current_speed"] = match request.speed {
            Some(speed) => serde_json::json!(speed),
            None => serde_json::Value::Null,
        };
        fs::write(&metadata_path, serde_json::to_string(&value).unwrap())
            .map_err(|error| error.to_string())
    }
}

struct FixtureOpenRgbKeyboardRgbSdkWriter {
    snapshot: Arc<std::sync::Mutex<OpenRgbKeyboardRgbSdkSnapshot>>,
    calls: Arc<std::sync::Mutex<Vec<String>>>,
    force_mismatch: bool,
    fail_restore: bool,
}

impl OpenRgbKeyboardRgbSdkWriter for FixtureOpenRgbKeyboardRgbSdkWriter {
    fn read_keyboard_rgb_snapshot(
        &self,
        _path: &str,
    ) -> std::result::Result<OpenRgbKeyboardRgbSdkSnapshot, String> {
        self.calls.lock().unwrap().push("read".to_owned());
        Ok(self.snapshot.lock().unwrap().clone())
    }

    fn write_keyboard_rgb(
        &self,
        _path: &str,
        request: &KeyboardRgbWriteRequest,
    ) -> std::result::Result<(), String> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("write:{}", request.effect));
        let mut snapshot = self.snapshot.lock().unwrap();
        snapshot.colors = request.colors.clone();
        if !self.force_mismatch {
            snapshot.active_mode = request.effect.clone();
        }
        Ok(())
    }

    fn restore_keyboard_rgb_snapshot(
        &self,
        _path: &str,
        snapshot: &OpenRgbKeyboardRgbSdkSnapshot,
    ) -> std::result::Result<(), String> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("restore:{}", snapshot.active_mode));
        if self.fail_restore {
            return Err("simulated SDK restore failure".to_owned());
        }
        *self.snapshot.lock().unwrap() = snapshot.clone();
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
