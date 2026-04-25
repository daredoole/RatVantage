use anyhow::Result;
use clap::Parser;
use legion_control_tray::TraySummary;
#[cfg(not(feature = "status-notifier"))]
use legion_control_tray::{DesktopSession, TrayMenu};
use legion_control_ui::LegionControlClient;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    status: bool,

    #[arg(long)]
    tooltip: bool,

    #[arg(long)]
    bus_address: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if args.status || args.tooltip {
        let client = match args.bus_address {
            Some(address) => LegionControlClient::address(&address)?,
            None => LegionControlClient::system()?,
        };
        let summary = TraySummary::from_status(&client.status()?);
        if args.tooltip {
            println!("{}", summary.tooltip);
            return Ok(());
        }
        for line in summary.render_lines() {
            println!("{line}");
        }
        return Ok(());
    }

    run_tray(args.bus_address)
}

#[cfg(feature = "status-notifier")]
fn run_tray(bus_address: Option<String>) -> Result<()> {
    legion_control_tray::run_status_notifier_tray(bus_address)
}

#[cfg(not(feature = "status-notifier"))]
fn run_tray(_bus_address: Option<String>) -> Result<()> {
    println!("Legion Control tray scaffold");
    println!("Read-only status summary is available with --status.");
    println!(
        "Read-only menu items: {}",
        TrayMenu::read_only_scaffold().items.len()
    );
    let desktop = DesktopSession::from_env();
    if desktop.may_need_appindicator_extension() {
        println!("Desktop note: GNOME may require an AppIndicator extension.");
    }
    println!("Build with --features status-notifier to enable the tray backend.");
    Ok(())
}
