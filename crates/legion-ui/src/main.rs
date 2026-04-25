use anyhow::{bail, Result};
use clap::Parser;
#[cfg(not(feature = "gtk-ui"))]
use legion_control_ui::DBUS_INTERFACE;
use legion_control_ui::{
    render_diagnostics_json, render_overview_lines, render_write_plan_json, DiagnosticsBundle,
    LegionControlClient, UiStatus,
};

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    status: bool,

    #[arg(long)]
    overview: bool,

    #[arg(long)]
    diagnostics: bool,

    #[arg(long, value_name = "PROFILE")]
    plan_platform_profile: Option<String>,

    #[arg(long, value_name = "CHARGE_TYPE")]
    plan_battery_charge_type: Option<String>,

    #[arg(long, value_name = "MODE")]
    plan_gpu_mode: Option<String>,

    #[arg(long)]
    bus_address: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let action_count = [
        args.status,
        args.overview,
        args.diagnostics,
        args.plan_platform_profile.is_some(),
        args.plan_battery_charge_type.is_some(),
        args.plan_gpu_mode.is_some(),
    ]
    .into_iter()
    .filter(|enabled| *enabled)
    .count();
    if action_count > 1 {
        bail!("choose exactly one read-only UI command");
    }

    if action_count == 1 {
        let client = match args.bus_address {
            Some(address) => LegionControlClient::address(&address)?,
            None => LegionControlClient::system()?,
        };
        if let Some(profile) = args.plan_platform_profile {
            print_write_plan(&client.plan_platform_profile_write(&profile)?)?;
        } else if let Some(charge_type) = args.plan_battery_charge_type {
            print_write_plan(&client.plan_battery_charge_type_write(&charge_type)?)?;
        } else if let Some(mode) = args.plan_gpu_mode {
            print_write_plan(&client.plan_gpu_mode_write(&mode)?)?;
        } else if args.diagnostics {
            print_diagnostics(&client.diagnostics_bundle()?)?;
        } else if args.overview {
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

fn print_diagnostics(bundle: &DiagnosticsBundle) -> Result<()> {
    println!("{}", render_diagnostics_json(bundle)?);
    Ok(())
}

fn print_write_plan(plan: &legion_common::WriteDryRunPlan) -> Result<()> {
    println!("{}", render_write_plan_json(plan)?);
    Ok(())
}
