pub mod desktop_detection;
pub mod menu;
pub mod status_icon;

pub use desktop_detection::DesktopSession;
pub use menu::{TrayAction, TrayMenu, TrayMenuItem};
pub use status_icon::TraySummary;
