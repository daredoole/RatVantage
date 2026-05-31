use anyhow::Result;
use clap::Parser;
use legion_common::WriteExecutionResult;
#[cfg(not(feature = "status-notifier"))]
use legion_control_tray::DesktopSession;
use legion_control_tray::{TrayAction, TrayDesktopCheck, TrayMenu, TraySummary};
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

    #[arg(long, value_name = "ACTION")]
    action: Option<String>,

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
        args.action.is_some(),
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
            Some(ref address) => LegionControlClient::address(address)?,
            None => LegionControlClient::system()?,
        };
        let snapshot = client.refresh_runtime_snapshot()?;
        let status = snapshot.status;
        let report = snapshot.diagnostics.raw_probe_report;
        let gpu_pending = snapshot.diagnostics.gpu_mode_pending;
        let fan_snapshot = snapshot.diagnostics.last_known_good_fan_curve;
        let hardware_profile_apply = snapshot.diagnostics.last_hardware_profile_apply;
        let summary = TraySummary::from_status_and_report(
            &status,
            &report,
            gpu_pending.as_ref(),
            fan_snapshot.as_ref(),
            hardware_profile_apply.as_ref(),
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
                hardware_profile_apply.as_ref(),
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

    if let Some(action) = args.action {
        let client = match args.bus_address {
            Some(address) => LegionControlClient::address(&address)?,
            None => LegionControlClient::system()?,
        };
        let action: TrayAction = action.parse().map_err(anyhow::Error::msg)?;
        let result = execute_action(&client, &action)?;
        print_action_result(&action, &result);
        return Ok(());
    }

    run_tray(args.bus_address)
}

fn print_desktop_check(check: &TrayDesktopCheck) {
    for line in check.render_lines() {
        println!("{line}");
    }
}

fn execute_action(
    client: &LegionControlClient,
    action: &TrayAction,
) -> Result<WriteExecutionResult> {
    match action {
        TrayAction::SetPlatformProfile(profile) => client.set_platform_profile(profile),
        TrayAction::SetBatteryChargeType(charge_type) => {
            client.set_battery_charge_type(charge_type)
        }
        TrayAction::SetLedState(led_id, enabled) => client.set_led_state(led_id, *enabled),
        TrayAction::SetIdeapadToggle(toggle_id, enabled) => {
            client.set_ideapad_toggle(toggle_id, *enabled)
        }
        _ => anyhow::bail!(
            "tray action `{}` is not a daemon write action",
            action.as_str()
        ),
    }
}

fn print_action_result(action: &TrayAction, result: &WriteExecutionResult) {
    println!("action={}", action.as_str());
    println!("status={:?}", result.status);
    println!("applied={}", result.applied);
    println!("message={}", result.message);
    if let Some(readback) = &result.readback_value {
        println!("readback={readback}");
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
