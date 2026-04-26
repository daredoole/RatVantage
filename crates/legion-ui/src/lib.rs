use std::collections::BTreeMap;
use std::process::Command;

use anyhow::Result;
use legion_common::{
    Capability, CapabilityRegistry, CapabilityStatus, FanCurveSnapshot, GpuModePending,
    HardwareSummary, RiskLevel, TelemetrySnapshot, WriteDryRunPlan,
};
use serde::{de::DeserializeOwned, Serialize};
use zbus::blocking::{Connection, ConnectionBuilder, Proxy};

#[cfg(feature = "gtk-ui")]
pub mod gtk_shell;

pub const DBUS_INTERFACE: &str = "org.ratvantage.LegionControl1";
pub const DBUS_PATH: &str = "/org/ratvantage/LegionControl1";

pub struct LegionControlClient {
    connection: Connection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiStatus {
    pub hardware: UiHardwareStatus,
    pub capabilities: Vec<UiCapabilityStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiHardwareStatus {
    pub vendor: String,
    pub product_name: String,
    pub product_version: String,
    pub product_sku: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiCapabilityStatus {
    pub id: String,
    pub label: String,
    pub status: CapabilityStatus,
    pub risk: RiskLevel,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DiagnosticsBundle {
    pub hardware: HardwareSummary,
    pub kernel_version: Option<String>,
    pub summary: DiagnosticsSummary,
    pub detected_sysfs_paths: Vec<String>,
    pub recent_daemon_logs: Vec<String>,
    pub raw_probe_report: CapabilityRegistry,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DiagnosticsSummary {
    pub capability_count: usize,
    pub available_capability_count: usize,
    pub missing_capability_count: usize,
    pub capability_status_counts: BTreeMap<String, usize>,
    pub sensor_count: usize,
    pub fan_curve_count: usize,
    pub detected_sysfs_path_count: usize,
}

impl DiagnosticsBundle {
    pub fn from_report(report: CapabilityRegistry, kernel_version: Option<String>) -> Self {
        Self::from_report_with_logs(report, kernel_version, Vec::new())
    }

    pub fn from_report_with_logs(
        report: CapabilityRegistry,
        kernel_version: Option<String>,
        recent_daemon_logs: Vec<String>,
    ) -> Self {
        let detected_sysfs_paths = detected_sysfs_paths(&report);
        let summary = DiagnosticsSummary::from_report(&report, detected_sysfs_paths.len());
        Self {
            hardware: report.hardware.clone(),
            kernel_version,
            summary,
            detected_sysfs_paths,
            recent_daemon_logs,
            raw_probe_report: report,
        }
    }
}

impl DiagnosticsSummary {
    fn from_report(report: &CapabilityRegistry, detected_sysfs_path_count: usize) -> Self {
        let mut capability_status_counts = BTreeMap::new();
        for capability in &report.capabilities {
            *capability_status_counts
                .entry(capability_status_label(capability.status).to_owned())
                .or_insert(0) += 1;
        }

        let missing_capability_count = report
            .capabilities
            .iter()
            .filter(|capability| capability.status == CapabilityStatus::Missing)
            .count();

        Self {
            capability_count: report.capabilities.len(),
            available_capability_count: report.capabilities.len() - missing_capability_count,
            missing_capability_count,
            capability_status_counts,
            sensor_count: report.telemetry.sensors.len(),
            fan_curve_count: report.fan_curves.len(),
            detected_sysfs_path_count,
        }
    }
}

impl UiStatus {
    pub fn from_client(client: &LegionControlClient) -> Result<Self> {
        Self::from_parts(client.hardware_summary()?, client.capabilities()?)
    }

    pub fn from_parts(hardware: HardwareSummary, capabilities: Vec<Capability>) -> Result<Self> {
        let mut capabilities = capabilities
            .into_iter()
            .map(|capability| UiCapabilityStatus {
                id: capability.id,
                label: capability.label,
                status: capability.status,
                risk: capability.risk,
            })
            .collect::<Vec<_>>();
        capabilities.sort_by(|left, right| left.id.cmp(&right.id));

        Ok(Self {
            hardware: UiHardwareStatus {
                vendor: hardware.vendor.unwrap_or_else(|| "unknown".to_owned()),
                product_name: hardware
                    .product_name
                    .unwrap_or_else(|| "unknown".to_owned()),
                product_version: hardware
                    .product_version
                    .unwrap_or_else(|| "unknown".to_owned()),
                product_sku: hardware.product_sku,
            },
            capabilities,
        })
    }

    pub fn capability_count(&self) -> usize {
        self.capabilities.len()
    }

    pub fn capability_ids(&self) -> Vec<&str> {
        self.capabilities
            .iter()
            .map(|capability| capability.id.as_str())
            .collect()
    }

    pub fn capability_statuses(&self) -> Vec<String> {
        self.capabilities
            .iter()
            .map(|capability| {
                format!(
                    "{}:{}:{}",
                    capability.id,
                    capability_status_label(capability.status),
                    risk_level_label(capability.risk)
                )
            })
            .collect()
    }

    pub fn render_lines(&self) -> Vec<String> {
        vec![
            "Legion Control status".to_owned(),
            format!("vendor={}", self.hardware.vendor),
            format!("product_name={}", self.hardware.product_name),
            format!("product_version={}", self.hardware.product_version),
            format!("capability_count={}", self.capability_count()),
            format!("capabilities={}", self.capability_ids().join(",")),
            format!(
                "capability_statuses={}",
                self.capability_statuses().join(",")
            ),
        ]
    }
}

pub fn capability_status_label(status: CapabilityStatus) -> &'static str {
    match status {
        CapabilityStatus::Detected => "detected",
        CapabilityStatus::Missing => "missing",
        CapabilityStatus::ProbeOnly => "probe_only",
        CapabilityStatus::Unsupported => "unsupported",
    }
}

pub fn risk_level_label(risk: RiskLevel) -> &'static str {
    match risk {
        RiskLevel::ReadOnly => "read_only",
        RiskLevel::ReversibleWrite => "reversible_write",
        RiskLevel::ExperimentalWrite => "experimental_write",
        RiskLevel::Unsupported => "unsupported",
    }
}

pub fn render_overview_lines(report: &CapabilityRegistry) -> Vec<String> {
    render_overview_lines_with_pending(report, None, None)
}

pub fn render_overview_lines_with_pending(
    report: &CapabilityRegistry,
    pending: Option<&GpuModePending>,
    fan_snapshot: Option<&FanCurveSnapshot>,
) -> Vec<String> {
    let mut lines = vec![
        "Legion Control overview".to_owned(),
        format!(
            "platform_profile={}",
            report
                .platform_profile
                .as_ref()
                .and_then(|profile| profile.current.as_deref())
                .unwrap_or("unknown")
        ),
        format!(
            "battery_charge_type={}",
            report
                .battery_charge_type
                .as_ref()
                .and_then(|charge_type| charge_type.current.as_deref())
                .unwrap_or("unknown")
        ),
        format!(
            "fan_rpm={}",
            render_sensor_values(&report.telemetry.sensors, "fan")
        ),
        format!(
            "temperatures={}",
            render_sensor_values(&report.telemetry.sensors, "temp")
        ),
        format!(
            "gpu_mode={}",
            report
                .gpu
                .as_ref()
                .and_then(|gpu| gpu.mode.as_deref())
                .unwrap_or("unknown")
        ),
        format!("gpu_pending_reboot={}", render_gpu_pending(pending)),
        format!(
            "last_known_good_fan_curve={}",
            render_fan_curve_snapshot(fan_snapshot)
        ),
        format!(
            "battery_capacity_percent={}",
            report
                .telemetry
                .battery
                .as_ref()
                .and_then(|battery| battery.capacity_percent)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_owned())
        ),
        format!(
            "battery_status={}",
            report
                .telemetry
                .battery
                .as_ref()
                .and_then(|battery| battery.status.as_deref())
                .unwrap_or("unknown")
        ),
        format!(
            "battery_health={}",
            report
                .telemetry
                .battery
                .as_ref()
                .and_then(|battery| battery.health.as_deref())
                .unwrap_or("unknown")
        ),
    ];
    lines.push(format!("leds={}", render_led_values(report)));
    lines.push(format!(
        "firmware_toggles={}",
        render_ideapad_toggle_values(report)
    ));
    lines
}

fn render_gpu_pending(pending: Option<&GpuModePending>) -> String {
    match pending {
        Some(pending) => {
            let previous = pending.previous_mode.as_deref().unwrap_or("unknown");
            format!(
                "{} previous={} reboot_required={}",
                pending.requested_mode, previous, pending.reboot_required
            )
        }
        None => "none".to_owned(),
    }
}

fn render_fan_curve_snapshot(snapshot: Option<&FanCurveSnapshot>) -> String {
    match snapshot {
        Some(snapshot) => format!(
            "{} values from {}",
            snapshot.points.len(),
            snapshot.curve_id
        ),
        None => "none".to_owned(),
    }
}

fn render_sensor_values(sensors: &[legion_common::HwmonSensor], kind: &str) -> String {
    let values = sensors
        .iter()
        .filter(|sensor| sensor.kind == kind)
        .map(|sensor| {
            format!(
                "{}:{}",
                sensor.label.as_deref().unwrap_or("unknown"),
                sensor
                    .value
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_owned())
            )
        })
        .collect::<Vec<_>>();

    if values.is_empty() {
        "unknown".to_owned()
    } else {
        values.join(",")
    }
}

fn render_led_values(report: &CapabilityRegistry) -> String {
    let values = report
        .leds
        .iter()
        .map(|led| {
            format!(
                "{}:{}",
                led.name,
                led.brightness
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_owned())
            )
        })
        .collect::<Vec<_>>();

    if values.is_empty() {
        "unknown".to_owned()
    } else {
        values.join(",")
    }
}

fn render_ideapad_toggle_values(report: &CapabilityRegistry) -> String {
    let values = report
        .ideapad_toggles
        .iter()
        .map(|toggle| {
            format!(
                "{}:{}",
                toggle.name,
                toggle.current_value.as_deref().unwrap_or("unknown")
            )
        })
        .collect::<Vec<_>>();

    if values.is_empty() {
        "unknown".to_owned()
    } else {
        values.join(",")
    }
}

pub fn render_diagnostics_json(bundle: &DiagnosticsBundle) -> Result<String> {
    Ok(serde_json::to_string_pretty(bundle)?)
}

pub fn render_write_plan_json<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string_pretty(value)?)
}

fn detected_sysfs_paths(report: &CapabilityRegistry) -> Vec<String> {
    let mut paths = Vec::new();
    push_path(&mut paths, &report.hardware.sysfs_root);

    if let Some(profile) = &report.platform_profile {
        push_path(&mut paths, &profile.path);
        push_path(&mut paths, &profile.choices_path);
    }
    if let Some(charge_type) = &report.battery_charge_type {
        push_path(&mut paths, &charge_type.path);
        push_path(&mut paths, &charge_type.choices_path);
    }
    for sensor in &report.hwmon_sensors {
        push_path(&mut paths, &sensor.input_path);
    }
    for curve in &report.fan_curves {
        if let Some(path) = &curve.path {
            push_path(&mut paths, path);
        }
        for point_path in &curve.point_paths {
            push_path(&mut paths, point_path);
        }
    }
    for led in &report.leds {
        push_path(&mut paths, &led.path);
    }
    for attribute in &report.firmware_attributes {
        push_path(&mut paths, &attribute.path);
    }
    for toggle in &report.ideapad_toggles {
        if let Some(path) = &toggle.path {
            push_path(&mut paths, path);
        }
    }
    for sensor in &report.telemetry.sensors {
        push_path(&mut paths, &sensor.input_path);
    }
    if let Some(battery) = &report.telemetry.battery {
        push_path(&mut paths, &battery.path);
    }

    paths.sort();
    paths.dedup();
    paths
}

fn push_path(paths: &mut Vec<String>, path: &str) {
    if !path.is_empty() {
        paths.push(path.to_owned());
    }
}

fn kernel_version() -> Option<String> {
    let output = Command::new("uname").arg("-r").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let version = String::from_utf8(output.stdout).ok()?;
    let version = version.trim();
    if version.is_empty() {
        None
    } else {
        Some(version.to_owned())
    }
}

fn recent_daemon_logs() -> Vec<String> {
    let output = Command::new("journalctl")
        .args([
            "-u",
            "legion-control-daemon.service",
            "-n",
            "40",
            "--no-pager",
            "--output=short-iso",
        ])
        .output();

    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    String::from_utf8(output.stdout)
        .ok()
        .map(|stdout| {
            stdout
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

impl LegionControlClient {
    pub fn system() -> Result<Self> {
        Ok(Self {
            connection: Connection::system()?,
        })
    }

    pub fn address(address: &str) -> Result<Self> {
        Ok(Self {
            connection: ConnectionBuilder::address(address)?.build()?,
        })
    }

    pub fn hardware_summary(&self) -> Result<HardwareSummary> {
        self.call_json("GetHardwareSummary")
    }

    pub fn capabilities(&self) -> Result<Vec<Capability>> {
        self.call_json("GetCapabilities")
    }

    pub fn refresh_capabilities(&self) -> Result<Vec<Capability>> {
        self.call_json("RefreshCapabilities")
    }

    pub fn telemetry(&self) -> Result<TelemetrySnapshot> {
        self.call_json("GetTelemetry")
    }

    pub fn raw_probe_report(&self) -> Result<CapabilityRegistry> {
        self.call_json("GetRawProbeReport")
    }

    pub fn diagnostics_bundle(&self) -> Result<DiagnosticsBundle> {
        Ok(DiagnosticsBundle::from_report_with_logs(
            self.raw_probe_report()?,
            kernel_version(),
            recent_daemon_logs(),
        ))
    }

    pub fn plan_platform_profile_write(&self, requested: &str) -> Result<WriteDryRunPlan> {
        self.call_json_arg("PlanPlatformProfileWrite", requested)
    }

    pub fn plan_battery_charge_type_write(&self, requested: &str) -> Result<WriteDryRunPlan> {
        self.call_json_arg("PlanBatteryChargeTypeWrite", requested)
    }

    pub fn plan_gpu_mode_write(&self, requested: &str) -> Result<WriteDryRunPlan> {
        self.call_json_arg("PlanGpuModeWrite", requested)
    }

    pub fn plan_fan_preset_write(&self, requested: &str) -> Result<WriteDryRunPlan> {
        self.call_json_arg("PlanFanPresetWrite", requested)
    }

    pub fn plan_restore_auto_fan_write(&self) -> Result<WriteDryRunPlan> {
        self.call_json("PlanRestoreAutoFanWrite")
    }

    pub fn gpu_mode_pending(&self) -> Result<Option<GpuModePending>> {
        self.call_json("GetGpuModePending")
    }

    pub fn set_gpu_mode_pending(&self, requested: &str) -> Result<GpuModePending> {
        self.call_json_arg("SetGpuModePending", requested)
    }

    pub fn clear_gpu_mode_pending(&self) -> Result<Option<GpuModePending>> {
        self.call_json("ClearGpuModePending")
    }

    pub fn last_known_good_fan_curve(&self) -> Result<Option<FanCurveSnapshot>> {
        self.call_json("GetLastKnownGoodFanCurve")
    }

    pub fn capture_last_known_good_fan_curve(&self) -> Result<FanCurveSnapshot> {
        self.call_json("CaptureLastKnownGoodFanCurve")
    }

    pub fn status(&self) -> Result<UiStatus> {
        UiStatus::from_client(self)
    }

    fn call_json<T>(&self, method: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let proxy = Proxy::new(&self.connection, DBUS_INTERFACE, DBUS_PATH, DBUS_INTERFACE)?;
        let payload: String = proxy.call(method, &())?;
        Ok(serde_json::from_str(&payload)?)
    }

    fn call_json_arg<T>(&self, method: &str, arg: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let proxy = Proxy::new(&self.connection, DBUS_INTERFACE, DBUS_PATH, DBUS_INTERFACE)?;
        let payload: String = proxy.call(method, &(arg,))?;
        Ok(serde_json::from_str(&payload)?)
    }
}
