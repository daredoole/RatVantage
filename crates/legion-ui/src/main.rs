use anyhow::{bail, Result};
use clap::Parser;
#[cfg(not(feature = "gtk-ui"))]
use legion_control_ui::DBUS_INTERFACE;
use legion_control_ui::{
    render_diagnostics_json, render_overview_lines_with_pending, render_write_plan_json,
    DiagnosticsBundle, LegionControlClient, UiStatus,
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

    #[arg(long, value_name = "PROFILE")]
    set_platform_profile: Option<String>,

    #[arg(long, value_name = "CHARGE_TYPE")]
    plan_battery_charge_type: Option<String>,

    #[arg(long, value_name = "CHARGE_TYPE")]
    set_battery_charge_type: Option<String>,

    #[arg(long, value_name = "LED_ID=on|off")]
    plan_led_state: Option<String>,

    #[arg(long, value_name = "LED_ID=on|off")]
    set_led_state: Option<String>,

    #[arg(long, value_name = "MODE")]
    plan_gpu_mode: Option<String>,

    #[arg(long, value_name = "PRESET_ID")]
    plan_fan_preset: Option<String>,

    #[arg(long)]
    plan_restore_auto_fan: bool,

    #[arg(long)]
    gpu_mode_pending: bool,

    #[arg(long, value_name = "MODE")]
    set_gpu_mode_pending: Option<String>,

    #[arg(long)]
    clear_gpu_mode_pending: bool,

    #[arg(long)]
    last_known_good_fan_curve: bool,

    #[arg(long)]
    capture_last_known_good_fan_curve: bool,

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
        args.set_platform_profile.is_some(),
        args.plan_battery_charge_type.is_some(),
        args.set_battery_charge_type.is_some(),
        args.plan_led_state.is_some(),
        args.set_led_state.is_some(),
        args.plan_gpu_mode.is_some(),
        args.plan_fan_preset.is_some(),
        args.plan_restore_auto_fan,
        args.gpu_mode_pending,
        args.set_gpu_mode_pending.is_some(),
        args.clear_gpu_mode_pending,
        args.last_known_good_fan_curve,
        args.capture_last_known_good_fan_curve,
    ]
    .into_iter()
    .filter(|enabled| *enabled)
    .count();
    if action_count > 1 {
        bail!("choose exactly one UI command");
    }

    if action_count == 1 {
        let client = match args.bus_address {
            Some(address) => LegionControlClient::address(&address)?,
            None => LegionControlClient::system()?,
        };
        if let Some(profile) = args.plan_platform_profile {
            print_write_plan(&client.plan_platform_profile_write(&profile)?)?;
        } else if let Some(profile) = args.set_platform_profile {
            print_json(&client.set_platform_profile(&profile)?)?;
        } else if let Some(charge_type) = args.plan_battery_charge_type {
            print_write_plan(&client.plan_battery_charge_type_write(&charge_type)?)?;
        } else if let Some(charge_type) = args.set_battery_charge_type {
            print_json(&client.set_battery_charge_type(&charge_type)?)?;
        } else if let Some(spec) = args.plan_led_state {
            let (led_id, enabled) = parse_led_state_spec(&spec)?;
            print_write_plan(&client.plan_led_state_write(&led_id, enabled)?)?;
        } else if let Some(spec) = args.set_led_state {
            let (led_id, enabled) = parse_led_state_spec(&spec)?;
            print_json(&client.set_led_state(&led_id, enabled)?)?;
        } else if let Some(mode) = args.plan_gpu_mode {
            print_write_plan(&client.plan_gpu_mode_write(&mode)?)?;
        } else if let Some(preset_id) = args.plan_fan_preset {
            print_write_plan(&client.plan_fan_preset_write(&preset_id)?)?;
        } else if args.plan_restore_auto_fan {
            print_write_plan(&client.plan_restore_auto_fan_write()?)?;
        } else if args.gpu_mode_pending {
            print_json(&client.gpu_mode_pending()?)?;
        } else if let Some(mode) = args.set_gpu_mode_pending {
            print_json(&client.set_gpu_mode_pending(&mode)?)?;
        } else if args.clear_gpu_mode_pending {
            print_json(&client.clear_gpu_mode_pending()?)?;
        } else if args.last_known_good_fan_curve {
            print_json(&client.last_known_good_fan_curve()?)?;
        } else if args.capture_last_known_good_fan_curve {
            print_json(&client.capture_last_known_good_fan_curve()?)?;
        } else if args.diagnostics {
            print_diagnostics(&client.diagnostics_bundle()?)?;
        } else if args.overview {
            print_overview(
                &client.raw_probe_report()?,
                client.gpu_mode_pending()?,
                client.last_known_good_fan_curve()?,
            );
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

fn print_overview(
    report: &legion_common::CapabilityRegistry,
    pending: Option<legion_common::GpuModePending>,
    fan_snapshot: Option<legion_common::FanCurveSnapshot>,
) {
    for line in render_overview_lines_with_pending(report, pending.as_ref(), fan_snapshot.as_ref())
    {
        println!("{line}");
    }
}

fn print_diagnostics(bundle: &DiagnosticsBundle) -> Result<()> {
    println!("{}", render_diagnostics_json(bundle)?);
    Ok(())
}

fn print_write_plan(plan: &legion_common::WriteDryRunPlan) -> Result<()> {
    print_json(plan)
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<()> {
    println!("{}", render_write_plan_json(value)?);
    Ok(())
}

fn parse_led_state_spec(spec: &str) -> Result<(String, bool)> {
    let Some((led_id, requested)) = spec.split_once('=') else {
        bail!("expected LED spec in the form <led_id>=on|off");
    };

    let enabled = match requested {
        "1" | "on" | "true" => true,
        "0" | "off" | "false" => false,
        _ => bail!("expected LED state to be one of: on, off, true, false, 1, 0"),
    };

    Ok((led_id.to_owned(), enabled))
}
