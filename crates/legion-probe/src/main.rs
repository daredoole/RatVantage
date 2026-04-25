use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use legion_probe::{probe, ProbeOptions};

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    json: bool,

    #[arg(long, default_value = "/")]
    sysfs_root: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let registry = probe(&ProbeOptions {
        sysfs_root: args.sysfs_root,
    });

    if args.json {
        println!("{}", serde_json::to_string_pretty(&registry)?);
    } else {
        println!(
            "Detected hardware: {}",
            registry
                .hardware
                .product_name
                .as_deref()
                .unwrap_or("unknown")
        );
        println!("Capabilities: {}", registry.capabilities.len());
    }

    Ok(())
}
