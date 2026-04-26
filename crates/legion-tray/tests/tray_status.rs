use std::process::Command;
use std::{fs, path::PathBuf};

use legion_control_daemon::{LegionControl, DBUS_INTERFACE, DBUS_PATH};
use legion_probe::ProbeOptions;
use ratvantage_test_support::{fixture_root, PrivateBus};
use zbus::blocking::ConnectionBuilder;

#[test]
fn status_cli_prints_tray_summary_over_private_bus() {
    let (_bus, _service_connection, address) = fixture_service();
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-tray"))
        .args(["--status", "--bus-address", &address])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Legion Control tray status"));
    assert!(stdout.contains(
        "tooltip=82WM Legion Pro 5 16ARX8: profile balanced, fan 2410 RPM, 7 available capabilities, 1 missing"
    ));
    assert!(stdout.contains("capability_count=8"));
    assert!(stdout.contains("available_capability_count=7"));
    assert!(stdout.contains("missing_capability_count=1"));
    assert!(stdout.contains("platform_profile=balanced"));
    assert!(stdout.contains("fan_rpm=2410 RPM"));
    assert!(stdout.contains(
        "capabilities=battery_charge_type,fan_curves,firmware_attributes,gpu,hwmon,ideapad_toggles,leds,platform_profile"
    ));
}

#[test]
fn tooltip_cli_prints_single_line_over_private_bus() {
    let (_bus, _service_connection, address) = fixture_service();
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-tray"))
        .args(["--tooltip", "--bus-address", &address])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "82WM Legion Pro 5 16ARX8: profile balanced, fan 2410 RPM, 7 available capabilities, 1 missing\n"
    );
}

#[test]
fn menu_check_cli_prints_dynamic_menu_over_private_bus() {
    let state_path = unique_state_path("tray-menu-state");
    fs::write(
        &state_path,
        r#"schema_version = 1

[gpu_mode_pending]
requested_mode = "hybrid"
previous_mode = "nvidia"
reboot_required = true

[last_known_good_fan_curve]
curve_id = "legion_hwmon"
path = "/tmp/fixture/sys/class/hwmon/hwmon7"

[[last_known_good_fan_curve.points]]
path = "/tmp/fixture/sys/class/hwmon/hwmon7/pwm1_auto_point1_temp"
value = "42000"
"#,
    )
    .unwrap();

    let (_bus, _service_connection, address) = fixture_service_with_state(&state_path);
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-tray"))
        .args(["--menu-check", "--bus-address", &address])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Legion Control tray menu"));
    assert!(stdout.contains("label=82WM Legion Pro 5 16ARX8"));
    assert!(stdout.contains("label=Platform profile: balanced"));
    assert!(stdout.contains("label=Profile choices: low-power, balanced, performance"));
    assert!(stdout.contains("label=Battery charge type: Standard"));
    assert!(stdout.contains("label=Charge choices: Standard, Conservation, Fast"));
    assert!(stdout.contains("label=Battery: 79% / Charging / Good"));
    assert!(stdout.contains("label=LED: platform::ylogo on"));
    assert!(stdout.contains("label=Fan: CPU Fan 2410 RPM"));
    assert!(stdout.contains("label=GPU pending: hybrid (previous nvidia, reboot required)"));
    assert!(stdout.contains("label=Saved fan curve: 1 values from legion_hwmon"));
    assert!(stdout.contains("label=Fan presets: Quiet office, Balanced daily, Gaming, Max safe"));
    assert!(stdout.contains("label=Capabilities: 7 available, 1 missing"));
    assert!(stdout.contains("label=Missing: gpu"));
    assert!(stdout.contains("label=Platform profile actions"));
    assert!(stdout.contains(
        "enabled action=set_platform_profile:low-power label=Set platform profile: low-power"
    ));
    assert!(stdout.contains(
        "enabled action=set_platform_profile:performance label=Set platform profile: performance"
    ));
    assert!(stdout.contains("label=Battery charge type actions"));
    assert!(stdout.contains(
        "enabled action=set_battery_charge_type:Conservation label=Set battery charge type: Conservation"
    ));
    assert!(stdout.contains(
        "enabled action=set_battery_charge_type:Fast label=Set battery charge type: Fast"
    ));
    assert!(stdout.contains("label=LED actions"));
    assert!(stdout.contains(
        "enabled action=set_led_state:platform::ylogo:off label=Set LED state: platform::ylogo off"
    ));
    assert!(stdout.contains("enabled action=open_dashboard label=Open dashboard"));
    assert!(stdout.contains("enabled action=refresh_status label=Refresh status"));
    assert!(stdout.contains("enabled action=quit label=Quit"));
    assert!(!stdout.contains("Apply preset:"));
    assert!(!stdout.contains("Toggle logo LED"));

    let _ = fs::remove_file(state_path);
}

#[test]
fn desktop_check_cli_reports_kde_env_without_session_bus() {
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-tray"))
        .arg("--desktop-check")
        .env_clear()
        .env("XDG_CURRENT_DESKTOP", "KDE")
        .env("XDG_SESSION_TYPE", "wayland")
        .env("WAYLAND_DISPLAY", "wayland-0")
        .env("DISPLAY", ":0")
        .env("KDE_FULL_SESSION", "true")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Legion Control tray desktop check"));
    assert!(stdout.contains("current_desktop=KDE"));
    assert!(stdout.contains("session_type=wayland"));
    assert!(stdout.contains("wayland_display=wayland-0"));
    assert!(stdout.contains("display=:0"));
    assert!(stdout.contains("kde_full_session=true"));
    assert!(stdout.contains("dbus_session_bus_address_set=false"));
    assert!(stdout.contains("prefers_status_notifier=true"));
    assert!(stdout.contains("may_need_appindicator_extension=false"));
    assert!(stdout.contains("status_notifier_watcher_available=false"));
    assert!(stdout.contains("autostart_hidden=true"));
    assert!(stdout.contains("gnome_autostart_disabled=true"));
    assert!(
        stdout.contains("desktop_guidance=KDE/Plasma should expose StatusNotifier items natively.")
    );
}

#[test]
fn desktop_check_cli_reports_gnome_guidance_without_session_bus() {
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-tray"))
        .arg("--desktop-check")
        .env_clear()
        .env("XDG_CURRENT_DESKTOP", "GNOME")
        .env("XDG_SESSION_TYPE", "wayland")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("current_desktop=GNOME"));
    assert!(stdout.contains("dbus_session_bus_address_set=false"));
    assert!(stdout.contains("prefers_status_notifier=false"));
    assert!(stdout.contains("may_need_appindicator_extension=true"));
    assert!(stdout.contains("status_notifier_watcher_available=false"));
    assert!(stdout.contains(
        "desktop_guidance=GNOME may require an AppIndicator/KStatusNotifier extension for tray icons."
    ));
}

#[test]
fn tray_status_includes_pending_gpu_and_saved_fan_curve_state() {
    let state_path = unique_state_path("tray-state");
    fs::write(
        &state_path,
        r#"schema_version = 1

[gpu_mode_pending]
requested_mode = "hybrid"
previous_mode = "nvidia"
reboot_required = true

[last_known_good_fan_curve]
curve_id = "legion_hwmon"
path = "/tmp/fixture/sys/class/hwmon/hwmon7"

[[last_known_good_fan_curve.points]]
path = "/tmp/fixture/sys/class/hwmon/hwmon7/pwm1_auto_point1_temp"
value = "42000"
"#,
    )
    .unwrap();

    let (_bus, _service_connection, address) = fixture_service_with_state(&state_path);
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-tray"))
        .args(["--status", "--bus-address", &address])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("pending reboot hybrid previous=nvidia reboot_required=true"));
    assert!(stdout.contains("saved fan curve 1 values from legion_hwmon"));
    assert!(stdout.contains("gpu_pending_reboot=hybrid previous=nvidia reboot_required=true"));
    assert!(stdout.contains("last_known_good_fan_curve=1 values from legion_hwmon"));

    let _ = fs::remove_file(state_path);
}

fn fixture_service() -> (PrivateBus, zbus::blocking::Connection, String) {
    let bus = PrivateBus::start();
    let service = LegionControl::new(ProbeOptions {
        sysfs_root: fixture_root(),
    });
    let service_connection = ConnectionBuilder::address(bus.address())
        .unwrap()
        .name(DBUS_INTERFACE)
        .unwrap()
        .serve_at(DBUS_PATH, service)
        .unwrap()
        .build()
        .unwrap();

    let address = bus.address().to_owned();
    (bus, service_connection, address)
}

fn fixture_service_with_state(
    state_path: &std::path::Path,
) -> (PrivateBus, zbus::blocking::Connection, String) {
    let bus = PrivateBus::start();
    let service = LegionControl::new_with_state_path(
        ProbeOptions {
            sysfs_root: fixture_root(),
        },
        state_path,
    );
    let service_connection = ConnectionBuilder::address(bus.address())
        .unwrap()
        .name(DBUS_INTERFACE)
        .unwrap()
        .serve_at(DBUS_PATH, service)
        .unwrap()
        .build()
        .unwrap();

    let address = bus.address().to_owned();
    (bus, service_connection, address)
}

fn unique_state_path(label: &str) -> PathBuf {
    PathBuf::from("/tmp").join(format!("ratvantage-{label}-{}.toml", std::process::id()))
}
