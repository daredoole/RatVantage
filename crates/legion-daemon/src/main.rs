use anyhow::Result;
use clap::Parser;
use legion_probe::{probe, ProbeOptions};

const DBUS_INTERFACE: &str = "org.ratvantage.LegionControl1";
const DBUS_PATH: &str = "/org/ratvantage/LegionControl1";

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    dry_run: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if !args.dry_run {
        eprintln!("Only --dry-run is implemented in the first scaffold.");
        std::process::exit(2);
    }

    let registry = probe(&ProbeOptions::default());
    println!("daemon dry-run");
    println!("interface={DBUS_INTERFACE}");
    println!("path={DBUS_PATH}");
    println!("read_only_methods=GetHardwareSummary,GetCapabilities,RefreshCapabilities,GetTelemetry,GetRawProbeReport");
    println!("capability_count={}", registry.capabilities.len());

    Ok(())
}
