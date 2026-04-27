use crate::{
    capability_status_label, render_diagnostics_json, risk_level_label, runtime_refresh_notice,
    DiagnosticsBundle, LegionControlClient, RuntimeSnapshot, UiStatus,
};
use adw::prelude::*;
use anyhow::{anyhow, Result};
use legion_common::{
    decode_fan_scratchpad_toml_v1, encode_fan_scratchpad_toml_v1, fan_curve_hwmon_point_pairs,
    fan_curve_snapshot_chart_pairs, fan_preset_points_as_sysfs_raw, format_fan_curve_live_vs_saved,
    format_gpu_mode_pending_summary, format_manual_fan_scratchpad_sysfs_preview,
    parse_fan_preset_toml, validate_fan_preset_document, validate_manual_fan_curve_pairs,
    BatteryChargeTypeCapability, FanCurveCapability, FanCurveHwmonPointPair, FanCurveSnapshot,
    GpuCapability, GpuModePending, IdeapadToggleCapability, LedCapability,
    PlatformProfileCapability, WriteDryRunPlan, WriteExecutionResult, WriteExecutionStatus,
};
use std::path::Path;
use std::thread_local;
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap},
    rc::Rc,
    time::{Duration, Instant},
};

const AUTO_REFRESH_INTERVAL: Duration = Duration::from_secs(30);
const RESUME_REFRESH_GAP: Duration = Duration::from_secs(90);
const DEFAULT_DASHBOARD_PAGE: &str = "status";
const GPU_MODE_CHOICES: &[&str] = &["integrated", "hybrid", "nvidia"];
const FAN_PRESET_IDS: &[&str] = &["quiet-office", "balanced-daily", "gaming", "max-safe"];
const FAN_CURVE_CHART_MARGIN: f64 = 36.0;
const SCRATCHPAD_CHART_DRAG_PICK_PX: f64 = 22.0;
const SCRATCHPAD_DRAG_TEMP_PER_PX: f64 = 25.0;
const SCRATCHPAD_DRAG_PWM_PER_PX: f64 = -0.45;
/// Fine keyboard nudge for raw sysfs temperature (same order of magnitude as typical millidegree steps).
const SCRATCHPAD_KEY_TEMP_FINE: i32 = 500;
const SCRATCHPAD_KEY_TEMP_COARSE: i32 = 5000;
const SCRATCHPAD_KEY_PWM_FINE: i32 = 1;
const SCRATCHPAD_KEY_PWM_COARSE: i32 = 8;

type ScratchpadChartSelection = Rc<RefCell<Option<usize>>>;
type ManualFanScratchRows = Rc<RefCell<Vec<(FanCurveHwmonPointPair, gtk4::Entry, gtk4::Entry)>>>;

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

    if let Some(pp) = &bundle.raw_probe_report.power_profiles {
        let desktop = adw::PreferencesGroup::new();
        desktop.set_title("Desktop PowerProfiles");
        desktop.set_description(Some(
            "Session D-Bus `org.freedesktop.UPower.PowerProfiles` (often power-profiles-daemon).",
        ));
        desktop.add(&info_row("Bus", &pp.bus));
        desktop.add(&info_row("Well-known name", &pp.well_known_name));
        if let Some(owner) = &pp.unique_owner {
            desktop.add(&info_row("Unique owner", owner));
            desktop.add(&info_row(
                "Active profile",
                pp.active_profile.as_deref().unwrap_or("unknown"),
            ));
        } else if let Some(detail) = &pp.detail {
            desktop.add(&info_row("Unavailable", detail));
        } else {
            desktop.add(&info_row("Unavailable", "no D-Bus owner"));
        }
        desktop.add(&info_row(
            "Probe status",
            capability_status_label(pp.status),
        ));
        page.append(&desktop);
    }
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
    let last_known_row = info_row("Last known good", &render_fan_snapshot_row(&fan_snapshot));
    curves.add(&last_known_row);
    page.append(&curves);

    append_fan_curve_readonly_preview(page, bundle, &fan_snapshot);

    append_fan_preset_per_profile_section(page, bundle);

    let lkg_points_group =
        append_saved_lkg_curve_detail(page, &clone_result(&fan_snapshot), last_known_row.clone());

    page.append(&build_fan_planning_controls(
        bundle.raw_probe_report.fan_curves.as_slice(),
        last_known_row,
        lkg_points_group,
    ));

    if !bundle.raw_probe_report.fan_curves.is_empty() {
        append_fan_live_curve_readings(page);
        append_fan_live_vs_saved_compare(page);
        append_manual_fan_curve_scratchpad(page, &bundle.raw_probe_report.fan_curves[0]);
    }
}

fn fan_curve_sysfs_basename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_owned()
}

fn clear_points_column(column: &gtk4::Box) {
    while let Some(child) = column.first_child() {
        column.remove(&child);
    }
}

fn append_fan_curve_point_rows_box(column: &gtk4::Box, snapshot: &FanCurveSnapshot) {
    if snapshot.points.is_empty() {
        column.append(&info_row("Points", "No pwm/temp values in this snapshot."));
        return;
    }
    const MAX_ROWS: usize = 32;
    for point in snapshot.points.iter().take(MAX_ROWS) {
        let title = fan_curve_sysfs_basename(&point.path);
        column.append(&info_row(&title, &point.value));
    }
    if snapshot.points.len() > MAX_ROWS {
        column.append(&info_row(
            "…",
            &format!(
                "{} additional nodes omitted for display",
                snapshot.points.len() - MAX_ROWS
            ),
        ));
    }
}

fn repopulate_live_points_column(column: &gtk4::Box, snapshot: &FanCurveSnapshot) {
    clear_points_column(column);
    append_fan_curve_point_rows_box(column, snapshot);
}

fn repopulate_saved_lkg_points_column(
    column: &gtk4::Box,
    snapshot: &Result<Option<FanCurveSnapshot>>,
) {
    clear_points_column(column);
    match snapshot {
        Err(error) => {
            column.append(&info_row("State unavailable", &error.to_string()));
        }
        Ok(None) => {
            column.append(&info_row(
                "No snapshot",
                "None captured yet. Use Capture snapshot in Guided fan planning, then refresh here.",
            ));
        }
        Ok(Some(curve)) => {
            append_fan_curve_point_rows_box(column, curve);
        }
    }
}

fn append_saved_lkg_curve_detail(
    page: &gtk4::Box,
    fan_snapshot: &Result<Option<FanCurveSnapshot>>,
    last_known_row: adw::ActionRow,
) -> gtk4::Box {
    let section_title = gtk4::Label::new(Some("Saved last-known-good detail"));
    section_title.add_css_class("title-3");
    section_title.set_xalign(0.0);
    page.append(&section_title);

    let hint = gtk4::Label::new(Some(
        "Point values from durable app state (GetLastKnownGoodFanCurve). Read-only; use Capture snapshot in Guided fan planning to update.",
    ));
    hint.set_wrap(true);
    hint.set_xalign(0.0);
    page.append(&hint);

    let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let refresh_saved = gtk4::Button::with_label("Refresh saved snapshot");
    actions.append(&refresh_saved);
    page.append(&actions);

    let points_heading = gtk4::Label::new(Some("Saved curve points (read-only)"));
    points_heading.add_css_class("title-4");
    points_heading.set_xalign(0.0);
    points_heading.set_margin_top(8);
    page.append(&points_heading);

    let points_column = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    points_column.set_margin_start(4);
    repopulate_saved_lkg_points_column(&points_column, fan_snapshot);
    page.append(&points_column);

    let points_for_refresh = points_column.clone();
    let last_known_for_refresh = last_known_row.clone();
    refresh_saved.connect_clicked(move |_| {
        match LegionControlClient::system().and_then(|client| client.last_known_good_fan_curve()) {
            Ok(maybe) => {
                repopulate_saved_lkg_points_column(&points_for_refresh, &Ok(maybe.clone()));
                last_known_for_refresh.set_subtitle(&render_fan_snapshot_row(&Ok(maybe)));
            }
            Err(error) => {
                repopulate_saved_lkg_points_column(
                    &points_for_refresh,
                    &Err(anyhow!(error.to_string())),
                );
                last_known_for_refresh.set_subtitle(&format!("state unavailable - {error}"));
            }
        }
    });

    points_column
}

fn format_fan_snapshot_multiline(snapshot: &FanCurveSnapshot) -> String {
    let mut out = format!("curve_id={}\n", snapshot.curve_id);
    if let Some(root) = snapshot.path.as_deref() {
        out.push_str(&format!("hwmon_root={root}\n"));
    }
    out.push('\n');
    for point in &snapshot.points {
        out.push_str(&format!("{} = {}\n", point.path, point.value));
    }
    out
}

fn append_fan_live_curve_readings(page: &gtk4::Box) {
    let section_title = gtk4::Label::new(Some("Live curve readings"));
    section_title.add_css_class("title-3");
    section_title.set_xalign(0.0);
    page.append(&section_title);

    let hint = gtk4::Label::new(Some(
        "Read-only sysfs snapshot from the daemon. This does not update the last-known-good capture.",
    ));
    hint.set_wrap(true);
    hint.set_xalign(0.0);
    page.append(&hint);

    let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let refresh = gtk4::Button::with_label("Refresh live readings");
    actions.append(&refresh);
    page.append(&actions);

    let points_heading = gtk4::Label::new(Some("Live sysfs points (read-only)"));
    points_heading.add_css_class("title-4");
    points_heading.set_xalign(0.0);
    points_heading.set_margin_top(8);
    page.append(&points_heading);

    let points_column = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    points_column.set_margin_start(4);
    points_column.append(&info_row(
        "No data yet",
        "Use Refresh live readings to load current pwm/temp values.",
    ));
    page.append(&points_column);

    let text = gtk4::TextView::new();
    text.set_editable(false);
    text.set_cursor_visible(false);
    text.set_monospace(true);
    text.set_wrap_mode(gtk4::WrapMode::WordChar);
    text.buffer().set_text(
        "Click \"Refresh live readings\" to fetch the current pwm/temp point values from sysfs.",
    );

    let scroller = gtk4::ScrolledWindow::builder()
        .min_content_height(140)
        .vexpand(false)
        .child(&text)
        .build();
    page.append(&scroller);

    let text_for_refresh = text.clone();
    let points_for_refresh = points_column.clone();
    refresh.connect_clicked(move |_| {
        match LegionControlClient::system().and_then(|client| client.live_fan_curve_readings()) {
            Ok(snapshot) => {
                repopulate_live_points_column(&points_for_refresh, &snapshot);
                text_for_refresh
                    .buffer()
                    .set_text(&format_fan_snapshot_multiline(&snapshot));
            }
            Err(error) => {
                clear_points_column(&points_for_refresh);
                points_for_refresh.append(&info_row("Live readings failed", &error.to_string()));
                text_for_refresh
                    .buffer()
                    .set_text(&format!("Live readings failed:\n{error}"));
            }
        }
    });
}

fn append_fan_live_vs_saved_compare(page: &gtk4::Box) {
    let section_title = gtk4::Label::new(Some("Live vs saved comparison"));
    section_title.add_css_class("title-3");
    section_title.set_xalign(0.0);
    page.append(&section_title);

    let hint = gtk4::Label::new(Some(
        "Read-only diff between current sysfs values (GetLiveFanCurveReadings) and the durable last-known-good capture (GetLastKnownGoodFanCurve). Does not refresh other sections.",
    ));
    hint.set_wrap(true);
    hint.set_xalign(0.0);
    page.append(&hint);

    let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let compare = gtk4::Button::with_label("Compare live to saved");
    actions.append(&compare);
    page.append(&actions);

    let text = gtk4::TextView::new();
    text.set_editable(false);
    text.set_cursor_visible(false);
    text.set_monospace(true);
    text.set_wrap_mode(gtk4::WrapMode::WordChar);
    text.buffer().set_text(
        "Click \"Compare live to saved\" after capturing a last-known-good snapshot to see where sysfs values diverge.",
    );

    let scroller = gtk4::ScrolledWindow::builder()
        .min_content_height(120)
        .vexpand(false)
        .child(&text)
        .build();
    page.append(&scroller);

    let text_for_compare = text.clone();
    compare.connect_clicked(move |_| {
        let client_result = LegionControlClient::system();
        let Ok(client) = client_result else {
            text_for_compare.buffer().set_text(&format!(
                "D-Bus client unavailable:\n{}",
                client_result.err().map(|e| e.to_string()).unwrap_or_default()
            ));
            return;
        };
        let live_result = client.live_fan_curve_readings();
        let saved_result = client.last_known_good_fan_curve();
        match (live_result, saved_result) {
            (Ok(live), Ok(Some(saved))) => {
                let report = format_fan_curve_live_vs_saved(&live, &saved);
                text_for_compare.buffer().set_text(&report);
            }
            (Ok(_live), Ok(None)) => {
                text_for_compare.buffer().set_text(
                    "No last-known-good snapshot is stored yet.\nUse Capture snapshot in Guided fan planning first, then compare again.",
                );
            }
            (Err(error), _) => {
                text_for_compare
                    .buffer()
                    .set_text(&format!("Live readings failed:\n{error}"));
            }
            (_, Err(error)) => {
                text_for_compare
                    .buffer()
                    .set_text(&format!("Saved snapshot read failed:\n{error}"));
            }
        }
    });
}

struct TempPwmChartLayout {
    t_min: f64,
    t_span: f64,
    t_min_u: u32,
    t_max_u: u32,
    plot_w: f64,
    plot_h: f64,
    margin: f64,
}

fn temp_pwm_chart_layout(
    pairs: &[(u32, u32)],
    width: i32,
    height: i32,
) -> Option<TempPwmChartLayout> {
    if pairs.is_empty() {
        return None;
    }
    let w = width.max(1) as f64;
    let h = height.max(1) as f64;
    let m = FAN_CURVE_CHART_MARGIN;
    let plot_w = (w - 2.0 * m).max(1.0);
    let plot_h = (h - 2.0 * m).max(1.0);
    let t_min_u = pairs.iter().map(|(temp, _)| *temp).min()?;
    let t_max_u = pairs.iter().map(|(temp, _)| *temp).max()?;
    let t_min = t_min_u as f64;
    let t_max = t_max_u as f64;
    let t_span = (t_max - t_min).max(1.0);
    Some(TempPwmChartLayout {
        t_min,
        t_span,
        t_min_u,
        t_max_u,
        plot_w,
        plot_h,
        margin: m,
    })
}

fn temp_pwm_pair_pixel_coords(
    pairs: &[(u32, u32)],
    width: i32,
    height: i32,
) -> Option<Vec<(f64, f64)>> {
    let lay = temp_pwm_chart_layout(pairs, width, height)?;
    Some(
        pairs
            .iter()
            .map(|(temp, pwm)| {
                let pwm_clamped = (*pwm).min(255) as f64;
                let x = lay.margin + ((*temp as f64) - lay.t_min) / lay.t_span * lay.plot_w;
                let y = lay.margin + lay.plot_h - pwm_clamped / 255.0 * lay.plot_h;
                (x, y)
            })
            .collect(),
    )
}

fn draw_temp_pwm_chart_grid_and_axes(
    cr: &gtk4::cairo::Context,
    lay: &TempPwmChartLayout,
    height: f64,
) {
    use gtk4::cairo::{FontSlant, FontWeight};

    cr.save().ok();
    cr.select_font_face("Sans", FontSlant::Normal, FontWeight::Normal);
    cr.set_font_size(10.0);
    cr.set_source_rgb(0.45, 0.48, 0.52);

    for frac in [0.25_f64, 0.5_f64, 0.75_f64] {
        cr.set_source_rgba(0.88, 0.89, 0.91, 1.0);
        cr.set_line_width(1.0);
        let y = lay.margin + lay.plot_h * (1.0 - frac);
        cr.move_to(lay.margin, y);
        cr.line_to(lay.margin + lay.plot_w, y);
        let _ = cr.stroke();
    }

    cr.set_source_rgb(0.45, 0.48, 0.52);
    cr.set_line_width(1.0);
    for pwm in [255u32, 128, 0] {
        let y = lay.margin + lay.plot_h - (pwm as f64 / 255.0) * lay.plot_h;
        cr.move_to(lay.margin - 6.0, y);
        cr.line_to(lay.margin, y);
        let _ = cr.stroke();
    }

    let label_x = 2.0_f64;
    for (pwm, dy) in [(255u32, 4.0_f64), (128, 4.0), (0, -2.0)] {
        let y = lay.margin + lay.plot_h - (pwm as f64 / 255.0) * lay.plot_h + dy;
        let text = pwm.to_string();
        cr.move_to(label_x, y);
        let _ = cr.show_text(&text);
    }

    let axis_title = "Temperature (raw sysfs) → PWM";
    if let Ok(ext) = cr.text_extents(axis_title) {
        let x = lay.margin + (lay.plot_w - ext.width()) / 2.0 - ext.x_bearing();
        let y = height - 22.0;
        cr.move_to(x, y);
        let _ = cr.show_text(axis_title);
    }

    let lo = lay.t_min_u.to_string();
    cr.move_to(lay.margin, height - 8.0);
    let _ = cr.show_text(&lo);

    let hi = lay.t_max_u.to_string();
    if let Ok(ext) = cr.text_extents(&hi) {
        let x = lay.margin + lay.plot_w - ext.width() - ext.x_bearing();
        cr.move_to(x, height - 8.0);
        let _ = cr.show_text(&hi);
    }

    cr.restore().ok();
}

fn nearest_temp_pwm_point_index(
    px: f64,
    py: f64,
    pairs: &[(u32, u32)],
    width: i32,
    height: i32,
) -> Option<usize> {
    let coords = temp_pwm_pair_pixel_coords(pairs, width, height)?;
    let (best_i, best_d) = coords
        .iter()
        .enumerate()
        .map(|(i, (x, y))| (i, (px - x).hypot(py - y)))
        .min_by(|(_, d1), (_, d2)| d1.partial_cmp(d2).unwrap_or(std::cmp::Ordering::Equal))?;
    if best_d <= SCRATCHPAD_CHART_DRAG_PICK_PX {
        Some(best_i)
    } else {
        None
    }
}

fn draw_temp_pwm_polyline_chart(
    cr: &gtk4::cairo::Context,
    width: i32,
    height: i32,
    pairs: &[(u32, u32)],
    highlight_index: Option<usize>,
) {
    let w = width.max(1) as f64;
    let h = height.max(1) as f64;

    cr.set_source_rgb(0.98, 0.98, 0.99);
    cr.rectangle(0.0, 0.0, w, h);
    let _ = cr.fill().ok();

    let Some(lay) = temp_pwm_chart_layout(pairs, width, height) else {
        return;
    };

    cr.set_source_rgb(0.78, 0.8, 0.86);
    cr.set_line_width(1.0);
    cr.rectangle(lay.margin, lay.margin, lay.plot_w, lay.plot_h);
    let _ = cr.stroke().ok();

    draw_temp_pwm_chart_grid_and_axes(cr, &lay, h);

    cr.set_source_rgb(0.1, 0.35, 0.82);
    cr.set_line_width(2.5);
    let mut first = true;
    for (temp, pwm) in pairs {
        let pwm_clamped = (*pwm).min(255) as f64;
        let x = lay.margin + ((*temp as f64) - lay.t_min) / lay.t_span * lay.plot_w;
        let y = lay.margin + lay.plot_h - pwm_clamped / 255.0 * lay.plot_h;
        if first {
            cr.move_to(x, y);
            first = false;
        } else {
            cr.line_to(x, y);
        }
    }
    let _ = cr.stroke().ok();

    for (i, (temp, pwm)) in pairs.iter().enumerate() {
        let pwm_clamped = (*pwm).min(255) as f64;
        let x = lay.margin + ((*temp as f64) - lay.t_min) / lay.t_span * lay.plot_w;
        let y = lay.margin + lay.plot_h - pwm_clamped / 255.0 * lay.plot_h;
        let highlight = highlight_index == Some(i);
        if highlight {
            cr.set_source_rgb(0.95, 0.55, 0.12);
            cr.arc(x, y, 9.0, 0.0, std::f64::consts::TAU);
            let _ = cr.fill().ok();
        }
        cr.set_source_rgb(0.05, 0.22, 0.58);
        let r = if highlight { 5.0 } else { 4.0 };
        cr.arc(x, y, r, 0.0, std::f64::consts::TAU);
        let _ = cr.fill().ok();
    }
}

fn draw_fan_curve_temp_pwm_chart(
    cr: &gtk4::cairo::Context,
    width: i32,
    height: i32,
    pairs: &[(u32, u32)],
) {
    draw_temp_pwm_polyline_chart(cr, width, height, pairs, None);
}

fn scratchpad_pairs_parsed(entries: &ManualFanScratchRows) -> Option<Vec<(u32, u32)>> {
    let rows = entries.borrow();
    if rows.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(rows.len());
    for (_, temp_entry, pwm_entry) in rows.iter() {
        let t = temp_entry.text().trim().parse::<u32>().ok()?;
        let p = pwm_entry.text().trim().parse::<u32>().ok()?;
        out.push((t, p.min(255)));
    }
    Some(out)
}

fn scratchpad_first_parse_blocker(entries: &ManualFanScratchRows) -> String {
    let rows = entries.borrow();
    if rows.is_empty() {
        return "No editable rows yet — use Load from live/saved or wait for the table to appear."
            .to_string();
    }
    for (i, (_, temp_entry, pwm_entry)) in rows.iter().enumerate() {
        let ti = temp_entry.text();
        let pi = pwm_entry.text();
        let t_trim = ti.trim();
        let p_trim = pi.trim();
        let n = i + 1;
        if t_trim.is_empty() || p_trim.is_empty() {
            return format!(
                "Point {n}: fill both temp and pwm cells with integers (empty cells hide the curve)."
            );
        }
        if t_trim.parse::<u32>().is_err() {
            return format!("Point {n}: temp `{t_trim}` is not a valid unsigned integer.");
        }
        if p_trim.parse::<u32>().is_err() {
            return format!("Point {n}: pwm `{p_trim}` is not a valid unsigned integer.");
        }
    }
    "Some values could not be read — check each row for stray characters.".to_string()
}

fn scratchpad_placeholder_text_lines(message: &str, max_chars: usize) -> Vec<String> {
    let t = message.trim();
    if t.is_empty() {
        return vec!["(no message)".to_string()];
    }
    let chars: Vec<char> = t.chars().collect();
    let mut lines = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        let end = (i + max_chars).min(chars.len());
        lines.push(chars[i..end].iter().collect::<String>());
        i = end;
    }
    lines
}

fn draw_scratchpad_chart_placeholder(
    cr: &gtk4::cairo::Context,
    width: i32,
    height: i32,
    message: &str,
) {
    use gtk4::cairo::{FontSlant, FontWeight};

    let w = width.max(1) as f64;
    let h = height.max(1) as f64;
    cr.set_source_rgb(0.94, 0.94, 0.95);
    cr.rectangle(0.0, 0.0, w, h);
    let _ = cr.fill();

    cr.save().ok();
    cr.select_font_face("Sans", FontSlant::Normal, FontWeight::Normal);
    cr.set_font_size(11.5);
    cr.set_source_rgb(0.32, 0.34, 0.38);
    let mut y = 22.0_f64;
    for line in scratchpad_placeholder_text_lines(message, 52) {
        if y > h - 8.0 {
            cr.move_to(12.0, y);
            let _ = cr.show_text("...");
            break;
        }
        cr.move_to(12.0, y);
        let _ = cr.show_text(line.as_str());
        y += 16.0;
    }
    cr.restore().ok();
}

fn apply_scratchpad_nudge(
    entries: &ManualFanScratchRows,
    idx: usize,
    delta_temp: i32,
    delta_pwm: i32,
) -> bool {
    let borrowed = entries.borrow();
    let Some((_, temp_entry, pwm_entry)) = borrowed.get(idx) else {
        return false;
    };
    let Ok(t0) = temp_entry.text().trim().parse::<u32>() else {
        return false;
    };
    let Ok(p0) = pwm_entry.text().trim().parse::<u32>() else {
        return false;
    };
    drop(borrowed);
    let t1 = (t0 as i64 + i64::from(delta_temp)).clamp(1, 900_000) as u32;
    let p1 = (p0 as i64 + i64::from(delta_pwm)).clamp(0, 255) as u32;
    let borrowed = entries.borrow();
    let Some((_, temp_entry, pwm_entry)) = borrowed.get(idx) else {
        return false;
    };
    temp_entry.set_text(&t1.to_string());
    pwm_entry.set_text(&p1.to_string());
    true
}

fn connect_scratchpad_chart_entry_signals(
    entries: &ManualFanScratchRows,
    chart: &gtk4::DrawingArea,
    selection_sync: Option<&ScratchpadChartSelection>,
) {
    for (row_i, (_, temp_entry, pwm_entry)) in entries.borrow().iter().enumerate() {
        let chart_t = chart.clone();
        temp_entry.connect_changed(move |_| chart_t.queue_draw());
        let chart_p = chart.clone();
        pwm_entry.connect_changed(move |_| chart_p.queue_draw());

        if let Some(sel) = selection_sync {
            let sel_temp = sel.clone();
            let chart_ft = chart.clone();
            let idx = row_i;
            let focus_temp = gtk4::EventControllerFocus::new();
            focus_temp.connect_enter(move |_c| {
                *sel_temp.borrow_mut() = Some(idx);
                chart_ft.queue_draw();
            });
            temp_entry.add_controller(focus_temp);

            let sel_pwm = sel.clone();
            let chart_fp = chart.clone();
            let focus_pwm = gtk4::EventControllerFocus::new();
            focus_pwm.connect_enter(move |_c| {
                *sel_pwm.borrow_mut() = Some(idx);
                chart_fp.queue_draw();
            });
            pwm_entry.add_controller(focus_pwm);
        }
    }
}

fn attach_scratchpad_chart_interactions(
    chart: &gtk4::DrawingArea,
    entries: &ManualFanScratchRows,
    selection: &ScratchpadChartSelection,
) {
    chart.set_focusable(true);
    chart.set_tooltip_text(Some(
        "Click a point to select it. Tab into a row's temp or pwm field to sync the highlight. Arrow keys adjust temp (←/→) and PWM (↑/↓); hold Shift for larger steps. Scratchpad only.",
    ));

    let click = gtk4::GestureClick::new();
    let entries_pick = entries.clone();
    let chart_pick = chart.clone();
    let selection_pick = selection.clone();
    click.connect_pressed(move |_gesture, _n_press, x, y| {
        let Some(pairs) = scratchpad_pairs_parsed(&entries_pick) else {
            return;
        };
        let w = chart_pick.width().max(1);
        let h = chart_pick.height().max(1);
        let Some(idx) = nearest_temp_pwm_point_index(x, y, &pairs, w, h) else {
            return;
        };
        *selection_pick.borrow_mut() = Some(idx);
        chart_pick.grab_focus();
        chart_pick.queue_draw();
    });
    chart.add_controller(click);

    let drag_state: Rc<RefCell<Option<(usize, u32, u32)>>> = Rc::new(RefCell::new(None));
    let gesture = gtk4::GestureDrag::new();
    let entries_begin = entries.clone();
    let chart_begin = chart.clone();
    let drag_begin_state = drag_state.clone();
    let selection_drag = selection.clone();
    gesture.connect_drag_begin(move |_gesture, start_x, start_y| {
        *drag_begin_state.borrow_mut() = None;
        let Some(pairs) = scratchpad_pairs_parsed(&entries_begin) else {
            return;
        };
        let w = chart_begin.width().max(1);
        let h = chart_begin.height().max(1);
        let Some(idx) = nearest_temp_pwm_point_index(start_x, start_y, &pairs, w, h) else {
            return;
        };
        if let Some(&(t, p)) = pairs.get(idx) {
            *selection_drag.borrow_mut() = Some(idx);
            *drag_begin_state.borrow_mut() = Some((idx, t, p.min(255)));
            chart_begin.grab_focus();
        }
    });

    let entries_update = entries.clone();
    let chart_update = chart.clone();
    let drag_update_state = drag_state.clone();
    gesture.connect_drag_update(move |_gesture, offset_x, offset_y| {
        let Some((idx, start_temp, start_pwm)) = *drag_update_state.borrow() else {
            return;
        };
        let new_temp = ((start_temp as f64 + offset_x * SCRATCHPAD_DRAG_TEMP_PER_PX).round() as i64)
            .clamp(1, 900_000) as u32;
        let new_pwm = ((start_pwm as f64 + offset_y * SCRATCHPAD_DRAG_PWM_PER_PX).round() as i64)
            .clamp(0, 255) as u32;
        let borrowed = entries_update.borrow();
        if let Some((_, temp_entry, pwm_entry)) = borrowed.get(idx) {
            temp_entry.set_text(&new_temp.to_string());
            pwm_entry.set_text(&new_pwm.to_string());
        }
        chart_update.queue_draw();
    });

    let drag_end_state = drag_state.clone();
    let chart_end = chart.clone();
    gesture.connect_drag_end(move |_gesture, _offset_x, _offset_y| {
        *drag_end_state.borrow_mut() = None;
        chart_end.queue_draw();
    });

    chart.add_controller(gesture);

    let key = gtk4::EventControllerKey::new();
    let entries_key = entries.clone();
    let chart_key = chart.clone();
    let selection_key = selection.clone();
    key.connect_key_pressed(move |_ctrl, key, _code, state| {
        use gtk4::gdk::{Key, ModifierType};
        use gtk4::glib::Propagation;

        let Some(idx) = *selection_key.borrow() else {
            return Propagation::Proceed;
        };
        if entries_key.borrow().len() <= idx {
            *selection_key.borrow_mut() = None;
            return Propagation::Proceed;
        }
        let shift = state.contains(ModifierType::SHIFT_MASK);
        let (dt, dp) = match key {
            Key::Right => (
                if shift {
                    SCRATCHPAD_KEY_TEMP_COARSE
                } else {
                    SCRATCHPAD_KEY_TEMP_FINE
                },
                0,
            ),
            Key::Left => (
                if shift {
                    -SCRATCHPAD_KEY_TEMP_COARSE
                } else {
                    -SCRATCHPAD_KEY_TEMP_FINE
                },
                0,
            ),
            Key::Up => (
                0,
                if shift {
                    SCRATCHPAD_KEY_PWM_COARSE
                } else {
                    SCRATCHPAD_KEY_PWM_FINE
                },
            ),
            Key::Down => (
                0,
                if shift {
                    -SCRATCHPAD_KEY_PWM_COARSE
                } else {
                    -SCRATCHPAD_KEY_PWM_FINE
                },
            ),
            _ => return Propagation::Proceed,
        };
        if apply_scratchpad_nudge(&entries_key, idx, dt, dp) {
            chart_key.queue_draw();
            Propagation::Stop
        } else {
            Propagation::Proceed
        }
    });
    chart.add_controller(key);
}

fn append_fan_curve_readonly_preview(
    page: &gtk4::Box,
    bundle: &DiagnosticsBundle,
    fan_snapshot: &Result<Option<FanCurveSnapshot>>,
) {
    let Some(curve) = bundle.raw_probe_report.fan_curves.first() else {
        return;
    };
    let Ok(Some(snapshot)) = fan_snapshot else {
        return;
    };
    let pairs = fan_curve_snapshot_chart_pairs(curve, snapshot);
    if pairs.is_empty() {
        return;
    }

    let group = adw::PreferencesGroup::new();
    group.set_title("Curve shape (read-only preview)");
    group.set_description(Some(
        "v0.2 groundwork: read-only PWM bars and a temperature→PWM chart from the saved last-known-good snapshot (interactive editing still to come).",
    ));

    let caption = gtk4::Label::new(Some(
        "Temperature vs PWM (saved last-known-good, read-only chart)",
    ));
    caption.add_css_class("dim-label");
    caption.set_halign(gtk4::Align::Start);
    caption.set_margin_start(12);
    caption.set_margin_end(12);
    caption.set_margin_top(4);
    group.add(&caption);

    for (pair_index, (temp, pwm)) in pairs.iter().enumerate() {
        let pwm_clamped = (*pwm).min(255);
        let pwm_fraction = pwm_clamped as f64 / 255.0;
        let row = adw::ActionRow::builder()
            .title(format!("Auto point {}", pair_index + 1))
            .subtitle(format!(
                "Raw sysfs temp {temp} (often millidegree), PWM {pwm} / 255"
            ))
            .selectable(false)
            .build();
        let bar = gtk4::ProgressBar::builder()
            .fraction(pwm_fraction)
            .show_text(true)
            .text(format!("PWM {pwm}"))
            .width_request(140)
            .build();
        row.add_suffix(&bar);
        group.add(&row);
    }

    let pairs_chart = Rc::new(pairs);
    let pairs_for_draw = pairs_chart.clone();
    let chart = gtk4::DrawingArea::builder()
        .content_width(400)
        .content_height(220)
        .margin_top(10)
        .margin_bottom(6)
        .build();
    chart.set_draw_func(move |_area, cr, w, h| {
        draw_fan_curve_temp_pwm_chart(cr, w, h, pairs_for_draw.as_ref());
    });
    group.add(&chart);

    page.append(&group);
}

fn append_fan_preset_per_profile_section(page: &gtk4::Box, bundle: &DiagnosticsBundle) {
    let Some(platform_cap) = bundle.raw_probe_report.platform_profile.as_ref() else {
        return;
    };
    if bundle.raw_probe_report.fan_curves.is_empty() || platform_cap.choices.is_empty() {
        return;
    }

    let group = adw::PreferencesGroup::new();
    group.set_title("Fan preset per platform profile");
    group.set_description(Some(
        "Daemon app state only: preferred packaged fan preset per detected platform profile. Save writes durable state. Resume re-apply uses systemd-logind and currently performs probe refresh plus dry-run fan preset planning (no sysfs writes).",
    ));

    let map_state: BTreeMap<String, String> = bundle.fan_preset_by_platform_profile.clone();
    let map_status = gtk4::Label::new(None);
    map_status.set_wrap(true);
    map_status.set_xalign(0.0);
    map_status.set_margin_top(6);

    let mut dropdown_labels: Vec<&str> = vec!["(none)"];
    dropdown_labels.extend_from_slice(FAN_PRESET_IDS);

    for profile in &platform_cap.choices {
        let row = adw::ActionRow::builder()
            .title(profile.as_str())
            .subtitle("Packaged fan preset for this platform profile")
            .selectable(false)
            .build();

        let model = gtk4::StringList::new(&dropdown_labels);
        let dropdown = gtk4::DropDown::builder().model(&model).build();
        dropdown.set_hexpand(true);
        let selected = if let Some(saved_id) = map_state.get(profile) {
            FAN_PRESET_IDS
                .iter()
                .position(|id| *id == saved_id.as_str())
                .map(|idx| (idx + 1) as u32)
                .unwrap_or(0)
        } else {
            0
        };
        dropdown.set_selected(selected);

        let save = gtk4::Button::with_label("Save");
        row.add_suffix(&dropdown);
        row.add_suffix(&save);

        let profile_for_save = profile.clone();
        let status_for_save = map_status.clone();
        save.connect_clicked(move |_| {
            let sel = dropdown.selected() as usize;
            let result = if sel == 0 {
                LegionControlClient::system()
                    .and_then(|client| client.remove_fan_preset_profile_map_entry(&profile_for_save))
            } else if let Some(preset_id) = FAN_PRESET_IDS.get(sel.saturating_sub(1)) {
                LegionControlClient::system().and_then(|client| {
                    client.set_fan_preset_profile_map_entry(&profile_for_save, preset_id)
                })
            } else {
                status_for_save.set_text("Invalid preset selection.");
                return;
            };
            match result {
                Ok(_) => {
                    if sel == 0 {
                        status_for_save.set_text(&format!(
                            "Removed mapping for profile `{profile_for_save}`."
                        ));
                    } else {
                        let preset_id = FAN_PRESET_IDS[sel - 1];
                        status_for_save.set_text(&format!(
                            "Saved `{profile_for_save}` → `{preset_id}` (dashboard refresh requested)."
                        ));
                    }
                    let _ = request_dashboard_refresh();
                }
                Err(error) => {
                    status_for_save.set_text(&format!("Update failed: {error}"));
                }
            }
        });

        group.add(&row);
    }

    let resume_row = adw::ActionRow::builder()
        .title("Re-apply mapped fan preset after resume")
        .subtitle("Listens for systemd resume; logs a dry-run plan for the preset mapped to the active platform profile. Fan curve writes stay disabled in RatVantage.")
        .selectable(false)
        .build();
    let resume_switch = gtk4::Switch::builder()
        .valign(gtk4::Align::Center)
        .active(bundle.fan_preset_reapply_after_resume)
        .build();
    resume_row.add_suffix(&resume_switch);
    let skip_resume_notify = Rc::new(Cell::new(true));
    let skip_resume_for_connect = skip_resume_notify.clone();
    let map_status_for_resume = map_status.clone();
    resume_switch.connect_active_notify(move |switch| {
        if skip_resume_for_connect.get() {
            skip_resume_for_connect.set(false);
            return;
        }
        let on = switch.is_active();
        match LegionControlClient::system()
            .and_then(|client| client.set_fan_preset_reapply_after_resume(on))
        {
            Ok(confirmed) => {
                switch.set_active(confirmed);
                map_status_for_resume
                    .set_text("Resume fan re-apply policy updated (dashboard refresh requested).");
                let _ = request_dashboard_refresh();
            }
            Err(error) => {
                switch.set_active(!on);
                map_status_for_resume.set_text(&format!("Resume policy update failed: {error}"));
            }
        }
    });
    group.add(&resume_row);

    let clear_all = gtk4::Button::with_label("Clear all profile mappings");
    let bulk_row = adw::ActionRow::builder()
        .title("All platform profiles")
        .subtitle("Remove every stored profile→preset pair from daemon state.")
        .selectable(false)
        .build();
    bulk_row.add_suffix(&clear_all);
    group.add(&bulk_row);

    let status_for_clear = map_status.clone();
    clear_all.connect_clicked(move |_| {
        match LegionControlClient::system().and_then(|client| client.clear_fan_preset_profile_map())
        {
            Ok(_) => {
                status_for_clear.set_text("Cleared every profile→preset mapping.");
                let _ = request_dashboard_refresh();
            }
            Err(error) => {
                status_for_clear.set_text(&format!("Clear failed: {error}"));
            }
        }
    });

    page.append(&group);
    page.append(&map_status);
}

fn repopulate_manual_fan_scratchpad_rows(
    column: &gtk4::Box,
    pairs: &[FanCurveHwmonPointPair],
    snapshot: Option<&FanCurveSnapshot>,
    entries: &ManualFanScratchRows,
    scratchpad_chart: Option<&gtk4::DrawingArea>,
    scratchpad_selection: Option<&ScratchpadChartSelection>,
) {
    clear_points_column(column);
    entries.borrow_mut().clear();
    if let Some(sel) = scratchpad_selection {
        *sel.borrow_mut() = None;
    }
    let lookup: HashMap<String, String> = snapshot
        .map(|snap| {
            snap.points
                .iter()
                .map(|point| (point.path.clone(), point.value.clone()))
                .collect()
        })
        .unwrap_or_default();

    for pair in pairs {
        let temp_entry = gtk4::Entry::builder()
            .hexpand(true)
            .placeholder_text("temp channel (e.g. millidegree)")
            .build();
        let pwm_entry = gtk4::Entry::builder()
            .hexpand(true)
            .placeholder_text("pwm (0–255)")
            .build();
        if let Some(value) = lookup.get(&pair.temp_path) {
            temp_entry.set_text(value);
        }
        if let Some(value) = lookup.get(&pair.pwm_path) {
            pwm_entry.set_text(value);
        }

        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        row.append(&gtk4::Label::new(Some(&format!("Point {}", pair.index))));
        row.append(&temp_entry);
        row.append(&pwm_entry);
        column.append(&row);
        entries
            .borrow_mut()
            .push((pair.clone(), temp_entry.clone(), pwm_entry.clone()));
    }
    if let Some(chart) = scratchpad_chart {
        connect_scratchpad_chart_entry_signals(entries, chart, scratchpad_selection);
        chart.queue_draw();
    }
}

fn append_manual_fan_curve_scratchpad(page: &gtk4::Box, curve: &FanCurveCapability) {
    let pairs = fan_curve_hwmon_point_pairs(curve);
    if pairs.is_empty() {
        return;
    }
    let pairs_for_ui = pairs.clone();

    let section_title = gtk4::Label::new(Some("Manual curve scratchpad"));
    section_title.add_css_class("title-3");
    section_title.set_xalign(0.0);
    page.append(&section_title);

    let hint = gtk4::Label::new(Some(
        "Edit raw sysfs integers per paired pwm*_auto_pointN node. Validate checks monotonic temp/pwm rules only — no daemon write or apply.",
    ));
    hint.set_wrap(true);
    hint.set_xalign(0.0);
    page.append(&hint);

    let actions_column = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    let actions_row_load = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    actions_row_load.set_spacing(8);
    let actions_row_export = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    actions_row_export.set_spacing(8);
    let load_live = gtk4::Button::with_label("Load from live");
    let load_saved = gtk4::Button::with_label("Load from saved");
    let clear_btn = gtk4::Button::with_label("Clear");
    let validate_btn = gtk4::Button::with_label("Validate pairs");
    let copy_json = gtk4::Button::with_label("Copy JSON");
    let copy_toml = gtk4::Button::with_label("Copy scratchpad TOML");
    let preview_sysfs = gtk4::Button::with_label("Preview sysfs targets");
    let copy_sysfs_preview = gtk4::Button::with_label("Copy sysfs preview");
    load_live.set_tooltip_text(Some(
        "Reload scratchpad rows from live sysfs readings via the daemon (read-only D-Bus).",
    ));
    load_saved.set_tooltip_text(Some(
        "Reload scratchpad rows from the saved last-known-good snapshot (read-only D-Bus).",
    ));
    clear_btn.set_tooltip_text(Some(
        "Clear all scratchpad cells to empty (local only; does not change hardware).",
    ));
    validate_btn.set_tooltip_text(Some(
        "Check monotonic temp and pwm rules on the current row integers (local; no D-Bus).",
    ));
    preview_sysfs.set_tooltip_text(Some(
        "Build the sysfs path listing in the preview pane from parsed rows (local; no D-Bus).",
    ));
    copy_sysfs_preview.set_tooltip_text(Some(
        "Copy the preview pane text to the clipboard (placeholder text is not copied).",
    ));
    copy_json.set_tooltip_text(Some(
        "Copy scratchpad rows as JSON to the clipboard (paths plus raw entry text).",
    ));
    copy_toml.set_tooltip_text(Some(
        "Copy rows as ratvantage_fan_scratchpad_v1 TOML when every cell holds a valid integer.",
    ));
    actions_row_load.append(&load_live);
    actions_row_load.append(&load_saved);
    actions_row_load.append(&clear_btn);
    actions_row_load.append(&validate_btn);
    actions_row_export.append(&preview_sysfs);
    actions_row_export.append(&copy_sysfs_preview);
    actions_row_export.append(&copy_json);
    actions_row_export.append(&copy_toml);
    actions_column.append(&actions_row_load);
    actions_column.append(&actions_row_export);
    page.append(&actions_column);

    let status = gtk4::Label::new(Some(
        "Use Load from live or saved after refreshing those sections, or type values manually.",
    ));
    status.set_wrap(true);
    status.set_xalign(0.0);
    status.set_selectable(true);
    page.append(&status);

    let rows_heading = gtk4::Label::new(Some("Editable pwm/temp pairs"));
    rows_heading.add_css_class("title-4");
    rows_heading.set_xalign(0.0);
    rows_heading.set_margin_top(8);
    page.append(&rows_heading);

    let chart_interaction_hint = gtk4::Label::new(Some(
        "Click or drag a point on the chart to edit temp/PWM in the rows, or select a point and use arrow keys (Shift = larger temp/PWM steps). Focusing a row's temp or pwm field syncs the chart highlight. Scratchpad only; not applied to hardware. Axes: raw sysfs temperature (min–max of your points) horizontally, PWM 0–255 vertically.",
    ));
    chart_interaction_hint.add_css_class("dim-label");
    chart_interaction_hint.set_wrap(true);
    chart_interaction_hint.set_xalign(0.0);
    page.append(&chart_interaction_hint);

    let entries: ManualFanScratchRows = Rc::new(RefCell::new(Vec::new()));
    let scratchpad_selection: ScratchpadChartSelection = Rc::new(RefCell::new(None));
    let entries_for_scratchpad_chart = entries.clone();
    let selection_for_scratchpad_chart = scratchpad_selection.clone();
    let scratchpad_chart = gtk4::DrawingArea::builder()
        .content_width(400)
        .content_height(220)
        .margin_top(6)
        .margin_bottom(8)
        .hexpand(true)
        .build();
    scratchpad_chart.set_draw_func(move |_area, cr, w, h| {
        let highlight = *selection_for_scratchpad_chart.borrow();
        if let Some(pairs) = scratchpad_pairs_parsed(&entries_for_scratchpad_chart) {
            draw_temp_pwm_polyline_chart(cr, w, h, &pairs, highlight);
        } else {
            let msg = scratchpad_first_parse_blocker(&entries_for_scratchpad_chart);
            draw_scratchpad_chart_placeholder(cr, w, h, &msg);
        }
    });
    attach_scratchpad_chart_interactions(&scratchpad_chart, &entries, &scratchpad_selection);
    page.append(&scratchpad_chart);

    let rows_column = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    rows_column.set_margin_start(4);
    page.append(&rows_column);

    let sysfs_preview_title = gtk4::Label::new(Some("Sysfs target preview (scratchpad)"));
    sysfs_preview_title.add_css_class("title-4");
    sysfs_preview_title.set_xalign(0.0);
    sysfs_preview_title.set_margin_top(10);
    page.append(&sysfs_preview_title);

    let sysfs_preview_hint = gtk4::Label::new(Some(
        "Requires every row to parse as integers and to pass the same monotonic rules as Validate pairs. Purely local text — no D-Bus call. Copy sysfs preview copies whatever is currently shown in the pane.",
    ));
    sysfs_preview_hint.add_css_class("dim-label");
    sysfs_preview_hint.set_wrap(true);
    sysfs_preview_hint.set_xalign(0.0);
    page.append(&sysfs_preview_hint);

    let sysfs_preview_view = gtk4::TextView::new();
    sysfs_preview_view.set_wrap_mode(gtk4::WrapMode::WordChar);
    sysfs_preview_view.set_monospace(true);
    sysfs_preview_view.set_editable(false);
    sysfs_preview_view.set_cursor_visible(false);
    sysfs_preview_view.set_top_margin(4);
    sysfs_preview_view.set_bottom_margin(4);
    sysfs_preview_view.set_left_margin(6);
    sysfs_preview_view.set_right_margin(6);
    sysfs_preview_view.buffer().set_text(
        "Click Preview sysfs targets after filling rows (or to see parse/validation errors here).",
    );
    let sysfs_preview_scroll = gtk4::ScrolledWindow::builder()
        .min_content_height(100)
        .vexpand(false)
        .child(&sysfs_preview_view)
        .build();
    page.append(&sysfs_preview_scroll);

    let toml_title = gtk4::Label::new(Some("TOML exchange"));
    toml_title.add_css_class("title-4");
    toml_title.set_xalign(0.0);
    toml_title.set_margin_top(12);
    page.append(&toml_title);

    let toml_hint = gtk4::Label::new(Some(
        "Paste a ratvantage_fan_scratchpad_v1 document or a packaged data/presets/*.toml fan preset. Import fills the rows above; it does not call the daemon.",
    ));
    toml_hint.set_wrap(true);
    toml_hint.set_xalign(0.0);
    page.append(&toml_hint);

    let toml_editor = gtk4::TextView::new();
    toml_editor.set_wrap_mode(gtk4::WrapMode::WordChar);
    toml_editor.set_monospace(true);
    toml_editor.set_top_margin(4);
    toml_editor.set_bottom_margin(4);
    toml_editor.set_left_margin(6);
    toml_editor.set_right_margin(6);
    toml_editor.buffer().set_text(
        "# Paste TOML here, then click Import.\n# Use Copy scratchpad TOML to export the current rows.",
    );

    let toml_scroll = gtk4::ScrolledWindow::builder()
        .min_content_height(120)
        .vexpand(false)
        .child(&toml_editor)
        .build();
    page.append(&toml_scroll);

    let toml_actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let import_toml = gtk4::Button::with_label("Import TOML from editor");
    toml_actions.append(&import_toml);
    page.append(&toml_actions);

    repopulate_manual_fan_scratchpad_rows(
        &rows_column,
        &pairs_for_ui,
        None,
        &entries,
        Some(&scratchpad_chart),
        Some(&scratchpad_selection),
    );

    let pairs_for_reload = pairs_for_ui.clone();
    let rows_for_reload = rows_column.clone();
    let entries_for_reload = entries.clone();
    let chart_for_live = scratchpad_chart.clone();
    let sel_for_live = scratchpad_selection.clone();
    let status_for_live = status.clone();
    load_live.connect_clicked(move |_| {
        match LegionControlClient::system().and_then(|client| client.live_fan_curve_readings()) {
            Ok(snapshot) => {
                repopulate_manual_fan_scratchpad_rows(
                    &rows_for_reload,
                    &pairs_for_reload,
                    Some(&snapshot),
                    &entries_for_reload,
                    Some(&chart_for_live),
                    Some(&sel_for_live),
                );
                status_for_live.set_text("Loaded values from live sysfs readings.");
            }
            Err(error) => {
                status_for_live.set_text(&format!("Live load failed: {error}"));
            }
        }
    });

    let pairs_for_saved = pairs_for_ui.clone();
    let rows_for_saved = rows_column.clone();
    let entries_for_saved = entries.clone();
    let chart_for_saved = scratchpad_chart.clone();
    let sel_for_saved = scratchpad_selection.clone();
    let status_for_saved = status.clone();
    load_saved.connect_clicked(move |_| {
        match LegionControlClient::system().and_then(|client| client.last_known_good_fan_curve()) {
            Ok(Some(snapshot)) => {
                repopulate_manual_fan_scratchpad_rows(
                    &rows_for_saved,
                    &pairs_for_saved,
                    Some(&snapshot),
                    &entries_for_saved,
                    Some(&chart_for_saved),
                    Some(&sel_for_saved),
                );
                status_for_saved.set_text("Loaded values from saved last-known-good snapshot.");
            }
            Ok(None) => {
                status_for_saved.set_text("No saved last-known-good snapshot yet.");
            }
            Err(error) => {
                status_for_saved.set_text(&format!("Saved snapshot load failed: {error}"));
            }
        }
    });

    let pairs_for_clear = pairs_for_ui.clone();
    let rows_for_clear = rows_column.clone();
    let entries_for_clear = entries.clone();
    let chart_for_clear = scratchpad_chart.clone();
    let sel_for_clear = scratchpad_selection.clone();
    let status_for_clear = status.clone();
    clear_btn.connect_clicked(move |_| {
        repopulate_manual_fan_scratchpad_rows(
            &rows_for_clear,
            &pairs_for_clear,
            None,
            &entries_for_clear,
            Some(&chart_for_clear),
            Some(&sel_for_clear),
        );
        status_for_clear.set_text("Cleared scratchpad entries.");
    });

    let entries_for_validate = entries.clone();
    let status_for_validate = status.clone();
    validate_btn.connect_clicked(move |_| {
        let mut parsed = Vec::new();
        for (_, temp_entry, pwm_entry) in entries_for_validate.borrow().iter() {
            let temp_text = temp_entry.text();
            let pwm_text = pwm_entry.text();
            let temp_trim = temp_text.trim();
            let pwm_trim = pwm_text.trim();
            if temp_trim.is_empty() || pwm_trim.is_empty() {
                status_for_validate.set_text("Fill every temp and pwm cell before validating.");
                return;
            }
            let temp_value = match temp_trim.parse::<u32>() {
                Ok(value) => value,
                Err(_) => {
                    status_for_validate
                        .set_text(&format!("Temp value `{temp_trim}` is not a valid integer."));
                    return;
                }
            };
            let pwm_value = match pwm_trim.parse::<u32>() {
                Ok(value) => value,
                Err(_) => {
                    status_for_validate
                        .set_text(&format!("Pwm value `{pwm_trim}` is not a valid integer."));
                    return;
                }
            };
            parsed.push((temp_value, pwm_value));
        }

        match validate_manual_fan_curve_pairs(&parsed) {
            Ok(()) => {
                status_for_validate.set_text(&format!(
                    "OK: {} pair(s) pass sysfs monotonic + pwm range checks (read-only).",
                    parsed.len()
                ));
            }
            Err(error) => {
                status_for_validate.set_text(&format!("Validation failed: {error:?}"));
            }
        }
    });

    let pairs_for_sysfs_preview = pairs_for_ui.clone();
    let entries_for_sysfs_preview = entries.clone();
    let status_for_sysfs_preview = status.clone();
    let sysfs_preview_buffer = sysfs_preview_view.buffer();
    preview_sysfs.connect_clicked(move |_| {
        let Some(parsed) = scratchpad_pairs_parsed(&entries_for_sysfs_preview) else {
            let blocker = scratchpad_first_parse_blocker(&entries_for_sysfs_preview);
            sysfs_preview_buffer.set_text(&blocker);
            status_for_sysfs_preview.set_text(
                "Sysfs preview: rows are not ready to plot — see the preview pane for the first issue.",
            );
            return;
        };
        match format_manual_fan_scratchpad_sysfs_preview(&pairs_for_sysfs_preview, &parsed) {
            Ok(text) => {
                sysfs_preview_buffer.set_text(&text);
                status_for_sysfs_preview.set_text(&format!(
                    "Sysfs preview: listed {} hwmon path pair(s) in the pane (read-only; not sent to the daemon).",
                    parsed.len()
                ));
            }
            Err(err) => {
                sysfs_preview_buffer.set_text(&format!("{err:?}"));
                status_for_sysfs_preview.set_text(
                    "Sysfs preview: monotonic or layout checks failed — see the preview pane.",
                );
            }
        }
    });

    let sysfs_preview_buffer_for_clip = sysfs_preview_view.buffer();
    let status_for_copy_sysfs_preview = status.clone();
    copy_sysfs_preview.connect_clicked(move |_| {
        let start = sysfs_preview_buffer_for_clip.start_iter();
        let end = sysfs_preview_buffer_for_clip.end_iter();
        let text = sysfs_preview_buffer_for_clip.text(&start, &end, false);
        let trimmed = text.trim();
        if trimmed.is_empty() || trimmed.starts_with("Click Preview sysfs targets") {
            status_for_copy_sysfs_preview
                .set_text("Nothing to copy — click Preview sysfs targets to fill the pane first.");
            return;
        }
        if let Some(display) = gtk4::gdk::Display::default() {
            display.clipboard().set_text(trimmed);
        }
        status_for_copy_sysfs_preview.set_text(
            "Copied sysfs preview text from the pane to the clipboard (read-only snippet).",
        );
    });

    let entries_for_copy = entries.clone();
    let status_for_copy = status.clone();
    copy_json.connect_clicked(move |_| {
        let payload: Vec<serde_json::Value> = entries_for_copy
            .borrow()
            .iter()
            .map(|(pair, temp_entry, pwm_entry)| {
                serde_json::json!({
                    "index": pair.index,
                    "temp_path": &pair.temp_path,
                    "temp": temp_entry.text().as_str(),
                    "pwm_path": &pair.pwm_path,
                    "pwm": pwm_entry.text().as_str(),
                })
            })
            .collect();
        let text = match serde_json::to_string_pretty(&payload) {
            Ok(rendered) => rendered,
            Err(error) => {
                status_for_copy.set_text(&format!("JSON encode failed: {error}"));
                return;
            }
        };
        if let Some(display) = gtk4::gdk::Display::default() {
            display.clipboard().set_text(&text);
        }
        status_for_copy.set_text("Copied scratchpad JSON to the clipboard.");
    });

    let entries_for_toml_copy = entries.clone();
    let status_for_toml_copy = status.clone();
    copy_toml.connect_clicked(move |_| {
        let mut rows = Vec::new();
        for (pair, temp_entry, pwm_entry) in entries_for_toml_copy.borrow().iter() {
            let temp_trim = temp_entry.text().trim().to_string();
            let pwm_trim = pwm_entry.text().trim().to_string();
            if temp_trim.is_empty() || pwm_trim.is_empty() {
                status_for_toml_copy
                    .set_text("Fill every temp and pwm cell with integers before copying TOML.");
                return;
            }
            let temp_raw = match temp_trim.parse::<u32>() {
                Ok(value) => value,
                Err(_) => {
                    status_for_toml_copy
                        .set_text(&format!("Temp `{temp_trim}` is not a valid integer."));
                    return;
                }
            };
            let pwm_raw = match pwm_trim.parse::<u32>() {
                Ok(value) => value,
                Err(_) => {
                    status_for_toml_copy
                        .set_text(&format!("Pwm `{pwm_trim}` is not a valid integer."));
                    return;
                }
            };
            rows.push((pair.clone(), temp_raw, pwm_raw));
        }

        match encode_fan_scratchpad_toml_v1(&rows) {
            Ok(rendered) => {
                if let Some(display) = gtk4::gdk::Display::default() {
                    display.clipboard().set_text(&rendered);
                }
                status_for_toml_copy.set_text("Copied scratchpad TOML to the clipboard.");
            }
            Err(error) => {
                status_for_toml_copy.set_text(&format!("TOML encode failed: {error}"));
            }
        }
    });

    let pairs_for_toml_import = pairs_for_ui.clone();
    let entries_for_toml_import = entries.clone();
    let chart_for_toml_import = scratchpad_chart.clone();
    let toml_buffer_for_import = toml_editor.buffer();
    let status_for_toml_import = status.clone();
    import_toml.connect_clicked(move |_| {
        let text = toml_buffer_for_import
            .text(
                &toml_buffer_for_import.start_iter(),
                &toml_buffer_for_import.end_iter(),
                false,
            )
            .trim()
            .to_string();
        if text.is_empty() {
            status_for_toml_import.set_text("Editor is empty; paste TOML first.");
            return;
        }

        if let Ok(doc) = decode_fan_scratchpad_toml_v1(&text) {
            if doc.schema_version != 1 {
                status_for_toml_import.set_text("Scratchpad TOML schema_version must be 1.");
                return;
            }
            if doc.pairs.is_empty() {
                status_for_toml_import.set_text("Scratchpad document contains no pairs.");
                return;
            }
            let mut file_pairs = doc.pairs.clone();
            file_pairs.sort_by_key(|pair| pair.index);
            if file_pairs.len() != pairs_for_toml_import.len() {
                status_for_toml_import.set_text(&format!(
                    "Scratchpad file has {} pair(s); this machine exposes {} — counts must match.",
                    file_pairs.len(),
                    pairs_for_toml_import.len()
                ));
                return;
            }
            for (index, hw_pair) in pairs_for_toml_import.iter().enumerate() {
                let file_pair = &file_pairs[index];
                if file_pair.index != hw_pair.index
                    || file_pair.temp_path != hw_pair.temp_path
                    || file_pair.pwm_path != hw_pair.pwm_path
                {
                    status_for_toml_import.set_text(
                        "TOML paths or point indices do not match this machine's curve layout.",
                    );
                    return;
                }
            }
            let borrowed = entries_for_toml_import.borrow_mut();
            if borrowed.len() != file_pairs.len() {
                status_for_toml_import.set_text("Internal scratchpad row count mismatch.");
                return;
            }
            for (index, file_pair) in file_pairs.iter().enumerate() {
                borrowed[index]
                    .1
                    .set_text(&file_pair.temp_raw.to_string());
                borrowed[index]
                    .2
                    .set_text(&file_pair.pwm_raw.to_string());
            }
            drop(borrowed);
            chart_for_toml_import.queue_draw();
            status_for_toml_import.set_text("Imported ratvantage_fan_scratchpad_v1 into the rows.");
            return;
        }

        if let Ok(preset) = parse_fan_preset_toml(&text) {
            if let Err(error) = validate_fan_preset_document(&preset) {
                status_for_toml_import.set_text(&format!("Preset TOML failed validation: {error:?}"));
                return;
            }
            let raw = match fan_preset_points_as_sysfs_raw(&preset) {
                Ok(values) => values,
                Err(error) => {
                    status_for_toml_import.set_text(&format!("Preset mapping failed: {error:?}"));
                    return;
                }
            };
            if raw.len() != pairs_for_toml_import.len() {
                status_for_toml_import.set_text(&format!(
                    "Preset has {} points; this machine exposes {} paired hwmon nodes — counts must match.",
                    raw.len(),
                    pairs_for_toml_import.len()
                ));
                return;
            }
            let borrowed = entries_for_toml_import.borrow_mut();
            if borrowed.len() != raw.len() {
                status_for_toml_import.set_text("Internal scratchpad row count mismatch.");
                return;
            }
            for (index, (temp_raw, pwm_raw)) in raw.iter().enumerate() {
                borrowed[index]
                    .1
                    .set_text(&temp_raw.to_string());
                borrowed[index]
                    .2
                    .set_text(&pwm_raw.to_string());
            }
            drop(borrowed);
            chart_for_toml_import.queue_draw();
            status_for_toml_import.set_text(&format!(
                "Imported packaged preset `{}` (deg C → millidegree pwm mapping).",
                preset.id
            ));
            return;
        }

        status_for_toml_import.set_text(
            "Could not parse as scratchpad v1 or as a fan preset TOML — check syntax and tables.",
        );
    });
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
        Ok(opt) => format_gpu_mode_pending_summary(opt.as_ref()),
        Err(error) => format!("state unavailable - {error}"),
    }
}

fn format_fan_snapshot_display(snapshot: &FanCurveSnapshot) -> String {
    let path = snapshot.path.as_deref().unwrap_or("unknown");
    format!("{path} - {} captured values", snapshot.points.len())
}

fn render_fan_snapshot_row(snapshot: &Result<Option<FanCurveSnapshot>>) -> String {
    match snapshot {
        Ok(Some(snapshot)) => format_fan_snapshot_display(snapshot),
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
    lkg_points_column: gtk4::Box,
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
    let lkg_points_for_capture = lkg_points_column.clone();
    capture.connect_clicked(move |_| {
        match LegionControlClient::system()
            .and_then(|client| client.capture_last_known_good_fan_curve())
        {
            Ok(snapshot) => {
                last_known_for_capture.set_subtitle(&format_fan_snapshot_display(&snapshot));
                repopulate_saved_lkg_points_column(
                    &lkg_points_for_capture,
                    &Ok(Some(snapshot.clone())),
                );
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
