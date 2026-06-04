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
        "tooltip=82WM Legion Pro 5 16ARX8: Platform: balanced, Fans: 2410 RPM, 11 available capabilities, 2 missing"
    ));
    assert!(stdout.contains("capability_count=13"));
    assert!(stdout.contains("available_capability_count=11"));
    assert!(stdout.contains("missing_capability_count=2"));
    assert!(stdout.contains("platform_profile=balanced"));
    assert!(stdout.contains("fan_rpm=2410 RPM"));
    assert!(stdout.contains(
        "capabilities=amd_gpu_power_dpm,battery_charge_type,cpu_power,fan_curves,firmware_attributes,gpu,hwmon,ideapad_toggles,keyboard_rgb_candidates,leds,platform_profile,power_profiles,thermal_zones"
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
        "82WM Legion Pro 5 16ARX8: Platform: balanced, Fans: 2410 RPM, 11 available capabilities, 2 missing\n"
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

[last_hardware_profile_apply]
profile_id = "co_test"
profile_label = "CO test"
timestamp_unix_secs = 1
completed = false
message = "hardware profile apply stopped after first non-applied action"

[[last_hardware_profile_apply.results]]
action_id = "curve_optimizer_all_core"

[last_hardware_profile_apply.results.result]
status = "blocked_by_policy"
applied = false
message = "Curve Optimizer writes are disabled by daemon policy"

[last_hardware_profile_apply.results.result.plan]
method = "SetCurveOptimizerAllCore"
capability_id = "curve_optimizer_all_core"
polkit_action = "org.ratvantage.LegionControl1.set-curve-optimizer"
path = "ryzenadj:/usr/local/bin/ryzenadj"
previous_value = "unknown"
requested_value = "-20 (encoded 4294967276)"
readback_required = false
rollback_value = "0"
rollback_instructions = []
reboot_required = false
safety_notes = []
steps = []
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
    assert!(stdout.contains("label=Platform profile: Balanced"));
    assert!(stdout.contains("label=Charge type: Standard"));
    assert!(stdout.contains("label=Battery: 79% / Charging / Good"));
    assert!(stdout.contains("label=Logo LED: on"));
    assert!(stdout.contains("label=Fn-lock: off"));
    assert!(stdout.contains("label=Camera power: on"));
    assert!(stdout.contains("label=Keyboard RGB: 3 HID research candidates (048D:C103, 048D:C985)"));
    assert!(stdout.contains("label=Fan: CPU Fan 2410 RPM"));
    assert!(stdout.contains("label=GPU: switch to hybrid pending (was nvidia) — reboot required"));
    assert!(stdout.contains(
        "label=Last profile apply: co_test stopped at curve_optimizer_all_core: blocked_by_policy - Curve Optimizer writes are disabled by daemon policy"
    ));
    assert!(stdout.contains("label=Unavailable: gpu"));
    assert!(!stdout.contains("label=Available profiles:"));
    assert!(!stdout.contains("label=Available charging modes:"));
    assert!(!stdout.contains("label=Saved fan curve:"));
    assert!(!stdout.contains("label=Fan presets:"));
    assert!(!stdout.contains("label=Capabilities:"));
    assert!(stdout.contains("enabled action=set_platform_profile:low-power label=Low Power"));
    assert!(stdout.contains("enabled action=set_platform_profile:performance label=Performance"));
    assert!(
        stdout.contains("enabled action=set_battery_charge_type:Conservation label=Conservation")
    );
    assert!(stdout.contains("enabled action=set_battery_charge_type:Fast label=Fast"));
    assert!(stdout.contains("label=Logo light (current: on, guarded)"));
    assert!(stdout.contains("enabled action=set_led_state:platform::ylogo:off label=Turn off"));
    assert!(stdout.contains("label=Keyboard RGB research (3 candidates, no safe write backend)"));
    assert!(stdout.contains("enabled action=open_dashboard label=RGB evidence"));
    assert!(stdout.contains("label=Fn-lock (current: off)"));
    assert!(stdout.contains("enabled action=set_ideapad_toggle:fn_lock:on label=Turn on"));
    assert!(stdout.contains("label=Camera power: on · guarded change in Dashboard"));
    assert!(stdout.contains("enabled action=open_dashboard label=Camera settings"));
    assert!(stdout.contains("enabled action=open_dashboard label=Dashboard"));
    assert!(stdout.contains("enabled action=refresh_status label=Refresh"));
    assert!(stdout.contains("enabled action=quit label=Quit"));
    assert!(!stdout.contains("input12::capslock"));
    assert!(!stdout.contains("enp5s0-0::lan"));
    assert!(!stdout.contains("label=Toggle: conservation_mode"));
    assert!(!stdout.contains("label=Toggle: fan_mode"));
    assert!(!stdout.contains("Apply preset:"));
    assert!(!stdout.contains("Toggle logo LED"));
    assert!(!stdout.contains("set_keyboard_rgb"));

    let _ = fs::remove_file(state_path);
}

#[test]
fn action_cli_executes_menu_action_over_private_bus() {
    let (_bus, _service_connection, address) = fixture_service();
    let output = Command::new(env!("CARGO_BIN_EXE_legion-control-tray"))
        .args([
            "--action",
            "set_battery_charge_type:Fast",
            "--bus-address",
            &address,
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("action=set_battery_charge_type:Fast"));
    assert!(stdout.contains("status=BlockedByPolicy"));
    assert!(stdout.contains("applied=false"));
    assert!(stdout.contains("battery charge type writes are disabled by daemon policy"));
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
    assert!(stdout.contains("autostart_hidden=false"));
    assert!(stdout.contains("gnome_autostart_disabled=false"));
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

[last_hardware_profile_apply]
profile_id = "co_test"
profile_label = "CO test"
timestamp_unix_secs = 1
completed = false
message = "hardware profile apply stopped after first non-applied action"

[[last_hardware_profile_apply.results]]
action_id = "curve_optimizer_all_core"

[last_hardware_profile_apply.results.result]
status = "blocked_by_policy"
applied = false
message = "Curve Optimizer writes are disabled by daemon policy"

[last_hardware_profile_apply.results.result.plan]
method = "SetCurveOptimizerAllCore"
capability_id = "curve_optimizer_all_core"
polkit_action = "org.ratvantage.LegionControl1.set-curve-optimizer"
path = "ryzenadj:/usr/local/bin/ryzenadj"
previous_value = "unknown"
requested_value = "-20 (encoded 4294967276)"
readback_required = false
rollback_value = "0"
rollback_instructions = []
reboot_required = false
safety_notes = []
steps = []
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
    assert!(stdout.contains("GPU: hybrid pending (was nvidia); reboot required"));
    assert!(stdout.contains("Saved curve: 1 point on legion_hwmon"));
    assert!(stdout.contains("gpu_pending_reboot=hybrid pending (was nvidia); reboot required"));
    assert!(stdout.contains("last_known_good_fan_curve=1 point on legion_hwmon"));
    assert!(
        stdout.contains(
            "last_hardware_profile_apply=co_test stopped at curve_optimizer_all_core: blocked_by_policy - Curve Optimizer writes are disabled by daemon policy"
        ),
        "{stdout}"
    );

    let _ = fs::remove_file(state_path);
}

fn fixture_service() -> (PrivateBus, zbus::blocking::Connection, String) {
    let state_path = unique_state_path("tray-empty-state");
    let _ = fs::remove_file(&state_path);
    fixture_service_with_state(&state_path)
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
