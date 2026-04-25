use anyhow::Result;
use clap::Parser;
use legion_control_daemon::{
    system_connection, LegionControl, DBUS_INTERFACE, DBUS_PATH, READ_ONLY_METHODS,
};
use legion_probe::{probe, ProbeOptions};

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    dry_run: bool,

    #[arg(long, default_value = "/")]
    sysfs_root: std::path::PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let options = ProbeOptions {
        sysfs_root: args.sysfs_root,
    };
    let service = LegionControl::new(options.clone());

    if args.dry_run {
        let registry = probe(&options);
        println!("daemon dry-run");
        println!("interface={DBUS_INTERFACE}");
        println!("path={DBUS_PATH}");
        println!("read_only_methods={READ_ONLY_METHODS}");
        println!("capability_count={}", registry.capabilities.len());
        return Ok(());
    }

    let _connection = system_connection(service)?;

    println!("serving interface={DBUS_INTERFACE} path={DBUS_PATH}");
    loop {
        std::thread::park();
    }
}
