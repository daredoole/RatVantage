use anyhow::Result;

fn main() -> Result<()> {
    println!("Legion Control UI scaffold");
    println!("D-Bus target: org.ratvantage.LegionControl1");
    println!("Read-only client module is available for hardware summary and capabilities.");
    println!("Direct sysfs access is intentionally not implemented.");
    Ok(())
}
