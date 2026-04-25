use anyhow::Result;
use legion_common::{
    Capability, CapabilityRegistry, CapabilityStatus, HardwareSummary, RiskLevel, TelemetrySnapshot,
};
use serde::de::DeserializeOwned;
use zbus::blocking::{Connection, ConnectionBuilder, Proxy};

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

    pub fn render_lines(&self) -> Vec<String> {
        vec![
            "Legion Control status".to_owned(),
            format!("vendor={}", self.hardware.vendor),
            format!("product_name={}", self.hardware.product_name),
            format!("product_version={}", self.hardware.product_version),
            format!("capability_count={}", self.capability_count()),
            format!("capabilities={}", self.capability_ids().join(",")),
        ]
    }
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
}
