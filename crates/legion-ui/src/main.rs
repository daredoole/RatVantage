use anyhow::Result;
use clap::Parser;
#[cfg(not(feature = "gtk-ui"))]
use legion_control_ui::DBUS_INTERFACE;
use legion_control_ui::{render_overview_lines, LegionControlClient, UiStatus};

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    status: bool,

    #[arg(long)]
    overview: bool,

    #[arg(long)]
    bus_address: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if args.status || args.overview {
        let client = match args.bus_address {
            Some(address) => LegionControlClient::address(&address)?,
            None => LegionControlClient::system()?,
        };
        if args.overview {
            print_overview(&client.raw_probe_report()?);
        } else {
            print_status(&client.status()?);
        }
        return Ok(());
    }

    #[cfg(feature = "gtk-ui")]
    {
        legion_control_ui::gtk_shell::run()
    }

    #[cfg(not(feature = "gtk-ui"))]
    {
        println!("Legion Control UI scaffold");
        println!("D-Bus target: {DBUS_INTERFACE}");
        println!("Read-only client module is available for hardware summary and capabilities.");
        println!("Direct sysfs access is intentionally not implemented.");
        Ok(())
    }
}

fn print_status(status: &UiStatus) {
    for line in status.render_lines() {
        println!("{line}");
    }
}

fn print_overview(report: &legion_common::CapabilityRegistry) {
    for line in render_overview_lines(report) {
        println!("{line}");
    }
}
