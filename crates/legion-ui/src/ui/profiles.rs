use crate::{
    capability_status_label, set_ppd_active_profile, DiagnosticsBundle, PPD_PROFILE_CHOICES,
};
use adw::prelude::*;
use anyhow::Result;
use legion_common::PlatformProfileCapability;
use legion_common::{
    CpuPowerCapability, FirmwareAttributeCapability, RyzenBackendStatus,
    SUPPORTED_FIRMWARE_SCALAR_ATTRIBUTES,
};

use super::shared::{
    append_error, build_write_controls, info_row, make_client, request_dashboard_refresh,
    section_note, spawn_dbus_call, store_write_feedback_state, write_feedback_row,
    write_feedback_subtitle, write_feedback_title,
};

pub fn profiles_page(diagnostics: Result<DiagnosticsBundle>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    match diagnostics {
        Ok(bundle) => append_profiles(&page, &bundle),
        Err(error) => append_error(&page, &error),
    }

    page
}

fn append_profiles(page: &adw::PreferencesPage, bundle: &DiagnosticsBundle) {
    let group = adw::PreferencesGroup::new();
    group.set_title("Power Profiles");
    group.add(&section_note(
        "Platform is firmware. Fedora power is the desktop profile. RatVantage writes one path and reads both back.",
    ));
    if let Some(profile) = &bundle.raw_probe_report.platform_profile {
        let current = info_row(
            "Platform profile",
            profile.current.as_deref().unwrap_or("unknown"),
        );
        group.add(&current);
        if let Some(power_profile) = bundle
            .raw_probe_report
            .power_profiles
            .as_ref()
            .and_then(|profile| profile.active_profile.as_deref())
        {
            group.add(&info_row("Plasma/Fedora power profile", power_profile));
        } else {
            group.add(&info_row("Plasma/Fedora power profile", "unavailable"));
        }
        group.add(&info_row("Choices", &profile.choices.join(", ")));
        page.add(&group);

        let controls = build_platform_profile_controls(
            bundle.raw_probe_report.platform_profile.as_ref(),
            Some(current),
        );
        page.add(&controls);
    } else {
        group.add(&info_row("Platform profile", "unavailable"));
        page.add(&group);

        let controls = build_platform_profile_controls(None, None);
        page.add(&controls);
    }

    if let Some(pp) = &bundle.raw_probe_report.power_profiles {
        let desktop = adw::PreferencesGroup::new();
        desktop.set_title("Desktop Profile Detail");
        desktop.add(&info_row("Bus", &pp.bus));
        if let Some(owner) = &pp.unique_owner {
            desktop.add(&info_row("Unique owner", owner));
            let active_row = info_row(
                "Active profile",
                pp.active_profile.as_deref().unwrap_or("unknown"),
            );
            desktop.add(&active_row);
        } else if let Some(detail) = &pp.detail {
            desktop.add(&info_row("Unavailable", detail));
        } else {
            desktop.add(&info_row("Unavailable", "no D-Bus owner"));
        }
        desktop.add(&info_row(
            "Probe status",
            capability_status_label(pp.status),
        ));
        page.add(&desktop);

        // PPD write control — direct system bus call, no daemon proxy needed.
        if pp.unique_owner.is_some() {
            page.add(&build_ppd_write_controls(pp.active_profile.as_deref()));
        }
    }

    // CPU frequency scaling / amd-pstate — read-only display.
    if let Some(cpu) = &bundle.raw_probe_report.cpu_power {
        let cpu_group = adw::PreferencesGroup::new();
        cpu_group.set_title("CPU Frequency Scaling");
        cpu_group.add(&section_note(
            "Live amd-pstate / cpufreq parameters. Governor, EPP, and boost write controls ship behind the experimental write gate once live-validated.",
        ));
        if let Some(driver) = &cpu.scaling_driver {
            cpu_group.add(&info_row("Scaling driver", driver));
        }
        if let Some(status) = &cpu.amd_pstate_status {
            cpu_group.add(&info_row("amd-pstate mode", status));
        }
        cpu_group.add(&info_row(
            "Governor",
            cpu.governor.as_deref().unwrap_or("unknown"),
        ));
        if !cpu.available_governors.is_empty() {
            cpu_group.add(&info_row(
                "Available governors",
                &cpu.available_governors.join(", "),
            ));
        }
        cpu_group.add(&info_row(
            "Energy performance preference",
            cpu.epp.as_deref().unwrap_or("unknown"),
        ));
        if !cpu.available_epp.is_empty() {
            cpu_group.add(&info_row("Available EPP", &cpu.available_epp.join(", ")));
        }
        if let Some(boost) = cpu.boost {
            cpu_group.add(&info_row(
                "Boost",
                if boost { "enabled" } else { "disabled" },
            ));
        }
        if let (Some(min), Some(max)) = (cpu.cpuinfo_min_khz, cpu.cpuinfo_max_khz) {
            cpu_group.add(&info_row(
                "Hardware frequency range",
                &format!("{:.2}–{:.2} GHz", min as f64 / 1.0e6, max as f64 / 1.0e6),
            ));
        }
        if let (Some(min), Some(max)) = (cpu.scaling_min_khz, cpu.scaling_max_khz) {
            cpu_group.add(&info_row(
                "Active scaling range",
                &format!("{:.2}–{:.2} GHz", min as f64 / 1.0e6, max as f64 / 1.0e6),
            ));
        }
        if let Some(cur) = cpu.scaling_cur_khz {
            cpu_group.add(&info_row(
                "Current frequency",
                &format!("{:.2} GHz", cur as f64 / 1.0e6),
            ));
        }
        page.add(&cpu_group);

        page.add(&build_cpu_governor_write_controls(
            bundle.raw_probe_report.cpu_power.as_ref(),
        ));
        page.add(&build_cpu_epp_write_controls(
            bundle.raw_probe_report.cpu_power.as_ref(),
        ));
        page.add(&build_cpu_boost_write_controls(
            bundle.raw_probe_report.cpu_power.as_ref(),
        ));
        append_advanced_cpu_tuning_controls(page, bundle);
    }

    // PPT firmware power limits.
    if !bundle.raw_probe_report.firmware_attributes.is_empty() {
        page.add(&build_firmware_attribute_controls(
            &bundle.raw_probe_report.firmware_attributes,
        ));
    }

    page.add(&build_hardware_profile_apply_controls(bundle));
}

fn build_ppd_write_controls(current: Option<&str>) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Desktop Profile Control");
    group.add(&section_note(
        "Writes directly to power-profiles-daemon via system D-Bus. No daemon restart needed.",
    ));

    let choices: Vec<String> = PPD_PROFILE_CHOICES.iter().map(|s| s.to_string()).collect();
    let selected_index = current
        .and_then(|c| PPD_PROFILE_CHOICES.iter().position(|&p| p == c))
        .unwrap_or(1) as u32; // default balanced

    let current_display = current.unwrap_or(PPD_PROFILE_CHOICES[selected_index as usize]);

    let expander = adw::ExpanderRow::builder()
        .title("Requested desktop profile")
        .subtitle(current_display)
        .build();

    let selected: std::rc::Rc<std::cell::Cell<u32>> =
        std::rc::Rc::new(std::cell::Cell::new(selected_index));

    let (choice_rows, check_images): (Vec<adw::ActionRow>, Vec<gtk4::Image>) = choices
        .iter()
        .enumerate()
        .map(|(i, choice)| {
            let icon = gtk4::Image::from_icon_name("object-select-symbolic");
            icon.set_visible(i as u32 == selected_index);

            let row = adw::ActionRow::builder()
                .title(choice.as_str())
                .activatable(true)
                .build();
            row.add_suffix(&icon);
            expander.add_row(&row);
            (row, icon)
        })
        .unzip();

    let check_images = std::rc::Rc::new(check_images);

    for (i, row) in choice_rows.iter().enumerate() {
        let selected_rc = selected.clone();
        let images_rc = check_images.clone();
        let expander_rc = expander.clone();
        let choice = choices[i].clone();

        row.connect_activated(move |_| {
            for img in images_rc.iter() {
                img.set_visible(false);
            }
            images_rc[i].set_visible(true);
            selected_rc.set(i as u32);
            expander_rc.set_subtitle(&choice);
            expander_rc.set_expanded(false);
        });
    }

    let apply = gtk4::Button::with_label("Apply desktop profile");
    apply.set_sensitive(true);
    apply.add_css_class("suggested-action");
    apply.add_css_class("pill");
    apply.set_valign(gtk4::Align::Center);
    expander.add_suffix(&apply);
    group.add(&expander);

    let feedback_row = write_feedback_row("PPD active profile");
    group.add(&feedback_row);

    let feedback_row_for_click = feedback_row.clone();
    let apply_for_click = apply.clone();
    let expander_for_click = expander.clone();
    let selected_for_click = selected.clone();

    apply.connect_clicked(move |_| {
        let requested = choices[selected_for_click.get() as usize].clone();
        feedback_row_for_click.set_title("Applying");
        feedback_row_for_click.set_subtitle("Sending to power-profiles-daemon...");
        apply_for_click.set_sensitive(false);
        expander_for_click.set_sensitive(false);

        let feedback_row_for_recv = feedback_row_for_click.clone();
        let apply_for_recv = apply_for_click.clone();
        let expander_for_recv = expander_for_click.clone();

        spawn_dbus_call(
            move || set_ppd_active_profile(&requested),
            move |result| {
                apply_for_recv.set_sensitive(true);
                expander_for_recv.set_sensitive(true);
                match result {
                    Ok(()) => {
                        feedback_row_for_recv.set_title("Applied");
                        feedback_row_for_recv
                            .set_subtitle("power-profiles-daemon accepted the profile change.");
                        store_write_feedback_state(
                            "PPD active profile",
                            "Applied",
                            "power-profiles-daemon accepted the profile change.",
                        );
                        let _ = request_dashboard_refresh();
                    }
                    Err(error) => {
                        let subtitle = format!("Failed: {error}");
                        feedback_row_for_recv.set_title("Apply error");
                        feedback_row_for_recv.set_subtitle(&subtitle);
                        store_write_feedback_state("PPD active profile", "Apply error", &subtitle);
                    }
                }
            },
        );
    });

    group
}

fn build_platform_profile_controls(
    capability: Option<&PlatformProfileCapability>,
    current_row: Option<adw::ActionRow>,
) -> adw::PreferencesGroup {
    build_write_controls(
        "Platform Control",
        capability.map(|capability| capability.current.as_deref().unwrap_or("unknown")),
        capability.map(|capability| capability.choices.as_slice()),
        "Requested profile",
        "Apply profile",
        "Platform profile",
        |requested| make_client().and_then(|client| client.set_platform_profile(requested)),
        move |_| {
            if !request_dashboard_refresh() {
                if let Some(row) = &current_row {
                    refresh_platform_profile_row(row);
                }
            }
        },
    )
}

fn build_cpu_governor_write_controls(cpu: Option<&CpuPowerCapability>) -> adw::PreferencesGroup {
    build_write_controls(
        "CPU Governor Control",
        cpu.map(|c| c.governor.as_deref().unwrap_or("unknown")),
        cpu.map(|c| c.available_governors.as_slice()),
        "Requested governor",
        "Apply governor",
        "CPU governor",
        |requested| make_client().and_then(|client| client.set_cpu_governor(requested)),
        move |_| {
            request_dashboard_refresh();
        },
    )
}

fn build_cpu_epp_write_controls(cpu: Option<&CpuPowerCapability>) -> adw::PreferencesGroup {
    build_write_controls(
        "CPU EPP Control",
        cpu.map(|c| c.epp.as_deref().unwrap_or("unknown")),
        cpu.map(|c| c.available_epp.as_slice()),
        "Requested EPP",
        "Apply EPP",
        "CPU EPP",
        |requested| make_client().and_then(|client| client.set_cpu_epp(requested)),
        move |_| {
            request_dashboard_refresh();
        },
    )
}

fn build_cpu_boost_write_controls(cpu: Option<&CpuPowerCapability>) -> adw::PreferencesGroup {
    let choices = vec!["0".to_owned(), "1".to_owned()];
    let current = cpu.and_then(|c| c.boost.map(|boost| if boost { "1" } else { "0" }));
    build_write_controls(
        "CPU Boost Control",
        current,
        cpu.map(|_| choices.as_slice()),
        "Requested boost",
        "Apply boost",
        "CPU boost",
        |requested| make_client().and_then(|client| client.set_cpu_boost(requested)),
        move |_| {
            request_dashboard_refresh();
        },
    )
}

fn build_curve_optimizer_controls() -> adw::PreferencesGroup {
    let choices = (-30..=0)
        .map(|offset| offset.to_string())
        .collect::<Vec<_>>();
    let group = build_write_controls(
        "Advanced CPU Tuning - Curve Optimizer",
        Some("write-only"),
        Some(choices.as_slice()),
        "All-core offset",
        "Apply CO offset",
        "Curve Optimizer",
        |requested| make_client().and_then(|client| client.set_curve_optimizer_all_core(requested)),
        move |_| {
            request_dashboard_refresh();
        },
    );
    append_curve_optimizer_reset_control(&group);
    group.add(&section_note(
        "Experimental: RyzenAdj Curve Optimizer writes are currently write-only on this machine without ryzen_smu read-back. Bad values can cause crashes, reboots, app instability, or silent performance loss. Use 0 to reset.",
    ));
    group
}

fn append_curve_optimizer_reset_control(group: &adw::PreferencesGroup) {
    let reset_row = adw::ActionRow::builder()
        .title("Reset Curve Optimizer")
        .subtitle("Applies all-core offset 0 through the same daemon and polkit write path.")
        .selectable(false)
        .build();
    let reset = gtk4::Button::builder()
        .label("Reset to 0")
        .css_classes(["pill"])
        .valign(gtk4::Align::Center)
        .build();
    reset_row.add_suffix(&reset);
    group.add(&reset_row);

    let feedback_row = write_feedback_row("Curve Optimizer reset");
    group.add(&feedback_row);

    reset.connect_clicked(move |button| {
        button.set_sensitive(false);
        feedback_row.set_title("Reset in progress");
        feedback_row
            .set_subtitle("Request sent to the daemon; waiting for policy/auth/write result...");

        let button_for_result = button.clone();
        let feedback_for_result = feedback_row.clone();
        spawn_dbus_call(
            || make_client().and_then(|client| client.set_curve_optimizer_all_core("0")),
            move |result| {
                button_for_result.set_sensitive(true);
                match result {
                    Ok(res) => {
                        let title = write_feedback_title(Some(&res));
                        let subtitle = write_feedback_subtitle(Some(&res));
                        feedback_for_result.set_title(title);
                        feedback_for_result.set_subtitle(&subtitle);
                        store_write_feedback_state("Curve Optimizer reset", title, &subtitle);
                        let _ = request_dashboard_refresh();
                    }
                    Err(error) => {
                        feedback_for_result.set_title("Apply error");
                        let subtitle =
                            format!("Failed - daemon call could not be completed: {error}");
                        feedback_for_result.set_subtitle(&subtitle);
                        store_write_feedback_state(
                            "Curve Optimizer reset",
                            "Apply error",
                            &subtitle,
                        );
                        let _ = request_dashboard_refresh();
                    }
                }
            },
        );
    });
}

fn append_advanced_cpu_tuning_controls(page: &adw::PreferencesPage, bundle: &DiagnosticsBundle) {
    if let Some(status) = &bundle.ryzen_backend_status {
        page.add(&build_ryzen_backend_status_group(status));
    }

    let gate_group = adw::PreferencesGroup::new();
    gate_group.set_title("Advanced CPU Tuning");
    gate_group.add(&section_note(
        "Disabled by default. Curve Optimizer writes can crash or destabilize the machine and remain write-only until ryzen_smu read-back exists.",
    ));

    let gate_row = adw::ActionRow::builder()
        .title("Show Curve Optimizer controls")
        .subtitle("Reveals experimental all-core negative PBO controls for this session only.")
        .selectable(false)
        .build();
    let gate_switch = gtk4::Switch::new();
    gate_switch.set_active(false);
    gate_switch.set_valign(gtk4::Align::Center);
    gate_row.add_suffix(&gate_switch);
    gate_group.add(&gate_row);

    let controls = build_curve_optimizer_controls();
    controls.set_visible(false);
    let controls_for_switch = controls.clone();
    gate_switch.connect_active_notify(move |switch| {
        controls_for_switch.set_visible(switch.is_active());
    });

    page.add(&gate_group);
    page.add(&controls);
}

fn build_ryzen_backend_status_group(status: &RyzenBackendStatus) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Ryzen Backend Status");
    group.add(&section_note(
        "Read-only backend detection for advanced CPU tuning. RatVantage reports setup commands only; it does not install or load kernel modules automatically.",
    ));
    group.add(&info_row(
        "Curve Optimizer backend",
        &status.curve_optimizer_backend,
    ));
    group.add(&info_row(
        "Curve Optimizer read-back",
        match status.curve_optimizer_readback_status {
            legion_common::CurveOptimizerReadbackStatus::WriteOnly => "write-only",
            legion_common::CurveOptimizerReadbackStatus::Verified => "available",
            legion_common::CurveOptimizerReadbackStatus::Failed => "failed",
        },
    ));
    group.add(&info_row(
        "RyzenAdj",
        if status.ryzenadj.supports_curve_optimizer {
            "available"
        } else if status.ryzenadj.available {
            "not executable"
        } else {
            "missing"
        },
    ));
    group.add(&info_row("RyzenAdj path", &status.ryzenadj.path));
    group.add(&info_row(
        "ryzen_smu",
        if status.ryzen_smu.readback_available {
            "read-back surface present"
        } else if status.ryzen_smu.module_loaded || status.ryzen_smu.sysfs_available {
            "partial"
        } else {
            "missing"
        },
    ));
    group.add(&info_row("ryzen_smu sysfs", &status.ryzen_smu.sysfs_path));

    let setup = adw::ExpanderRow::builder()
        .title("ryzen_smu setup assistant")
        .subtitle(status.setup_assistant.reason.as_str())
        .build();
    for command in &status.setup_assistant.commands {
        setup.add_row(
            &adw::ActionRow::builder()
                .title(command.as_str())
                .selectable(true)
                .build(),
        );
    }
    for note in &status.setup_assistant.notes {
        setup.add_row(
            &adw::ActionRow::builder()
                .title("Note")
                .subtitle(note.as_str())
                .selectable(false)
                .build(),
        );
    }
    group.add(&setup);
    group
}

fn build_hardware_profile_apply_controls(bundle: &DiagnosticsBundle) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Saved Hardware Profiles");
    group.add(&section_note(
        "Manual apply validates the stored profile first, then executes daemon-gated actions in order and stops on the first non-applied result.",
    ));

    if bundle.hardware_profiles.is_empty() {
        group.add(&info_row("Profiles", "none saved"));
    } else {
        for (profile_id, profile) in &bundle.hardware_profiles {
            let action_count = profile.actions.firmware_attributes.len()
                + profile.actions.platform_profile.is_some() as usize
                + profile.actions.battery_charge_type.is_some() as usize
                + profile.actions.cpu_governor.is_some() as usize
                + profile.actions.cpu_epp.is_some() as usize
                + profile.actions.cpu_boost.is_some() as usize
                + profile.actions.conservation_mode.is_some() as usize
                + profile.actions.amd_gpu_dpm_force_level.is_some() as usize
                + profile.actions.curve_optimizer_all_core.is_some() as usize;
            let row = adw::ActionRow::builder()
                .title(profile.label.as_str())
                .subtitle(format!("{profile_id} · {action_count} action(s)"))
                .build();
            let apply = gtk4::Button::with_label("Apply");
            apply.add_css_class("suggested-action");
            apply.add_css_class("pill");
            apply.set_valign(gtk4::Align::Center);
            row.add_suffix(&apply);
            group.add(&row);

            let feedback = write_feedback_row("Hardware profile");
            group.add(&feedback);

            let profile_id_for_click = profile_id.clone();
            let apply_for_click = apply.clone();
            let row_for_click = row.clone();
            let feedback_for_click = feedback.clone();
            apply.connect_clicked(move |_| {
                feedback_for_click.set_title("Applying");
                feedback_for_click.set_subtitle("Sending hardware profile apply to daemon...");
                apply_for_click.set_sensitive(false);
                row_for_click.set_sensitive(false);

                let profile_id_for_call = profile_id_for_click.clone();
                let apply_for_recv = apply_for_click.clone();
                let row_for_recv = row_for_click.clone();
                let feedback_for_recv = feedback_for_click.clone();
                spawn_dbus_call(
                    move || {
                        make_client()
                            .and_then(|client| client.apply_hardware_profile(&profile_id_for_call))
                    },
                    move |result| {
                        apply_for_recv.set_sensitive(true);
                        row_for_recv.set_sensitive(true);
                        match result {
                            Ok(run) => {
                                let subtitle = format!(
                                    "{}; {} action result(s)",
                                    run.message,
                                    run.results.len()
                                );
                                feedback_for_recv.set_title(if run.completed {
                                    "Applied"
                                } else {
                                    "Apply stopped"
                                });
                                feedback_for_recv.set_subtitle(&subtitle);
                                store_write_feedback_state(
                                    "Hardware profile",
                                    if run.completed {
                                        "Applied"
                                    } else {
                                        "Apply stopped"
                                    },
                                    &subtitle,
                                );
                                let _ = request_dashboard_refresh();
                            }
                            Err(error) => {
                                let subtitle = format!("Failed: {error}");
                                feedback_for_recv.set_title("Apply error");
                                feedback_for_recv.set_subtitle(&subtitle);
                                store_write_feedback_state(
                                    "Hardware profile",
                                    "Apply error",
                                    &subtitle,
                                );
                            }
                        }
                    },
                );
            });
        }
    }

    if let Some(run) = &bundle.last_hardware_profile_apply {
        group.add(&info_row(
            "Last apply",
            &format!(
                "{} · {} · {} action result(s)",
                run.profile_id,
                if run.completed {
                    "completed"
                } else {
                    "stopped"
                },
                run.results.len()
            ),
        ));
    }

    group
}

fn build_firmware_attribute_controls(
    attributes: &[FirmwareAttributeCapability],
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Firmware Power Limits (TDP)");
    group.add(&section_note(
        "Writes are polkit-gated and limited to validated 82WM PPT scalar attributes.",
    ));

    for attr in attributes.iter().filter(|attr| {
        SUPPORTED_FIRMWARE_SCALAR_ATTRIBUTES
            .iter()
            .any(|supported| supported == &attr.name)
    }) {
        let Some((current, min, max, step)) = parse_firmware_attribute_numbers(attr) else {
            let label = attr.display_name.as_deref().unwrap_or(&attr.name);
            group.add(&info_row(label, "metadata incomplete"));
            continue;
        };

        let row = adw::ActionRow::builder()
            .title(attr.display_name.as_deref().unwrap_or(&attr.name))
            .subtitle(format!(
                "{} · range {}..{} step {}",
                attr.name, min, max, step
            ))
            .build();
        let spin = gtk4::SpinButton::with_range(min as f64, max as f64, step as f64);
        spin.set_value(current as f64);
        spin.set_digits(0);
        spin.set_numeric(true);
        spin.set_width_chars(5);

        let apply = gtk4::Button::with_label("Apply");
        apply.add_css_class("suggested-action");
        apply.add_css_class("pill");
        apply.set_valign(gtk4::Align::Center);

        row.add_suffix(&spin);
        row.add_suffix(&apply);
        group.add(&row);

        let feedback = write_feedback_row("Firmware attribute");
        group.add(&feedback);

        let attribute_id = attr.name.clone();
        let spin_for_click = spin.clone();
        let apply_for_click = apply.clone();
        let row_for_click = row.clone();
        let feedback_for_click = feedback.clone();
        apply.connect_clicked(move |_| {
            let requested = format!("{:.0}", spin_for_click.value());
            feedback_for_click.set_title("Applying");
            feedback_for_click.set_subtitle("Sending firmware attribute write to daemon...");
            apply_for_click.set_sensitive(false);
            row_for_click.set_sensitive(false);

            let attribute_id_for_call = attribute_id.clone();
            let feedback_for_recv = feedback_for_click.clone();
            let apply_for_recv = apply_for_click.clone();
            let row_for_recv = row_for_click.clone();
            spawn_dbus_call(
                move || {
                    make_client().and_then(|client| {
                        client.set_firmware_attribute(&attribute_id_for_call, &requested)
                    })
                },
                move |result| {
                    apply_for_recv.set_sensitive(true);
                    row_for_recv.set_sensitive(true);
                    match result {
                        Ok(execution) => {
                            feedback_for_recv.set_title("Apply result");
                            feedback_for_recv.set_subtitle(
                                &super::shared::write_feedback_subtitle(Some(&execution)),
                            );
                            store_write_feedback_state(
                                "Firmware attribute",
                                "Apply result",
                                &super::shared::write_feedback_subtitle(Some(&execution)),
                            );
                            let _ = request_dashboard_refresh();
                        }
                        Err(error) => {
                            let subtitle = format!("Failed: {error}");
                            feedback_for_recv.set_title("Apply error");
                            feedback_for_recv.set_subtitle(&subtitle);
                            store_write_feedback_state(
                                "Firmware attribute",
                                "Apply error",
                                &subtitle,
                            );
                        }
                    }
                },
            );
        });
    }

    group
}

fn parse_firmware_attribute_numbers(
    attr: &FirmwareAttributeCapability,
) -> Option<(i64, i64, i64, i64)> {
    Some((
        attr.current_value.as_deref()?.parse().ok()?,
        attr.min_value.as_deref()?.parse().ok()?,
        attr.max_value.as_deref()?.parse().ok()?,
        attr.scalar_increment.as_deref()?.parse().ok()?,
    ))
}

fn refresh_platform_profile_row(row: &adw::ActionRow) {
    let row = row.clone();
    spawn_dbus_call(
        || make_client().and_then(|client| client.refresh_runtime_snapshot()),
        move |result| {
            if let Ok(snapshot) = result {
                if let Some(profile) = snapshot.diagnostics.raw_probe_report.platform_profile {
                    row.set_subtitle(profile.current.as_deref().unwrap_or("unknown"));
                }
            }
        },
    );
}
