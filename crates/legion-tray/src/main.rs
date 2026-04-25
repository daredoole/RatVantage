use anyhow::Result;
use clap::Parser;
use legion_control_tray::{DesktopSession, TrayMenu, TraySummary};
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
    println!("AppIndicator/StatusNotifier integration is intentionally not implemented yet.");
    Ok(())
}
