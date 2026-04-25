use anyhow::Result;
use clap::Parser;
use legion_control_ui::{LegionControlClient, UiStatus, DBUS_INTERFACE};

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
        print_status(&client.status()?);
        return Ok(());
    }

    println!("Legion Control UI scaffold");
    println!("D-Bus target: {DBUS_INTERFACE}");
    println!("Read-only client module is available for hardware summary and capabilities.");
    println!("Direct sysfs access is intentionally not implemented.");
    Ok(())
}

fn print_status(status: &UiStatus) {
    for line in status.render_lines() {
        println!("{line}");
    }
}
