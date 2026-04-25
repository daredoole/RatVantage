use anyhow::Result;
use legion_common::{Capability, CapabilityRegistry, HardwareSummary, TelemetrySnapshot};
use serde::de::DeserializeOwned;
use zbus::blocking::{Connection, ConnectionBuilder, Proxy};

pub const DBUS_INTERFACE: &str = "org.ratvantage.LegionControl1";
pub const DBUS_PATH: &str = "/org/ratvantage/LegionControl1";

pub struct LegionControlClient {
    connection: Connection,
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

    fn call_json<T>(&self, method: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let proxy = Proxy::new(&self.connection, DBUS_INTERFACE, DBUS_PATH, DBUS_INTERFACE)?;
        let payload: String = proxy.call(method, &())?;
        Ok(serde_json::from_str(&payload)?)
    }
}
