use anyhow::Result;
use clap::Parser;
use legion_control_daemon::{
    session_connection, system_connection, LegionControl, PkcheckAuthorizer,
    SysfsBatteryChargeTypeWriter, SysfsIdeapadToggleWriter, SysfsLedStateWriter,
    SysfsPlatformProfileWriter, WriteAccessPolicy, DBUS_INTERFACE, DBUS_PATH, DEFAULT_STATE_PATH,
    GATED_WRITE_METHODS, READ_ONLY_METHODS,
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
}

fn main() -> Result<()> {
    let args = Args::parse();
    let options = ProbeOptions {
        sysfs_root: args.sysfs_root,
    };
    let service = LegionControl::new_with_runtime(
        options.clone(),
        args.state_path,
        WriteAccessPolicy {
            platform_profile_enabled: args.enable_platform_profile_write,
            battery_charge_type_enabled: args.enable_battery_charge_type_write,
            led_state_enabled: args.enable_led_state_write,
            ideapad_toggle_enabled: args.enable_ideapad_toggle_write,
            camera_power_enabled: args.enable_camera_power_write,
            usb_charging_enabled: args.enable_usb_charging_write,
        },
        std::sync::Arc::new(PkcheckAuthorizer),
        std::sync::Arc::new(SysfsPlatformProfileWriter),
        std::sync::Arc::new(SysfsBatteryChargeTypeWriter),
        std::sync::Arc::new(SysfsLedStateWriter),
        std::sync::Arc::new(SysfsIdeapadToggleWriter),
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
            if (args.enable_camera_power_write || args.enable_usb_charging_write)
                && !methods.contains(&"SetIdeapadToggle")
            {
                methods.push("SetIdeapadToggle");
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
        let connection = system_connection(service)?;
        println!("serving system bus");
        connection
    };

    println!("serving interface={DBUS_INTERFACE} path={DBUS_PATH}");
    loop {
        std::thread::park();
    }
}
