use crate::{
    runtime_refresh_notice, DiagnosticsBundle, LegionControlClient, RuntimeSnapshot, UiStatus,
};
use adw::prelude::*;
use anyhow::{anyhow, Result};
use legion_common::{FanCurveSnapshot, GpuModePending};
use std::thread_local;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use zbus::blocking::{Connection as ZbusConnection, MessageIterator};
use zbus::MatchRule;

const AUTO_REFRESH_INTERVAL: Duration = Duration::from_secs(30);
const RESUME_REFRESH_GAP: Duration = Duration::from_secs(90);
const DEFAULT_DASHBOARD_PAGE: &str = "status";
const POWER_PROFILES_PATH: &str = "/org/freedesktop/UPower/PowerProfiles";

type SnapshotLoader = Rc<dyn Fn() -> Result<RuntimeSnapshot>>;

thread_local! {
    static DASHBOARD_REFRESH_HOOK: RefCell<Option<Rc<dyn Fn()>>> = RefCell::new(None);
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

    if let Some(ref addr) = bus_address {
        crate::ui::shared::install_bus_address(addr);
    }

    let app = adw::Application::builder()
        .application_id("org.ratvantage.LegionControl")
        .build();

    let bus_address = Rc::new(bus_address);
    let initial_page = Rc::new(normalize_dashboard_page_name(initial_page.as_deref()));
    app.connect_activate(move |app| {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("RatVantage")
            .default_width(960)
            .default_height(680)
            .build();

        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.set_hexpand(true);
        root.set_vexpand(true);
        let header = adw::HeaderBar::new();
        let title = adw::WindowTitle::new("RatVantage", "Legion hardware control");
        header.set_title_widget(Some(&title));
        root.append(&header);
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
        crate::ui::shared::install_dashboard_refresh_hook({
            let runtime = Rc::clone(&runtime);
            Rc::new(move || runtime.borrow_mut().refresh_now())
        });
        install_runtime_refresh(&window, Rc::clone(&runtime));
        install_power_profiles_refresh(Rc::clone(&runtime));
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
    initial_page: Rc<RefCell<String>>,
) -> gtk4::Widget {
    let stack = adw::ViewStack::new();
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    {
        let p = stack.add_titled(
            &crate::ui::status::status_page(
                status,
                crate::ui::shared::clone_result(&diagnostics),
                crate::ui::shared::clone_result(&gpu_pending),
            ),
            Some("status"),
            "Overview",
        );
        p.set_icon_name(Some("system-run-symbolic"));
    }
    {
        let p = stack.add_titled(
            &crate::ui::profiles::profiles_page(crate::ui::shared::clone_result(&diagnostics)),
            Some("profiles"),
            "Power",
        );
        p.set_icon_name(Some("power-profile-balanced-symbolic"));
    }
    {
        let p = stack.add_titled(
            &crate::ui::battery::battery_page(crate::ui::shared::clone_result(&diagnostics)),
            Some("battery"),
            "Battery",
        );
        p.set_icon_name(Some("battery-good-symbolic"));
    }
    {
        let p = stack.add_titled(
            &crate::ui::gpu::gpu_page(
                crate::ui::shared::clone_result(&diagnostics),
                crate::ui::shared::clone_result(&gpu_pending),
            ),
            Some("gpu"),
            "GPU",
        );
        p.set_icon_name(Some("video-display-symbolic"));
    }
    {
        let p = stack.add_titled(
            &crate::ui::fans::fans_page(
                crate::ui::shared::clone_result(&diagnostics),
                fan_snapshot,
            ),
            Some("fans"),
            "Fans",
        );
        p.set_icon_name(Some("temperature-symbolic"));
    }
    {
        let p = stack.add_titled(
            &crate::ui::appearance::appearance_page(crate::ui::shared::clone_result(&diagnostics)),
            Some("appearance"),
            "Devices",
        );
        p.set_icon_name(Some("input-keyboard-symbolic"));
    }
    {
        let p = stack.add_titled(
            &crate::ui::diagnostics::diagnostics_page(diagnostics),
            Some("diagnostics"),
            "Diagnostics",
        );
        p.set_icon_name(Some("utilities-system-monitor-symbolic"));
    }

    let current = initial_page.borrow().clone();
    stack.set_visible_child_name(&current);

    stack.connect_visible_child_notify(move |stack| {
        if let Some(name) = stack.visible_child_name() {
            *initial_page.borrow_mut() = name.to_string();
        }
    });

    let switcher = adw::ViewSwitcher::new();
    switcher.set_stack(Some(&stack));
    switcher.set_policy(adw::ViewSwitcherPolicy::Wide);
    switcher.set_hexpand(true);

    let switcher_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    switcher_box.set_margin_top(4);
    switcher_box.set_margin_bottom(4);
    switcher_box.append(&switcher);
    switcher_box.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    page.set_hexpand(true);
    page.set_vexpand(true);
    page.append(&switcher_box);
    page.append(&stack);
    page.upcast()
}

fn snapshot_page_with_active_sync(
    snapshot: Result<RuntimeSnapshot>,
    active_page: Rc<RefCell<String>>,
) -> gtk4::Widget {
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
    dashboard_page(status, diagnostics, gpu_pending, fan_snapshot, active_page)
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

fn install_power_profiles_refresh(runtime: Rc<RefCell<DashboardRuntime>>) {
    let (sender, receiver) = mpsc::channel::<()>();
    gtk4::glib::timeout_add_local(Duration::from_millis(250), move || {
        let mut should_refresh = false;
        while receiver.try_recv().is_ok() {
            should_refresh = true;
        }
        if should_refresh {
            runtime.borrow_mut().refresh_now();
        }
        gtk4::glib::ControlFlow::Continue
    });

    thread::spawn(move || {
        if let Err(error) = watch_power_profiles_changes(move || {
            let _ = sender.send(());
        }) {
            eprintln!("legion-control-ui: PowerProfiles signal watch unavailable: {error}");
        }
    });
}

fn watch_power_profiles_changes(mut notify: impl FnMut()) -> zbus::Result<()> {
    let connection = ZbusConnection::system()?;
    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .path(POWER_PROFILES_PATH)?
        .interface("org.freedesktop.DBus.Properties")?
        .member("PropertiesChanged")?
        .build();
    let mut messages = MessageIterator::for_match_rule(rule, &connection, Some(8))?;
    for message in &mut messages {
        message?;
        notify();
    }
    Ok(())
}

pub fn should_auto_refresh(now: Instant, last_refresh: Instant, last_tick: Instant) -> bool {
    now.duration_since(last_refresh) >= AUTO_REFRESH_INTERVAL
        || now.duration_since(last_tick) >= RESUME_REFRESH_GAP
}

struct DashboardRuntime {
    host: gtk4::Box,
    banner: adw::Banner,
    active_page: Rc<RefCell<String>>,
    loader: SnapshotLoader,
    last_snapshot: Option<RuntimeSnapshot>,
    degraded: bool,
    last_refresh: Instant,
    last_tick: Instant,
}

impl DashboardRuntime {
    fn new(root: &gtk4::Box, initial_page: &str, loader: SnapshotLoader) -> Rc<RefCell<Self>> {
        let banner = adw::Banner::new("");
        banner.set_revealed(false);
        root.append(&banner);

        let host = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        host.set_hexpand(true);
        host.set_vexpand(true);
        root.append(&host);

        Rc::new(RefCell::new(Self {
            host,
            banner,
            active_page: Rc::new(RefCell::new(initial_page.to_owned())),
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
                    self.banner.set_title(&notice.message);
                    self.banner.set_revealed(true);
                } else {
                    self.banner.set_revealed(false);
                }

                let active_page = self.active_page.clone();
                self.replace_page(snapshot_page_with_active_sync(Ok(snapshot), active_page));
            }
            Err(error) => {
                self.degraded = true;
                self.last_tick = Instant::now();
                let message = format!("Daemon connection lost — keeping last known state: {error}");
                if self.last_snapshot.is_none() {
                    let active_page = self.active_page.clone();
                    self.replace_page(snapshot_page_with_active_sync(
                        Err(anyhow!(error.to_string())),
                        active_page,
                    ));
                }
                self.banner.set_title(&message);
                self.banner.set_revealed(self.last_snapshot.is_some());
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

pub fn normalize_dashboard_page_name(page: Option<&str>) -> String {
    match page {
        Some("status" | "profiles" | "battery" | "gpu" | "fans" | "appearance" | "diagnostics") => {
            page.unwrap().to_owned()
        }
        _ => DEFAULT_DASHBOARD_PAGE.to_owned(),
    }
}
