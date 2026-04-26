use anyhow::Result;
use clap::Parser;
#[cfg(not(feature = "status-notifier"))]
use legion_control_tray::DesktopSession;
use legion_control_tray::{TrayDesktopCheck, TrayMenu, TraySummary};
use legion_control_ui::LegionControlClient;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    status: bool,

    #[arg(long)]
    tooltip: bool,

    #[arg(long)]
    desktop_check: bool,

    #[arg(long)]
    menu_check: bool,

    #[arg(long)]
    bus_address: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if [
        args.status,
        args.tooltip,
        args.desktop_check,
        args.menu_check,
    ]
    .into_iter()
    .filter(|enabled| *enabled)
    .count()
        > 1
    {
        anyhow::bail!("choose exactly one tray command");
    }

    if args.desktop_check {
        print_desktop_check(&TrayDesktopCheck::detect());
        return Ok(());
    }

    if args.status || args.tooltip || args.menu_check {
        let client = match args.bus_address {
            Some(address) => LegionControlClient::address(&address)?,
            None => LegionControlClient::system()?,
        };
        let status = client.status()?;
        let report = client.raw_probe_report()?;
        let gpu_pending = client.gpu_mode_pending()?;
        let fan_snapshot = client.last_known_good_fan_curve()?;
        let summary = TraySummary::from_status_and_report(
            &status,
            &report,
            gpu_pending.as_ref(),
            fan_snapshot.as_ref(),
        );
        if args.tooltip {
            println!("{}", summary.tooltip);
            return Ok(());
        }
        if args.menu_check {
            let menu = TrayMenu::from_status_and_report(
                &status,
                &report,
                gpu_pending.as_ref(),
                fan_snapshot.as_ref(),
            );
            for line in menu.render_lines() {
                println!("{line}");
            }
            return Ok(());
        }
        for line in summary.render_lines() {
            println!("{line}");
        }
        return Ok(());
    }

    run_tray(args.bus_address)
}

fn print_desktop_check(check: &TrayDesktopCheck) {
    for line in check.render_lines() {
        println!("{line}");
    }
}

#[cfg(feature = "status-notifier")]
fn run_tray(bus_address: Option<String>) -> Result<()> {
    legion_control_tray::run_status_notifier_tray(bus_address)
}

#[cfg(not(feature = "status-notifier"))]
fn run_tray(_bus_address: Option<String>) -> Result<()> {
    println!("Legion Control tray scaffold");
    println!("Read-only status summary is available with --status.");
    println!("Read-only menu diagnostics are available with --menu-check.");
    let desktop = DesktopSession::from_env();
    if let Some(guidance) = desktop.status_notifier_guidance() {
        println!("Desktop note: {guidance}");
    }
    println!("Build with --features status-notifier to enable the tray backend.");
    Ok(())
}
