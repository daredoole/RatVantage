use legion_control_ui::UiStatus;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraySummary {
    pub title: String,
    pub tooltip: String,
    pub capability_count: usize,
    pub capability_ids: Vec<String>,
}

impl TraySummary {
    pub fn from_status(status: &UiStatus) -> Self {
        let capability_ids = status
            .capability_ids()
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();

        Self {
            title: "Legion Control".to_owned(),
            tooltip: format!(
                "{} {}: {} read-only capabilities",
                status.hardware.product_name,
                status.hardware.product_version,
                status.capability_count()
            ),
            capability_count: status.capability_count(),
            capability_ids,
        }
    }

    pub fn render_lines(&self) -> Vec<String> {
        vec![
            "Legion Control tray status".to_owned(),
            format!("title={}", self.title),
            format!("tooltip={}", self.tooltip),
            format!("capability_count={}", self.capability_count),
            format!("capabilities={}", self.capability_ids.join(",")),
        ]
    }
}

#[cfg(test)]
mod tests {
    use legion_common::{Capability, CapabilityStatus, HardwareSummary, RiskLevel};
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
        assert_eq!(
            summary.capability_ids,
            ["battery_charge_type", "platform_profile"]
        );
        assert_eq!(
            summary.render_lines(),
            [
                "Legion Control tray status",
                "title=Legion Control",
                "tooltip=82WM Legion Pro 5 16ARX8: 2 read-only capabilities",
                "capability_count=2",
                "capabilities=battery_charge_type,platform_profile",
            ]
        );
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
}
