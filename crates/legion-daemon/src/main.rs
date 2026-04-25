use anyhow::Result;
use clap::Parser;
use legion_common::CapabilityRegistry;
use legion_probe::{probe, ProbeOptions};
use serde::Serialize;
use std::sync::Mutex;
use zbus::{blocking::ConnectionBuilder, fdo, interface};

const DBUS_INTERFACE: &str = "org.ratvantage.LegionControl1";
const DBUS_PATH: &str = "/org/ratvantage/LegionControl1";
const READ_ONLY_METHODS: &str =
    "GetHardwareSummary,GetCapabilities,RefreshCapabilities,GetTelemetry,GetRawProbeReport";

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    dry_run: bool,
}

struct LegionControl {
    options: ProbeOptions,
    registry: Mutex<CapabilityRegistry>,
}

impl LegionControl {
    fn new(options: ProbeOptions) -> Self {
        let registry = probe(&options);

        Self {
            options,
            registry: Mutex::new(registry),
        }
    }

    fn snapshot(&self) -> fdo::Result<CapabilityRegistry> {
        self.registry
            .lock()
            .map(|registry| registry.clone())
            .map_err(|_| fdo::Error::Failed("capability registry lock poisoned".to_owned()))
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
}

fn to_json<T: Serialize>(value: &T) -> fdo::Result<String> {
    serde_json::to_string(value).map_err(|error| fdo::Error::Failed(error.to_string()))
}

fn main() -> Result<()> {
    let args = Args::parse();
    let service = LegionControl::new(ProbeOptions::default());

    if args.dry_run {
        let registry = service.snapshot()?;
        println!("daemon dry-run");
        println!("interface={DBUS_INTERFACE}");
        println!("path={DBUS_PATH}");
        println!("read_only_methods={READ_ONLY_METHODS}");
        println!("capability_count={}", registry.capabilities.len());
        return Ok(());
    }

    let _connection = ConnectionBuilder::system()?
        .name(DBUS_INTERFACE)?
        .serve_at(DBUS_PATH, service)?
        .build()?;

    println!("serving interface={DBUS_INTERFACE} path={DBUS_PATH}");
    loop {
        std::thread::park();
    }
}
