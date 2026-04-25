pub mod desktop_detection;
pub mod menu;
pub mod status_icon;
#[cfg(feature = "status-notifier")]
pub mod status_notifier;

pub use desktop_detection::DesktopSession;
pub use menu::{TrayAction, TrayMenu, TrayMenuItem};
pub use status_icon::TraySummary;
#[cfg(feature = "status-notifier")]
pub use status_notifier::{run_status_notifier_tray, StatusNotifierTray};
