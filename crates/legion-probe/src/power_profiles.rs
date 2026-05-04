//! Read-only D-Bus probe for `org.freedesktop.UPower.PowerProfiles` (generic desktop power profile).

use std::path::Path;

use legion_common::{CapabilityStatus, PowerProfilesCapability};
use zbus::blocking::Connection;
use zbus::blocking::Proxy;

const WELL_KNOWN: &str = "org.freedesktop.UPower.PowerProfiles";
const OBJECT_PATH: &str = "/org/freedesktop/UPower/PowerProfiles";
const IFACE: &str = "org.freedesktop.UPower.PowerProfiles";

/// When `sysfs_root` is not `/`, skip entirely so fixture and CI trees stay deterministic.
pub fn detect_power_profiles(sysfs_root: &Path) -> Option<PowerProfilesCapability> {
    if sysfs_root != Path::new("/") {
        return None;
    }

    let system = detect_power_profiles_on_bus("system", Connection::system);
    if system
        .as_ref()
        .and_then(|capability| capability.unique_owner.as_ref())
        .is_some()
    {
        return system;
    }

    let session = detect_power_profiles_on_bus("session", Connection::session);
    if session
        .as_ref()
        .and_then(|capability| capability.unique_owner.as_ref())
        .is_some()
    {
        return session;
    }

    system.or(session)
}

fn detect_power_profiles_on_bus(
    bus: &str,
    connect: fn() -> zbus::Result<Connection>,
) -> Option<PowerProfilesCapability> {
    let conn = match connect() {
        Ok(c) => c,
        Err(error) => {
            return Some(PowerProfilesCapability {
                bus: bus.to_owned(),
                well_known_name: WELL_KNOWN.to_owned(),
                unique_owner: None,
                active_profile: None,
                status: CapabilityStatus::Missing,
                detail: Some(format!("{bus}_bus_unavailable: {error}")),
            });
        }
    };

    let dbus = match Proxy::new(
        &conn,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    ) {
        Ok(p) => p,
        Err(error) => {
            return Some(PowerProfilesCapability {
                bus: bus.to_owned(),
                well_known_name: WELL_KNOWN.to_owned(),
                unique_owner: None,
                active_profile: None,
                status: CapabilityStatus::Missing,
                detail: Some(format!("dbus_proxy: {error}")),
            });
        }
    };

    let owner = match dbus.call_method("GetNameOwner", &(WELL_KNOWN,)) {
        Ok(reply) => match reply.body().deserialize::<String>() {
            Ok(s) => s,
            Err(error) => {
                return Some(PowerProfilesCapability {
                    bus: bus.to_owned(),
                    well_known_name: WELL_KNOWN.to_owned(),
                    unique_owner: None,
                    active_profile: None,
                    status: CapabilityStatus::Missing,
                    detail: Some(format!("get_name_owner_body: {error}")),
                });
            }
        },
        Err(zbus::Error::MethodError(name, _, _))
            if name.as_str() == "org.freedesktop.DBus.Error.NameHasNoOwner" =>
        {
            return Some(PowerProfilesCapability {
                bus: bus.to_owned(),
                well_known_name: WELL_KNOWN.to_owned(),
                unique_owner: None,
                active_profile: None,
                status: CapabilityStatus::Missing,
                detail: Some("name_has_no_owner".to_owned()),
            });
        }
        Err(error) => {
            return Some(PowerProfilesCapability {
                bus: bus.to_owned(),
                well_known_name: WELL_KNOWN.to_owned(),
                unique_owner: None,
                active_profile: None,
                status: CapabilityStatus::Missing,
                detail: Some(format!("get_name_owner: {error}")),
            });
        }
    };

    let active_profile = Proxy::new(&conn, WELL_KNOWN, OBJECT_PATH, IFACE)
        .ok()
        .and_then(|proxy| proxy.get_property::<String>("ActiveProfile").ok());

    Some(PowerProfilesCapability {
        bus: bus.to_owned(),
        well_known_name: WELL_KNOWN.to_owned(),
        unique_owner: Some(owner),
        active_profile,
        status: CapabilityStatus::ProbeOnly,
        detail: None,
    })
}
