use anyhow::Result;
use clap::Parser;
use legion_control_daemon::{
    session_connection, spawn_amd_gpu_power_profile_sync_observer, spawn_automation_observer,
    spawn_fan_preset_resume_observer, system_connection, LegionControl, PkcheckAuthorizer,
    SysfsBatteryChargeTypeWriter, SysfsCpuEppWriter, SysfsCpuGovernorWriter,
    SysfsIdeapadToggleWriter, SysfsLedStateWriter, SysfsPlatformProfileWriter, WriteAccessPolicy,
    DBUS_INTERFACE, DBUS_PATH, DEFAULT_STATE_PATH, GATED_WRITE_METHODS, READ_ONLY_METHODS,
};
use legion_probe::{probe, ProbeOptions};

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    dry_run: bool,

    #[arg(long)]
    session: bool,

    #[arg(long, default_value = "/")]
    sysfs_root: std::path::PathBuf,

    #[arg(long, default_value = DEFAULT_STATE_PATH)]
    state_path: std::path::PathBuf,

    #[arg(long)]
    enable_platform_profile_write: bool,

    #[arg(long)]
    enable_battery_charge_type_write: bool,

    #[arg(long)]
    enable_led_state_write: bool,

    #[arg(long)]
    enable_ideapad_toggle_write: bool,

    #[arg(long)]
    enable_camera_power_write: bool,

    #[arg(long)]
    enable_usb_charging_write: bool,

    #[arg(long)]
    enable_fan_mode_write: bool,

    #[arg(long)]
    enable_gpu_mode_write: bool,

    #[arg(long)]
    enable_cpu_governor_write: bool,

    #[arg(long)]
    enable_cpu_epp_write: bool,

    #[arg(long)]
    enable_firmware_attribute_write: bool,

    #[arg(long)]
    enable_cpu_boost_write: bool,

    #[arg(long)]
    enable_conservation_mode_write: bool,

    #[arg(long)]
    enable_amd_gpu_dpm_write: bool,

    #[arg(long)]
    enable_curve_optimizer_write: bool,

    #[arg(long)]
    enable_hardware_profile_apply: bool,

    /// Mirror Fedora power-profiles-daemon state to amdgpu power_dpm_force_performance_level.
    #[arg(long)]
    enable_amd_gpu_power_profile_sync: bool,

    /// Run persisted automation rules from a daemon-owned background observer.
    #[arg(long)]
    enable_automation_observer: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let options = ProbeOptions {
        sysfs_root: args.sysfs_root,
    };
    let write_policy = WriteAccessPolicy {
        platform_profile_enabled: args.enable_platform_profile_write,
        battery_charge_type_enabled: args.enable_battery_charge_type_write,
        led_state_enabled: args.enable_led_state_write,
        ideapad_toggle_enabled: args.enable_ideapad_toggle_write,
        camera_power_enabled: args.enable_camera_power_write,
        usb_charging_enabled: args.enable_usb_charging_write,
        fan_mode_enabled: args.enable_fan_mode_write,
        gpu_mode_enabled: args.enable_gpu_mode_write,
        cpu_governor_enabled: args.enable_cpu_governor_write,
        cpu_epp_enabled: args.enable_cpu_epp_write,
        firmware_attribute_enabled: args.enable_firmware_attribute_write,
        cpu_boost_enabled: args.enable_cpu_boost_write,
        conservation_mode_enabled: args.enable_conservation_mode_write,
        amd_gpu_dpm_enabled: args.enable_amd_gpu_dpm_write,
        curve_optimizer_enabled: args.enable_curve_optimizer_write,
        hardware_profile_apply_enabled: args.enable_hardware_profile_apply,
    };
    let service = LegionControl::new_with_runtime(
        options.clone(),
        args.state_path.clone(),
        write_policy.clone(),
        std::sync::Arc::new(PkcheckAuthorizer),
        std::sync::Arc::new(SysfsPlatformProfileWriter),
        std::sync::Arc::new(SysfsBatteryChargeTypeWriter),
        std::sync::Arc::new(SysfsLedStateWriter),
        std::sync::Arc::new(SysfsIdeapadToggleWriter),
        std::sync::Arc::new(SysfsCpuGovernorWriter),
        std::sync::Arc::new(SysfsCpuEppWriter),
    );

    if args.dry_run {
        let registry = probe(&options);
        println!("daemon dry-run");
        println!("interface={DBUS_INTERFACE}");
        println!("path={DBUS_PATH}");
        println!("read_only_methods={READ_ONLY_METHODS}");
        println!("gated_write_methods={GATED_WRITE_METHODS}");
        println!("enabled_write_methods={}", {
            let mut methods = Vec::new();
            if args.enable_platform_profile_write {
                methods.push("SetPlatformProfile");
            }
            if args.enable_battery_charge_type_write {
                methods.push("SetBatteryChargeType");
            }
            if args.enable_led_state_write {
                methods.push("SetLedState");
            }
            if args.enable_ideapad_toggle_write {
                methods.push("SetIdeapadToggle");
            }
            if (args.enable_camera_power_write
                || args.enable_usb_charging_write
                || args.enable_fan_mode_write)
                && !methods.contains(&"SetIdeapadToggle")
            {
                methods.push("SetIdeapadToggle");
            }
            if args.enable_gpu_mode_write {
                methods.push("SetGpuMode");
            }
            if args.enable_cpu_governor_write {
                methods.push("SetCpuGovernor");
            }
            if args.enable_cpu_epp_write {
                methods.push("SetCpuEpp");
            }
            if args.enable_firmware_attribute_write {
                methods.push("SetFirmwareAttribute");
            }
            if args.enable_cpu_boost_write {
                methods.push("SetCpuBoost");
            }
            if args.enable_conservation_mode_write {
                methods.push("SetConservationMode");
            }
            if args.enable_amd_gpu_dpm_write {
                methods.push("SetAmdGpuDpmForceLevel");
            }
            if args.enable_curve_optimizer_write {
                methods.push("SetCurveOptimizerAllCore");
            }
            if args.enable_hardware_profile_apply {
                methods.push("ApplyHardwareProfile");
            }
            if args.enable_automation_observer {
                methods.push("AutomationObserver");
            }
            if args.enable_amd_gpu_power_profile_sync {
                methods.push("AmdGpuPowerProfileSync");
            }
            if methods.is_empty() {
                "none".to_owned()
            } else {
                methods.join(",")
            }
        });
        println!("capability_count={}", registry.capabilities.len());
        return Ok(());
    }

    let _connection = if args.session {
        let connection = session_connection(service)?;
        println!("serving development session bus");
        connection
    } else {
        spawn_fan_preset_resume_observer(args.state_path.clone(), options.clone());
        if args.enable_amd_gpu_power_profile_sync {
            spawn_amd_gpu_power_profile_sync_observer(options.clone());
        }
        if args.enable_automation_observer {
            spawn_automation_observer(args.state_path, options.clone(), write_policy);
        }
        let connection = system_connection(service)?;
        println!("serving system bus");
        connection
    };

    println!("serving interface={DBUS_INTERFACE} path={DBUS_PATH}");
    loop {
        std::thread::park();
    }
}
