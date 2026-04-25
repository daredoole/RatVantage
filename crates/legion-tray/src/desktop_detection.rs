#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopSession {
    pub current_desktop: Option<String>,
    pub session_type: Option<String>,
}

impl DesktopSession {
    pub fn from_env() -> Self {
        Self {
            current_desktop: std::env::var("XDG_CURRENT_DESKTOP").ok(),
            session_type: std::env::var("XDG_SESSION_TYPE").ok(),
        }
    }

    pub fn from_values(current_desktop: Option<&str>, session_type: Option<&str>) -> Self {
        Self {
            current_desktop: current_desktop.map(str::to_owned),
            session_type: session_type.map(str::to_owned),
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

    pub fn status_notifier_guidance(&self) -> Option<&'static str> {
        if self.may_need_appindicator_extension() {
            Some("GNOME may require an AppIndicator/KStatusNotifier extension for tray icons.")
        } else {
            None
        }
    }
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
        let kde = DesktopSession::from_values(Some("KDE"), Some("wayland"));
        assert!(kde.prefers_status_notifier());
        assert!(!kde.may_need_appindicator_extension());

        let gnome = DesktopSession::from_values(Some("GNOME"), Some("wayland"));
        assert!(!gnome.prefers_status_notifier());
        assert!(gnome.may_need_appindicator_extension());
        assert_eq!(
            gnome.status_notifier_guidance(),
            Some("GNOME may require an AppIndicator/KStatusNotifier extension for tray icons.")
        );

        let unknown = DesktopSession::from_values(None, None);
        assert_eq!(unknown.status_notifier_guidance(), None);
    }
}
