use legion_common::{
    CapabilityRegistry, CapabilityStatus, FanCurveSnapshot, GpuModePending,
    HardwareProfileApplyRun, HwmonSensor,
};
use legion_control_ui::{GpuSwitchingDiagnostics, UiStatus};

struct TooltipTelemetry<'a> {
    platform_profile: Option<&'a str>,
    desktop_power_profile: Option<&'a str>,
    fan_rpm: Option<&'a str>,
    gpu_pending_reboot: Option<&'a str>,
    gpu_switching: Option<&'a str>,
    fan_curve_snapshot: Option<&'a str>,
    hardware_profile_apply: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraySummary {
    pub title: String,
    pub tooltip: String,
    pub capability_count: usize,
    pub available_capability_count: usize,
    pub missing_capability_count: usize,
    pub capability_ids: Vec<String>,
    pub platform_profile: Option<String>,
    pub desktop_power_profile: Option<String>,
    pub fan_rpm: Option<String>,
    pub gpu_pending_reboot: Option<String>,
    pub gpu_switching: Option<String>,
    pub fan_curve_snapshot: Option<String>,
    pub hardware_profile_apply: Option<String>,
}

impl TraySummary {
    pub fn from_status(status: &UiStatus) -> Self {
        let capability_ids = status
            .capability_ids()
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let missing_capability_count = status
            .capabilities
            .iter()
            .filter(|capability| capability.status == CapabilityStatus::Missing)
            .count();
        let available_capability_count = status.capability_count() - missing_capability_count;

        Self {
            title: "Legion Control".to_owned(),
            tooltip: capability_tooltip(
                &status.hardware.product_name,
                &status.hardware.product_version,
                available_capability_count,
                missing_capability_count,
                TooltipTelemetry {
                    platform_profile: None,
                    desktop_power_profile: None,
                    fan_rpm: None,
                    gpu_pending_reboot: None,
                    gpu_switching: None,
                    fan_curve_snapshot: None,
                    hardware_profile_apply: None,
                },
            ),
            capability_count: status.capability_count(),
            available_capability_count,
            missing_capability_count,
            capability_ids,
            platform_profile: None,
            desktop_power_profile: None,
            fan_rpm: None,
            gpu_pending_reboot: None,
            gpu_switching: None,
            fan_curve_snapshot: None,
            hardware_profile_apply: None,
        }
    }

    pub fn from_status_and_report(
        status: &UiStatus,
        report: &CapabilityRegistry,
        gpu_pending: Option<&GpuModePending>,
        gpu_switching: Option<&GpuSwitchingDiagnostics>,
        fan_snapshot: Option<&FanCurveSnapshot>,
        hardware_profile_apply: Option<&HardwareProfileApplyRun>,
    ) -> Self {
        let mut summary = Self::from_status(status);
        summary.platform_profile = report
            .platform_profile
            .as_ref()
            .and_then(|profile| profile.current.clone());
        summary.desktop_power_profile = report
            .power_profiles
            .as_ref()
            .and_then(|profile| profile.active_profile.clone());
        summary.fan_rpm = fan_rpm_label(&report.telemetry.sensors);
        summary.gpu_pending_reboot = gpu_pending
            .map(|pending| legion_common::format_gpu_mode_pending_summary(Some(pending)));
        summary.gpu_switching = gpu_switching.and_then(gpu_switching_summary);
        summary.fan_curve_snapshot = fan_snapshot
            .map(|snapshot| legion_common::format_fan_curve_snapshot_summary(Some(snapshot)));
        summary.hardware_profile_apply = hardware_profile_apply
            .map(|run| legion_common::format_hardware_profile_apply_run_summary(Some(run)));
        summary.tooltip = capability_tooltip(
            &status.hardware.product_name,
            &status.hardware.product_version,
            summary.available_capability_count,
            summary.missing_capability_count,
            TooltipTelemetry {
                platform_profile: summary.platform_profile.as_deref(),
                desktop_power_profile: summary.desktop_power_profile.as_deref(),
                fan_rpm: summary.fan_rpm.as_deref(),
                gpu_pending_reboot: summary.gpu_pending_reboot.as_deref(),
                gpu_switching: summary.gpu_switching.as_deref(),
                fan_curve_snapshot: summary.fan_curve_snapshot.as_deref(),
                hardware_profile_apply: summary.hardware_profile_apply.as_deref(),
            },
        );
        summary
    }

    pub fn render_lines(&self) -> Vec<String> {
        let mut lines = vec![
            "Legion Control tray status".to_owned(),
            format!("title={}", self.title),
            format!("tooltip={}", self.tooltip),
            format!("capability_count={}", self.capability_count),
            format!(
                "available_capability_count={}",
                self.available_capability_count
            ),
            format!("missing_capability_count={}", self.missing_capability_count),
            format!("capabilities={}", self.capability_ids.join(",")),
        ];
        if let Some(profile) = &self.platform_profile {
            lines.push(format!("platform_profile={profile}"));
        }
        if let Some(profile) = &self.desktop_power_profile {
            lines.push(format!("desktop_power_profile={profile}"));
        }
        if let Some(fan_rpm) = &self.fan_rpm {
            lines.push(format!("fan_rpm={fan_rpm}"));
        }
        if let Some(gpu_pending_reboot) = &self.gpu_pending_reboot {
            lines.push(format!("gpu_pending_reboot={gpu_pending_reboot}"));
        }
        if let Some(gpu_switching) = &self.gpu_switching {
            lines.push(format!("gpu_switching={gpu_switching}"));
        }
        if let Some(fan_curve_snapshot) = &self.fan_curve_snapshot {
            lines.push(format!("last_known_good_fan_curve={fan_curve_snapshot}"));
        }
        if let Some(hardware_profile_apply) = &self.hardware_profile_apply {
            lines.push(format!(
                "last_hardware_profile_apply={hardware_profile_apply}"
            ));
        }
        lines
    }
}

fn capability_tooltip(
    product_name: &str,
    product_version: &str,
    available_count: usize,
    missing_count: usize,
    telemetry_state: TooltipTelemetry<'_>,
) -> String {
    let mut telemetry = Vec::new();
    if let Some(profile) = telemetry_state.platform_profile {
        telemetry.push(format!("Platform: {profile}"));
    }
    if let Some(profile) = telemetry_state.desktop_power_profile {
        telemetry.push(format!("Power: {profile}"));
    }
    if let Some(fan_rpm) = telemetry_state.fan_rpm {
        telemetry.push(format!("Fans: {fan_rpm}"));
    }
    if let Some(gpu_pending_reboot) = telemetry_state.gpu_pending_reboot {
        telemetry.push(format!("GPU: {gpu_pending_reboot}"));
    }
    if let Some(gpu_switching) = telemetry_state.gpu_switching {
        telemetry.push(format!("GPU switching: {gpu_switching}"));
    }
    if let Some(fan_curve_snapshot) = telemetry_state.fan_curve_snapshot {
        telemetry.push(format!("Saved curve: {fan_curve_snapshot}"));
    }
    if let Some(hardware_profile_apply) = telemetry_state.hardware_profile_apply {
        telemetry.push(format!("Profile apply: {hardware_profile_apply}"));
    }
    let telemetry = if telemetry.is_empty() {
        String::new()
    } else {
        format!("{}, ", telemetry.join(", "))
    };

    if missing_count == 0 {
        format!(
            "{product_name} {product_version}: {telemetry}{available_count} available capabilities"
        )
    } else {
        format!(
            "{product_name} {product_version}: {telemetry}{available_count} available capabilities, {missing_count} missing"
        )
    }
}

fn gpu_switching_summary(gpu_switching: &GpuSwitchingDiagnostics) -> Option<String> {
    if gpu_switching.status == "unavailable" {
        return None;
    }

    let runtime_state = if gpu_switching.runtime_plan_available {
        "runtime plan available"
    } else {
        "runtime plan blocked"
    };
    Some(format!(
        "{}; switch type {}; {runtime_state}",
        humanize_status(&gpu_switching.status),
        gpu_switching.switch_type
    ))
}

fn humanize_status(value: &str) -> String {
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

fn fan_rpm_label(sensors: &[HwmonSensor]) -> Option<String> {
    let values = sensors
        .iter()
        .filter(|sensor| sensor.kind == "fan")
        .filter_map(|sensor| sensor.value)
        .map(|value| value.to_string())
        .collect::<Vec<_>>();

    if values.is_empty() {
        None
    } else {
        Some(format!("{} RPM", values.join("/")))
    }
}

#[cfg(test)]
mod tests {
    use legion_common::{
        Capability, CapabilityRegistry, CapabilityStatus, FanCurvePointSnapshot, FanCurveSnapshot,
        GpuModePending, HardwareProfileApplyActionResult, HardwareProfileApplyRun, HardwareSummary,
        HwmonSensor, PlatformProfileCapability, PowerProfilesCapability, RiskLevel,
        WriteDryRunPlan, WriteExecutionResult,
    };
    use legion_control_ui::{GpuSwitchingDiagnostics, UiStatus};

    use super::*;

    #[test]
    fn tray_summary_renders_stable_read_only_status() {
        let status = UiStatus::from_parts(
            HardwareSummary {
                sysfs_root: "/tmp/fixture".to_owned(),
                vendor: Some("LENOVO".to_owned()),
                product_name: Some("82WM".to_owned()),
                product_version: Some("Legion Pro 5 16ARX8".to_owned()),
                product_sku: None,
            },
            vec![
                capability("platform_profile", "Platform profiles"),
                capability("battery_charge_type", "Battery charge types"),
            ],
        )
        .unwrap();

        let summary = TraySummary::from_status(&status);

        assert_eq!(summary.title, "Legion Control");
        assert_eq!(summary.capability_count, 2);
        assert_eq!(summary.available_capability_count, 2);
        assert_eq!(summary.missing_capability_count, 0);
        assert_eq!(
            summary.capability_ids,
            ["battery_charge_type", "platform_profile"]
        );
        assert_eq!(
            summary.render_lines(),
            [
                "Legion Control tray status",
                "title=Legion Control",
                "tooltip=82WM Legion Pro 5 16ARX8: 2 available capabilities",
                "capability_count=2",
                "available_capability_count=2",
                "missing_capability_count=0",
                "capabilities=battery_charge_type,platform_profile",
            ]
        );
    }

    #[test]
    fn tray_summary_reports_missing_capabilities_separately() {
        let status = UiStatus::from_parts(
            HardwareSummary {
                sysfs_root: "/tmp/fixture".to_owned(),
                vendor: Some("LENOVO".to_owned()),
                product_name: Some("82WM".to_owned()),
                product_version: Some("Legion Pro 5 16ARX8".to_owned()),
                product_sku: None,
            },
            vec![
                capability("platform_profile", "Platform profiles"),
                missing_capability("gpu", "GPU mode"),
            ],
        )
        .unwrap();

        let summary = TraySummary::from_status(&status);

        assert_eq!(summary.capability_count, 2);
        assert_eq!(summary.available_capability_count, 1);
        assert_eq!(summary.missing_capability_count, 1);
        assert_eq!(
            summary.tooltip,
            "82WM Legion Pro 5 16ARX8: 1 available capabilities, 1 missing"
        );
    }

    #[test]
    fn tray_summary_tooltip_includes_profile_and_fan_rpm_when_available() {
        let status = UiStatus::from_parts(
            HardwareSummary {
                sysfs_root: "/tmp/fixture".to_owned(),
                vendor: Some("LENOVO".to_owned()),
                product_name: Some("82WM".to_owned()),
                product_version: Some("Legion Pro 5 16ARX8".to_owned()),
                product_sku: None,
            },
            vec![
                capability("platform_profile", "Platform profiles"),
                missing_capability("gpu", "GPU mode"),
            ],
        )
        .unwrap();
        let report = CapabilityRegistry {
            platform_profile: Some(PlatformProfileCapability {
                current: Some("balanced".to_owned()),
                choices: vec!["balanced".to_owned(), "performance".to_owned()],
                path: "/sys/firmware/acpi/platform_profile".to_owned(),
                choices_path: "/sys/firmware/acpi/platform_profile_choices".to_owned(),
            }),
            power_profiles: Some(PowerProfilesCapability {
                bus: "system".to_owned(),
                well_known_name: "org.freedesktop.UPower.PowerProfiles".to_owned(),
                unique_owner: Some(":1.42".to_owned()),
                active_profile: Some("power-saver".to_owned()),
                status: CapabilityStatus::ProbeOnly,
                detail: None,
            }),
            telemetry: legion_common::TelemetrySnapshot {
                sensors: vec![HwmonSensor {
                    hwmon_name: Some("legion".to_owned()),
                    label: Some("CPU Fan".to_owned()),
                    kind: "fan".to_owned(),
                    input_path: "/sys/class/hwmon/hwmon7/fan1_input".to_owned(),
                    value: Some(2410),
                }],
                battery: None,
                ac_adapters: Vec::new(),
            },
            ..Default::default()
        };

        let summary = TraySummary::from_status_and_report(&status, &report, None, None, None, None);

        assert_eq!(
            summary.tooltip,
            "82WM Legion Pro 5 16ARX8: Platform: balanced, Power: power-saver, Fans: 2410 RPM, 1 available capabilities, 1 missing"
        );
        assert_eq!(summary.platform_profile.as_deref(), Some("balanced"));
        assert_eq!(
            summary.desktop_power_profile.as_deref(),
            Some("power-saver")
        );
        assert_eq!(summary.fan_rpm.as_deref(), Some("2410 RPM"));
        assert!(summary
            .render_lines()
            .contains(&"platform_profile=balanced".to_owned()));
        assert!(summary
            .render_lines()
            .contains(&"desktop_power_profile=power-saver".to_owned()));
        assert!(summary
            .render_lines()
            .contains(&"fan_rpm=2410 RPM".to_owned()));
    }

    #[test]
    fn tray_summary_tooltip_includes_pending_gpu_and_saved_fan_curve() {
        let status = UiStatus::from_parts(
            HardwareSummary {
                sysfs_root: "/tmp/fixture".to_owned(),
                vendor: Some("LENOVO".to_owned()),
                product_name: Some("82WM".to_owned()),
                product_version: Some("Legion Pro 5 16ARX8".to_owned()),
                product_sku: None,
            },
            vec![capability("platform_profile", "Platform profiles")],
        )
        .unwrap();
        let report = CapabilityRegistry {
            platform_profile: Some(PlatformProfileCapability {
                current: Some("balanced".to_owned()),
                choices: vec!["balanced".to_owned()],
                path: "/sys/firmware/acpi/platform_profile".to_owned(),
                choices_path: "/sys/firmware/acpi/platform_profile_choices".to_owned(),
            }),
            ..Default::default()
        };
        let gpu_pending = GpuModePending {
            requested_mode: "hybrid".to_owned(),
            previous_mode: Some("nvidia".to_owned()),
            reboot_required: true,
        };
        let fan_snapshot = FanCurveSnapshot {
            curve_id: "legion_hwmon".to_owned(),
            path: Some("/tmp/fixture/sys/class/hwmon/hwmon7".to_owned()),
            points: vec![FanCurvePointSnapshot {
                path: "/tmp/fixture/sys/class/hwmon/hwmon7/pwm1_auto_point1_temp".to_owned(),
                value: "42000".to_owned(),
            }],
        };

        let summary = TraySummary::from_status_and_report(
            &status,
            &report,
            Some(&gpu_pending),
            None,
            Some(&fan_snapshot),
            None,
        );

        assert!(summary
            .tooltip
            .contains("GPU: hybrid pending (was nvidia); reboot required"));
        assert!(summary
            .tooltip
            .contains("Saved curve: 1 point on legion_hwmon"));
        assert!(summary.render_lines().contains(
            &"gpu_pending_reboot=hybrid pending (was nvidia); reboot required".to_owned()
        ));
        assert!(summary
            .render_lines()
            .contains(&"last_known_good_fan_curve=1 point on legion_hwmon".to_owned()));
    }

    #[test]
    fn tray_summary_tooltip_includes_gpu_switching_classification() {
        let status = UiStatus::from_parts(
            HardwareSummary {
                sysfs_root: "/tmp/fixture".to_owned(),
                vendor: Some("LENOVO".to_owned()),
                product_name: Some("82WM".to_owned()),
                product_version: Some("Legion Pro 5 16ARX8".to_owned()),
                product_sku: None,
            },
            vec![capability("gpu", "GPU mode")],
        )
        .unwrap();
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

        let summary = TraySummary::from_status_and_report(
            &status,
            &CapabilityRegistry::default(),
            None,
            Some(&gpu_switching),
            None,
            None,
        );

        let expected =
            "Runtime Mux Research Blocked; switch type runtime-mux; runtime plan blocked";
        assert_eq!(summary.gpu_switching.as_deref(), Some(expected));
        assert!(summary
            .tooltip
            .contains(&format!("GPU switching: {expected}")));
        assert!(summary
            .render_lines()
            .contains(&format!("gpu_switching={expected}")));
    }

    #[test]
    fn tray_summary_tooltip_includes_last_hardware_profile_apply() {
        let status = UiStatus::from_parts(
            HardwareSummary {
                sysfs_root: "/tmp/fixture".to_owned(),
                vendor: Some("LENOVO".to_owned()),
                product_name: Some("82WM".to_owned()),
                product_version: Some("Legion Pro 5 16ARX8".to_owned()),
                product_sku: None,
            },
            vec![capability("hardware_profiles", "Hardware profiles")],
        )
        .unwrap();
        let run = HardwareProfileApplyRun {
            profile_id: "co_test".to_owned(),
            profile_label: "CO test".to_owned(),
            timestamp_unix_secs: 1,
            completed: false,
            message: "hardware profile apply stopped after first non-applied action".to_owned(),
            results: vec![HardwareProfileApplyActionResult {
                action_id: "curve_optimizer_all_core".to_owned(),
                result: WriteExecutionResult::blocked_by_policy(
                    WriteDryRunPlan {
                        method: "SetCurveOptimizerAllCore".to_owned(),
                        capability_id: "curve_optimizer_all_core".to_owned(),
                        polkit_action: "org.ratvantage.LegionControl1.set-curve-optimizer"
                            .to_owned(),
                        path: "ryzenadj:/usr/local/bin/ryzenadj".to_owned(),
                        previous_value: "unknown".to_owned(),
                        requested_value: "-20 (encoded 4294967276)".to_owned(),
                        readback_required: false,
                        rollback_value: "0".to_owned(),
                        rollback_instructions: Vec::new(),
                        reboot_required: false,
                        safety_notes: Vec::new(),
                        steps: Vec::new(),
                    },
                    "Curve Optimizer writes are disabled by daemon policy",
                ),
            }],
        };

        let summary = TraySummary::from_status_and_report(
            &status,
            &CapabilityRegistry::default(),
            None,
            None,
            None,
            Some(&run),
        );

        let expected = "co_test stopped at curve_optimizer_all_core: blocked_by_policy - Curve Optimizer writes are disabled by daemon policy";
        assert!(summary
            .tooltip
            .contains(&format!("Profile apply: {expected}")));
        assert!(summary
            .render_lines()
            .contains(&format!("last_hardware_profile_apply={expected}")));
    }

    fn capability(id: &str, label: &str) -> Capability {
        Capability {
            id: id.to_owned(),
            label: label.to_owned(),
            status: CapabilityStatus::ProbeOnly,
            risk: RiskLevel::ReadOnly,
            evidence: vec![],
            details: serde_json::Value::Null,
        }
    }

    fn missing_capability(id: &str, label: &str) -> Capability {
        Capability {
            id: id.to_owned(),
            label: label.to_owned(),
            status: CapabilityStatus::Missing,
            risk: RiskLevel::ReadOnly,
            evidence: vec![],
            details: serde_json::Value::Null,
        }
    }
}
