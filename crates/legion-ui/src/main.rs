use anyhow::Result;
use clap::Parser;
use legion_common::{Capability, HardwareSummary};
use legion_control_ui::{LegionControlClient, DBUS_INTERFACE};

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    status: bool,

    #[arg(long)]
    bus_address: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if args.status {
        let client = match args.bus_address {
            Some(address) => LegionControlClient::address(&address)?,
            None => LegionControlClient::system()?,
        };
        let hardware = client.hardware_summary()?;
        let capabilities = client.capabilities()?;
        print_status(&hardware, &capabilities);
        return Ok(());
    }

    println!("Legion Control UI scaffold");
    println!("D-Bus target: {DBUS_INTERFACE}");
    println!("Read-only client module is available for hardware summary and capabilities.");
    println!("Direct sysfs access is intentionally not implemented.");
    Ok(())
}

fn print_status(hardware: &HardwareSummary, capabilities: &[Capability]) {
    println!("Legion Control status");
    println!("vendor={}", hardware.vendor.as_deref().unwrap_or("unknown"));
    println!(
        "product_name={}",
        hardware.product_name.as_deref().unwrap_or("unknown")
    );
    println!(
        "product_version={}",
        hardware.product_version.as_deref().unwrap_or("unknown")
    );
    println!("capability_count={}", capabilities.len());

    let mut capability_ids = capabilities
        .iter()
        .map(|capability| capability.id.as_str())
        .collect::<Vec<_>>();
    capability_ids.sort_unstable();
    println!("capabilities={}", capability_ids.join(","));
}
