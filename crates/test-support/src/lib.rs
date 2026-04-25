use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use serde::de::DeserializeOwned;
use zbus::blocking::Proxy;

pub struct PrivateBus {
    address: String,
    child: Child,
}

impl PrivateBus {
    pub fn start() -> Self {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--print-address=1", "--nofork"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("dbus-daemon must be available for D-Bus integration tests");
        let stdout = child.stdout.take().unwrap();
        let mut lines = BufReader::new(stdout).lines();
        let address = lines.next().unwrap().unwrap();

        Self { address, child }
    }

    pub fn address(&self) -> &str {
        &self.address
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("tests/fixtures/sysfs-82wm-confirmed")
}

pub fn call_json<T>(proxy: &Proxy<'_>, method: &str) -> T
where
    T: DeserializeOwned,
{
    let payload: String = proxy.call(method, &()).unwrap();
    serde_json::from_str(&payload).unwrap()
}

pub fn introspected_methods(xml: &str, interface: &str) -> Vec<String> {
    let Some(interface_start) = xml.find(&format!("<interface name=\"{interface}\">")) else {
        return Vec::new();
    };
    let interface_xml = &xml[interface_start..];
    let Some(interface_end) = interface_xml.find("</interface>") else {
        return Vec::new();
    };

    interface_xml[..interface_end]
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("<method name=\"")?
                .split_once('"')
                .map(|(name, _)| name.to_owned())
        })
        .collect()
}
