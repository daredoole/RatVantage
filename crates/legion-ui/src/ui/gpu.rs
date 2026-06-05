use crate::{capability_status_label, DiagnosticsBundle, GpuModePending};
use adw::prelude::*;
use anyhow::Result;
use legion_common::{
    format_gpu_mode_pending_summary, format_gpu_switch_type, AmdGpuPowerDpmCapability,
    GpuCapability,
};

use super::shared::{
    append_error, build_confirmed_write_controls, info_row, make_client,
    render_dry_run_plan_summary, render_dry_run_recovery_summary, request_dashboard_refresh,
    section_note, selected_dropdown_value, spawn_dbus_call,
};

const GPU_MODE_CHOICES: &[&str] = &["integrated", "hybrid", "nvidia"];
const GPU_SWITCHING_EVIDENCE_COMMANDS: &str = "ratvantage-capture-compatibility-bundle --output target/validation/gpu-switching-evidence\nratvantage-capture-gpu-mux-evidence --phase mux-only --output target/validation/gpu-mux-evidence\nratvantage-review-gpu-mux-evidence target/validation/gpu-mux-evidence\nlegion-control-ui --diagnostics\nlegion-control-ui --reset-diagnostics\nlegion-control-ui --overview";
const GPU_RUNTIME_PLAN_COMMANDS: &str = "legion-control-ui --plan-gpu-mode-runtime integrated\nlegion-control-ui --plan-gpu-mode-runtime hybrid";

pub fn gpu_page(
    diagnostics: Result<DiagnosticsBundle>,
    gpu_pending: Result<Option<GpuModePending>>,
) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    match diagnostics {
        Ok(bundle) => append_gpu(&page, &bundle, gpu_pending),
        Err(error) => append_error(&page, &error),
    }

    page
}

fn append_gpu(
    page: &adw::PreferencesPage,
    bundle: &DiagnosticsBundle,
    gpu_pending: Result<Option<GpuModePending>>,
) {
    let mode = adw::PreferencesGroup::new();
    mode.set_title("GPU");
    mode.add(&section_note(
        "GPU mode execution is daemon-only, policy-gated, and records a reboot-pending state after EnvyControl accepts the switch.",
    ));
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
        mode.add(&info_row(
            "Switch type",
            format_gpu_switch_type(gpu.switch_type),
        ));
        mode.add(&info_row(
            "GPU switching status",
            &bundle.gpu_switching.status,
        ));
        mode.add(&info_row(
            "Execution model",
            &bundle.gpu_switching.execution_model,
        ));
        mode.add(&info_row(
            "Runtime plan",
            if bundle.gpu_switching.runtime_plan_available {
                "available"
            } else {
                "blocked"
            },
        ));
        for blocker in &bundle.gpu_switching.blockers {
            mode.add(&info_row("Switch blocker", blocker));
        }
        mode.add(&info_row(
            "Switch next action",
            &bundle.gpu_switching.next_action,
        ));
        for evidence in bundle.gpu_switching.evidence.iter().take(4) {
            mode.add(&info_row("Runtime evidence", evidence));
        }
        for note in &gpu.switch_notes {
            mode.add(&info_row("Switch evidence", note));
        }
    } else {
        mode.add(&info_row("GPU mode", "unavailable"));
    }
    let pending_row = info_row("Pending reboot", &render_gpu_pending_row(&gpu_pending));
    mode.add(&pending_row);
    page.add(&mode);
    page.add(&build_gpu_switching_evidence_controls());

    if let Some(dpm) = &bundle.raw_probe_report.amd_gpu_power_dpm {
        let dpm_group = adw::PreferencesGroup::new();
        dpm_group.set_title("AMD GPU DPM");
        dpm_group.add(&info_row(
            "Force level",
            dpm.current_force_performance_level
                .as_deref()
                .unwrap_or("unknown"),
        ));
        if let Some(state) = &dpm.power_dpm_state {
            dpm_group.add(&info_row("Power state", state));
        }
        if let Some(sclk) = &dpm.current_sclk {
            dpm_group.add(&info_row("SCLK", sclk));
        }
        if let Some(mclk) = &dpm.current_mclk {
            dpm_group.add(&info_row("MCLK", mclk));
        }
        dpm_group.add(&section_note(
            "Manual SCLK/MCLK clock states are read-only here and are not exposed as write controls. Use DPM force level for the supported GPU power control path.",
        ));
        page.add(&dpm_group);
        page.add(&build_amd_gpu_dpm_controls(Some(dpm)));
    } else {
        page.add(&build_amd_gpu_dpm_controls(None));
    }

    page.add(&build_gpu_mode_controls(
        bundle.raw_probe_report.gpu.as_ref(),
        pending_row,
    ));
}

fn build_gpu_switching_evidence_controls() -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("GPU Switching Evidence");
    group.add(&section_note(
        "Runtime/session GPU switching stays research-only until read-only state, blocker, and display recovery evidence are captured.",
    ));

    let row = adw::ActionRow::builder()
        .title("Read-only GPU switching evidence")
        .subtitle("Copies compatibility, mux-only, review, diagnostics, reset, and overview commands; no GPU mode write is sent")
        .selectable(false)
        .build();
    let copy = gtk4::Button::with_label("Copy evidence commands");
    copy.add_css_class("pill");
    copy.set_valign(gtk4::Align::Center);
    copy.connect_clicked(move |_| {
        if let Some(display) = gtk4::gdk::Display::default() {
            display
                .clipboard()
                .set_text(GPU_SWITCHING_EVIDENCE_COMMANDS);
        }
    });
    row.add_suffix(&copy);
    group.add(&row);

    let runtime_row = adw::ActionRow::builder()
        .title("Read-only runtime plan commands")
        .subtitle("Copies plan-only integrated/hybrid runtime commands; current daemon validation blocks them until reviewed mux/session evidence promotes the candidate")
        .selectable(false)
        .build();
    let copy_runtime = gtk4::Button::with_label("Copy runtime plans");
    copy_runtime.add_css_class("pill");
    copy_runtime.set_valign(gtk4::Align::Center);
    copy_runtime.connect_clicked(move |_| {
        if let Some(display) = gtk4::gdk::Display::default() {
            display.clipboard().set_text(GPU_RUNTIME_PLAN_COMMANDS);
        }
    });
    runtime_row.add_suffix(&copy_runtime);
    group.add(&runtime_row);
    group
}

fn build_amd_gpu_dpm_controls(
    capability: Option<&AmdGpuPowerDpmCapability>,
) -> adw::PreferencesGroup {
    let group = build_confirmed_write_controls(
        "AMD GPU DPM Control",
        capability.and_then(|capability| capability.current_force_performance_level.as_deref()),
        capability.map(|capability| capability.choices.as_slice()),
        "Requested force level",
        "Apply force level",
        "AMD GPU DPM force level",
        "Confirm DPM force-level write",
        "I understand this can affect display/GPU stability and that auto restores driver control.",
        |requested| make_client().and_then(|client| client.set_amd_gpu_dpm_force_level(requested)),
        move |_| {
            request_dashboard_refresh();
        },
    );
    group.add(&section_note(
        "Confirm this is intentional before applying. DPM force-level writes may affect display/GPU stability; use auto to restore driver control.",
    ));
    group
}

fn render_gpu_pending_row(pending: &Result<Option<GpuModePending>>) -> String {
    match pending {
        Ok(opt) => format_gpu_mode_pending_summary(opt.as_ref()),
        Err(error) => format!("state unavailable - {error}"),
    }
}

fn build_gpu_mode_controls(
    capability: Option<&GpuCapability>,
    pending_row: adw::ActionRow,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Switch Planning");

    let Some(capability) = capability.filter(|capability| capability.provider == "envycontrol")
    else {
        group.add(&info_row(
            "GPU mode planning",
            "Unavailable: envycontrol was not detected, so GPU switch planning is disabled.",
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
    preview.add_css_class("pill");
    preview.set_valign(gtk4::Align::Center);
    let execute = gtk4::Button::with_label("Switch mode");
    execute.add_css_class("pill");
    execute.add_css_class("suggested-action");
    execute.set_valign(gtk4::Align::Center);
    execute.set_sensitive(false);
    let record_pending = gtk4::Button::with_label("Record pending");
    record_pending.add_css_class("pill");
    record_pending.set_valign(gtk4::Align::Center);
    let clear_pending = gtk4::Button::with_label("Clear pending");
    clear_pending.add_css_class("pill");
    clear_pending.add_css_class("destructive-action");
    clear_pending.set_valign(gtk4::Align::Center);

    let chooser_row = adw::ActionRow::builder()
        .title("Target mode")
        .subtitle("Preview before switching. Switching requires daemon policy, polkit, EnvyControl, and reboot.")
        .selectable(false)
        .build();
    chooser_row.add_suffix(&chooser);
    chooser_row.add_suffix(&preview);
    chooser_row.add_suffix(&execute);
    chooser_row.set_activatable_widget(Some(&chooser));
    group.add(&chooser_row);

    let confirm_row = adw::ActionRow::builder()
        .title("Confirm GPU switch")
        .subtitle(
            "I have reviewed the plan/recovery guidance and understand a reboot may be required.",
        )
        .selectable(false)
        .build();
    let confirm = gtk4::CheckButton::new();
    confirm.set_valign(gtk4::Align::Center);
    confirm_row.add_suffix(&confirm);
    group.add(&confirm_row);

    let pending_controls = adw::ActionRow::builder()
        .title("Pending reboot state")
        .subtitle("Track an external switch that needs reboot verification.")
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
            "Preview a plan to see the rollback path before running a gated EnvyControl switch.",
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

        let plan_row_for_preview = plan_row_for_preview.clone();
        let recovery_row_for_preview = recovery_row_for_preview.clone();
        plan_row_for_preview.set_title("Plan preview requested");
        plan_row_for_preview.set_subtitle(
            "Request sent to the daemon for dry-run planning; no hardware write will run.",
        );
        recovery_row_for_preview.set_subtitle("Waiting for rollback guidance from the daemon...");
        spawn_dbus_call(
            move || make_client().and_then(|client| client.plan_gpu_mode_write(&requested)),
            move |result| match result {
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
            },
        );
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

        let pending_row_for_record = pending_row_for_record.clone();
        let plan_row_for_record = plan_row_for_record.clone();
        let recovery_row_for_record = recovery_row_for_record.clone();
        plan_row_for_record.set_title("Recording pending reboot state");
        plan_row_for_record.set_subtitle(
            "Updating RatVantage app state only; this does not perform the GPU switch.",
        );
        recovery_row_for_record.set_subtitle("Waiting for daemon app-state update...");
        spawn_dbus_call(
            move || {
                make_client()
                    .and_then(|client| client.set_gpu_mode_pending(&requested))
            },
            move |result| match result {
                Ok(pending) => {
                    pending_row_for_record
                        .set_subtitle(&render_gpu_pending_row(&Ok(Some(pending.clone()))));
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
            },
        );
    });

    let chooser_for_execute = chooser.clone();
    let confirm_for_execute = confirm.clone();
    let pending_row_for_execute = pending_row.clone();
    let plan_row_for_execute = plan_row.clone();
    let recovery_row_for_execute = recovery_row.clone();
    let execute_for_confirm = execute.clone();
    confirm.connect_toggled(move |confirm| {
        execute_for_confirm.set_sensitive(confirm.is_active());
    });

    execute.connect_clicked(move |_| {
        if !confirm_for_execute.is_active() {
            plan_row_for_execute.set_title("GPU mode switch not confirmed");
            plan_row_for_execute
                .set_subtitle("Review the plan/recovery guidance and check Confirm GPU switch.");
            return;
        }

        let Some(requested) = selected_dropdown_value(&chooser_for_execute) else {
            plan_row_for_execute.set_title("GPU mode switch not started");
            plan_row_for_execute.set_subtitle("No target GPU mode was selected.");
            return;
        };

        let pending_row_for_execute = pending_row_for_execute.clone();
        let plan_row_for_execute = plan_row_for_execute.clone();
        let recovery_row_for_execute = recovery_row_for_execute.clone();
        plan_row_for_execute.set_title("GPU mode switch requested");
        plan_row_for_execute.set_subtitle(
            "The daemon is executing EnvyControl if policy and authorization allow it.",
        );
        recovery_row_for_execute
            .set_subtitle("If the switch succeeds, reboot and verify the requested GPU mode.");
        spawn_dbus_call(
            move || make_client().and_then(|client| client.set_gpu_mode(&requested)),
            move |result| match result {
                Ok(result) if result.applied => {
                    pending_row_for_execute
                        .set_subtitle(result.readback_value.as_deref().unwrap_or("reboot pending"));
                    plan_row_for_execute.set_title("GPU mode switch accepted");
                    plan_row_for_execute.set_subtitle(&result.message);
                    recovery_row_for_execute
                        .set_subtitle(&render_dry_run_recovery_summary(&result.plan));
                    let _ = request_dashboard_refresh();
                }
                Ok(result) => {
                    plan_row_for_execute.set_title("GPU mode switch blocked");
                    plan_row_for_execute.set_subtitle(&result.message);
                    recovery_row_for_execute
                        .set_subtitle(&render_dry_run_recovery_summary(&result.plan));
                }
                Err(error) => {
                    plan_row_for_execute.set_title("GPU mode switch failed");
                    plan_row_for_execute.set_subtitle(&format!("Daemon call failed: {error}"));
                }
            },
        );
    });

    let pending_row_for_clear = pending_row.clone();
    let plan_row_for_clear = plan_row.clone();
    let recovery_row_for_clear = recovery_row.clone();
    clear_pending.connect_clicked(move |_| {
        let pending_row_for_clear = pending_row_for_clear.clone();
        let plan_row_for_clear = plan_row_for_clear.clone();
        let recovery_row_for_clear = recovery_row_for_clear.clone();
        plan_row_for_clear.set_title("Clearing pending reboot state");
        plan_row_for_clear
            .set_subtitle("Clearing RatVantage app state only; no hardware write will run.");
        recovery_row_for_clear.set_subtitle("Waiting for daemon app-state update...");
        spawn_dbus_call(
            || make_client().and_then(|client| client.clear_gpu_mode_pending()),
            move |result| match result {
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
            },
        );
    });

    group
}
