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
        let provider = gtk4::CssProvider::new();
        provider.load_from_data(include_str!("style.css"));
        if let Some(display) = gtk4::gdk::Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }

        let default_width = std::env::var("RATVANTAGE_GTK_DEFAULT_WIDTH")
            .ok()
            .and_then(|value| value.parse::<i32>().ok())
            .unwrap_or(960);
        let default_height = std::env::var("RATVANTAGE_GTK_DEFAULT_HEIGHT")
            .ok()
            .and_then(|value| value.parse::<i32>().ok())
            .unwrap_or(680);

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("RatVantage")
            .default_width(default_width)
            .default_height(default_height)
            .build();

        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.set_hexpand(true);
        root.set_vexpand(true);
        let header = adw::HeaderBar::new();
        let title = adw::WindowTitle::new("RatVantage", "Hardware control");
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
    let hardware_product = status
        .as_ref()
        .ok()
        .map(|s| s.hardware.product_name.clone());

    let stack = adw::ViewStack::new();
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    {
        stack.add_titled(
            &crate::ui::status::status_page(
                status,
                crate::ui::shared::clone_result(&diagnostics),
                crate::ui::shared::clone_result(&gpu_pending),
            ),
            Some("status"),
            "Overview",
        );
    }
    {
        stack.add_titled(
            &crate::ui::profiles::profiles_page(crate::ui::shared::clone_result(&diagnostics)),
            Some("profiles"),
            "Power",
        );
    }
    {
        stack.add_titled(
            &crate::ui::battery::battery_page(crate::ui::shared::clone_result(&diagnostics)),
            Some("battery"),
            "Battery",
        );
    }
    {
        stack.add_titled(
            &crate::ui::gpu::gpu_page(
                crate::ui::shared::clone_result(&diagnostics),
                crate::ui::shared::clone_result(&gpu_pending),
            ),
            Some("gpu"),
            "GPU",
        );
    }
    {
        stack.add_titled(
            &crate::ui::fans::fans_page(
                crate::ui::shared::clone_result(&diagnostics),
                fan_snapshot,
            ),
            Some("fans"),
            "Fans",
        );
    }
    {
        stack.add_titled(
            &crate::ui::appearance::appearance_page(crate::ui::shared::clone_result(&diagnostics)),
            Some("appearance"),
            "Devices",
        );
    }
    {
        stack.add_titled(
            &crate::ui::automations::automations_page(crate::ui::shared::clone_result(
                &diagnostics,
            )),
            Some("automations"),
            "Automations",
        );
    }
    {
        stack.add_titled(
            &crate::ui::settings::settings_page(crate::ui::shared::clone_result(&diagnostics)),
            Some("settings"),
            "Settings",
        );
    }
    {
        stack.add_titled(
            &crate::ui::diagnostics::diagnostics_page(diagnostics),
            Some("diagnostics"),
            "Diagnostics",
        );
    }

    let current = initial_page.borrow().clone();
    stack.set_visible_child_name(&current);

    stack.connect_visible_child_notify(move |s| {
        if let Some(name) = s.visible_child_name() {
            *initial_page.borrow_mut() = name.to_string();
        }
    });

    let sidebar = build_sidebar(&stack, &current, hardware_product.as_deref());

    let body = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    body.set_hexpand(true);
    body.set_vexpand(true);
    body.append(&sidebar);
    body.append(&gtk4::Separator::new(gtk4::Orientation::Vertical));
    body.append(&stack);
    body.upcast()
}

fn build_sidebar(
    stack: &adw::ViewStack,
    active_id: &str,
    hardware_product: Option<&str>,
) -> gtk4::Box {
    let sidebar = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    sidebar.add_css_class("rv-sidebar");
    sidebar.set_size_request(196, -1);
    sidebar.set_vexpand(true);

    // Brand
    let brand = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    brand.add_css_class("rv-brand");

    let logo = rv_logo(28);
    brand.append(&logo);

    let brand_text = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    let brand_name = gtk4::Label::new(Some("RatVantage"));
    brand_name.set_xalign(0.0);
    brand_name.add_css_class("rv-brand-name");
    brand_text.append(&brand_name);
    let brand_sub = gtk4::Label::new(Some(hardware_product.unwrap_or("Supported hardware")));
    brand_sub.set_xalign(0.0);
    brand_sub.set_max_width_chars(18);
    brand_sub.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    brand_sub.add_css_class("rv-brand-sub");
    brand_text.append(&brand_sub);
    brand.append(&brand_text);
    sidebar.append(&brand);

    sidebar.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

    // Nav
    let nav_tabs: &[(&str, &str, &str)] = &[
        ("status", "◉", "Overview"),
        ("profiles", "◐", "Power"),
        ("battery", "▮", "Battery"),
        ("gpu", "▤", "GPU"),
        ("fans", "✺", "Fans"),
        ("appearance", "✦", "Devices"),
        ("automations", "⟳", "Automations"),
        ("settings", "⚙", "Settings"),
        ("diagnostics", "≣", "Diagnostics"),
    ];

    let nav_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    nav_box.set_margin_top(6);
    nav_box.set_margin_bottom(6);

    let buttons: Vec<gtk4::Button> = nav_tabs
        .iter()
        .enumerate()
        .map(|(index, (id, glyph, label))| {
            if index == 0 || index == 6 {
                let section = gtk4::Label::new(Some(if index == 0 { "CONTROL" } else { "TOOLS" }));
                section.set_xalign(0.0);
                section.add_css_class("rv-nav-section");
                nav_box.append(&section);
            }
            let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            row.set_hexpand(true);

            let glyph_label = gtk4::Label::new(Some(glyph));
            glyph_label.add_css_class("rv-nav-glyph");
            glyph_label.set_width_chars(2);
            glyph_label.set_xalign(0.5);
            row.append(&glyph_label);

            let name_label = gtk4::Label::new(Some(label));
            name_label.set_xalign(0.0);
            name_label.set_hexpand(true);
            row.append(&name_label);

            let btn = gtk4::Button::new();
            btn.set_child(Some(&row));
            btn.add_css_class("rv-nav-btn");
            btn.set_has_frame(false);
            btn.set_tooltip_text(Some(&format!("Open {label} page")));
            btn.update_property(&[gtk4::accessible::Property::Label(&format!(
                "Open {label} page"
            ))]);
            if *id == active_id {
                btn.add_css_class("active");
            }

            nav_box.append(&btn);
            btn
        })
        .collect();

    for (i, (id, _, _)) in nav_tabs.iter().enumerate() {
        let id = *id;
        let stack_clone = stack.clone();
        let all_buttons = buttons.clone();
        buttons[i].connect_clicked(move |clicked| {
            stack_clone.set_visible_child_name(id);
            for btn in &all_buttons {
                btn.remove_css_class("active");
            }
            clicked.add_css_class("active");
        });
    }

    sidebar.append(&nav_box);

    // Spacer
    let spacer = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    spacer.set_vexpand(true);
    sidebar.append(&spacer);

    // Footer
    let footer = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    footer.set_margin_start(12);
    footer.set_margin_end(12);
    footer.set_margin_bottom(12);

    for text in &["Hardware changes use system approval", "Read-back verified"] {
        let lbl = gtk4::Label::new(Some(text));
        lbl.set_xalign(0.0);
        lbl.add_css_class("rv-side-foot-label");
        lbl.set_max_width_chars(22);
        lbl.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        footer.append(&lbl);
    }
    sidebar.append(&footer);

    sidebar
}

fn rv_logo(size: i32) -> gtk4::DrawingArea {
    let area = gtk4::DrawingArea::new();
    area.set_content_width(size);
    area.set_content_height(size);
    area.set_draw_func(move |_, cr, w, _h| {
        let scale = w as f64 / 40.0;
        let _ = cr.save();
        cr.scale(scale, scale);

        // Ember #d65a3a
        let (r, g, b) = (214.0 / 255.0, 90.0 / 255.0, 58.0 / 255.0);

        // Octagon fill
        cr.set_source_rgba(r, g, b, 0.15);
        rv_logo_octagon(cr);
        let _ = cr.fill();

        // Octagon stroke
        cr.set_source_rgba(r, g, b, 1.0);
        cr.set_line_width(1.5);
        rv_logo_octagon(cr);
        let _ = cr.stroke();

        // R vertical bar: x=11 y=11 w=3 h=18
        cr.rectangle(11.0, 11.0, 3.0, 18.0);
        let _ = cr.fill();

        // R bowl: M14 11 Q24 11 24 17 Q24 23 14 23 (quadratic → cubic)
        cr.set_line_width(2.8);
        cr.move_to(14.0, 11.0);
        cr.curve_to(20.67, 11.0, 24.0, 13.0, 24.0, 17.0);
        cr.curve_to(24.0, 21.0, 20.67, 23.0, 14.0, 23.0);
        let _ = cr.stroke();

        // V slash: M14 23 L26 29
        cr.set_line_width(2.6);
        cr.move_to(14.0, 23.0);
        cr.line_to(26.0, 29.0);
        let _ = cr.stroke();

        // V dot: circle cx=26 cy=29 r=1.8
        cr.arc(26.0, 29.0, 1.8, 0.0, 2.0 * std::f64::consts::PI);
        let _ = cr.fill();

        let _ = cr.restore();
    });
    area
}

fn rv_logo_octagon(cr: &gtk4::cairo::Context) {
    // M20 2 L34 8 L38 22 L32 35 L20 38 L8 35 L2 22 L6 8 Z
    cr.move_to(20.0, 2.0);
    cr.line_to(34.0, 8.0);
    cr.line_to(38.0, 22.0);
    cr.line_to(32.0, 35.0);
    cr.line_to(20.0, 38.0);
    cr.line_to(8.0, 35.0);
    cr.line_to(2.0, 22.0);
    cr.line_to(6.0, 8.0);
    cr.close_path();
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
        Some(
            "status" | "profiles" | "battery" | "gpu" | "fans" | "appearance" | "automations"
            | "settings" | "diagnostics",
        ) => page.unwrap().to_owned(),
        _ => DEFAULT_DASHBOARD_PAGE.to_owned(),
    }
}
