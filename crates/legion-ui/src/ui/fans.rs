use crate::DiagnosticsBundle;
use adw::prelude::*;
use anyhow::{anyhow, Result};
use legion_common::{
    decode_fan_scratchpad_toml_v1, encode_fan_scratchpad_toml_v1, fan_curve_hwmon_point_pairs,
    fan_curve_snapshot_chart_pairs, fan_preset_points_as_sysfs_raw, format_fan_curve_live_vs_saved,
    format_manual_fan_scratchpad_sysfs_preview, parse_fan_preset_toml,
    validate_fan_preset_document, validate_manual_fan_curve_pairs, FanCurveCapability,
    FanCurveHwmonPointPair, FanCurveSnapshot,
};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::rc::Rc;

use super::shared::{
    append_error, clone_result, info_row, make_client, render_dry_run_plan_summary,
    render_dry_run_recovery_summary, request_dashboard_refresh, section_note,
    selected_dropdown_value, spawn_dbus_call,
};

const FAN_PRESET_IDS: &[&str] = &["quiet-office", "balanced-daily", "gaming", "max-safe"];
const FAN_CURVE_CHART_MARGIN: f64 = 36.0;
const SCRATCHPAD_CHART_DRAG_PICK_PX: f64 = 22.0;
const SCRATCHPAD_DRAG_TEMP_PER_PX: f64 = 25.0;
const SCRATCHPAD_DRAG_PWM_PER_PX: f64 = -0.45;
const SCRATCHPAD_KEY_TEMP_FINE: i32 = 500;
const SCRATCHPAD_KEY_TEMP_COARSE: i32 = 5000;
const SCRATCHPAD_KEY_PWM_FINE: i32 = 1;
const SCRATCHPAD_KEY_PWM_COARSE: i32 = 8;
const FAN_VALIDATION_EVIDENCE_COMMANDS: &str = "ratvantage-capture-compatibility-bundle --output target/validation/fan-validation-evidence\nlegion-control-ui --fan-curve-live\nlegion-control-ui --last-known-good-fan-curve\nlegion-control-ui --plan-restore-auto-fan";

type ScratchpadChartSelection = Rc<RefCell<Option<usize>>>;
type ManualFanScratchRows = Rc<RefCell<Vec<(FanCurveHwmonPointPair, gtk4::Entry, gtk4::Entry)>>>;

pub fn fans_page(
    diagnostics: Result<DiagnosticsBundle>,
    fan_snapshot: Result<Option<FanCurveSnapshot>>,
) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    match diagnostics {
        Ok(bundle) => append_fans(&page, &bundle, fan_snapshot),
        Err(error) => append_error(&page, &error),
    }

    page
}

fn append_fans(
    page: &adw::PreferencesPage,
    bundle: &DiagnosticsBundle,
    fan_snapshot: Result<Option<FanCurveSnapshot>>,
) {
    let telemetry = adw::PreferencesGroup::new();
    telemetry.set_title("Fan Telemetry");
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
    page.add(&telemetry);

    let temps = adw::PreferencesGroup::new();
    temps.set_title("Temperature Sensors");
    let temp_sensors = bundle
        .raw_probe_report
        .telemetry
        .sensors
        .iter()
        .filter(|sensor| sensor.kind == "temp")
        .collect::<Vec<_>>();
    let mut any_temp = false;
    for sensor in temp_sensors {
        let name = sensor.hwmon_name.as_deref().unwrap_or("hwmon");
        let title = match &sensor.label {
            Some(label) => format!("{name} / {label}"),
            None => name.to_owned(),
        };
        let value = sensor
            .value
            .map(|value| format!("{:.1} °C", value as f64 / 1000.0))
            .unwrap_or_else(|| "unknown".to_owned());
        temps.add(&info_row(&title, &value));
        any_temp = true;
    }
    for zone in &bundle.raw_probe_report.thermal_zones {
        let title = match &zone.zone_type {
            Some(kind) => format!("{} ({kind})", zone.name),
            None => zone.name.clone(),
        };
        let value = zone
            .temp_millicelsius
            .map(|value| format!("{:.1} °C", value as f64 / 1000.0))
            .unwrap_or_else(|| "unknown".to_owned());
        temps.add(&info_row(&title, &value));
        any_temp = true;
    }
    if !any_temp {
        temps.add(&info_row("Temperature sensors", "unavailable"));
    }
    page.add(&temps);

    let curves = adw::PreferencesGroup::new();
    curves.set_title("Fan Curves");
    if bundle.raw_probe_report.fan_curves.is_empty() {
        curves.add(&info_row("Fan curves", "unavailable"));
    } else {
        for curve in &bundle.raw_probe_report.fan_curves {
            curves.add(&info_row(
                &curve.id,
                &format!("{} point files", curve.point_paths.len()),
            ));
        }
    }
    let last_known_row = info_row("Last known good", &render_fan_snapshot_row(&fan_snapshot));
    curves.add(&last_known_row);
    page.add(&curves);

    append_fan_curve_readonly_preview(page, bundle, &fan_snapshot);

    append_fan_preset_per_profile_section(page, bundle);

    let lkg_points_group =
        append_saved_lkg_curve_detail(page, &clone_result(&fan_snapshot), last_known_row.clone());

    page.add(&build_fan_planning_controls(
        bundle.raw_probe_report.fan_curves.as_slice(),
        last_known_row,
        lkg_points_group,
    ));
    page.add(&build_fan_validation_evidence_controls());

    if !bundle.raw_probe_report.fan_curves.is_empty() {
        append_fan_live_curve_readings(page);
        append_fan_live_vs_saved_compare(page);
        append_manual_fan_curve_scratchpad(page, &bundle.raw_probe_report.fan_curves[0]);
    }
}

fn build_fan_validation_evidence_controls() -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Fan Validation Evidence");
    group.add(&section_note(
        "Fan curve execution remains plan-only until live writable curve evidence exists; these commands collect read-only bundle, live, saved, and restore-plan data.",
    ));

    let row = adw::ActionRow::builder()
        .title("Read-only fan evidence")
        .subtitle("Copies compatibility, live curve, saved snapshot, and restore-plan commands without applying fan writes")
        .selectable(false)
        .build();
    let copy = gtk4::Button::with_label("Copy evidence commands");
    copy.add_css_class("pill");
    copy.set_valign(gtk4::Align::Center);
    copy.connect_clicked(move |_| {
        if let Some(display) = gtk4::gdk::Display::default() {
            display
                .clipboard()
                .set_text(FAN_VALIDATION_EVIDENCE_COMMANDS);
        }
    });
    row.add_suffix(&copy);
    group.add(&row);
    group
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
    page: &adw::PreferencesPage,
    fan_snapshot: &Result<Option<FanCurveSnapshot>>,
    last_known_row: adw::ActionRow,
) -> gtk4::Box {
    let group = adw::PreferencesGroup::new();
    group.set_title("Saved last-known-good detail");
    group.set_description(Some(
        "Point values from durable app state (GetLastKnownGoodFanCurve). Read-only; use Capture snapshot in Guided fan planning to update.",
    ));

    let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let refresh_saved = gtk4::Button::with_label("Refresh saved snapshot");
    actions.append(&refresh_saved);
    group.add(&actions);

    let points_heading = gtk4::Label::new(Some("Saved curve points (read-only)"));
    points_heading.add_css_class("title-4");
    points_heading.set_xalign(0.0);
    points_heading.set_margin_top(8);
    points_heading.set_margin_bottom(8);
    group.add(&points_heading);

    let points_column = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    points_column.set_margin_start(4);
    repopulate_saved_lkg_points_column(&points_column, fan_snapshot);
    group.add(&points_column);

    let points_for_refresh = points_column.clone();
    let last_known_for_refresh = last_known_row.clone();
    refresh_saved.connect_clicked(move |_| {
        let points_for_refresh = points_for_refresh.clone();
        let last_known_for_refresh = last_known_for_refresh.clone();
        spawn_dbus_call(
            || make_client().and_then(|client| client.last_known_good_fan_curve()),
            move |result| match result {
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
            },
        );
    });

    page.add(&group);
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

fn append_fan_live_curve_readings(page: &adw::PreferencesPage) {
    let group = adw::PreferencesGroup::new();
    group.set_title("Live curve readings");
    group.set_description(Some(
        "Read-only sysfs snapshot from the daemon. This does not update the last-known-good capture.",
    ));

    let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let refresh = gtk4::Button::with_label("Refresh live readings");
    actions.append(&refresh);
    group.add(&actions);

    let points_heading = gtk4::Label::new(Some("Live sysfs points (read-only)"));
    points_heading.add_css_class("title-4");
    points_heading.set_xalign(0.0);
    points_heading.set_margin_top(8);
    points_heading.set_margin_bottom(8);
    group.add(&points_heading);

    let points_column = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    points_column.set_margin_start(4);
    points_column.append(&info_row(
        "No data yet",
        "Use Refresh live readings to load current pwm/temp values.",
    ));
    group.add(&points_column);

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
    group.add(&scroller);

    page.add(&group);

    let text_for_refresh = text.clone();
    let points_for_refresh = points_column.clone();
    refresh.connect_clicked(move |_| {
        let text_for_refresh = text_for_refresh.clone();
        let points_for_refresh = points_for_refresh.clone();
        spawn_dbus_call(
            || make_client().and_then(|client| client.live_fan_curve_readings()),
            move |result| match result {
                Ok(snapshot) => {
                    repopulate_live_points_column(&points_for_refresh, &snapshot);
                    text_for_refresh
                        .buffer()
                        .set_text(&format_fan_snapshot_multiline(&snapshot));
                }
                Err(error) => {
                    clear_points_column(&points_for_refresh);
                    points_for_refresh
                        .append(&info_row("Live readings failed", &error.to_string()));
                    text_for_refresh
                        .buffer()
                        .set_text(&format!("Live readings failed:\n{error}"));
                }
            },
        );
    });
}

fn append_fan_live_vs_saved_compare(page: &adw::PreferencesPage) {
    let group = adw::PreferencesGroup::new();
    group.set_title("Live vs saved comparison");
    group.set_description(Some(
        "Read-only diff between current sysfs values (GetLiveFanCurveReadings) and the durable last-known-good capture (GetLastKnownGoodFanCurve). Does not refresh other sections.",
    ));

    let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let compare = gtk4::Button::with_label("Compare live to saved");
    actions.append(&compare);
    group.add(&actions);

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
    group.add(&scroller);

    page.add(&group);

    let text_for_compare = text.clone();
    compare.connect_clicked(move |_| {
        let text_for_compare = text_for_compare.clone();
        spawn_dbus_call(
            || {
                let client = make_client()?;
                let live = client.live_fan_curve_readings()?;
                let saved = client.last_known_good_fan_curve()?;
                Ok((live, saved))
            },
            move |result| match result {
                Ok((live, Some(saved))) => {
                    let report = format_fan_curve_live_vs_saved(&live, &saved);
                    text_for_compare.buffer().set_text(&report);
                }
                Ok((_live, None)) => {
                    text_for_compare.buffer().set_text(
                        "No last-known-good snapshot is stored yet.\nUse Capture snapshot in Guided fan planning first, then compare again.",
                    );
                }
                Err(error) => {
                    text_for_compare
                        .buffer()
                        .set_text(&format!("Comparison failed:\n{error}"));
                }
            },
        );
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
    line_rgb: (f64, f64, f64),
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

    cr.set_source_rgb(line_rgb.0, line_rgb.1, line_rgb.2);
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
        cr.set_source_rgb(line_rgb.0 * 0.8, line_rgb.1 * 0.8, line_rgb.2 * 0.8);
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
    draw_temp_pwm_polyline_chart(cr, width, height, pairs, None, (0.1, 0.35, 0.82));
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
    key.connect_key_pressed(move |_ctrl, keyval, _code, state| {
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
        let (dt, dp) = match keyval {
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
    page: &adw::PreferencesPage,
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
    caption.set_margin_bottom(8);
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

    page.add(&group);
}

fn append_fan_preset_per_profile_section(page: &adw::PreferencesPage, bundle: &DiagnosticsBundle) {
    let Some(platform_cap) = bundle.raw_probe_report.platform_profile.as_ref() else {
        return;
    };
    if bundle.raw_probe_report.fan_curves.is_empty() || platform_cap.choices.is_empty() {
        return;
    }

    let group = adw::PreferencesGroup::new();
    group.set_title("Fan preset per platform profile");
    group.set_description(Some(
        "App-state only: Save stores a preferred packaged fan preset for each platform profile. Resume re-apply logs a dry-run plan only; no fan sysfs writes run from RatVantage.",
    ));

    let map_state: BTreeMap<String, String> = bundle.fan_preset_by_platform_profile.clone();
    let map_status = gtk4::Label::new(None);
    map_status.set_wrap(true);
    map_status.set_xalign(0.0);
    map_status.set_margin_top(6);
    map_status.set_margin_bottom(6);

    let mut dropdown_labels: Vec<&str> = vec!["(none)"];
    dropdown_labels.extend_from_slice(FAN_PRESET_IDS);

    for profile in &platform_cap.choices {
        let row = adw::ActionRow::builder()
            .title(profile.as_str())
            .subtitle("Interactive: click this row or the dropdown to choose an app-state preset mapping.")
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

        let save = gtk4::Button::with_label("Save app-state mapping");
        row.add_suffix(&dropdown);
        row.add_suffix(&save);
        row.set_activatable_widget(Some(&dropdown));

        let profile_for_save = profile.clone();
        let status_for_save = map_status.clone();
        save.connect_clicked(move |_| {
            status_for_save.set_text(
                "Saving fan preset mapping in daemon app state only; no fan sysfs write will run...",
            );
            let sel = dropdown.selected() as usize;
            let result = if sel == 0 {
                make_client()
                    .and_then(|client| client.remove_fan_preset_profile_map_entry(&profile_for_save))
            } else if let Some(preset_id) = FAN_PRESET_IDS.get(sel.saturating_sub(1)) {
                make_client().and_then(|client| {
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
                            "Saved app-state mapping `{profile_for_save}` -> `{preset_id}` (dashboard refresh requested; no fan sysfs write ran)."
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
    resume_row.set_activatable_widget(Some(&resume_switch));
    let skip_resume_notify = Rc::new(Cell::new(true));
    let skip_resume_for_connect = skip_resume_notify.clone();
    let map_status_for_resume = map_status.clone();
    resume_switch.connect_active_notify(move |switch| {
        if skip_resume_for_connect.get() {
            skip_resume_for_connect.set(false);
            return;
        }
        let on = switch.is_active();
        match make_client()
            .and_then(|client| client.set_fan_preset_reapply_after_resume(on))
        {
            Ok(confirmed) => {
                switch.set_active(confirmed);
                map_status_for_resume
                    .set_text("Resume fan re-apply policy updated (dry-run planning only; dashboard refresh requested).");
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
    bulk_row.set_activatable_widget(Some(&clear_all));
    group.add(&bulk_row);

    let status_for_clear = map_status.clone();
    clear_all.connect_clicked(move |_| {
        match make_client().and_then(|client| client.clear_fan_preset_profile_map()) {
            Ok(_) => {
                status_for_clear.set_text("Cleared every profile->preset app-state mapping.");
                let _ = request_dashboard_refresh();
            }
            Err(error) => {
                status_for_clear.set_text(&format!("Clear failed: {error}"));
            }
        }
    });

    group.add(&map_status);
    page.add(&group);
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

fn append_manual_fan_curve_scratchpad(page: &adw::PreferencesPage, curve: &FanCurveCapability) {
    let pairs = fan_curve_hwmon_point_pairs(curve);
    if pairs.is_empty() {
        return;
    }
    let pairs_for_ui = pairs.clone();

    let group = adw::PreferencesGroup::new();
    group.set_title("Manual curve scratchpad");
    group.set_description(Some(
        "Edit raw sysfs integers per paired pwm*_auto_pointN node. Local validation and previews only; there is no Apply button and no daemon write.",
    ));

    let actions_column = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    let actions_row_load = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    actions_row_load.set_spacing(8);
    let actions_row_export = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    actions_row_export.set_spacing(8);
    let load_live = gtk4::Button::with_label("Load live readings");
    let load_saved = gtk4::Button::with_label("Load saved snapshot");
    let clear_btn = gtk4::Button::with_label("Clear");
    let validate_btn = gtk4::Button::with_label("Validate pairs");
    let copy_json = gtk4::Button::with_label("Copy JSON");
    let copy_toml = gtk4::Button::with_label("Copy scratchpad TOML");
    let preview_sysfs = gtk4::Button::with_label("Preview sysfs text");
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
    group.add(&actions_column);

    let status = gtk4::Label::new(Some(
        "Use Load from live or saved after refreshing those sections, or type values manually.",
    ));
    status.set_wrap(true);
    status.set_xalign(0.0);
    status.set_selectable(true);
    status.set_margin_top(8);
    status.set_margin_bottom(8);
    group.add(&status);

    let rows_heading = gtk4::Label::new(Some("Editable pwm/temp pairs"));
    rows_heading.add_css_class("title-4");
    rows_heading.set_xalign(0.0);
    rows_heading.set_margin_top(8);
    group.add(&rows_heading);

    let chart_interaction_hint = gtk4::Label::new(Some(
        "Click or drag a point on the chart to edit temp/PWM in the rows, or select a point and use arrow keys (Shift = larger temp/PWM steps). Focusing a row's temp or pwm field syncs the chart highlight. Scratchpad only; not applied to hardware. Axes: raw sysfs temperature (min–max of your points) horizontally, PWM 0–255 vertically.",
    ));
    chart_interaction_hint.add_css_class("dim-label");
    chart_interaction_hint.set_wrap(true);
    chart_interaction_hint.set_xalign(0.0);
    chart_interaction_hint.set_margin_bottom(8);
    group.add(&chart_interaction_hint);

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
            draw_temp_pwm_polyline_chart(cr, w, h, &pairs, highlight, (0.85, 0.45, 0.1));
        } else {
            let msg = scratchpad_first_parse_blocker(&entries_for_scratchpad_chart);
            draw_scratchpad_chart_placeholder(cr, w, h, &msg);
        }
    });
    attach_scratchpad_chart_interactions(&scratchpad_chart, &entries, &scratchpad_selection);
    group.add(&scratchpad_chart);

    let rows_column = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    rows_column.set_margin_start(4);
    group.add(&rows_column);

    let sysfs_preview_title = gtk4::Label::new(Some("Sysfs target preview (scratchpad)"));
    sysfs_preview_title.add_css_class("title-4");
    sysfs_preview_title.set_xalign(0.0);
    sysfs_preview_title.set_margin_top(10);
    group.add(&sysfs_preview_title);

    let sysfs_preview_hint = gtk4::Label::new(Some(
        "Requires every row to parse as integers and to pass the same monotonic rules as Validate pairs. Purely local text — no D-Bus call. Copy sysfs preview copies whatever is currently shown in the pane.",
    ));
    sysfs_preview_hint.add_css_class("dim-label");
    sysfs_preview_hint.set_wrap(true);
    sysfs_preview_hint.set_xalign(0.0);
    sysfs_preview_hint.set_margin_bottom(8);
    group.add(&sysfs_preview_hint);

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
    group.add(&sysfs_preview_scroll);

    let toml_title = gtk4::Label::new(Some("TOML exchange"));
    toml_title.add_css_class("title-4");
    toml_title.set_xalign(0.0);
    toml_title.set_margin_top(12);
    group.add(&toml_title);

    let toml_hint = gtk4::Label::new(Some(
        "Paste a ratvantage_fan_scratchpad_v1 document or a packaged data/presets/*.toml fan preset. Import fills the rows above; it does not call the daemon.",
    ));
    toml_hint.set_wrap(true);
    toml_hint.set_xalign(0.0);
    toml_hint.set_margin_bottom(8);
    group.add(&toml_hint);

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
    group.add(&toml_scroll);

    let toml_actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let import_toml = gtk4::Button::with_label("Import TOML from editor");
    toml_actions.append(&import_toml);
    group.add(&toml_actions);

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
        let pairs_for_reload = pairs_for_reload.clone();
        let rows_for_reload = rows_for_reload.clone();
        let entries_for_reload = entries_for_reload.clone();
        let chart_for_live = chart_for_live.clone();
        let sel_for_live = sel_for_live.clone();
        let status_for_live = status_for_live.clone();
        spawn_dbus_call(
            || make_client().and_then(|client| client.live_fan_curve_readings()),
            move |result| match result {
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
            },
        );
    });

    let pairs_for_saved = pairs_for_ui.clone();
    let rows_for_saved = rows_column.clone();
    let entries_for_saved = entries.clone();
    let chart_for_saved = scratchpad_chart.clone();
    let sel_for_saved = scratchpad_selection.clone();
    let status_for_saved = status.clone();
    load_saved.connect_clicked(move |_| {
        let pairs_for_saved = pairs_for_saved.clone();
        let rows_for_saved = rows_for_saved.clone();
        let entries_for_saved = entries_for_saved.clone();
        let chart_for_saved = chart_for_saved.clone();
        let sel_for_saved = sel_for_saved.clone();
        let status_for_saved = status_for_saved.clone();
        spawn_dbus_call(
            || make_client().and_then(|client| client.last_known_good_fan_curve()),
            move |result| match result {
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
            },
        );
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
                .set_text("Nothing to copy - click Preview sysfs text to fill the pane first.");
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

    page.add(&group);
}

fn build_fan_planning_controls(
    fan_curves: &[FanCurveCapability],
    last_known_row: adw::ActionRow,
    lkg_points_column: gtk4::Box,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Guided fan planning");
    group.set_description(Some(
        "Fan actions here are dry-run planning, rollback reference capture, or app-state updates. They do not apply fan curves to hardware.",
    ));

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

    let preview_preset = gtk4::Button::with_label("Preview dry-run plan");
    let preview_restore = gtk4::Button::with_label("Preview restore dry-run");
    let capture = gtk4::Button::with_label("Capture snapshot");

    let chooser_row = adw::ActionRow::builder()
        .title("Packaged preset")
        .subtitle(
            "Interactive: click this row or the dropdown to choose a preset. Dry-run preview only; there is no dashboard ApplyFanPreset execution until live validation evidence exists.",
        )
        .selectable(false)
        .build();
    chooser_row.add_suffix(&chooser);
    chooser_row.add_suffix(&preview_preset);
    chooser_row.set_activatable_widget(Some(&chooser));
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
    restore_row.set_activatable_widget(Some(&preview_restore));
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
    capture_row.set_activatable_widget(Some(&capture));
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

        let preset_plan_for_preset = preset_plan_for_preset.clone();
        let preset_recovery_for_preset = preset_recovery_for_preset.clone();
        preset_plan_for_preset.set_title("Preset plan preview requested");
        preset_plan_for_preset.set_subtitle(
            "Request sent to the daemon for dry-run planning; no fan sysfs write will run.",
        );
        preset_recovery_for_preset.set_subtitle("Waiting for rollback guidance from the daemon...");
        spawn_dbus_call(
            move || {
                make_client()
                    .and_then(|client| client.plan_fan_preset_write(&requested))
            },
            move |result| match result {
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
            },
        );
    });

    let restore_plan_for_restore = restore_plan_row.clone();
    let restore_recovery_for_restore = restore_recovery_row.clone();
    preview_restore.connect_clicked(move |_| {
        let restore_plan_for_restore = restore_plan_for_restore.clone();
        let restore_recovery_for_restore = restore_recovery_for_restore.clone();
        restore_plan_for_restore.set_title("Restore plan preview requested");
        restore_plan_for_restore.set_subtitle(
            "Request sent to the daemon for dry-run planning; no fan sysfs write will run.",
        );
        restore_recovery_for_restore
            .set_subtitle("Waiting for rollback guidance from the daemon...");
        spawn_dbus_call(
            || make_client().and_then(|client| client.plan_restore_auto_fan_write()),
            move |result| match result {
                Ok(plan) => {
                    restore_plan_for_restore.set_title("Restore plan preview ready");
                    restore_plan_for_restore.set_subtitle(&render_dry_run_plan_summary(&plan));
                    restore_recovery_for_restore
                        .set_subtitle(&render_dry_run_recovery_summary(&plan));
                }
                Err(error) => {
                    restore_plan_for_restore.set_title("Restore plan preview failed");
                    restore_plan_for_restore
                        .set_subtitle(&format!("Dry-run planning could not be completed: {error}"));
                    restore_recovery_for_restore.set_subtitle(
                        "The daemon rejected this request or no automatic fan curve was detected.",
                    );
                }
            },
        );
    });

    let last_known_for_capture = last_known_row.clone();
    let lkg_points_for_capture = lkg_points_column.clone();
    capture.connect_clicked(move |_| {
        let last_known_for_capture = last_known_for_capture.clone();
        let lkg_points_for_capture = lkg_points_for_capture.clone();
        last_known_for_capture.set_subtitle(
            "Capture requested; reading current fan curve values into app state only...",
        );
        spawn_dbus_call(
            || make_client().and_then(|client| client.capture_last_known_good_fan_curve()),
            move |result| match result {
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
            },
        );
    });

    group
}
