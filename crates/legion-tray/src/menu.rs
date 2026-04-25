#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    OpenDashboard,
    SetPlatformProfile,
    SetBatteryChargeType,
    ApplyFanPreset,
    ToggleLogoLed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayMenuItem {
    pub label: String,
    pub action: TrayAction,
    pub enabled: bool,
    pub disabled_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayMenu {
    pub items: Vec<TrayMenuItem>,
}

impl TrayMenu {
    pub fn read_only_scaffold() -> Self {
        let write_disabled = "D-Bus write methods are not enabled".to_owned();
        Self {
            items: vec![
                TrayMenuItem {
                    label: "Open dashboard".to_owned(),
                    action: TrayAction::OpenDashboard,
                    enabled: true,
                    disabled_reason: None,
                },
                disabled_item(
                    "Set platform profile",
                    TrayAction::SetPlatformProfile,
                    &write_disabled,
                ),
                disabled_item(
                    "Set battery charge type",
                    TrayAction::SetBatteryChargeType,
                    &write_disabled,
                ),
                disabled_item(
                    "Apply fan preset",
                    TrayAction::ApplyFanPreset,
                    &write_disabled,
                ),
                disabled_item(
                    "Toggle logo LED",
                    TrayAction::ToggleLogoLed,
                    &write_disabled,
                ),
            ],
        }
    }
}

fn disabled_item(label: &str, action: TrayAction, reason: &str) -> TrayMenuItem {
    TrayMenuItem {
        label: label.to_owned(),
        action,
        enabled: false,
        disabled_reason: Some(reason.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_menu_enables_dashboard_and_disables_write_actions() {
        let menu = TrayMenu::read_only_scaffold();

        assert_eq!(menu.items[0].action, TrayAction::OpenDashboard);
        assert!(menu.items[0].enabled);
        assert!(menu.items[1..].iter().all(|item| {
            !item.enabled
                && item
                    .disabled_reason
                    .as_deref()
                    .is_some_and(|reason| reason.contains("write methods are not enabled"))
        }));
    }
}
