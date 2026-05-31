use crate::DiagnosticsBundle;
use adw::prelude::*;
use anyhow::Result;
use legion_common::HARDWARE_PROFILE_TRIGGER_IDS;

use super::shared::{
    append_error, make_client, request_dashboard_refresh, section_note, spawn_dbus_call,
    store_write_feedback_state, write_feedback_row,
};

pub fn automations_page(diagnostics: Result<DiagnosticsBundle>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    // ── Planning banner ──────────────────────────────────────────────────────
    let info_group = adw::PreferencesGroup::new();
    info_group.set_title("Automation Engine");
    info_group.set_description(Some(
        "Hardware profile triggers map selected system events to daemon-owned hardware profiles. \
         Trigger execution is explicit and still uses the same daemon policy, polkit, validators, \
         and per-action read-back behavior as manual profile apply.",
    ));
    page.add(&info_group);

    match diagnostics {
        Ok(bundle) => {
            append_hardware_profile_triggers(&page, &bundle);
            append_persisted_automation_rules(&page, &bundle);
        }
        Err(error) => append_error(&page, &error),
    }

    // ── Seed rules ───────────────────────────────────────────────────────────
    let rules_group = adw::PreferencesGroup::new();
    rules_group.set_title("Automation Rules");
    rules_group.set_description(Some(
        "Full multi-condition rules remain planned. Hardware-profile trigger mappings above are persisted by the daemon now.",
    ));

    let add_row = adw::ActionRow::builder()
        .title("New automation rule")
        .subtitle(
            "Create a rule to automate power profiles, fan presets, and more on system events",
        )
        .selectable(false)
        .build();
    let add_btn = gtk4::Button::builder()
        .label("Add rule")
        .css_classes(["suggested-action", "pill"])
        .valign(gtk4::Align::Center)
        .build();
    add_btn.connect_clicked(|_| {
        // TODO: open rule-editor dialog once the backend ships
    });
    add_row.add_suffix(&add_btn);
    rules_group.add(&add_row);

    type SeedRule<'a> = (&'a str, &'a str, Option<&'a str>, &'a str, bool, &'a str);
    let seed_rules: &[SeedRule<'_>] = &[
        (
            "Power saver at night",
            "Schedule · 23:00 daily",
            None,
            "Profile → Quiet · Charge → Conservation · Notify",
            true,
            "yesterday 23:00 · OK",
        ),
        (
            "Performance on AC",
            "AC plugged in",
            Some("Battery above 20%"),
            "Profile → Balanced · Fan preset → Balanced daily",
            true,
            "2 hours ago · OK",
        ),
        (
            "Gaming mode (high GPU load)",
            "Game detected",
            Some("AC connected"),
            "Profile → Performance · Fan preset → Performance sustained",
            false,
            "Never run",
        ),
        (
            "Quiet on battery below 30%",
            "Battery below 30%",
            Some("AC disconnected"),
            "Profile → Quiet · Fan preset → Quiet desk · Notify",
            true,
            "3 days ago · OK",
        ),
    ];

    for (name, trigger, condition, actions, enabled, last_run) in seed_rules {
        let expander = adw::ExpanderRow::new();
        expander.set_title(name);
        expander.set_subtitle(&format!("When: {trigger}"));

        let switch = gtk4::Switch::new();
        switch.set_active(*enabled);
        switch.set_valign(gtk4::Align::Center);
        expander.add_suffix(&switch);

        if let Some(cond) = condition {
            expander.add_row(
                &adw::ActionRow::builder()
                    .title("Condition")
                    .subtitle(*cond)
                    .selectable(false)
                    .build(),
            );
        }
        expander.add_row(
            &adw::ActionRow::builder()
                .title("Actions")
                .subtitle(*actions)
                .selectable(false)
                .build(),
        );
        expander.add_row(
            &adw::ActionRow::builder()
                .title("Last run")
                .subtitle(*last_run)
                .selectable(false)
                .build(),
        );

        rules_group.add(&expander);
    }

    page.add(&rules_group);

    // ── Quick templates ──────────────────────────────────────────────────────
    let templates_group = adw::PreferencesGroup::new();
    templates_group.set_title("Quick Templates");
    templates_group.set_description(Some(
        "One-click starting points. The fast-charge starter creates real daemon-owned profiles; full threshold rules remain the next backend slice.",
    ));

    append_fast_charge_profile_starter(&templates_group);

    let templates: &[(&str, &str, &str)] = &[
        (
            "Quiet at night",
            "Schedule · 23:00 daily",
            "Profile → Quiet · Charge → Conservation",
        ),
        (
            "Performance on AC plug-in",
            "AC plugged in",
            "Profile → Balanced · Fan → Balanced daily",
        ),
        (
            "Quiet on battery below 20%",
            "Battery below 20%",
            "Profile → Quiet · Notify",
        ),
        (
            "Gaming mode on high GPU load",
            "Game detected · AC connected",
            "Profile → Performance · Fan → Performance sustained",
        ),
        (
            "Reset to balanced on lid open",
            "Lid opens",
            "Profile → Balanced",
        ),
        (
            "Conservation charge overnight",
            "Schedule · 23:00 daily",
            "Charge → Conservation",
        ),
        (
            "Notify on thermal spike above 85°C",
            "CPU temp above 85°C",
            "Send desktop notification",
        ),
        (
            "Delay suspend fan spin-down",
            "System suspends",
            "Fan preset → Quiet desk · Wait 30 s",
        ),
    ];

    for (name, trigger, actions) in templates {
        let row = adw::ActionRow::builder()
            .title(*name)
            .subtitle(format!("Trigger: {trigger} · Actions: {actions}"))
            .selectable(false)
            .build();
        let use_btn = gtk4::Button::builder()
            .label("Use template")
            .css_classes(["flat"])
            .valign(gtk4::Align::Center)
            .build();
        use_btn.connect_clicked(|_| {
            // TODO: open rule editor pre-filled with template
        });
        row.add_suffix(&use_btn);
        templates_group.add(&row);
    }

    page.add(&templates_group);

    // ── Available triggers ───────────────────────────────────────────────────
    let triggers_group = adw::PreferencesGroup::new();
    triggers_group.set_title("Available Triggers");
    triggers_group.set_description(Some("Events that can start an automation rule"));

    let trigger_categories: &[(&str, &str)] = &[
        ("Schedule", "Time of day · Time window · Day of week"),
        (
            "Power",
            "AC plugged in · AC unplugged · Battery below % · Battery above %",
        ),
        (
            "Thermal",
            "CPU temp above / below °C · Fan RPM above threshold",
        ),
        (
            "System",
            "Suspends · Resumes · Lid closes · Lid opens · External display connected/removed \
             · Network connected/disconnected · Bluetooth device connected",
        ),
        (
            "Application",
            "App launches · App closes · Game detected (high GPU load) \
             · Session goes idle · Session activity resumes",
        ),
        (
            "Manual",
            "One-shot trigger for testing rules on demand — no system event required",
        ),
    ];

    for (category, items) in trigger_categories {
        triggers_group.add(
            &adw::ActionRow::builder()
                .title(*category)
                .subtitle(*items)
                .selectable(false)
                .build(),
        );
    }

    page.add(&triggers_group);

    // ── Available actions ────────────────────────────────────────────────────
    let actions_group = adw::PreferencesGroup::new();
    actions_group.set_title("Available Actions");
    actions_group.set_description(Some(
        "Hardware and system operations a rule can perform, executed in order",
    ));

    let action_items: &[(&str, &str)] = &[
        (
            "Set power profile",
            "Switch to quiet, balanced, or performance via platform_profile",
        ),
        (
            "Set fan preset",
            "Apply a packaged fan curve preset — dry-run plan gating applies",
        ),
        (
            "Set battery charge type",
            "Switch to standard, conservation, or express charging",
        ),
        ("Set Y-logo LED", "Toggle on/off and set brightness 0–100%"),
        ("Set Fn lock", "Enable or disable Fn key lock"),
        (
            "Set USB charging",
            "Toggle USB always-on charging when lid is closed",
        ),
        ("Set camera power", "Toggle camera power"),
        (
            "Send desktop notification",
            "Show a message via the system notification daemon",
        ),
        (
            "Wait",
            "Pause a set number of seconds before the next action",
        ),
        (
            "Run shell command",
            "Execute a user shell command — no root access; runs under daemon user context",
        ),
    ];

    for (name, desc) in action_items {
        actions_group.add(
            &adw::ActionRow::builder()
                .title(*name)
                .subtitle(*desc)
                .selectable(false)
                .build(),
        );
    }

    page.add(&actions_group);

    page
}

fn append_fast_charge_profile_starter(group: &adw::PreferencesGroup) {
    let row = adw::ActionRow::builder()
        .title("Fast charge starter")
        .subtitle(
            "Creates Fast charge and Battery protect hardware profiles, then maps AC plugged in to Fast charge",
        )
        .selectable(false)
        .build();
    let create = gtk4::Button::builder()
        .label("Create profiles")
        .css_classes(["suggested-action", "pill"])
        .valign(gtk4::Align::Center)
        .build();
    row.add_suffix(&create);
    group.add(&row);

    let feedback = write_feedback_row("Fast charge starter");
    group.add(&feedback);

    create.connect_clicked(move |button| {
        button.set_sensitive(false);
        feedback.set_title("Creating profiles");
        feedback.set_subtitle("Saving daemon-owned fast-charge/protect profiles...");

        let button_for_recv = button.clone();
        let feedback_for_recv = feedback.clone();
        spawn_dbus_call(
            || {
                let client = make_client()?;
                let fast_charge = serde_json::json!({
                    "schema_version": 1,
                    "label": "Fast charge",
                    "actions": {
                        "battery_charge_type": "Fast"
                    }
                })
                .to_string();
                let battery_protect = serde_json::json!({
                    "schema_version": 1,
                    "label": "Battery protect",
                    "actions": {
                        "battery_charge_type": "Conservation"
                    }
                })
                .to_string();

                client.set_hardware_profile("fast_charge", &fast_charge)?;
                client.set_hardware_profile("battery_protect", &battery_protect)?;
                client.set_hardware_profile_trigger("ac_connected", "fast_charge")?;
                let rule = serde_json::json!({
                    "schema_version": 1,
                    "label": "Fast charge until 80%",
                    "enabled": true,
                    "kind": "fast_charge_until_threshold",
                    "threshold_percent": 80,
                    "fast_charge_profile_id": "fast_charge",
                    "protect_profile_id": "battery_protect",
                    "require_ac": true
                })
                .to_string();
                client.set_automation_rule("fast_charge_until_80", &rule)?;
                Ok(())
            },
            move |result| {
                button_for_recv.set_sensitive(true);
                match result {
                    Ok(()) => {
                        feedback_for_recv.set_title("Profiles ready");
                        feedback_for_recv.set_subtitle(
                            "Saved fast_charge/battery_protect profiles and fast_charge_until_80 rule.",
                        );
                        store_write_feedback_state(
                            "Fast charge starter",
                            "Profiles ready",
                            "Saved fast_charge/battery_protect profiles and fast_charge_until_80 rule.",
                        );
                        let _ = request_dashboard_refresh();
                    }
                    Err(error) => {
                        let subtitle = format!("Failed: {error}");
                        feedback_for_recv.set_title("Create error");
                        feedback_for_recv.set_subtitle(&subtitle);
                        store_write_feedback_state(
                            "Fast charge starter",
                            "Create error",
                            &subtitle,
                        );
                    }
                }
            },
        );
    });
}

fn append_hardware_profile_triggers(page: &adw::PreferencesPage, bundle: &DiagnosticsBundle) {
    let group = adw::PreferencesGroup::new();
    group.set_title("Hardware Profile Triggers");
    group.add(&section_note(
        "Map supported triggers to saved hardware profiles. Applying a mapped trigger runs through the daemon and stops on the first non-applied action.",
    ));

    if bundle.hardware_profiles.is_empty() {
        group.add(
            &adw::ActionRow::builder()
                .title("Saved profiles")
                .subtitle("No daemon-owned hardware profiles are saved yet")
                .selectable(false)
                .build(),
        );
        page.add(&group);
        return;
    }

    for trigger_id in HARDWARE_PROFILE_TRIGGER_IDS {
        let mapped_profile = bundle
            .hardware_profile_triggers
            .get(*trigger_id)
            .map(String::as_str);
        let expander = adw::ExpanderRow::new();
        expander.set_title(&trigger_label(trigger_id));
        expander.set_subtitle(mapped_profile.unwrap_or("unassigned"));

        if mapped_profile.is_some() {
            let apply = gtk4::Button::builder()
                .label("Run now")
                .css_classes(["suggested-action", "pill"])
                .valign(gtk4::Align::Center)
                .build();
            expander.add_suffix(&apply);
            let trigger_for_click = (*trigger_id).to_owned();
            apply.connect_clicked(move |button| {
                button.set_sensitive(false);
                let button_for_recv = button.clone();
                let trigger_for_call = trigger_for_click.clone();
                spawn_dbus_call(
                    move || {
                        make_client().and_then(|client| {
                            client.apply_hardware_profile_trigger(&trigger_for_call)
                        })
                    },
                    move |result| {
                        button_for_recv.set_sensitive(true);
                        match result {
                            Ok(run) => {
                                let subtitle = format!(
                                    "{}; {} action result(s)",
                                    run.message,
                                    run.results.len()
                                );
                                store_write_feedback_state(
                                    "Hardware profile trigger",
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
                                store_write_feedback_state(
                                    "Hardware profile trigger",
                                    "Apply error",
                                    &format!("Failed: {error}"),
                                );
                            }
                        }
                    },
                );
            });
        }

        for (profile_id, profile) in &bundle.hardware_profiles {
            let row = adw::ActionRow::builder()
                .title(profile.label.as_str())
                .subtitle(profile_id.as_str())
                .selectable(false)
                .build();
            let use_button = gtk4::Button::builder()
                .label(if mapped_profile == Some(profile_id.as_str()) {
                    "Mapped"
                } else {
                    "Use"
                })
                .css_classes(["flat"])
                .valign(gtk4::Align::Center)
                .build();
            use_button.set_sensitive(mapped_profile != Some(profile_id.as_str()));
            row.add_suffix(&use_button);
            expander.add_row(&row);

            let trigger_for_click = (*trigger_id).to_owned();
            let profile_for_click = profile_id.clone();
            use_button.connect_clicked(move |button| {
                button.set_sensitive(false);
                let trigger_for_call = trigger_for_click.clone();
                let profile_for_call = profile_for_click.clone();
                spawn_dbus_call(
                    move || {
                        make_client().and_then(|client| {
                            client
                                .set_hardware_profile_trigger(&trigger_for_call, &profile_for_call)
                        })
                    },
                    move |_| {
                        let _ = request_dashboard_refresh();
                    },
                );
            });
        }

        if mapped_profile.is_some() {
            let clear_row = adw::ActionRow::builder()
                .title("Clear mapping")
                .subtitle("Leave this trigger unassigned")
                .selectable(false)
                .build();
            let clear = gtk4::Button::builder()
                .label("Clear")
                .css_classes(["destructive-action", "pill"])
                .valign(gtk4::Align::Center)
                .build();
            clear_row.add_suffix(&clear);
            expander.add_row(&clear_row);
            let trigger_for_click = (*trigger_id).to_owned();
            clear.connect_clicked(move |button| {
                button.set_sensitive(false);
                let trigger_for_call = trigger_for_click.clone();
                spawn_dbus_call(
                    move || {
                        make_client().and_then(|client| {
                            client.remove_hardware_profile_trigger(&trigger_for_call)
                        })
                    },
                    move |_| {
                        let _ = request_dashboard_refresh();
                    },
                );
            });
        }

        group.add(&expander);
    }

    group.add(&write_feedback_row("Hardware profile trigger"));
    page.add(&group);
}

fn append_persisted_automation_rules(page: &adw::PreferencesPage, bundle: &DiagnosticsBundle) {
    let group = adw::PreferencesGroup::new();
    group.set_title("Saved Automation Rules");
    group.add(&section_note(
        "Rules are daemon-owned. Preview refreshes telemetry, selects a hardware profile, and test run applies that profile through the normal daemon write path.",
    ));

    if bundle.automation_rules.is_empty() {
        group.add(
            &adw::ActionRow::builder()
                .title("Rules")
                .subtitle("No automation rules saved yet")
                .selectable(false)
                .build(),
        );
        page.add(&group);
        return;
    }

    for (rule_id, rule) in &bundle.automation_rules {
        let row = adw::ActionRow::builder()
            .title(rule.label.as_str())
            .subtitle(format!(
                "{rule_id} · {}",
                if rule.enabled { "enabled" } else { "disabled" }
            ))
            .selectable(false)
            .build();
        let preview = gtk4::Button::builder()
            .label("Preview")
            .css_classes(["flat"])
            .valign(gtk4::Align::Center)
            .build();
        let run = gtk4::Button::builder()
            .label("Test run")
            .css_classes(["suggested-action", "pill"])
            .valign(gtk4::Align::Center)
            .build();
        row.add_suffix(&preview);
        row.add_suffix(&run);
        group.add(&row);

        let rule_id_for_preview = rule_id.clone();
        preview.connect_clicked(move |button| {
            button.set_sensitive(false);
            let button_for_recv = button.clone();
            let rule_id_for_call = rule_id_for_preview.clone();
            spawn_dbus_call(
                move || {
                    make_client()
                        .and_then(|client| client.automation_rule_preview(&rule_id_for_call))
                },
                move |result| {
                    button_for_recv.set_sensitive(true);
                    match result {
                        Ok(evaluation) => {
                            store_write_feedback_state(
                                "Automation rule",
                                if evaluation.matched {
                                    "Matched"
                                } else {
                                    "Skipped"
                                },
                                &evaluation.reason,
                            );
                            let _ = request_dashboard_refresh();
                        }
                        Err(error) => {
                            store_write_feedback_state(
                                "Automation rule",
                                "Preview error",
                                &format!("Failed: {error}"),
                            );
                        }
                    }
                },
            );
        });

        let rule_id_for_run = rule_id.clone();
        run.connect_clicked(move |button| {
            button.set_sensitive(false);
            let button_for_recv = button.clone();
            let rule_id_for_call = rule_id_for_run.clone();
            spawn_dbus_call(
                move || {
                    make_client().and_then(|client| client.apply_automation_rule(&rule_id_for_call))
                },
                move |result| {
                    button_for_recv.set_sensitive(true);
                    match result {
                        Ok(run) => {
                            let title = if run.profile_run.is_some() {
                                "Test run complete"
                            } else {
                                "Test run skipped"
                            };
                            store_write_feedback_state(
                                "Automation rule",
                                title,
                                &run.evaluation.reason,
                            );
                            let _ = request_dashboard_refresh();
                        }
                        Err(error) => {
                            store_write_feedback_state(
                                "Automation rule",
                                "Test run error",
                                &format!("Failed: {error}"),
                            );
                        }
                    }
                },
            );
        });
    }

    group.add(&write_feedback_row("Automation rule"));
    page.add(&group);
}

fn trigger_label(trigger_id: &str) -> String {
    match trigger_id {
        "ac_connected" => "AC plugged in".to_owned(),
        "ac_disconnected" => "AC unplugged".to_owned(),
        "resume" => "Resume from sleep".to_owned(),
        "platform_profile_changed" => "Platform profile changed".to_owned(),
        "manual" => "Manual test trigger".to_owned(),
        other => other.replace('_', " "),
    }
}
