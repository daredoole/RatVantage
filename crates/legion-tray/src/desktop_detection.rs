use std::path::Path;

use zbus::blocking::{Connection, Proxy};

pub const TRAY_AUTOSTART_DESKTOP_FILE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../data/desktop/org.ratvantage.LegionControl.Tray.desktop"
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopSession {
    pub current_desktop: Option<String>,
    pub session_type: Option<String>,
    pub wayland_display: Option<String>,
    pub display: Option<String>,
    pub kde_full_session: Option<String>,
    pub dbus_session_bus_address: Option<String>,
}

impl DesktopSession {
    pub fn from_env() -> Self {
        Self {
            current_desktop: std::env::var("XDG_CURRENT_DESKTOP").ok(),
            session_type: std::env::var("XDG_SESSION_TYPE").ok(),
            wayland_display: std::env::var("WAYLAND_DISPLAY").ok(),
            display: std::env::var("DISPLAY").ok(),
            kde_full_session: std::env::var("KDE_FULL_SESSION").ok(),
            dbus_session_bus_address: std::env::var("DBUS_SESSION_BUS_ADDRESS").ok(),
        }
    }

    pub fn from_values(
        current_desktop: Option<&str>,
        session_type: Option<&str>,
        wayland_display: Option<&str>,
        display: Option<&str>,
        kde_full_session: Option<&str>,
        dbus_session_bus_address: Option<&str>,
    ) -> Self {
        Self {
            current_desktop: current_desktop.map(str::to_owned),
            session_type: session_type.map(str::to_owned),
            wayland_display: wayland_display.map(str::to_owned),
            display: display.map(str::to_owned),
            kde_full_session: kde_full_session.map(str::to_owned),
            dbus_session_bus_address: dbus_session_bus_address.map(str::to_owned),
        }
    }

    pub fn prefers_status_notifier(&self) -> bool {
        self.current_desktop.as_deref().is_some_and(|desktop| {
            contains_token(desktop, "KDE") || contains_token(desktop, "PLASMA")
        })
    }

    pub fn may_need_appindicator_extension(&self) -> bool {
        self.current_desktop
            .as_deref()
            .is_some_and(|desktop| contains_token(desktop, "GNOME"))
    }

    pub fn has_session_bus(&self) -> bool {
        self.dbus_session_bus_address
            .as_deref()
            .is_some_and(|address| !address.is_empty())
    }

    pub fn status_notifier_guidance(&self) -> Option<&'static str> {
        if self.prefers_status_notifier() {
            Some("KDE/Plasma should expose StatusNotifier items natively.")
        } else if self.may_need_appindicator_extension() {
            Some("GNOME may require an AppIndicator/KStatusNotifier extension for tray icons.")
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayDesktopCheck {
    pub session: DesktopSession,
    pub status_notifier_watcher_available: bool,
    pub autostart_hidden: bool,
    pub gnome_autostart_disabled: bool,
}

impl TrayDesktopCheck {
    pub fn detect() -> Self {
        let session = DesktopSession::from_env();
        let (autostart_hidden, gnome_autostart_disabled) =
            autostart_desktop_flags(Path::new(TRAY_AUTOSTART_DESKTOP_FILE));
        Self::from_parts(
            session,
            status_notifier_watcher_available(),
            autostart_hidden,
            gnome_autostart_disabled,
        )
    }

    pub fn from_parts(
        session: DesktopSession,
        status_notifier_watcher_available: bool,
        autostart_hidden: bool,
        gnome_autostart_disabled: bool,
    ) -> Self {
        Self {
            session,
            status_notifier_watcher_available,
            autostart_hidden,
            gnome_autostart_disabled,
        }
    }

    pub fn render_lines(&self) -> Vec<String> {
        let mut lines = vec![
            "RatVantage tray desktop check".to_owned(),
            format!(
                "current_desktop={}",
                render_optional(&self.session.current_desktop)
            ),
            format!(
                "session_type={}",
                render_optional(&self.session.session_type)
            ),
            format!(
                "wayland_display={}",
                render_optional(&self.session.wayland_display)
            ),
            format!("display={}", render_optional(&self.session.display)),
            format!(
                "kde_full_session={}",
                render_optional(&self.session.kde_full_session)
            ),
            format!(
                "dbus_session_bus_address_set={}",
                self.session.has_session_bus()
            ),
            format!(
                "prefers_status_notifier={}",
                self.session.prefers_status_notifier()
            ),
            format!(
                "may_need_appindicator_extension={}",
                self.session.may_need_appindicator_extension()
            ),
            format!(
                "status_notifier_watcher_available={}",
                self.status_notifier_watcher_available
            ),
            format!("autostart_hidden={}", self.autostart_hidden),
            format!("gnome_autostart_disabled={}", self.gnome_autostart_disabled),
        ];
        if let Some(guidance) = self.session.status_notifier_guidance() {
            lines.push(format!("desktop_guidance={guidance}"));
        }
        lines
    }
}

pub fn autostart_desktop_flags(path: &Path) -> (bool, bool) {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return (false, false);
    };
    (
        contents.lines().any(|line| line.trim() == "Hidden=true"),
        contents
            .lines()
            .any(|line| line.trim() == "X-GNOME-Autostart-enabled=false"),
    )
}

fn status_notifier_watcher_available() -> bool {
    if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_none() {
        return false;
    }

    let Ok(connection) = Connection::session() else {
        return false;
    };
    let Ok(proxy) = Proxy::new(
        &connection,
        "org.kde.StatusNotifierWatcher",
        "/StatusNotifierWatcher",
        "org.kde.StatusNotifierWatcher",
    ) else {
        return false;
    };

    proxy.get_property::<i32>("ProtocolVersion").is_ok()
}

fn render_optional(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("unknown")
}

fn contains_token(value: &str, token: &str) -> bool {
    value
        .split([':', ';', ','])
        .any(|part| part.eq_ignore_ascii_case(token))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_desktop_tray_expectations_without_shelling_out() {
        let kde = DesktopSession::from_values(
            Some("KDE"),
            Some("wayland"),
            Some("wayland-0"),
            Some(":0"),
            Some("true"),
            Some("unix:path=/run/user/1000/bus"),
        );
        assert!(kde.prefers_status_notifier());
        assert!(!kde.may_need_appindicator_extension());
        assert!(kde.has_session_bus());
        assert_eq!(
            kde.status_notifier_guidance(),
            Some("KDE/Plasma should expose StatusNotifier items natively.")
        );

        let gnome = DesktopSession::from_values(
            Some("GNOME"),
            Some("wayland"),
            Some("wayland-0"),
            Some(":0"),
            None,
            Some("unix:path=/run/user/1000/bus"),
        );
        assert!(!gnome.prefers_status_notifier());
        assert!(gnome.may_need_appindicator_extension());
        assert_eq!(
            gnome.status_notifier_guidance(),
            Some("GNOME may require an AppIndicator/KStatusNotifier extension for tray icons.")
        );

        let unknown = DesktopSession::from_values(None, None, None, None, None, None);
        assert_eq!(unknown.status_notifier_guidance(), None);
        assert!(!unknown.has_session_bus());
    }

    #[test]
    fn tray_desktop_check_renders_stable_read_only_lines() {
        let check = TrayDesktopCheck::from_parts(
            DesktopSession::from_values(
                Some("KDE"),
                Some("wayland"),
                Some("wayland-0"),
                Some(":0"),
                Some("true"),
                Some("unix:path=/run/user/1000/bus"),
            ),
            true,
            false,
            false,
        );

        assert_eq!(
            check.render_lines(),
            [
                "RatVantage tray desktop check",
                "current_desktop=KDE",
                "session_type=wayland",
                "wayland_display=wayland-0",
                "display=:0",
                "kde_full_session=true",
                "dbus_session_bus_address_set=true",
                "prefers_status_notifier=true",
                "may_need_appindicator_extension=false",
                "status_notifier_watcher_available=true",
                "autostart_hidden=false",
                "gnome_autostart_disabled=false",
                "desktop_guidance=KDE/Plasma should expose StatusNotifier items natively.",
            ]
        );
    }

    #[test]
    fn detects_autostart_desktop_flags_from_file_contents() {
        let path = std::env::temp_dir().join(format!(
            "ratvantage-tray-autostart-{}.desktop",
            std::process::id()
        ));
        std::fs::write(
            &path,
            "[Desktop Entry]\nHidden=true\nX-GNOME-Autostart-enabled=false\n",
        )
        .unwrap();

        assert_eq!(autostart_desktop_flags(&path), (true, true));

        let _ = std::fs::remove_file(path);
    }
}
