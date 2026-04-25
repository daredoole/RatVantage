use legion_common::{CapabilityRegistry, CapabilityStatus, HwmonSensor};
use legion_control_ui::UiStatus;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraySummary {
    pub title: String,
    pub tooltip: String,
    pub capability_count: usize,
    pub available_capability_count: usize,
    pub missing_capability_count: usize,
    pub capability_ids: Vec<String>,
    pub platform_profile: Option<String>,
    pub fan_rpm: Option<String>,
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
                None,
                None,
            ),
            capability_count: status.capability_count(),
            available_capability_count,
            missing_capability_count,
            capability_ids,
            platform_profile: None,
            fan_rpm: None,
        }
    }

    pub fn from_status_and_report(status: &UiStatus, report: &CapabilityRegistry) -> Self {
        let mut summary = Self::from_status(status);
        summary.platform_profile = report
            .platform_profile
            .as_ref()
            .and_then(|profile| profile.current.clone());
        summary.fan_rpm = fan_rpm_label(&report.telemetry.sensors);
        summary.tooltip = capability_tooltip(
            &status.hardware.product_name,
            &status.hardware.product_version,
            summary.available_capability_count,
            summary.missing_capability_count,
            summary.platform_profile.as_deref(),
            summary.fan_rpm.as_deref(),
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
        if let Some(fan_rpm) = &self.fan_rpm {
            lines.push(format!("fan_rpm={fan_rpm}"));
        }
        lines
    }
}

fn capability_tooltip(
    product_name: &str,
    product_version: &str,
    available_count: usize,
    missing_count: usize,
    platform_profile: Option<&str>,
    fan_rpm: Option<&str>,
) -> String {
    let telemetry = match (platform_profile, fan_rpm) {
        (Some(profile), Some(fan_rpm)) => format!("profile {profile}, fan {fan_rpm}, "),
        (Some(profile), None) => format!("profile {profile}, "),
        (None, Some(fan_rpm)) => format!("fan {fan_rpm}, "),
        (None, None) => String::new(),
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
        Capability, CapabilityRegistry, CapabilityStatus, HardwareSummary, HwmonSensor,
        PlatformProfileCapability, RiskLevel,
    };
    use legion_control_ui::UiStatus;

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
            telemetry: legion_common::TelemetrySnapshot {
                sensors: vec![HwmonSensor {
                    hwmon_name: Some("legion".to_owned()),
                    label: Some("CPU Fan".to_owned()),
                    kind: "fan".to_owned(),
                    input_path: "/sys/class/hwmon/hwmon7/fan1_input".to_owned(),
                    value: Some(2410),
                }],
                battery: None,
            },
            ..Default::default()
        };

        let summary = TraySummary::from_status_and_report(&status, &report);

        assert_eq!(
            summary.tooltip,
            "82WM Legion Pro 5 16ARX8: profile balanced, fan 2410 RPM, 1 available capabilities, 1 missing"
        );
        assert_eq!(summary.platform_profile.as_deref(), Some("balanced"));
        assert_eq!(summary.fan_rpm.as_deref(), Some("2410 RPM"));
        assert!(summary
            .render_lines()
            .contains(&"platform_profile=balanced".to_owned()));
        assert!(summary
            .render_lines()
            .contains(&"fan_rpm=2410 RPM".to_owned()));
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
