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

    #[arg(long, value_name = "TOGGLE_ID=on|off")]
    plan_ideapad_toggle: Option<String>,

    #[arg(long, value_name = "TOGGLE_ID=on|off")]
    set_ideapad_toggle: Option<String>,

    #[arg(long, value_name = "ATTRIBUTE_ID=VALUE")]
    plan_firmware_attribute: Option<String>,

    #[arg(long, value_name = "ATTRIBUTE_ID=VALUE")]
    set_firmware_attribute: Option<String>,

    #[arg(long, value_name = "0|1")]
    plan_cpu_boost: Option<String>,

    #[arg(long, value_name = "0|1")]
    set_cpu_boost: Option<String>,

    #[arg(long, value_name = "GOVERNOR")]
    plan_cpu_governor: Option<String>,

    #[arg(long, value_name = "GOVERNOR")]
    set_cpu_governor: Option<String>,

    #[arg(long, value_name = "EPP")]
    plan_cpu_epp: Option<String>,

    #[arg(long, value_name = "EPP")]
    set_cpu_epp: Option<String>,

    #[arg(long, value_name = "0|-1..-30")]
    plan_curve_optimizer_all_core: Option<String>,

    #[arg(long, value_name = "0|-1..-30")]
    set_curve_optimizer_all_core: Option<String>,

    #[arg(long)]
    reset_curve_optimizer_all_core: bool,

    #[arg(long)]
    last_curve_optimizer_all_core: bool,

    #[arg(long, value_name = "0|1")]
    plan_conservation_mode: Option<String>,

    #[arg(long, value_name = "0|1")]
    set_conservation_mode: Option<String>,

    #[arg(long, value_name = "LEVEL")]
    plan_amd_gpu_dpm_force_level: Option<String>,

    #[arg(long, value_name = "LEVEL")]
    set_amd_gpu_dpm_force_level: Option<String>,

    #[arg(long, value_name = "MODE")]
    plan_gpu_mode: Option<String>,

    #[arg(long, value_name = "MODE")]
    set_gpu_mode: Option<String>,

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
    hardware_profiles: bool,

    #[arg(long)]
    hardware_profile_triggers: bool,

    #[arg(long)]
    automation_rules: bool,

    #[arg(long)]
    last_automation_rule_apply: bool,

    #[arg(long, value_name = "PROFILE_ID")]
    plan_hardware_profile: Option<String>,

    #[arg(long, value_name = "TRIGGER_ID")]
    plan_hardware_profile_trigger: Option<String>,

    #[arg(long, value_name = "PROFILE_ID")]
    apply_hardware_profile: Option<String>,

    #[arg(long, value_name = "TRIGGER_ID")]
    apply_hardware_profile_trigger: Option<String>,

    #[arg(long)]
    last_hardware_profile_apply: bool,

    #[arg(long, value_name = "PROFILE_ID=JSON")]
    set_hardware_profile: Option<String>,

    #[arg(long, value_name = "TRIGGER_ID=PROFILE_ID")]
    set_hardware_profile_trigger: Option<String>,

    #[arg(long, value_name = "TRIGGER_ID")]
    remove_hardware_profile_trigger: Option<String>,

    #[arg(long)]
    clear_hardware_profile_triggers: bool,

    #[arg(long, value_name = "PROFILE_ID")]
    remove_hardware_profile: Option<String>,

    #[arg(long)]
    clear_hardware_profiles: bool,

    #[arg(long)]
    last_known_good_fan_curve: bool,

    #[arg(long)]
    capture_last_known_good_fan_curve: bool,

    #[arg(long)]
    fan_curve_live: bool,

    #[arg(long)]
    bus_address: Option<String>,

    #[cfg(feature = "gtk-ui")]
    #[arg(long, value_name = "PAGE")]
    gtk_page: Option<String>,

    #[cfg(feature = "gtk-ui")]
    #[arg(long, value_name = "MILLISECONDS")]
    gtk_auto_quit_ms: Option<u64>,
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
        args.plan_ideapad_toggle.is_some(),
        args.set_ideapad_toggle.is_some(),
        args.plan_firmware_attribute.is_some(),
        args.set_firmware_attribute.is_some(),
        args.plan_cpu_boost.is_some(),
        args.set_cpu_boost.is_some(),
        args.plan_cpu_governor.is_some(),
        args.set_cpu_governor.is_some(),
        args.plan_cpu_epp.is_some(),
        args.set_cpu_epp.is_some(),
        args.plan_curve_optimizer_all_core.is_some(),
        args.set_curve_optimizer_all_core.is_some(),
        args.reset_curve_optimizer_all_core,
        args.last_curve_optimizer_all_core,
        args.plan_conservation_mode.is_some(),
        args.set_conservation_mode.is_some(),
        args.plan_amd_gpu_dpm_force_level.is_some(),
        args.set_amd_gpu_dpm_force_level.is_some(),
        args.plan_gpu_mode.is_some(),
        args.set_gpu_mode.is_some(),
        args.plan_fan_preset.is_some(),
        args.plan_restore_auto_fan,
        args.gpu_mode_pending,
        args.set_gpu_mode_pending.is_some(),
        args.clear_gpu_mode_pending,
        args.hardware_profiles,
        args.hardware_profile_triggers,
        args.automation_rules,
        args.last_automation_rule_apply,
        args.plan_hardware_profile.is_some(),
        args.plan_hardware_profile_trigger.is_some(),
        args.apply_hardware_profile.is_some(),
        args.apply_hardware_profile_trigger.is_some(),
        args.last_hardware_profile_apply,
        args.set_hardware_profile.is_some(),
        args.set_hardware_profile_trigger.is_some(),
        args.remove_hardware_profile_trigger.is_some(),
        args.clear_hardware_profile_triggers,
        args.remove_hardware_profile.is_some(),
        args.clear_hardware_profiles,
        args.last_known_good_fan_curve,
        args.capture_last_known_good_fan_curve,
        args.fan_curve_live,
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
        } else if let Some(spec) = args.plan_ideapad_toggle {
            let (toggle_id, enabled) = parse_binary_switch_spec(&spec, "ideapad toggle")?;
            print_write_plan(&client.plan_ideapad_toggle_write(&toggle_id, enabled)?)?;
        } else if let Some(spec) = args.set_ideapad_toggle {
            let (toggle_id, enabled) = parse_binary_switch_spec(&spec, "ideapad toggle")?;
            print_json(&client.set_ideapad_toggle(&toggle_id, enabled)?)?;
        } else if let Some(spec) = args.plan_firmware_attribute {
            let (attribute_id, requested) = parse_key_value_spec(&spec, "firmware attribute")?;
            print_write_plan(&client.plan_firmware_attribute_write(&attribute_id, &requested)?)?;
        } else if let Some(spec) = args.set_firmware_attribute {
            let (attribute_id, requested) = parse_key_value_spec(&spec, "firmware attribute")?;
            print_json(&client.set_firmware_attribute(&attribute_id, &requested)?)?;
        } else if let Some(requested) = args.plan_cpu_boost {
            print_write_plan(&client.plan_cpu_boost_write(&requested)?)?;
        } else if let Some(requested) = args.set_cpu_boost {
            print_json(&client.set_cpu_boost(&requested)?)?;
        } else if let Some(requested) = args.plan_cpu_governor {
            print_write_plan(&client.plan_cpu_governor_write(&requested)?)?;
        } else if let Some(requested) = args.set_cpu_governor {
            print_json(&client.set_cpu_governor(&requested)?)?;
        } else if let Some(requested) = args.plan_cpu_epp {
            print_write_plan(&client.plan_cpu_epp_write(&requested)?)?;
        } else if let Some(requested) = args.set_cpu_epp {
            print_json(&client.set_cpu_epp(&requested)?)?;
        } else if let Some(requested) = args.plan_curve_optimizer_all_core {
            print_write_plan(&client.plan_curve_optimizer_all_core_write(&requested)?)?;
        } else if let Some(requested) = args.set_curve_optimizer_all_core {
            print_json(&client.set_curve_optimizer_all_core(&requested)?)?;
        } else if args.reset_curve_optimizer_all_core {
            print_json(&client.set_curve_optimizer_all_core("0")?)?;
        } else if args.last_curve_optimizer_all_core {
            print_json(&client.last_curve_optimizer_all_core()?)?;
        } else if let Some(requested) = args.plan_conservation_mode {
            print_write_plan(&client.plan_conservation_mode_write(&requested)?)?;
        } else if let Some(requested) = args.set_conservation_mode {
            print_json(&client.set_conservation_mode(&requested)?)?;
        } else if let Some(requested) = args.plan_amd_gpu_dpm_force_level {
            print_write_plan(&client.plan_amd_gpu_dpm_force_level_write(&requested)?)?;
        } else if let Some(requested) = args.set_amd_gpu_dpm_force_level {
            print_json(&client.set_amd_gpu_dpm_force_level(&requested)?)?;
        } else if let Some(mode) = args.plan_gpu_mode {
            print_write_plan(&client.plan_gpu_mode_write(&mode)?)?;
        } else if let Some(mode) = args.set_gpu_mode {
            print_json(&client.set_gpu_mode(&mode)?)?;
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
        } else if args.hardware_profiles {
            print_json(&client.hardware_profiles()?)?;
        } else if args.hardware_profile_triggers {
            print_json(&client.hardware_profile_triggers()?)?;
        } else if args.automation_rules {
            print_json(&client.automation_rules()?)?;
        } else if args.last_automation_rule_apply {
            print_json(&client.last_automation_rule_apply()?)?;
        } else if let Some(profile_id) = args.plan_hardware_profile {
            print_json(&client.hardware_profile_apply_preview(&profile_id)?)?;
        } else if let Some(trigger_id) = args.plan_hardware_profile_trigger {
            print_json(&client.hardware_profile_trigger_apply_preview(&trigger_id)?)?;
        } else if let Some(profile_id) = args.apply_hardware_profile {
            print_json(&client.apply_hardware_profile(&profile_id)?)?;
        } else if let Some(trigger_id) = args.apply_hardware_profile_trigger {
            print_json(&client.apply_hardware_profile_trigger(&trigger_id)?)?;
        } else if args.last_hardware_profile_apply {
            print_json(&client.last_hardware_profile_apply()?)?;
        } else if let Some(spec) = args.set_hardware_profile {
            let (profile_id, profile_json) = parse_key_value_spec(&spec, "hardware profile")?;
            print_json(&client.set_hardware_profile(&profile_id, &profile_json)?)?;
        } else if let Some(spec) = args.set_hardware_profile_trigger {
            let (trigger_id, profile_id) = parse_key_value_spec(&spec, "hardware profile trigger")?;
            print_json(&client.set_hardware_profile_trigger(&trigger_id, &profile_id)?)?;
        } else if let Some(trigger_id) = args.remove_hardware_profile_trigger {
            print_json(&client.remove_hardware_profile_trigger(&trigger_id)?)?;
        } else if args.clear_hardware_profile_triggers {
            print_json(&client.clear_hardware_profile_triggers()?)?;
        } else if let Some(profile_id) = args.remove_hardware_profile {
            print_json(&client.remove_hardware_profile(&profile_id)?)?;
        } else if args.clear_hardware_profiles {
            print_json(&client.clear_hardware_profiles()?)?;
        } else if args.last_known_good_fan_curve {
            print_json(&client.last_known_good_fan_curve()?)?;
        } else if args.capture_last_known_good_fan_curve {
            print_json(&client.capture_last_known_good_fan_curve()?)?;
        } else if args.fan_curve_live {
            print_json(&client.live_fan_curve_readings()?)?;
        } else if args.diagnostics {
            print_diagnostics(&client.diagnostics_bundle()?)?;
        } else if args.overview {
            print_overview(
                &client.raw_probe_report()?,
                client.gpu_mode_pending()?,
                client.last_known_good_fan_curve()?,
                client.fan_preset_by_platform_profile()?,
                client.fan_preset_reapply_after_resume()?,
            );
        } else {
            print_status(&client.status()?);
        }
        return Ok(());
    }

    #[cfg(feature = "gtk-ui")]
    {
        legion_control_ui::gtk_shell::run(args.bus_address, args.gtk_page, args.gtk_auto_quit_ms)
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
    fan_preset_map: std::collections::BTreeMap<String, String>,
    fan_preset_reapply_after_resume: bool,
) {
    for line in render_overview_lines_with_pending(
        report,
        pending.as_ref(),
        fan_snapshot.as_ref(),
        &fan_preset_map,
        fan_preset_reapply_after_resume,
    ) {
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
    parse_binary_switch_spec(spec, "LED")
}

fn parse_binary_switch_spec(spec: &str, label: &str) -> Result<(String, bool)> {
    let (led_id, requested) = parse_key_value_spec(spec, label)?;

    let enabled = match requested.as_str() {
        "1" | "on" | "true" => true,
        "0" | "off" | "false" => false,
        _ => bail!("expected {label} state to be one of: on, off, true, false, 1, 0"),
    };

    Ok((led_id, enabled))
}

fn parse_key_value_spec(spec: &str, label: &str) -> Result<(String, String)> {
    let Some((id, requested)) = spec.split_once('=') else {
        bail!("expected {label} spec in the form <id>=<value>");
    };
    if id.trim().is_empty() || requested.trim().is_empty() {
        bail!("expected {label} spec to contain a non-empty id and value");
    }
    Ok((id.to_owned(), requested.to_owned()))
}
