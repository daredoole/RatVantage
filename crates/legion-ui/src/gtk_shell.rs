use crate::{
    capability_status_label, render_diagnostics_json, risk_level_label, runtime_refresh_notice,
    DiagnosticsBundle, LegionControlClient, RuntimeSnapshot, UiStatus,
};
use adw::prelude::*;
use anyhow::{anyhow, Result};
use legion_common::{
    BatteryChargeTypeCapability, FanCurveCapability, FanCurveSnapshot, GpuCapability,
    GpuModePending, IdeapadToggleCapability, LedCapability, PlatformProfileCapability,
    WriteDryRunPlan, WriteExecutionResult, WriteExecutionStatus,
};
use std::thread_local;
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
    time::{Duration, Instant},
};

const AUTO_REFRESH_INTERVAL: Duration = Duration::from_secs(30);
const RESUME_REFRESH_GAP: Duration = Duration::from_secs(90);
const DEFAULT_DASHBOARD_PAGE: &str = "status";
const GPU_MODE_CHOICES: &[&str] = &["integrated", "hybrid", "nvidia"];
const FAN_PRESET_IDS: &[&str] = &["quiet-office", "balanced-daily", "gaming", "max-safe"];

type SnapshotLoader = Rc<dyn Fn() -> Result<RuntimeSnapshot>>;

thread_local! {
    static DASHBOARD_REFRESH_HOOK: RefCell<Option<Rc<dyn Fn()>>> = RefCell::new(None);
    static WRITE_FEEDBACK_STATE: RefCell<HashMap<&'static str, WriteFeedbackState>> =
        RefCell::new(HashMap::new());
}

#[derive(Clone)]
struct WriteFeedbackState {
    title: String,
    subtitle: String,
}

pub fn run(
    bus_address: Option<String>,
    initial_page: Option<String>,
    auto_quit_ms: Option<u64>,
) -> Result<()> {
    if should_force_gl_renderer() {
        // GTK 4.15+ defaults to Vulkan on this stack, which renders a black
        // window on KDE Wayland with NVIDIA. Force the known-good renderer
        // before the application initializes unless the caller already set one.
        unsafe {
            std::env::set_var("GSK_RENDERER", "gl");
        }
        eprintln!(
            "legion-control-ui: Wayland+NVIDIA detected; forcing GSK_RENDERER=gl for this launch"
        );
    }

    let app = adw::Application::builder()
        .application_id("org.ratvantage.LegionControl")
        .build();

    let bus_address = Rc::new(bus_address);
    let initial_page = Rc::new(normalize_dashboard_page_name(initial_page.as_deref()));
    app.connect_activate(move |app| {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Legion Control")
            .default_width(720)
            .default_height(480)
            .build();

        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.set_hexpand(true);
        root.set_vexpand(true);
        root.append(&adw::HeaderBar::new());
        let bus_address = Rc::clone(&bus_address);
        let initial_page = Rc::clone(&initial_page);
        let runtime = DashboardRuntime::new(
            &root,
            initial_page.as_ref().as_str(),
            Rc::new(move || {
                let client = match bus_address.as_deref() {
                    Some(address) => LegionControlClient::address(address),
                    None => LegionControlClient::system(),
                }?;
                client.refresh_runtime_snapshot()
            }),
        );
        install_dashboard_refresh_hook({
            let runtime = Rc::clone(&runtime);
            Rc::new(move || runtime.borrow_mut().refresh_now())
        });
        install_runtime_refresh(&window, Rc::clone(&runtime));
        window.set_content(Some(&root));
        runtime.borrow_mut().refresh_now();
        window.present();
        if let Some(auto_quit_ms) = auto_quit_ms {
            let app = app.clone();
            let window = window.clone();
            gtk4::glib::timeout_add_local_once(Duration::from_millis(auto_quit_ms), move || {
                window.close();
                app.quit();
            });
        }
    });

    app.run_with_args(&["legion-control-ui"]);
    Ok(())
}

pub fn should_force_gl_renderer() -> bool {
    std::env::var_os("GSK_RENDERER").is_none()
        && (std::env::var_os("WAYLAND_DISPLAY").is_some()
            || std::env::var("XDG_SESSION_TYPE").ok().as_deref() == Some("wayland"))
        && std::path::Path::new("/proc/driver/nvidia").exists()
}

pub fn dashboard_page(
    status: Result<UiStatus>,
    diagnostics: Result<DiagnosticsBundle>,
    gpu_pending: Result<Option<GpuModePending>>,
    fan_snapshot: Result<Option<FanCurveSnapshot>>,
    initial_page: Option<&str>,
) -> gtk4::Widget {
    let stack = gtk4::Stack::new();
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    stack.add_titled(
        &scrollable_dashboard_page(status_page(status, clone_result(&gpu_pending))),
        Some("status"),
        "Status",
    );
    stack.add_titled(
        &scrollable_dashboard_page(profiles_page(clone_result(&diagnostics))),
        Some("profiles"),
        "Profiles",
    );
    stack.add_titled(
        &scrollable_dashboard_page(battery_page(clone_result(&diagnostics))),
        Some("battery"),
        "Battery",
    );
    stack.add_titled(
        &scrollable_dashboard_page(gpu_page(
            clone_result(&diagnostics),
            clone_result(&gpu_pending),
        )),
        Some("gpu"),
        "GPU",
    );
    stack.add_titled(
        &scrollable_dashboard_page(fans_page(clone_result(&diagnostics), fan_snapshot)),
        Some("fans"),
        "Fans",
    );
    stack.add_titled(
        &scrollable_dashboard_page(appearance_page(clone_result(&diagnostics))),
        Some("appearance"),
        "Appearance",
    );
    stack.add_titled(
        &scrollable_dashboard_page(diagnostics_page(diagnostics)),
        Some("diagnostics"),
        "Diagnostics",
    );
    stack.set_visible_child_name(initial_page.unwrap_or(DEFAULT_DASHBOARD_PAGE));

    let switcher = gtk4::StackSwitcher::new();
    switcher.set_stack(Some(&stack));
    switcher.set_halign(gtk4::Align::Start);
    switcher.set_margin_top(12);
    switcher.set_margin_start(24);
    switcher.set_margin_end(24);

    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    page.set_hexpand(true);
    page.set_vexpand(true);
    page.append(&switcher);
    page.append(&stack);
    page.upcast()
}

fn scrollable_dashboard_page(child: gtk4::Widget) -> gtk4::ScrolledWindow {
    let scroller = gtk4::ScrolledWindow::new();
    scroller.set_hscrollbar_policy(gtk4::PolicyType::Never);
    scroller.set_vscrollbar_policy(gtk4::PolicyType::Automatic);
    scroller.set_hexpand(true);
    scroller.set_vexpand(true);
    scroller.set_propagate_natural_height(false);
    scroller.set_propagate_natural_width(true);
    scroller.set_child(Some(&child));
    scroller
}

fn install_runtime_refresh(
    window: &adw::ApplicationWindow,
    runtime: Rc<RefCell<DashboardRuntime>>,
) {
    let runtime_for_active = Rc::clone(&runtime);
    window.connect_is_active_notify(move |window| {
        if window.is_active() && runtime_for_active.borrow().last_snapshot.is_some() {
            runtime_for_active.borrow_mut().refresh_now();
        }
    });

    let runtime_for_visible = Rc::clone(&runtime);
    window.connect_visible_notify(move |window| {
        if window.is_visible() && runtime_for_visible.borrow().last_snapshot.is_some() {
            runtime_for_visible.borrow_mut().refresh_now();
        }
    });

    gtk4::glib::timeout_add_local(Duration::from_secs(5), move || {
        runtime.borrow_mut().maybe_auto_refresh();
        gtk4::glib::ControlFlow::Continue
    });
}

fn install_dashboard_refresh_hook(hook: Rc<dyn Fn()>) {
    DASHBOARD_REFRESH_HOOK.with(|slot| {
        *slot.borrow_mut() = Some(hook);
    });
}

#[cfg(test)]
fn clear_dashboard_refresh_hook() {
    DASHBOARD_REFRESH_HOOK.with(|slot| {
        *slot.borrow_mut() = None;
    });
}

fn request_dashboard_refresh() -> bool {
    DASHBOARD_REFRESH_HOOK.with(|slot| {
        let hook = slot.borrow().as_ref().map(Rc::clone);
        if let Some(hook) = hook {
            hook();
            true
        } else {
            false
        }
    })
}

fn default_write_feedback_state() -> WriteFeedbackState {
    WriteFeedbackState {
        title: "Apply result".to_owned(),
        subtitle: "No write attempted yet.".to_owned(),
    }
}

fn load_write_feedback_state(capability_label: &'static str) -> WriteFeedbackState {
    WRITE_FEEDBACK_STATE.with(|state| {
        state
            .borrow()
            .get(capability_label)
            .cloned()
            .unwrap_or_else(default_write_feedback_state)
    })
}

fn store_write_feedback_state(capability_label: &'static str, title: &str, subtitle: &str) {
    WRITE_FEEDBACK_STATE.with(|state| {
        state.borrow_mut().insert(
            capability_label,
            WriteFeedbackState {
                title: title.to_owned(),
                subtitle: subtitle.to_owned(),
            },
        );
    });
}

pub fn should_auto_refresh(now: Instant, last_refresh: Instant, last_tick: Instant) -> bool {
    now.duration_since(last_refresh) >= AUTO_REFRESH_INTERVAL
        || now.duration_since(last_tick) >= RESUME_REFRESH_GAP
}

struct DashboardRuntime {
    host: gtk4::Box,
    banner: gtk4::Label,
    initial_page: String,
    loader: SnapshotLoader,
    last_snapshot: Option<RuntimeSnapshot>,
    degraded: bool,
    last_refresh: Instant,
    last_tick: Instant,
}

impl DashboardRuntime {
    fn new(root: &gtk4::Box, initial_page: &str, loader: SnapshotLoader) -> Rc<RefCell<Self>> {
        let banner = gtk4::Label::new(None);
        banner.set_xalign(0.0);
        banner.set_wrap(true);
        banner.set_margin_top(12);
        banner.set_margin_start(24);
        banner.set_margin_end(24);
        banner.set_visible(false);
        root.append(&banner);

        let host = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        host.set_hexpand(true);
        host.set_vexpand(true);
        root.append(&host);

        Rc::new(RefCell::new(Self {
            host,
            banner,
            initial_page: initial_page.to_owned(),
            loader,
            last_snapshot: None,
            degraded: false,
            last_refresh: Instant::now(),
            last_tick: Instant::now(),
        }))
    }

    fn refresh_now(&mut self) {
        match (self.loader)() {
            Ok(snapshot) => {
                let recovered_from_error = self.degraded;
                let notice = runtime_refresh_notice(
                    self.last_snapshot.as_ref(),
                    &snapshot,
                    recovered_from_error,
                );
                self.last_snapshot = Some(snapshot.clone());
                self.degraded = false;
                self.last_refresh = Instant::now();
                self.last_tick = self.last_refresh;
                if let Some(notice) = notice {
                    self.banner.set_label(&notice.message);
                    self.banner.set_visible(true);
                } else {
                    self.banner.set_visible(false);
                }
                self.replace_page(snapshot_page(Ok(snapshot), Some(&self.initial_page)));
            }
            Err(error) => {
                self.degraded = true;
                self.last_tick = Instant::now();
                let message = format!(
                    "Runtime refresh degraded. Keeping the last known dashboard state until the daemon responds again: {error}"
                );
                if self.last_snapshot.is_none() {
                    self.replace_page(snapshot_page(
                        Err(anyhow!(error.to_string())),
                        Some(&self.initial_page),
                    ));
                }
                self.banner.set_label(&message);
                self.banner.set_visible(self.last_snapshot.is_some());
            }
        }
    }

    fn maybe_auto_refresh(&mut self) {
        let now = Instant::now();
        if should_auto_refresh(now, self.last_refresh, self.last_tick) {
            self.refresh_now();
            return;
        }
        self.last_tick = now;
    }

    fn replace_page(&self, widget: gtk4::Widget) {
        while let Some(child) = self.host.first_child() {
            self.host.remove(&child);
        }
        self.host.append(&widget);
    }
}

fn snapshot_page(snapshot: Result<RuntimeSnapshot>, initial_page: Option<&str>) -> gtk4::Widget {
    let status = snapshot
        .as_ref()
        .map(|snapshot| snapshot.status.clone())
        .map_err(clone_error);
    let diagnostics = snapshot
        .as_ref()
        .map(|snapshot| snapshot.diagnostics.clone())
        .map_err(clone_error);
    let gpu_pending = runtime_snapshot_gpu_pending(&snapshot);
    let fan_snapshot = runtime_snapshot_fan_snapshot(&snapshot);
    dashboard_page(status, diagnostics, gpu_pending, fan_snapshot, initial_page)
}

pub fn normalize_dashboard_page_name(page: Option<&str>) -> String {
    match page {
        Some("status" | "profiles" | "battery" | "gpu" | "fans" | "appearance" | "diagnostics") => {
            page.unwrap().to_owned()
        }
        _ => DEFAULT_DASHBOARD_PAGE.to_owned(),
    }
}

pub fn status_page(
    status: Result<UiStatus>,
    gpu_pending: Result<Option<GpuModePending>>,
) -> gtk4::Widget {
    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(24);
    page.set_margin_end(24);

    match status {
        Ok(status) => append_status(&page, &status, gpu_pending),
        Err(error) => append_error(&page, &error),
    }

    page.upcast()
}

pub fn profiles_page(diagnostics: Result<DiagnosticsBundle>) -> gtk4::Widget {
    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(24);
    page.set_margin_end(24);

    match diagnostics {
        Ok(bundle) => append_profiles(&page, &bundle),
        Err(error) => append_error(&page, &error),
    }

    page.upcast()
}

pub fn battery_page(diagnostics: Result<DiagnosticsBundle>) -> gtk4::Widget {
    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(24);
    page.set_margin_end(24);

    match diagnostics {
        Ok(bundle) => append_battery(&page, &bundle),
        Err(error) => append_error(&page, &error),
    }

    page.upcast()
}

pub fn gpu_page(
    diagnostics: Result<DiagnosticsBundle>,
    gpu_pending: Result<Option<GpuModePending>>,
) -> gtk4::Widget {
    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(24);
    page.set_margin_end(24);

    match diagnostics {
        Ok(bundle) => append_gpu(&page, &bundle, gpu_pending),
        Err(error) => append_error(&page, &error),
    }

    page.upcast()
}

pub fn fans_page(
    diagnostics: Result<DiagnosticsBundle>,
    fan_snapshot: Result<Option<FanCurveSnapshot>>,
) -> gtk4::Widget {
    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(24);
    page.set_margin_end(24);

    match diagnostics {
        Ok(bundle) => append_fans(&page, &bundle, fan_snapshot),
        Err(error) => append_error(&page, &error),
    }

    page.upcast()
}

pub fn appearance_page(diagnostics: Result<DiagnosticsBundle>) -> gtk4::Widget {
    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(24);
    page.set_margin_end(24);

    match diagnostics {
        Ok(bundle) => append_appearance(&page, &bundle),
        Err(error) => append_error(&page, &error),
    }

    page.upcast()
}

pub fn diagnostics_page(diagnostics: Result<DiagnosticsBundle>) -> gtk4::Widget {
    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(24);
    page.set_margin_end(24);

    match diagnostics {
        Ok(bundle) => append_diagnostics(&page, &bundle),
        Err(error) => append_error(&page, &error),
    }

    page.upcast()
}

fn append_status(page: &gtk4::Box, status: &UiStatus, gpu_pending: Result<Option<GpuModePending>>) {
    let title = gtk4::Label::new(Some("Detected Hardware"));
    title.add_css_class("title-2");
    title.set_xalign(0.0);
    page.append(&title);

    let group = adw::PreferencesGroup::new();
    group.add(&info_row("Vendor", &status.hardware.vendor));
    group.add(&info_row("Product", &status.hardware.product_name));
    group.add(&info_row("Version", &status.hardware.product_version));
    if let Some(sku) = &status.hardware.product_sku {
        group.add(&info_row("SKU", sku));
    }
    group.add(&info_row(
        "Capabilities",
        &status.capability_count().to_string(),
    ));
    group.add(&info_row(
        "GPU pending reboot",
        &render_gpu_pending_row(gpu_pending),
    ));
    page.append(&group);

    let capabilities = adw::PreferencesGroup::new();
    capabilities.set_title("Read-only Capabilities");
    for capability in &status.capabilities {
        capabilities.add(&info_row(
            &capability.label,
            &format!(
                "{} - {} - {}",
                capability.id,
                capability_status_label(capability.status),
                risk_level_label(capability.risk)
            ),
        ));
    }
    page.append(&capabilities);
}

fn append_profiles(page: &gtk4::Box, bundle: &DiagnosticsBundle) {
    let title = gtk4::Label::new(Some("Profiles"));
    title.add_css_class("title-2");
    title.set_xalign(0.0);
    page.append(&title);

    let group = adw::PreferencesGroup::new();
    group.set_title("Platform Profile");
    if let Some(profile) = &bundle.raw_probe_report.platform_profile {
        let current = info_row("Current", profile.current.as_deref().unwrap_or("unknown"));
        group.add(&current);
        group.add(&info_row("Choices", &profile.choices.join(", ")));
        group.add(&info_row("Profile path", &profile.path));
        group.add(&info_row("Choices path", &profile.choices_path));
        page.append(&group);

        let controls = build_platform_profile_controls(
            bundle.raw_probe_report.platform_profile.as_ref(),
            Some(current),
        );
        page.append(&controls);
    } else {
        group.add(&info_row("Platform profile", "unavailable"));
        page.append(&group);

        let controls = build_platform_profile_controls(None, None);
        page.append(&controls);
    }

    let feedback = build_write_feedback_group("Platform profile");
    page.append(&feedback);
}

fn append_battery(page: &gtk4::Box, bundle: &DiagnosticsBundle) {
    let title = gtk4::Label::new(Some("Battery"));
    title.add_css_class("title-2");
    title.set_xalign(0.0);
    page.append(&title);

    let charge_type = adw::PreferencesGroup::new();
    charge_type.set_title("Charge Type");
    if let Some(charge_type_capability) = &bundle.raw_probe_report.battery_charge_type {
        let current = info_row(
            "Current",
            charge_type_capability
                .current
                .as_deref()
                .unwrap_or("unknown"),
        );
        charge_type.add(&current);
        charge_type.add(&info_row(
            "Choices",
            &charge_type_capability.choices.join(", "),
        ));
        charge_type.add(&info_row("Status path", &charge_type_capability.path));
        charge_type.add(&info_row(
            "Choices path",
            &charge_type_capability.choices_path,
        ));
        page.append(&charge_type);

        let controls = build_battery_charge_type_controls(
            bundle.raw_probe_report.battery_charge_type.as_ref(),
            Some(current),
        );
        page.append(&controls);
    } else {
        charge_type.add(&info_row("Charge type", "unavailable"));
        page.append(&charge_type);

        let controls = build_battery_charge_type_controls(None, None);
        page.append(&controls);
    }

    let feedback = build_write_feedback_group("Battery charge type");
    page.append(&feedback);

    let telemetry = adw::PreferencesGroup::new();
    telemetry.set_title("Telemetry");
    if let Some(battery) = &bundle.raw_probe_report.telemetry.battery {
        telemetry.add(&info_row("Name", &battery.name));
        telemetry.add(&info_row(
            "Capacity",
            &battery
                .capacity_percent
                .map(|capacity| format!("{capacity}%"))
                .unwrap_or_else(|| "unknown".to_owned()),
        ));
        telemetry.add(&info_row(
            "Status",
            battery.status.as_deref().unwrap_or("unknown"),
        ));
        telemetry.add(&info_row(
            "Health",
            battery.health.as_deref().unwrap_or("unknown"),
        ));
        telemetry.add(&info_row("Path", &battery.path));
    } else {
        telemetry.add(&info_row("Battery telemetry", "unavailable"));
    }
    page.append(&telemetry);
}

fn append_gpu(
    page: &gtk4::Box,
    bundle: &DiagnosticsBundle,
    gpu_pending: Result<Option<GpuModePending>>,
) {
    let title = gtk4::Label::new(Some("GPU"));
    title.add_css_class("title-2");
    title.set_xalign(0.0);
    page.append(&title);

    let mode = adw::PreferencesGroup::new();
    mode.set_title("GPU Mode");
    if let Some(gpu) = &bundle.raw_probe_report.gpu {
        mode.add(&info_row("Provider", &gpu.provider));
        mode.add(&info_row(
            "Capability status",
            capability_status_label(gpu.status),
        ));
        mode.add(&info_row(
            "Current mode",
            gpu.mode.as_deref().unwrap_or("unknown"),
        ));
    } else {
        mode.add(&info_row("GPU mode", "unavailable"));
    }
    let pending_row = info_row("Pending reboot", &render_gpu_pending_row(gpu_pending));
    mode.add(&pending_row);
    page.append(&mode);

    page.append(&build_gpu_mode_controls(
        bundle.raw_probe_report.gpu.as_ref(),
        pending_row,
    ));
}

fn append_fans(
    page: &gtk4::Box,
    bundle: &DiagnosticsBundle,
    fan_snapshot: Result<Option<FanCurveSnapshot>>,
) {
    let title = gtk4::Label::new(Some("Fans"));
    title.add_css_class("title-2");
    title.set_xalign(0.0);
    page.append(&title);

    let telemetry = adw::PreferencesGroup::new();
    telemetry.set_title("Telemetry");
    let fan_sensors = bundle
        .raw_probe_report
        .telemetry
        .sensors
        .iter()
        .filter(|sensor| sensor.kind == "fan")
        .collect::<Vec<_>>();
    if fan_sensors.is_empty() {
        telemetry.add(&info_row("Fan telemetry", "unavailable"));
    } else {
        for sensor in fan_sensors {
            let title = sensor.label.as_deref().unwrap_or("Fan");
            let value = sensor
                .value
                .map(|value| format!("{value} RPM"))
                .unwrap_or_else(|| "unknown".to_owned());
            telemetry.add(&info_row(title, &value));
        }
    }
    page.append(&telemetry);

    let curves = adw::PreferencesGroup::new();
    curves.set_title("Fan Curves");
    if bundle.raw_probe_report.fan_curves.is_empty() {
        curves.add(&info_row("Fan curves", "unavailable"));
    } else {
        for curve in &bundle.raw_probe_report.fan_curves {
            let path = curve.path.as_deref().unwrap_or("unknown");
            curves.add(&info_row(
                &curve.id,
                &format!("{} point files - {path}", curve.point_paths.len()),
            ));
        }
    }
    let last_known_row = info_row("Last known good", &render_fan_snapshot_row(fan_snapshot));
    curves.add(&last_known_row);
    page.append(&curves);

    page.append(&build_fan_planning_controls(
        bundle.raw_probe_report.fan_curves.as_slice(),
        last_known_row,
    ));
}

fn append_appearance(page: &gtk4::Box, bundle: &DiagnosticsBundle) {
    let title = gtk4::Label::new(Some("Appearance"));
    title.add_css_class("title-2");
    title.set_xalign(0.0);
    page.append(&title);

    let leds = adw::PreferencesGroup::new();
    leds.set_title("LEDs");
    if bundle.raw_probe_report.leds.is_empty() {
        leds.add(&info_row("LEDs", "unavailable"));
        page.append(&leds);
        page.append(&build_led_state_controls(None, None));
    } else {
        let mut ylogo_row = None;
        for led in &bundle.raw_probe_report.leds {
            let row = info_row(&led.name, &render_led_row(led));
            if led.name == "platform::ylogo" {
                ylogo_row = Some(row.clone());
            }
            leds.add(&row);
        }
        page.append(&leds);
        page.append(&build_led_state_controls(
            writable_ylogo(bundle.raw_probe_report.leds.as_slice()),
            ylogo_row,
        ));
    }
    page.append(&build_write_feedback_group("Y-logo LED"));

    let toggles = adw::PreferencesGroup::new();
    toggles.set_title("Firmware Toggles");
    if bundle.raw_probe_report.ideapad_toggles.is_empty() {
        toggles.add(&info_row("Firmware toggles", "unavailable"));
        page.append(&toggles);
        page.append(&build_ideapad_toggle_controls(None, None));
        page.append(&build_camera_power_controls(None, None));
        page.append(&build_usb_charging_controls(None, None));
    } else {
        let mut fn_lock_row = None;
        let mut camera_power_row = None;
        let mut usb_charging_row = None;
        for toggle in &bundle.raw_probe_report.ideapad_toggles {
            let row = info_row(&toggle.name, &render_ideapad_toggle_row(toggle));
            if toggle.name == "fn_lock" {
                fn_lock_row = Some(row.clone());
            }
            if toggle.name == "camera_power" {
                camera_power_row = Some(row.clone());
            }
            if toggle.name == "usb_charging" {
                usb_charging_row = Some(row.clone());
            }
            toggles.add(&row);
        }
        page.append(&toggles);
        page.append(&build_ideapad_toggle_controls(
            writable_fn_lock_toggle(
                bundle.raw_probe_report.ideapad_toggles.as_slice(),
                bundle.raw_probe_report.leds.as_slice(),
            ),
            fn_lock_row,
        ));
        page.append(&build_camera_power_controls(
            writable_camera_power_toggle(bundle.raw_probe_report.ideapad_toggles.as_slice()),
            camera_power_row,
        ));
        page.append(&build_usb_charging_controls(
            writable_usb_charging_toggle(bundle.raw_probe_report.ideapad_toggles.as_slice()),
            usb_charging_row,
        ));
    }
    page.append(&build_write_feedback_group("Fn-lock"));
    page.append(&build_write_feedback_group("Camera power"));
    page.append(&build_write_feedback_group("USB charging"));
}

fn append_diagnostics(page: &gtk4::Box, bundle: &DiagnosticsBundle) {
    let title = gtk4::Label::new(Some("Diagnostics"));
    title.add_css_class("title-2");
    title.set_xalign(0.0);
    page.append(&title);

    let group = adw::PreferencesGroup::new();
    group.add(&info_row(
        "Vendor",
        bundle.hardware.vendor.as_deref().unwrap_or("unknown"),
    ));
    group.add(&info_row(
        "Product",
        bundle.hardware.product_name.as_deref().unwrap_or("unknown"),
    ));
    group.add(&info_row(
        "Kernel",
        bundle.kernel_version.as_deref().unwrap_or("unknown"),
    ));
    group.add(&info_row(
        "Detected sysfs paths",
        &bundle.detected_sysfs_paths.len().to_string(),
    ));
    group.add(&info_row(
        "Capabilities",
        &format!(
            "{} available, {} missing",
            bundle.summary.available_capability_count, bundle.summary.missing_capability_count
        ),
    ));
    group.add(&info_row(
        "Sensors",
        &bundle.summary.sensor_count.to_string(),
    ));
    group.add(&info_row(
        "Fan curves",
        &bundle.summary.fan_curve_count.to_string(),
    ));
    group.add(&info_row(
        "Daemon log lines",
        &bundle.recent_daemon_logs.len().to_string(),
    ));
    page.append(&group);

    let json = render_diagnostics_json(bundle).unwrap_or_else(|error| error.to_string());
    let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let copy = gtk4::Button::with_label("Copy JSON");
    copy.set_tooltip_text(Some("Copy diagnostics JSON"));
    let json_for_clipboard = json.clone();
    copy.connect_clicked(move |_| {
        if let Some(display) = gtk4::gdk::Display::default() {
            display.clipboard().set_text(&json_for_clipboard);
        }
    });
    actions.append(&copy);
    page.append(&actions);

    let text = gtk4::TextView::new();
    text.set_editable(false);
    text.set_cursor_visible(false);
    text.set_monospace(true);
    text.set_wrap_mode(gtk4::WrapMode::WordChar);
    text.buffer().set_text(&json);

    let scroller = gtk4::ScrolledWindow::builder()
        .min_content_height(220)
        .vexpand(true)
        .child(&text)
        .build();
    page.append(&scroller);
}

fn clone_result<T: Clone>(result: &Result<T>) -> Result<T> {
    match result {
        Ok(value) => Ok(value.clone()),
        Err(error) => Err(anyhow!(error.to_string())),
    }
}

fn clone_error(error: &anyhow::Error) -> anyhow::Error {
    anyhow!(error.to_string())
}

fn runtime_snapshot_gpu_pending(
    snapshot: &Result<RuntimeSnapshot>,
) -> Result<Option<GpuModePending>> {
    snapshot
        .as_ref()
        .map(|snapshot| snapshot.diagnostics.gpu_mode_pending.clone())
        .map_err(clone_error)
}

fn runtime_snapshot_fan_snapshot(
    snapshot: &Result<RuntimeSnapshot>,
) -> Result<Option<FanCurveSnapshot>> {
    snapshot
        .as_ref()
        .map(|snapshot| snapshot.diagnostics.last_known_good_fan_curve.clone())
        .map_err(clone_error)
}

fn render_gpu_pending_row(pending: Result<Option<GpuModePending>>) -> String {
    match pending {
        Ok(Some(pending)) => {
            let previous = pending.previous_mode.as_deref().unwrap_or("unknown");
            format!(
                "{} - previous {} - reboot required {}",
                pending.requested_mode, previous, pending.reboot_required
            )
        }
        Ok(None) => "none".to_owned(),
        Err(error) => format!("state unavailable - {error}"),
    }
}

fn format_fan_snapshot_display(snapshot: &FanCurveSnapshot) -> String {
    let path = snapshot.path.as_deref().unwrap_or("unknown");
    format!("{path} - {} captured values", snapshot.points.len())
}

fn render_fan_snapshot_row(snapshot: Result<Option<FanCurveSnapshot>>) -> String {
    match snapshot {
        Ok(Some(snapshot)) => format_fan_snapshot_display(&snapshot),
        Ok(None) => "none captured".to_owned(),
        Err(error) => format!("state unavailable - {error}"),
    }
}

fn append_error(page: &gtk4::Box, error: &anyhow::Error) {
    let title = gtk4::Label::new(Some("Daemon unavailable"));
    title.add_css_class("title-2");
    title.set_xalign(0.0);
    page.append(&title);

    let message = gtk4::Label::new(Some(&error.to_string()));
    message.set_wrap(true);
    message.set_xalign(0.0);
    page.append(&message);
}

fn info_row(title: &str, value: &str) -> adw::ActionRow {
    adw::ActionRow::builder()
        .title(title)
        .subtitle(value)
        .selectable(false)
        .build()
}

fn selected_dropdown_value(chooser: &gtk4::DropDown) -> Option<String> {
    chooser
        .selected_item()
        .and_then(|item| item.downcast::<gtk4::StringObject>().ok())
        .map(|selected| selected.string().to_string())
}

fn render_dry_run_plan_summary(plan: &WriteDryRunPlan) -> String {
    format!(
        "{} -> {} via {} - reboot required {} - read-back required {}",
        plan.previous_value,
        plan.requested_value,
        plan.path,
        plan.reboot_required,
        plan.readback_required
    )
}

fn render_dry_run_recovery_summary(plan: &WriteDryRunPlan) -> String {
    if plan.rollback_instructions.is_empty() {
        "No rollback instructions were provided by the daemon.".to_owned()
    } else {
        plan.rollback_instructions.join(" ")
    }
}

fn build_gpu_mode_controls(
    capability: Option<&GpuCapability>,
    pending_row: adw::ActionRow,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Guided GPU switch planning");

    let Some(capability) = capability.filter(|capability| capability.provider == "envycontrol")
    else {
        group.add(&info_row(
            "GPU mode planning",
            "unavailable - envycontrol was not detected",
        ));
        return group;
    };

    let model = gtk4::StringList::new(GPU_MODE_CHOICES);
    let chooser = gtk4::DropDown::builder().model(&model).build();
    chooser.set_hexpand(true);
    chooser.set_selected(
        GPU_MODE_CHOICES
            .iter()
            .position(|mode| capability.mode.as_deref() == Some(*mode))
            .unwrap_or(0) as u32,
    );
    chooser.set_sensitive(!GPU_MODE_CHOICES.is_empty());

    let preview = gtk4::Button::with_label("Preview plan");
    let record_pending = gtk4::Button::with_label("Record pending");
    let clear_pending = gtk4::Button::with_label("Clear pending");

    let chooser_row = adw::ActionRow::builder()
        .title("Target mode")
        .subtitle(
            "Preview the validated EnvyControl plan first. Dashboard execution stays disabled until live validation evidence exists.",
        )
        .selectable(false)
        .build();
    chooser_row.add_suffix(&chooser);
    chooser_row.add_suffix(&preview);
    group.add(&chooser_row);

    let pending_controls = adw::ActionRow::builder()
        .title("Pending reboot state")
        .subtitle(
            "Record the requested mode after an external switch starts requiring a reboot, or clear it after reboot and verification.",
        )
        .selectable(false)
        .build();
    pending_controls.add_suffix(&record_pending);
    pending_controls.add_suffix(&clear_pending);
    group.add(&pending_controls);

    let plan_row = adw::ActionRow::builder()
        .title("Plan preview")
        .subtitle("No dry-run plan requested yet.")
        .selectable(false)
        .build();
    group.add(&plan_row);

    let recovery_row = adw::ActionRow::builder()
        .title("Recovery guidance")
        .subtitle(
            "Preview a plan to see the rollback path. GPU mode changes remain dry-run only in the dashboard for now.",
        )
        .selectable(false)
        .build();
    group.add(&recovery_row);

    let chooser_for_preview = chooser.clone();
    let plan_row_for_preview = plan_row.clone();
    let recovery_row_for_preview = recovery_row.clone();
    preview.connect_clicked(move |_| {
        let Some(requested) = selected_dropdown_value(&chooser_for_preview) else {
            plan_row_for_preview.set_title("Plan preview failed");
            plan_row_for_preview.set_subtitle("No target GPU mode was selected.");
            recovery_row_for_preview.set_subtitle(
                "Pick one of the detected GPU modes before requesting a dry-run plan.",
            );
            return;
        };

        match LegionControlClient::system()
            .and_then(|client| client.plan_gpu_mode_write(&requested))
        {
            Ok(plan) => {
                plan_row_for_preview.set_title("Plan preview ready");
                plan_row_for_preview.set_subtitle(&render_dry_run_plan_summary(&plan));
                recovery_row_for_preview.set_subtitle(&render_dry_run_recovery_summary(&plan));
            }
            Err(error) => {
                plan_row_for_preview.set_title("Plan preview failed");
                plan_row_for_preview
                    .set_subtitle(&format!("Dry-run planning could not be completed: {error}"));
                recovery_row_for_preview.set_subtitle(
                    "The daemon rejected this request or the GPU capability was not available.",
                );
            }
        }
    });

    let chooser_for_record = chooser.clone();
    let pending_row_for_record = pending_row.clone();
    let plan_row_for_record = plan_row.clone();
    let recovery_row_for_record = recovery_row.clone();
    record_pending.connect_clicked(move |_| {
        let Some(requested) = selected_dropdown_value(&chooser_for_record) else {
            plan_row_for_record.set_title("Pending reboot not recorded");
            plan_row_for_record.set_subtitle("No target GPU mode was selected.");
            return;
        };

        match LegionControlClient::system()
            .and_then(|client| client.set_gpu_mode_pending(&requested))
        {
            Ok(pending) => {
                pending_row_for_record
                    .set_subtitle(&render_gpu_pending_row(Ok(Some(pending.clone()))));
                plan_row_for_record.set_title("Pending reboot recorded");
                plan_row_for_record.set_subtitle(
                    "Recorded the requested GPU mode in app state. Clear it after reboot and verification.",
                );
                recovery_row_for_record.set_subtitle(
                    "The pending reboot banner now reflects the requested mode. This does not perform the hardware switch itself.",
                );
                let _ = request_dashboard_refresh();
            }
            Err(error) => {
                plan_row_for_record.set_title("Pending reboot not recorded");
                plan_row_for_record.set_subtitle(&format!("App-state update failed: {error}"));
            }
        }
    });

    let pending_row_for_clear = pending_row.clone();
    let plan_row_for_clear = plan_row.clone();
    let recovery_row_for_clear = recovery_row.clone();
    clear_pending.connect_clicked(move |_| {
        match LegionControlClient::system().and_then(|client| client.clear_gpu_mode_pending()) {
            Ok(previous) => {
                pending_row_for_clear.set_subtitle("none");
                plan_row_for_clear.set_title("Pending reboot cleared");
                plan_row_for_clear.set_subtitle(match previous {
                    Some(previous) if previous.reboot_required => {
                        "Cleared the app-state reboot marker after verification."
                    }
                    Some(_) => "Cleared the app-state GPU marker.",
                    None => "No pending reboot marker was set.",
                });
                recovery_row_for_clear.set_subtitle(
                    "If graphics still do not come back after reboot, use the rollback instructions from the dry-run plan in a TTY or rescue session.",
                );
                let _ = request_dashboard_refresh();
            }
            Err(error) => {
                plan_row_for_clear.set_title("Pending reboot not cleared");
                plan_row_for_clear.set_subtitle(&format!("App-state update failed: {error}"));
            }
        }
    });

    group
}

fn build_fan_planning_controls(
    fan_curves: &[FanCurveCapability],
    last_known_row: adw::ActionRow,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Guided fan planning");

    if fan_curves.is_empty() {
        group.add(&info_row(
            "Fan preset planning",
            "unavailable - no fan curve capability was detected",
        ));
        return group;
    }

    let model = gtk4::StringList::new(FAN_PRESET_IDS);
    let chooser = gtk4::DropDown::builder().model(&model).build();
    chooser.set_hexpand(true);
    chooser.set_selected(1.min(FAN_PRESET_IDS.len().saturating_sub(1)) as u32);
    chooser.set_sensitive(!FAN_PRESET_IDS.is_empty());

    let preview_preset = gtk4::Button::with_label("Preview plan");
    let preview_restore = gtk4::Button::with_label("Preview restore plan");
    let capture = gtk4::Button::with_label("Capture snapshot");

    let chooser_row = adw::ActionRow::builder()
        .title("Packaged preset")
        .subtitle(
            "Dry-run preview only. ApplyFanPreset execution stays disabled until live validation evidence exists.",
        )
        .selectable(false)
        .build();
    chooser_row.add_suffix(&chooser);
    chooser_row.add_suffix(&preview_preset);
    group.add(&chooser_row);

    let preset_plan_row = adw::ActionRow::builder()
        .title("Preset plan preview")
        .subtitle("No dry-run plan requested yet.")
        .selectable(false)
        .build();
    group.add(&preset_plan_row);

    let preset_recovery_row = adw::ActionRow::builder()
        .title("Preset recovery guidance")
        .subtitle(
            "Preview a packaged preset plan to see rollback notes. Fan preset writes remain dry-run only in the dashboard.",
        )
        .selectable(false)
        .build();
    group.add(&preset_recovery_row);

    let restore_row = adw::ActionRow::builder()
        .title("Restore automatic fan control")
        .subtitle(
            "Preview returning the EC to automatic fan curves. RestoreAutoFan execution stays disabled in the dashboard.",
        )
        .selectable(false)
        .build();
    restore_row.add_suffix(&preview_restore);
    group.add(&restore_row);

    let restore_plan_row = adw::ActionRow::builder()
        .title("Restore plan preview")
        .subtitle("No restore dry-run requested yet.")
        .selectable(false)
        .build();
    group.add(&restore_plan_row);

    let restore_recovery_row = adw::ActionRow::builder()
        .title("Restore recovery guidance")
        .subtitle("Preview a restore plan to see rollback notes.")
        .selectable(false)
        .build();
    group.add(&restore_recovery_row);

    let capture_row = adw::ActionRow::builder()
        .title("Last-known-good snapshot")
        .subtitle(
            "Capture the current hwmon curve values into app state for rollback reference. Updates the Fan curves summary row.",
        )
        .selectable(false)
        .build();
    capture_row.add_suffix(&capture);
    group.add(&capture_row);

    let chooser_for_preset = chooser.clone();
    let preset_plan_for_preset = preset_plan_row.clone();
    let preset_recovery_for_preset = preset_recovery_row.clone();
    preview_preset.connect_clicked(move |_| {
        let Some(requested) = selected_dropdown_value(&chooser_for_preset) else {
            preset_plan_for_preset.set_title("Preset plan preview failed");
            preset_plan_for_preset.set_subtitle("No packaged preset was selected.");
            preset_recovery_for_preset.set_subtitle(
                "Pick one of the packaged preset IDs before requesting a dry-run plan.",
            );
            return;
        };

        match LegionControlClient::system()
            .and_then(|client| client.plan_fan_preset_write(&requested))
        {
            Ok(plan) => {
                preset_plan_for_preset.set_title("Preset plan preview ready");
                preset_plan_for_preset.set_subtitle(&render_dry_run_plan_summary(&plan));
                preset_recovery_for_preset.set_subtitle(&render_dry_run_recovery_summary(&plan));
            }
            Err(error) => {
                preset_plan_for_preset.set_title("Preset plan preview failed");
                preset_plan_for_preset.set_subtitle(&format!(
                    "Dry-run planning could not be completed: {error}"
                ));
                preset_recovery_for_preset.set_subtitle(
                    "The daemon rejected this request or the fan curve capability was not available.",
                );
            }
        }
    });

    let restore_plan_for_restore = restore_plan_row.clone();
    let restore_recovery_for_restore = restore_recovery_row.clone();
    preview_restore.connect_clicked(move |_| {
        match LegionControlClient::system().and_then(|client| client.plan_restore_auto_fan_write())
        {
            Ok(plan) => {
                restore_plan_for_restore.set_title("Restore plan preview ready");
                restore_plan_for_restore.set_subtitle(&render_dry_run_plan_summary(&plan));
                restore_recovery_for_restore.set_subtitle(&render_dry_run_recovery_summary(&plan));
            }
            Err(error) => {
                restore_plan_for_restore.set_title("Restore plan preview failed");
                restore_plan_for_restore
                    .set_subtitle(&format!("Dry-run planning could not be completed: {error}"));
                restore_recovery_for_restore.set_subtitle(
                    "The daemon rejected this request or no automatic fan curve was detected.",
                );
            }
        }
    });

    let last_known_for_capture = last_known_row.clone();
    capture.connect_clicked(move |_| {
        match LegionControlClient::system()
            .and_then(|client| client.capture_last_known_good_fan_curve())
        {
            Ok(snapshot) => {
                last_known_for_capture.set_subtitle(&format_fan_snapshot_display(&snapshot));
                let _ = request_dashboard_refresh();
            }
            Err(error) => {
                last_known_for_capture.set_subtitle(&format!("Capture failed: {error}"));
            }
        }
    });

    group
}

fn build_platform_profile_controls(
    capability: Option<&PlatformProfileCapability>,
    current_row: Option<adw::ActionRow>,
) -> adw::PreferencesGroup {
    build_write_controls(
        "Platform profile quick apply",
        capability.map(|capability| capability.current.as_deref().unwrap_or("unknown")),
        capability.map(|capability| capability.choices.as_slice()),
        "Requested profile",
        "Apply profile",
        "Platform profile",
        |requested| {
            LegionControlClient::system().and_then(|client| client.set_platform_profile(requested))
        },
        move |_| {
            if !request_dashboard_refresh() {
                if let Some(row) = &current_row {
                    refresh_platform_profile_row(row);
                }
            }
        },
    )
}

fn build_battery_charge_type_controls(
    capability: Option<&BatteryChargeTypeCapability>,
    current_row: Option<adw::ActionRow>,
) -> adw::PreferencesGroup {
    build_write_controls(
        "Battery charge type quick apply",
        capability.map(|capability| capability.current.as_deref().unwrap_or("unknown")),
        capability.map(|capability| capability.choices.as_slice()),
        "Requested charge type",
        "Apply charge type",
        "Battery charge type",
        |requested| {
            LegionControlClient::system()
                .and_then(|client| client.set_battery_charge_type(requested))
        },
        move |_| {
            if !request_dashboard_refresh() {
                if let Some(row) = &current_row {
                    refresh_battery_charge_type_row(row);
                }
            }
        },
    )
}

fn build_led_state_controls(
    led: Option<&LedCapability>,
    current_row: Option<adw::ActionRow>,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("LED quick apply");

    let Some(led) = led else {
        group.add(&info_row(
            "Y-logo LED",
            "unavailable - quick apply disabled",
        ));
        return group;
    };

    let row = adw::ActionRow::builder()
        .title("Y-logo LED")
        .subtitle("Apply a reversible on/off change to the detected ylogo LED.")
        .selectable(false)
        .build();

    let off = gtk4::Button::with_label("Turn off");
    let on = gtk4::Button::with_label("Turn on");
    off.set_sensitive(led.brightness != Some(0));
    on.set_sensitive(led.brightness != Some(1));
    row.add_suffix(&off);
    row.add_suffix(&on);
    group.add(&row);

    let feedback_row = write_feedback_row("Y-logo LED");
    group.add(&feedback_row);

    let led_id = led.name.clone();
    let path = led.path.clone();
    let max_brightness = led.max_brightness.unwrap_or(1);
    let feedback_row_for_off = feedback_row.clone();
    let current_row_for_off = current_row.clone();
    off.connect_clicked(move |_| {
        handle_led_button_click(
            &feedback_row_for_off,
            current_row_for_off.as_ref(),
            &led_id,
            &path,
            max_brightness,
            false,
        );
    });

    let led_id = led.name.clone();
    let path = led.path.clone();
    let feedback_row_for_on = feedback_row.clone();
    let current_row_for_on = current_row.clone();
    on.connect_clicked(move |_| {
        handle_led_button_click(
            &feedback_row_for_on,
            current_row_for_on.as_ref(),
            &led_id,
            &path,
            max_brightness,
            true,
        );
    });

    group
}

fn build_ideapad_toggle_controls(
    toggle: Option<&IdeapadToggleCapability>,
    current_row: Option<adw::ActionRow>,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Fn-lock quick apply");

    let Some(toggle) = toggle else {
        group.add(&info_row(
            "Functional Fn-lock",
            "unavailable - quick apply disabled",
        ));
        return group;
    };

    let row = adw::ActionRow::builder()
        .title("Functional Fn-lock")
        .subtitle("Apply a reversible on/off change to the detected fn_lock ideapad toggle.")
        .selectable(false)
        .build();

    let off = gtk4::Button::with_label("Turn off");
    let on = gtk4::Button::with_label("Turn on");
    off.set_sensitive(toggle.current_value.as_deref() != Some("0"));
    on.set_sensitive(toggle.current_value.as_deref() != Some("1"));
    row.add_suffix(&off);
    row.add_suffix(&on);
    group.add(&row);

    let feedback_row = write_feedback_row("Fn-lock");
    group.add(&feedback_row);

    let toggle_id = toggle.name.clone();
    let path = toggle.path.clone().unwrap_or_else(|| "unknown".to_owned());
    let feedback_row_for_off = feedback_row.clone();
    let current_row_for_off = current_row.clone();
    off.connect_clicked(move |_| {
        handle_ideapad_toggle_button_click(
            &feedback_row_for_off,
            current_row_for_off.as_ref(),
            "Fn-lock",
            &toggle_id,
            &path,
            false,
        );
    });

    let toggle_id = toggle.name.clone();
    let path = toggle.path.clone().unwrap_or_else(|| "unknown".to_owned());
    let feedback_row_for_on = feedback_row.clone();
    let current_row_for_on = current_row.clone();
    on.connect_clicked(move |_| {
        handle_ideapad_toggle_button_click(
            &feedback_row_for_on,
            current_row_for_on.as_ref(),
            "Fn-lock",
            &toggle_id,
            &path,
            true,
        );
    });

    group
}

fn build_camera_power_controls(
    toggle: Option<&IdeapadToggleCapability>,
    current_row: Option<adw::ActionRow>,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Camera privacy quick apply");

    let Some(toggle) = toggle else {
        group.add(&info_row(
            "Camera power",
            "unavailable - quick apply disabled",
        ));
        return group;
    };

    let row = adw::ActionRow::builder()
        .title("Camera power")
        .subtitle("Changes require confirmation because active camera apps can lose the device.")
        .selectable(false)
        .build();

    let request_off = gtk4::Button::with_label("Request off");
    let request_on = gtk4::Button::with_label("Request on");
    request_off.set_sensitive(toggle.current_value.as_deref() != Some("0"));
    request_on.set_sensitive(toggle.current_value.as_deref() != Some("1"));
    row.add_suffix(&request_off);
    row.add_suffix(&request_on);
    group.add(&row);

    let confirm_row = adw::ActionRow::builder()
        .title("Confirmation required")
        .subtitle("Choose Request on or Request off first. If apps lose the camera, re-enable it here and restart them.")
        .selectable(false)
        .build();
    let confirm = gtk4::Button::with_label("Confirm");
    let cancel = gtk4::Button::with_label("Cancel");
    confirm.set_sensitive(false);
    cancel.set_sensitive(false);
    confirm_row.add_suffix(&confirm);
    confirm_row.add_suffix(&cancel);
    group.add(&confirm_row);

    let feedback_row = write_feedback_row("Camera power");
    group.add(&feedback_row);

    let pending_enabled = Rc::new(RefCell::new(None::<bool>));

    let confirm_row_for_off = confirm_row.clone();
    let confirm_for_off = confirm.clone();
    let cancel_for_off = cancel.clone();
    let pending_for_off = Rc::clone(&pending_enabled);
    request_off.connect_clicked(move |_| {
        *pending_for_off.borrow_mut() = Some(false);
        confirm_row_for_off.set_title("Confirm camera power change");
        confirm_row_for_off.set_subtitle(
            "Turning camera power off can interrupt active camera apps. Confirm to continue or Cancel to leave the device alone.",
        );
        confirm_for_off.set_label("Confirm off");
        confirm_for_off.set_sensitive(true);
        cancel_for_off.set_sensitive(true);
    });

    let confirm_row_for_on = confirm_row.clone();
    let confirm_for_on = confirm.clone();
    let cancel_for_on = cancel.clone();
    let pending_for_on = Rc::clone(&pending_enabled);
    request_on.connect_clicked(move |_| {
        *pending_for_on.borrow_mut() = Some(true);
        confirm_row_for_on.set_title("Confirm camera power change");
        confirm_row_for_on.set_subtitle(
            "Turning camera power on should restore the device, but camera apps may still need to be restarted. Confirm to continue or Cancel to leave the device alone.",
        );
        confirm_for_on.set_label("Confirm on");
        confirm_for_on.set_sensitive(true);
        cancel_for_on.set_sensitive(true);
    });

    let confirm_row_for_cancel = confirm_row.clone();
    let confirm_for_cancel = confirm.clone();
    let cancel_for_cancel = cancel.clone();
    let pending_for_cancel = Rc::clone(&pending_enabled);
    cancel.connect_clicked(move |_| {
        *pending_for_cancel.borrow_mut() = None;
        confirm_row_for_cancel.set_title("Confirmation required");
        confirm_row_for_cancel.set_subtitle("Choose Request on or Request off first. If apps lose the camera, re-enable it here and restart them.");
        confirm_for_cancel.set_label("Confirm");
        confirm_for_cancel.set_sensitive(false);
        cancel_for_cancel.set_sensitive(false);
    });

    let feedback_row_for_confirm = feedback_row.clone();
    let confirm_row_for_confirm = confirm_row.clone();
    let confirm_for_confirm = confirm.clone();
    let cancel_for_confirm = cancel.clone();
    let current_row_for_confirm = current_row.clone();
    let toggle_id = toggle.name.clone();
    let path = toggle.path.clone().unwrap_or_else(|| "unknown".to_owned());
    let pending_for_confirm = Rc::clone(&pending_enabled);
    confirm.connect_clicked(move |_| {
        let Some(enabled) = *pending_for_confirm.borrow() else {
            return;
        };
        handle_ideapad_toggle_button_click(
            &feedback_row_for_confirm,
            current_row_for_confirm.as_ref(),
            "Camera power",
            &toggle_id,
            &path,
            enabled,
        );
        *pending_for_confirm.borrow_mut() = None;
        confirm_row_for_confirm.set_title("Confirmation required");
        confirm_row_for_confirm.set_subtitle("Choose Request on or Request off first. If apps lose the camera, re-enable it here and restart them.");
        confirm_for_confirm.set_label("Confirm");
        confirm_for_confirm.set_sensitive(false);
        cancel_for_confirm.set_sensitive(false);
    });

    group
}

fn build_usb_charging_controls(
    toggle: Option<&IdeapadToggleCapability>,
    current_row: Option<adw::ActionRow>,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("USB charging quick apply");

    let Some(toggle) = toggle else {
        group.add(&info_row(
            "USB charging",
            "unavailable - quick apply disabled",
        ));
        return group;
    };

    let row = adw::ActionRow::builder()
        .title("USB charging")
        .subtitle(
            "Changes require confirmation because enabling always-on USB charging can drain the laptop battery while it is shut down.",
        )
        .selectable(false)
        .build();

    let request_off = gtk4::Button::with_label("Request off");
    let request_on = gtk4::Button::with_label("Request on");
    request_off.set_sensitive(toggle.current_value.as_deref() != Some("0"));
    request_on.set_sensitive(toggle.current_value.as_deref() != Some("1"));
    row.add_suffix(&request_off);
    row.add_suffix(&request_on);
    group.add(&row);

    let confirm_row = adw::ActionRow::builder()
        .title("Confirmation required")
        .subtitle("Choose Request on or Request off first. Enabling USB charging may increase battery drain while the laptop is powered off, and the write alone does not verify real-world charging behavior.")
        .selectable(false)
        .build();
    let confirm = gtk4::Button::with_label("Confirm");
    let cancel = gtk4::Button::with_label("Cancel");
    confirm.set_sensitive(false);
    cancel.set_sensitive(false);
    confirm_row.add_suffix(&confirm);
    confirm_row.add_suffix(&cancel);
    group.add(&confirm_row);

    let feedback_row = write_feedback_row("USB charging");
    group.add(&feedback_row);

    let pending_enabled = Rc::new(RefCell::new(None::<bool>));

    let confirm_row_for_off = confirm_row.clone();
    let confirm_for_off = confirm.clone();
    let cancel_for_off = cancel.clone();
    let pending_for_off = Rc::clone(&pending_enabled);
    request_off.connect_clicked(move |_| {
        *pending_for_off.borrow_mut() = Some(false);
        confirm_row_for_off.set_title("Confirm USB charging change");
        confirm_row_for_off.set_subtitle(
            "Turning USB charging off should stop always-on charging behavior. Confirm to continue or Cancel to leave the current state in place.",
        );
        confirm_for_off.set_label("Confirm off");
        confirm_for_off.set_sensitive(true);
        cancel_for_off.set_sensitive(true);
    });

    let confirm_row_for_on = confirm_row.clone();
    let confirm_for_on = confirm.clone();
    let cancel_for_on = cancel.clone();
    let pending_for_on = Rc::clone(&pending_enabled);
    request_on.connect_clicked(move |_| {
        *pending_for_on.borrow_mut() = Some(true);
        confirm_row_for_on.set_title("Confirm USB charging change");
        confirm_row_for_on.set_subtitle(
            "Turning USB charging on can drain the battery while the laptop is shut down. Confirm to continue or Cancel to leave the current state in place.",
        );
        confirm_for_on.set_label("Confirm on");
        confirm_for_on.set_sensitive(true);
        cancel_for_on.set_sensitive(true);
    });

    let confirm_row_for_cancel = confirm_row.clone();
    let confirm_for_cancel = confirm.clone();
    let cancel_for_cancel = cancel.clone();
    let pending_for_cancel = Rc::clone(&pending_enabled);
    cancel.connect_clicked(move |_| {
        *pending_for_cancel.borrow_mut() = None;
        confirm_row_for_cancel.set_title("Confirmation required");
        confirm_row_for_cancel.set_subtitle("Choose Request on or Request off first. Enabling USB charging may increase battery drain while the laptop is powered off, and the write alone does not verify real-world charging behavior.");
        confirm_for_cancel.set_label("Confirm");
        confirm_for_cancel.set_sensitive(false);
        cancel_for_cancel.set_sensitive(false);
    });

    let feedback_row_for_confirm = feedback_row.clone();
    let confirm_row_for_confirm = confirm_row.clone();
    let confirm_for_confirm = confirm.clone();
    let cancel_for_confirm = cancel.clone();
    let current_row_for_confirm = current_row.clone();
    let toggle_id = toggle.name.clone();
    let path = toggle.path.clone().unwrap_or_else(|| "unknown".to_owned());
    let pending_for_confirm = Rc::clone(&pending_enabled);
    confirm.connect_clicked(move |_| {
        let Some(enabled) = *pending_for_confirm.borrow() else {
            return;
        };
        handle_ideapad_toggle_button_click(
            &feedback_row_for_confirm,
            current_row_for_confirm.as_ref(),
            "USB charging",
            &toggle_id,
            &path,
            enabled,
        );
        *pending_for_confirm.borrow_mut() = None;
        confirm_row_for_confirm.set_title("Confirmation required");
        confirm_row_for_confirm.set_subtitle("Choose Request on or Request off first. Enabling USB charging may increase battery drain while the laptop is powered off, and the write alone does not verify real-world charging behavior.");
        confirm_for_confirm.set_label("Confirm");
        confirm_for_confirm.set_sensitive(false);
        cancel_for_confirm.set_sensitive(false);
    });

    group
}

#[allow(clippy::too_many_arguments)]
fn build_write_controls<F, G>(
    title: &str,
    current: Option<&str>,
    choices: Option<&[String]>,
    chooser_title: &str,
    button_label: &str,
    capability_label: &'static str,
    execute: F,
    on_result: G,
) -> adw::PreferencesGroup
where
    F: Fn(&str) -> Result<WriteExecutionResult> + 'static,
    G: Fn(&WriteExecutionResult) + 'static,
{
    let group = adw::PreferencesGroup::new();
    group.set_title(title);

    let Some(choices) = choices else {
        group.add(&info_row(
            capability_label,
            "unavailable - quick apply disabled",
        ));
        return group;
    };

    let can_apply = !choices.is_empty();
    let choice_refs = choices.iter().map(String::as_str).collect::<Vec<_>>();
    let model = gtk4::StringList::new(&choice_refs);
    let selected_index = current
        .and_then(|current| choices.iter().position(|choice| choice == current))
        .unwrap_or(0) as u32;

    let chooser = gtk4::DropDown::builder().model(&model).build();
    chooser.set_hexpand(true);
    chooser.set_selected(selected_index);
    chooser.set_sensitive(can_apply);

    let apply = gtk4::Button::with_label(button_label);
    apply.set_sensitive(can_apply);

    let row = adw::ActionRow::builder()
        .title(chooser_title)
        .subtitle(if can_apply {
            "Choose a detected runtime value to request from the daemon."
        } else {
            "No detected runtime values are available."
        })
        .selectable(false)
        .build();
    row.add_suffix(&chooser);
    row.add_suffix(&apply);
    group.add(&row);

    let feedback_row = write_feedback_row(capability_label);
    group.add(&feedback_row);

    let feedback_row_for_click = feedback_row.clone();
    apply.connect_clicked(move |_| {
        let Some(selected) = chooser
            .selected_item()
            .and_then(|item| item.downcast::<gtk4::StringObject>().ok())
        else {
            feedback_row_for_click.set_title("Apply result");
            feedback_row_for_click.set_subtitle("Failed - no selected value was available.");
            store_write_feedback_state(
                capability_label,
                "Apply result",
                "Failed - no selected value was available.",
            );
            return;
        };

        let requested = selected.string().to_string();
        feedback_row_for_click.set_title("Apply result");
        feedback_row_for_click.set_subtitle("Applying write request...");

        match execute(&requested) {
            Ok(result) => {
                let title = write_feedback_title(Some(&result));
                let subtitle = write_feedback_subtitle(Some(&result));
                feedback_row_for_click.set_title(title);
                feedback_row_for_click.set_subtitle(&subtitle);
                store_write_feedback_state(capability_label, title, &subtitle);
                on_result(&result);
            }
            Err(error) => {
                feedback_row_for_click.set_title("Apply error");
                let subtitle = format!("Failed - daemon call could not be completed: {error}");
                feedback_row_for_click.set_subtitle(&subtitle);
                store_write_feedback_state(capability_label, "Apply error", &subtitle);
                let _ = request_dashboard_refresh();
            }
        }
    });

    group
}

fn build_write_feedback_group(capability_label: &str) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Write feedback");
    group.add(&info_row(
        capability_label,
        "Quick apply uses polkit-gated daemon writes and reports the last result inline.",
    ));
    group
}

fn write_feedback_row(capability_label: &'static str) -> adw::ActionRow {
    let state = load_write_feedback_state(capability_label);
    adw::ActionRow::builder()
        .title(&state.title)
        .subtitle(&state.subtitle)
        .selectable(false)
        .build()
}

pub fn write_feedback_title(result: Option<&WriteExecutionResult>) -> &'static str {
    match result.map(|result| result.status) {
        None => "Apply result",
        Some(WriteExecutionStatus::Applied) => "Apply succeeded",
        Some(WriteExecutionStatus::BlockedByPolicy) => "Apply blocked by policy",
        Some(WriteExecutionStatus::BlockedByAuthorization) => "Apply blocked by authorization",
        Some(WriteExecutionStatus::Failed) => "Apply failed",
    }
}

pub fn write_feedback_subtitle(result: Option<&WriteExecutionResult>) -> String {
    match result {
        None => "No write attempted yet.".to_owned(),
        Some(result) => {
            let readback = result
                .readback_value
                .as_deref()
                .map(|value| format!(" Read-back: {value}."))
                .unwrap_or_default();
            format!("{}{}", result.message, readback)
        }
    }
}

fn refresh_platform_profile_row(row: &adw::ActionRow) {
    if let Ok(snapshot) =
        LegionControlClient::system().and_then(|client| client.refresh_runtime_snapshot())
    {
        if let Some(profile) = snapshot.diagnostics.raw_probe_report.platform_profile {
            row.set_subtitle(profile.current.as_deref().unwrap_or("unknown"));
        }
    }
}

fn refresh_battery_charge_type_row(row: &adw::ActionRow) {
    if let Ok(snapshot) =
        LegionControlClient::system().and_then(|client| client.refresh_runtime_snapshot())
    {
        if let Some(charge_type) = snapshot.diagnostics.raw_probe_report.battery_charge_type {
            row.set_subtitle(charge_type.current.as_deref().unwrap_or("unknown"));
        }
    }
}

fn handle_led_button_click(
    feedback_row: &adw::ActionRow,
    current_row: Option<&adw::ActionRow>,
    led_id: &str,
    path: &str,
    max_brightness: i64,
    enabled: bool,
) {
    feedback_row.set_title("Apply result");
    feedback_row.set_subtitle("Applying write request...");

    match LegionControlClient::system().and_then(|client| client.set_led_state(led_id, enabled)) {
        Ok(result) => {
            let title = write_feedback_title(Some(&result));
            let subtitle = write_feedback_subtitle(Some(&result));
            feedback_row.set_title(title);
            feedback_row.set_subtitle(&subtitle);
            store_write_feedback_state("Y-logo LED", title, &subtitle);
            if !request_dashboard_refresh() {
                if let Some(row) = current_row {
                    refresh_led_row(row, led_id, path, max_brightness, &result);
                }
            }
        }
        Err(error) => {
            feedback_row.set_title("Apply error");
            let subtitle = format!("Failed - daemon call could not be completed: {error}");
            feedback_row.set_subtitle(&subtitle);
            store_write_feedback_state("Y-logo LED", "Apply error", &subtitle);
            let _ = request_dashboard_refresh();
        }
    }
}

fn handle_ideapad_toggle_button_click(
    feedback_row: &adw::ActionRow,
    current_row: Option<&adw::ActionRow>,
    capability_label: &'static str,
    toggle_id: &str,
    path: &str,
    enabled: bool,
) {
    feedback_row.set_title("Apply result");
    feedback_row.set_subtitle("Applying write request...");

    match LegionControlClient::system()
        .and_then(|client| client.set_ideapad_toggle(toggle_id, enabled))
    {
        Ok(result) => {
            let title = write_feedback_title(Some(&result));
            let subtitle = write_feedback_subtitle(Some(&result));
            feedback_row.set_title(title);
            feedback_row.set_subtitle(&subtitle);
            store_write_feedback_state(capability_label, title, &subtitle);
            if !request_dashboard_refresh() {
                if let Some(row) = current_row {
                    refresh_ideapad_toggle_row(row, toggle_id, path, &result);
                }
            }
        }
        Err(error) => {
            feedback_row.set_title("Apply error");
            let subtitle = format!("Failed - daemon call could not be completed: {error}");
            feedback_row.set_subtitle(&subtitle);
            store_write_feedback_state(capability_label, "Apply error", &subtitle);
            let _ = request_dashboard_refresh();
        }
    }
}

fn refresh_led_row(
    row: &adw::ActionRow,
    led_id: &str,
    path: &str,
    max_brightness: i64,
    result: &WriteExecutionResult,
) {
    if let Ok(snapshot) =
        LegionControlClient::system().and_then(|client| client.refresh_runtime_snapshot())
    {
        if let Some(led) = snapshot
            .diagnostics
            .raw_probe_report
            .leds
            .into_iter()
            .find(|led| led.name == led_id)
        {
            row.set_subtitle(&render_led_row(&led));
            return;
        }
    }

    if let Some(readback) = result.readback_value.as_deref() {
        row.set_subtitle(&format!(
            "brightness {} / max {} - {}",
            readback, max_brightness, path
        ));
    }
}

fn refresh_ideapad_toggle_row(
    row: &adw::ActionRow,
    toggle_id: &str,
    path: &str,
    result: &WriteExecutionResult,
) {
    if let Ok(snapshot) =
        LegionControlClient::system().and_then(|client| client.refresh_runtime_snapshot())
    {
        if let Some(toggle) = snapshot
            .diagnostics
            .raw_probe_report
            .ideapad_toggles
            .into_iter()
            .find(|toggle| toggle.name == toggle_id)
        {
            row.set_subtitle(&render_ideapad_toggle_row(&toggle));
            return;
        }
    }

    if let Some(readback) = result.readback_value.as_deref() {
        row.set_subtitle(&format!("{readback} - {path}"));
    }
}

fn writable_ylogo(leds: &[LedCapability]) -> Option<&LedCapability> {
    leds.iter().find(|led| {
        led.name == "platform::ylogo"
            && led.max_brightness == Some(1)
            && matches!(led.brightness, Some(0 | 1))
    })
}

fn writable_fn_lock_toggle<'a>(
    toggles: &'a [IdeapadToggleCapability],
    leds: &[LedCapability],
) -> Option<&'a IdeapadToggleCapability> {
    toggles.iter().find(|toggle| {
        if toggle.name != "fn_lock" || !matches!(toggle.current_value.as_deref(), Some("0" | "1")) {
            return false;
        }
        let Some(path) = toggle.path.as_deref() else {
            return false;
        };
        if path.is_empty() {
            return false;
        }
        leds.iter().any(|led| {
            led.name == "platform::fnlock"
                && led.max_brightness == Some(1)
                && matches!(led.brightness, Some(0 | 1))
                && toggle.current_value.as_deref()
                    == led
                        .brightness
                        .map(|brightness| if brightness == 0 { "0" } else { "1" })
        })
    })
}

fn writable_camera_power_toggle(
    toggles: &[IdeapadToggleCapability],
) -> Option<&IdeapadToggleCapability> {
    toggles.iter().find(|toggle| {
        toggle.name == "camera_power"
            && matches!(toggle.current_value.as_deref(), Some("0" | "1"))
            && toggle.path.as_deref().is_some_and(|path| !path.is_empty())
    })
}

fn writable_usb_charging_toggle(
    toggles: &[IdeapadToggleCapability],
) -> Option<&IdeapadToggleCapability> {
    toggles.iter().find(|toggle| {
        toggle.name == "usb_charging"
            && matches!(toggle.current_value.as_deref(), Some("0" | "1"))
            && toggle.path.as_deref().is_some_and(|path| !path.is_empty())
    })
}

fn render_led_row(led: &LedCapability) -> String {
    let brightness = led
        .brightness
        .map(|brightness| brightness.to_string())
        .unwrap_or_else(|| "unknown".to_owned());
    let max = led
        .max_brightness
        .map(|max| max.to_string())
        .unwrap_or_else(|| "unknown".to_owned());
    format!("brightness {brightness} / max {max} - {}", led.path)
}

fn render_ideapad_toggle_row(toggle: &IdeapadToggleCapability) -> String {
    let value = toggle.current_value.as_deref().unwrap_or("unknown");
    let path = toggle.path.as_deref().unwrap_or("unknown");
    format!("{value} - {path}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};

    #[test]
    fn request_dashboard_refresh_reports_missing_hook() {
        clear_dashboard_refresh_hook();
        assert!(!request_dashboard_refresh());
    }

    #[test]
    fn request_dashboard_refresh_invokes_installed_hook() {
        clear_dashboard_refresh_hook();

        let refresh_count = Rc::new(Cell::new(0usize));
        let refresh_count_for_hook = Rc::clone(&refresh_count);
        install_dashboard_refresh_hook(Rc::new(move || {
            refresh_count_for_hook.set(refresh_count_for_hook.get() + 1);
        }));

        assert!(request_dashboard_refresh());
        assert_eq!(refresh_count.get(), 1);

        clear_dashboard_refresh_hook();
    }

    #[test]
    fn write_feedback_row_reuses_stored_feedback_state() {
        store_write_feedback_state(
            "Platform profile",
            "Apply blocked by policy",
            "Writes are disabled for this daemon instance.",
        );

        let state = load_write_feedback_state("Platform profile");
        assert_eq!(state.title, "Apply blocked by policy");
        assert_eq!(
            state.subtitle,
            "Writes are disabled for this daemon instance."
        );

        WRITE_FEEDBACK_STATE.with(|state| {
            state.borrow_mut().remove("Platform profile");
        });
    }
}
