use crate::DiagnosticsBundle;
use adw::prelude::*;
use anyhow::Result;
use legion_common::{
    AutomationRuleKind, CurveOptimizerReadbackStatus, HardwareProfile, RyzenBackendStatus,
    HARDWARE_PROFILE_TRIGGER_IDS,
};

use super::shared::{
    append_error, make_client, request_dashboard_refresh, section_note, selected_dropdown_value,
    spawn_dbus_call, store_write_feedback_state, write_feedback_row,
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
            append_recent_platform_profile_changes(&page, &bundle);
            append_recent_desktop_power_profile_changes(&page, &bundle);
            append_persisted_automation_rules(&page, &bundle);
            append_automation_rule_creator(&page, &bundle);
            append_ac_router_rule_creator(&page, &bundle);
            append_fast_charge_rule_creator(&page, &bundle);
        }
        Err(error) => append_error(&page, &error),
    }

    // ── Seed rules ───────────────────────────────────────────────────────────
    let rules_group = adw::PreferencesGroup::new();
    rules_group.set_title("Planned Rule Types");
    rules_group.set_description(Some(
        "Saved rules above are live and editable. These examples remain planned multi-condition templates.",
    ));

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
        "One-click starting points that save daemon-owned profiles and rules through the normal validation path.",
    ));

    append_fast_charge_profile_starter(&templates_group);
    append_balanced_daily_mixed_profile_starter(&templates_group);
    append_quiet_battery_mixed_profile_starter(&templates_group);
    append_performance_ac_mixed_profile_starter(&templates_group);
    append_ac_profile_router_starter(&templates_group);
    append_ac_cpu_profile_router_starter(&templates_group);
    append_quiet_on_battery_starter(&templates_group);
    append_battery_threshold_profile_starter(&templates_group);
    append_integrated_gpu_on_battery_starter(&templates_group);
    append_resume_balanced_profile_starter(&templates_group);
    append_gpu_reboot_completion_starter(&templates_group);
    append_profile_change_tuning_starter(&templates_group);
    append_rgb_breathing_profile_starter(&templates_group);
    append_co_experimental_profile_starter(&templates_group);

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
            "Integrated GPU on battery",
            "AC unplugged",
            "GPU → Integrated · Reboot-gated profile",
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
            "AC plugged in · AC unplugged · Battery below % · Battery above % · Desktop power profile changes",
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
                    "require_ac": true,
                    "cooldown_secs": 300
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

fn append_balanced_daily_mixed_profile_starter(group: &adw::PreferencesGroup) {
    let row = adw::ActionRow::builder()
        .title("Balanced daily mixed profile starter")
        .subtitle(
            "Creates a balanced daily profile with platform, charge, CPU, AMD GPU DPM, staged RGB, and balanced fan-preset mapping",
        )
        .selectable(false)
        .build();
    let create = gtk4::Button::builder()
        .label("Create mixed")
        .css_classes(["suggested-action", "pill"])
        .valign(gtk4::Align::Center)
        .build();
    row.add_suffix(&create);
    group.add(&row);

    let feedback = write_feedback_row("Balanced daily mixed profile starter");
    group.add(&feedback);

    create.connect_clicked(move |button| {
        button.set_sensitive(false);
        feedback.set_title("Creating mixed profile");
        feedback.set_subtitle(
            "Saving daemon-owned balanced daily profile and fan-preset mapping...",
        );

        let button_for_recv = button.clone();
        let feedback_for_recv = feedback.clone();
        spawn_dbus_call(
            || {
                let client = make_client()?;
                let mixed_profile = serde_json::json!({
                    "schema_version": 1,
                    "label": "Balanced daily mixed",
                    "actions": {
                        "platform_profile": "balanced",
                        "battery_charge_type": "Standard",
                        "cpu_governor": "powersave",
                        "cpu_epp": "balance_performance",
                        "cpu_boost": "1",
                        "amd_gpu_dpm_force_level": "auto",
                        "keyboard_rgb": {
                            "effect": "Breathing",
                            "colors": {
                                "left_side": "#333333",
                                "left_center": "#333333",
                                "right_center": "#333333",
                                "right_side": "#333333"
                            },
                            "brightness": 40,
                            "speed": 30
                        }
                    }
                })
                .to_string();

                client.set_hardware_profile("balanced_daily_mixed", &mixed_profile)?;
                client.set_fan_preset_profile_map_entry("balanced", "balanced-daily")?;
                Ok(())
            },
            move |result| {
                button_for_recv.set_sensitive(true);
                match result {
                    Ok(()) => {
                        feedback_for_recv.set_title("Mixed profile ready");
                        feedback_for_recv.set_subtitle(
                            "Saved balanced_daily_mixed profile and balanced -> balanced-daily fan preset mapping.",
                        );
                        store_write_feedback_state(
                            "Balanced daily mixed profile starter",
                            "Mixed profile ready",
                            "Saved balanced_daily_mixed profile and balanced -> balanced-daily fan preset mapping.",
                        );
                        let _ = request_dashboard_refresh();
                    }
                    Err(error) => {
                        let subtitle = format!("Failed: {error}");
                        feedback_for_recv.set_title("Create error");
                        feedback_for_recv.set_subtitle(&subtitle);
                        store_write_feedback_state(
                            "Balanced daily mixed profile starter",
                            "Create error",
                            &subtitle,
                        );
                    }
                }
            },
        );
    });
}

fn append_quiet_battery_mixed_profile_starter(group: &adw::PreferencesGroup) {
    let row = adw::ActionRow::builder()
        .title("Quiet battery mixed profile starter")
        .subtitle(
            "Creates a quiet battery profile with low-power platform, conservation charge, CPU efficiency, AMD GPU DPM low, staged RGB, and quiet fan-preset mapping",
        )
        .selectable(false)
        .build();
    let create = gtk4::Button::builder()
        .label("Create quiet mixed")
        .css_classes(["suggested-action", "pill"])
        .valign(gtk4::Align::Center)
        .build();
    row.add_suffix(&create);
    group.add(&row);

    let feedback = write_feedback_row("Quiet battery mixed profile starter");
    group.add(&feedback);

    create.connect_clicked(move |button| {
        button.set_sensitive(false);
        feedback.set_title("Creating quiet mixed profile");
        feedback.set_subtitle("Saving daemon-owned quiet battery profile and fan-preset mapping...");

        let button_for_recv = button.clone();
        let feedback_for_recv = feedback.clone();
        spawn_dbus_call(
            || {
                let client = make_client()?;
                let mixed_profile = serde_json::json!({
                    "schema_version": 1,
                    "label": "Quiet battery mixed",
                    "actions": {
                        "platform_profile": "low-power",
                        "battery_charge_type": "Conservation",
                        "cpu_governor": "powersave",
                        "cpu_epp": "power",
                        "cpu_boost": "0",
                        "amd_gpu_dpm_force_level": "low",
                        "keyboard_rgb": {
                            "effect": "Breathing",
                            "colors": {
                                "left_side": "#222222",
                                "left_center": "#222222",
                                "right_center": "#222222",
                                "right_side": "#222222"
                            },
                            "brightness": 20,
                            "speed": 20
                        }
                    }
                })
                .to_string();

                client.set_hardware_profile("quiet_battery_mixed", &mixed_profile)?;
                client.set_fan_preset_profile_map_entry("low-power", "quiet-office")?;
                Ok(())
            },
            move |result| {
                button_for_recv.set_sensitive(true);
                match result {
                    Ok(()) => {
                        feedback_for_recv.set_title("Quiet mixed profile ready");
                        feedback_for_recv.set_subtitle(
                            "Saved quiet_battery_mixed profile and low-power -> quiet-office fan preset mapping.",
                        );
                        store_write_feedback_state(
                            "Quiet battery mixed profile starter",
                            "Quiet mixed profile ready",
                            "Saved quiet_battery_mixed profile and low-power -> quiet-office fan preset mapping.",
                        );
                        let _ = request_dashboard_refresh();
                    }
                    Err(error) => {
                        let subtitle = format!("Failed: {error}");
                        feedback_for_recv.set_title("Create error");
                        feedback_for_recv.set_subtitle(&subtitle);
                        store_write_feedback_state(
                            "Quiet battery mixed profile starter",
                            "Create error",
                            &subtitle,
                        );
                    }
                }
            },
        );
    });
}

fn append_performance_ac_mixed_profile_starter(group: &adw::PreferencesGroup) {
    let row = adw::ActionRow::builder()
        .title("Performance AC mixed profile starter")
        .subtitle(
            "Creates a performance AC profile with performance platform, standard charge, CPU performance, AMD GPU DPM auto, staged RGB, AC trigger, and performance fan-preset mapping",
        )
        .selectable(false)
        .build();
    let create = gtk4::Button::builder()
        .label("Create performance mixed")
        .css_classes(["suggested-action", "pill"])
        .valign(gtk4::Align::Center)
        .build();
    row.add_suffix(&create);
    group.add(&row);

    let feedback = write_feedback_row("Performance AC mixed profile starter");
    group.add(&feedback);

    create.connect_clicked(move |button| {
        button.set_sensitive(false);
        feedback.set_title("Creating performance mixed profile");
        feedback.set_subtitle(
            "Saving daemon-owned performance AC profile, trigger, and fan-preset mapping...",
        );

        let button_for_recv = button.clone();
        let feedback_for_recv = feedback.clone();
        spawn_dbus_call(
            || {
                let client = make_client()?;
                let mixed_profile = serde_json::json!({
                    "schema_version": 1,
                    "label": "Performance AC mixed",
                    "actions": {
                        "platform_profile": "performance",
                        "battery_charge_type": "Standard",
                        "cpu_governor": "performance",
                        "cpu_epp": "performance",
                        "cpu_boost": "1",
                        "amd_gpu_dpm_force_level": "auto",
                        "keyboard_rgb": {
                            "effect": "Breathing",
                            "colors": {
                                "left_side": "#555555",
                                "left_center": "#555555",
                                "right_center": "#555555",
                                "right_side": "#555555"
                            },
                            "brightness": 60,
                            "speed": 40
                        }
                    }
                })
                .to_string();

                client.set_hardware_profile("performance_ac_mixed", &mixed_profile)?;
                client.set_hardware_profile_trigger("ac_connected", "performance_ac_mixed")?;
                client.set_fan_preset_profile_map_entry("performance", "performance-sustained")?;
                Ok(())
            },
            move |result| {
                button_for_recv.set_sensitive(true);
                match result {
                    Ok(()) => {
                        feedback_for_recv.set_title("Performance mixed profile ready");
                        feedback_for_recv.set_subtitle(
                            "Saved performance_ac_mixed profile, AC-connected trigger, and performance -> performance-sustained fan preset mapping.",
                        );
                        store_write_feedback_state(
                            "Performance AC mixed profile starter",
                            "Performance mixed profile ready",
                            "Saved performance_ac_mixed profile, AC-connected trigger, and performance -> performance-sustained fan preset mapping.",
                        );
                        let _ = request_dashboard_refresh();
                    }
                    Err(error) => {
                        let subtitle = format!("Failed: {error}");
                        feedback_for_recv.set_title("Create error");
                        feedback_for_recv.set_subtitle(&subtitle);
                        store_write_feedback_state(
                            "Performance AC mixed profile starter",
                            "Create error",
                            &subtitle,
                        );
                    }
                }
            },
        );
    });
}

fn append_ac_profile_router_starter(group: &adw::PreferencesGroup) {
    let row = adw::ActionRow::builder()
        .title("AC profile router starter")
        .subtitle(
            "Creates plugged-in and battery hardware profiles, then routes AC state to the matching profile",
        )
        .selectable(false)
        .build();
    let create = gtk4::Button::builder()
        .label("Create router")
        .css_classes(["suggested-action", "pill"])
        .valign(gtk4::Align::Center)
        .build();
    row.add_suffix(&create);
    group.add(&row);

    let feedback = write_feedback_row("AC profile router starter");
    group.add(&feedback);

    create.connect_clicked(move |button| {
        button.set_sensitive(false);
        feedback.set_title("Creating router");
        feedback.set_subtitle("Saving daemon-owned AC/battery profiles and router rule...");

        let button_for_recv = button.clone();
        let feedback_for_recv = feedback.clone();
        spawn_dbus_call(
            || {
                let client = make_client()?;
                let plugged_in = serde_json::json!({
                    "schema_version": 1,
                    "label": "Plugged in balanced",
                    "actions": {
                        "platform_profile": "balanced"
                    }
                })
                .to_string();
                let on_battery = serde_json::json!({
                    "schema_version": 1,
                    "label": "Battery saver",
                    "actions": {
                        "platform_profile": "low-power"
                    }
                })
                .to_string();

                client.set_hardware_profile("plugged_in_balanced", &plugged_in)?;
                client.set_hardware_profile("battery_saver", &on_battery)?;
                client.set_hardware_profile_trigger("ac_connected", "plugged_in_balanced")?;
                client.set_hardware_profile_trigger("ac_disconnected", "battery_saver")?;
                let rule = serde_json::json!({
                    "schema_version": 1,
                    "label": "AC profile router",
                    "enabled": true,
                    "kind": "ac_profile_router",
                    "ac_profile_id": "plugged_in_balanced",
                    "battery_profile_id": "battery_saver",
                    "cooldown_secs": 300
                })
                .to_string();
                client.set_automation_rule("ac_router", &rule)?;
                Ok(())
            },
            move |result| {
                button_for_recv.set_sensitive(true);
                match result {
                    Ok(()) => {
                        feedback_for_recv.set_title("Router ready");
                        feedback_for_recv.set_subtitle(
                            "Saved plugged_in_balanced/battery_saver profiles and ac_router rule.",
                        );
                        store_write_feedback_state(
                            "AC profile router starter",
                            "Router ready",
                            "Saved plugged_in_balanced/battery_saver profiles and ac_router rule.",
                        );
                        let _ = request_dashboard_refresh();
                    }
                    Err(error) => {
                        let subtitle = format!("Failed: {error}");
                        feedback_for_recv.set_title("Create error");
                        feedback_for_recv.set_subtitle(&subtitle);
                        store_write_feedback_state(
                            "AC profile router starter",
                            "Create error",
                            &subtitle,
                        );
                    }
                }
            },
        );
    });
}

fn append_ac_cpu_profile_router_starter(group: &adw::PreferencesGroup) {
    let row = adw::ActionRow::builder()
        .title("AC CPU performance router starter")
        .subtitle(
            "Creates AC performance and battery efficiency CPU profiles, then routes AC state with governor, EPP, boost, and platform profile actions",
        )
        .selectable(false)
        .build();
    let create = gtk4::Button::builder()
        .label("Create CPU router")
        .css_classes(["suggested-action", "pill"])
        .valign(gtk4::Align::Center)
        .build();
    row.add_suffix(&create);
    group.add(&row);

    let feedback = write_feedback_row("AC CPU performance router starter");
    group.add(&feedback);

    create.connect_clicked(move |button| {
        button.set_sensitive(false);
        feedback.set_title("Creating CPU router");
        feedback.set_subtitle("Saving daemon-owned AC/battery CPU tuning profiles and router rule...");

        let button_for_recv = button.clone();
        let feedback_for_recv = feedback.clone();
        spawn_dbus_call(
            || {
                let client = make_client()?;
                let ac_performance = serde_json::json!({
                    "schema_version": 1,
                    "label": "AC CPU performance",
                    "actions": {
                        "platform_profile": "performance",
                        "cpu_governor": "performance",
                        "cpu_epp": "performance",
                        "cpu_boost": "1"
                    }
                })
                .to_string();
                let battery_efficiency = serde_json::json!({
                    "schema_version": 1,
                    "label": "Battery CPU efficiency",
                    "actions": {
                        "platform_profile": "low-power",
                        "cpu_governor": "powersave",
                        "cpu_epp": "power",
                        "cpu_boost": "0"
                    }
                })
                .to_string();

                client.set_hardware_profile("ac_cpu_performance", &ac_performance)?;
                client.set_hardware_profile("battery_cpu_efficiency", &battery_efficiency)?;
                client.set_hardware_profile_trigger("ac_connected", "ac_cpu_performance")?;
                client.set_hardware_profile_trigger("ac_disconnected", "battery_cpu_efficiency")?;
                let rule = serde_json::json!({
                    "schema_version": 1,
                    "label": "AC CPU performance router",
                    "enabled": true,
                    "kind": "ac_profile_router",
                    "ac_profile_id": "ac_cpu_performance",
                    "battery_profile_id": "battery_cpu_efficiency",
                    "cooldown_secs": 300
                })
                .to_string();
                client.set_automation_rule("ac_cpu_router", &rule)?;
                Ok(())
            },
            move |result| {
                button_for_recv.set_sensitive(true);
                match result {
                    Ok(()) => {
                        feedback_for_recv.set_title("CPU router ready");
                        feedback_for_recv.set_subtitle(
                            "Saved ac_cpu_performance/battery_cpu_efficiency profiles and ac_cpu_router rule.",
                        );
                        store_write_feedback_state(
                            "AC CPU performance router starter",
                            "CPU router ready",
                            "Saved ac_cpu_performance/battery_cpu_efficiency profiles and ac_cpu_router rule.",
                        );
                        let _ = request_dashboard_refresh();
                    }
                    Err(error) => {
                        let subtitle = format!("Failed: {error}");
                        feedback_for_recv.set_title("Create error");
                        feedback_for_recv.set_subtitle(&subtitle);
                        store_write_feedback_state(
                            "AC CPU performance router starter",
                            "Create error",
                            &subtitle,
                        );
                    }
                }
            },
        );
    });
}

fn append_quiet_on_battery_starter(group: &adw::PreferencesGroup) {
    let row = adw::ActionRow::builder()
        .title("Quiet on battery starter")
        .subtitle(
            "Creates a low-power battery profile, maps AC unplugged and battery 30% or lower to it, and records quiet-office as the low-power fan preset",
        )
        .selectable(false)
        .build();
    let create = gtk4::Button::builder()
        .label("Create quiet")
        .css_classes(["suggested-action", "pill"])
        .valign(gtk4::Align::Center)
        .build();
    row.add_suffix(&create);
    group.add(&row);

    let feedback = write_feedback_row("Quiet on battery starter");
    group.add(&feedback);

    create.connect_clicked(move |button| {
        button.set_sensitive(false);
        feedback.set_title("Creating quiet profile");
        feedback.set_subtitle(
            "Saving daemon-owned battery profile, trigger/rule routing, and fan-preset app-state mapping...",
        );

        let button_for_recv = button.clone();
        let feedback_for_recv = feedback.clone();
        spawn_dbus_call(
            || {
                let client = make_client()?;
                let quiet_battery = serde_json::json!({
                    "schema_version": 1,
                    "label": "Quiet on battery",
                    "actions": {
                        "platform_profile": "low-power"
                    }
                })
                .to_string();

                client.set_hardware_profile("quiet_on_battery", &quiet_battery)?;
                client.set_hardware_profile_trigger("ac_disconnected", "quiet_on_battery")?;
                let quiet_rule = serde_json::json!({
                    "schema_version": 1,
                    "label": "Quiet on battery below 30%",
                    "enabled": true,
                    "kind": "battery_profile_threshold",
                    "threshold_percent": 30,
                    "profile_id": "quiet_on_battery",
                    "when_below_or_equal": true,
                    "require_ac": false,
                    "cooldown_secs": 300
                })
                .to_string();
                client.set_automation_rule("quiet_below_30", &quiet_rule)?;
                client.set_fan_preset_profile_map_entry("low-power", "quiet-office")?;
                Ok(())
            },
            move |result| {
                button_for_recv.set_sensitive(true);
                match result {
                    Ok(()) => {
                        feedback_for_recv.set_title("Quiet profile ready");
                        feedback_for_recv.set_subtitle(
                            "Saved quiet_on_battery profile, AC-unplugged trigger, quiet_below_30 rule, and low-power -> quiet-office fan preset mapping.",
                        );
                        store_write_feedback_state(
                            "Quiet on battery starter",
                            "Quiet profile ready",
                            "Saved quiet_on_battery profile, AC-unplugged trigger, quiet_below_30 rule, and low-power -> quiet-office fan preset mapping.",
                        );
                        let _ = request_dashboard_refresh();
                    }
                    Err(error) => {
                        let subtitle = format!("Failed: {error}");
                        feedback_for_recv.set_title("Create error");
                        feedback_for_recv.set_subtitle(&subtitle);
                        store_write_feedback_state(
                            "Quiet on battery starter",
                            "Create error",
                            &subtitle,
                        );
                    }
                }
            },
        );
    });
}

fn append_battery_threshold_profile_starter(group: &adw::PreferencesGroup) {
    let row = adw::ActionRow::builder()
        .title("Battery threshold rule starter")
        .subtitle(
            "Creates a balanced recovery profile and maps battery 80% or higher to it with a battery-threshold rule",
        )
        .selectable(false)
        .build();
    let create = gtk4::Button::builder()
        .label("Create threshold")
        .css_classes(["suggested-action", "pill"])
        .valign(gtk4::Align::Center)
        .build();
    row.add_suffix(&create);
    group.add(&row);

    let feedback = write_feedback_row("Battery threshold rule starter");
    group.add(&feedback);

    create.connect_clicked(move |button| {
        button.set_sensitive(false);
        feedback.set_title("Creating threshold rule");
        feedback.set_subtitle("Saving daemon-owned recovery profile and battery threshold rule...");

        let button_for_recv = button.clone();
        let feedback_for_recv = feedback.clone();
        spawn_dbus_call(
            || {
                let client = make_client()?;
                let recovered_profile = serde_json::json!({
                    "schema_version": 1,
                    "label": "Battery recovered balanced",
                    "actions": {
                        "platform_profile": "balanced"
                    }
                })
                .to_string();

                client.set_hardware_profile("battery_recovered_balanced", &recovered_profile)?;
                let recovered_rule = serde_json::json!({
                    "schema_version": 1,
                    "label": "Battery recovered at 80%",
                    "enabled": true,
                    "kind": "battery_profile_threshold",
                    "threshold_percent": 80,
                    "profile_id": "battery_recovered_balanced",
                    "when_below_or_equal": false,
                    "cooldown_secs": 300
                })
                .to_string();
                client.set_automation_rule("battery_recovered_80", &recovered_rule)?;
                Ok(())
            },
            move |result| {
                button_for_recv.set_sensitive(true);
                match result {
                    Ok(()) => {
                        feedback_for_recv.set_title("Threshold rule ready");
                        feedback_for_recv.set_subtitle(
                            "Saved battery_recovered_balanced profile and battery_recovered_80 threshold rule.",
                        );
                        store_write_feedback_state(
                            "Battery threshold rule starter",
                            "Threshold rule ready",
                            "Saved battery_recovered_balanced profile and battery_recovered_80 threshold rule.",
                        );
                        let _ = request_dashboard_refresh();
                    }
                    Err(error) => {
                        let subtitle = format!("Failed: {error}");
                        feedback_for_recv.set_title("Create error");
                        feedback_for_recv.set_subtitle(&subtitle);
                        store_write_feedback_state(
                            "Battery threshold rule starter",
                            "Create error",
                            &subtitle,
                        );
                    }
                }
            },
        );
    });
}

fn append_automation_rule_creator(page: &adw::PreferencesPage, bundle: &DiagnosticsBundle) {
    let group = adw::PreferencesGroup::new();
    group.set_title("Create Automation Rule");
    group.set_description(Some(
        "Create daemon-owned battery threshold routing rules from saved hardware profiles.",
    ));

    let mut profile_ids = bundle.hardware_profiles.keys().cloned().collect::<Vec<_>>();
    profile_ids.sort();

    let rule_id = gtk4::Entry::builder()
        .text("custom_battery_threshold")
        .hexpand(true)
        .build();
    let rule_id_row = adw::ActionRow::builder()
        .title("Rule ID")
        .subtitle("Lowercase letters, numbers, dash, or underscore")
        .selectable(false)
        .build();
    rule_id_row.add_suffix(&rule_id);
    group.add(&rule_id_row);

    let label = gtk4::Entry::builder()
        .text("Custom battery threshold")
        .hexpand(true)
        .build();
    let label_row = adw::ActionRow::builder()
        .title("Label")
        .subtitle("Shown in saved automation rules")
        .selectable(false)
        .build();
    label_row.add_suffix(&label);
    group.add(&label_row);

    let profile = profile_dropdown(&profile_ids, profile_ids.first().map_or("", String::as_str));
    profile.set_sensitive(!profile_ids.is_empty());
    profile.set_valign(gtk4::Align::Center);
    let profile_row = adw::ActionRow::builder()
        .title("Target profile")
        .subtitle(if profile_ids.is_empty() {
            "Create at least one hardware profile first"
        } else {
            "Profile selected when the threshold matches"
        })
        .selectable(false)
        .build();
    profile_row.add_suffix(&profile);
    group.add(&profile_row);

    let threshold = gtk4::SpinButton::with_range(1.0, 100.0, 1.0);
    threshold.set_value(30.0);
    threshold.set_digits(0);
    threshold.set_valign(gtk4::Align::Center);
    let threshold_row = adw::ActionRow::builder()
        .title("Battery threshold")
        .subtitle("Percent, 1-100")
        .selectable(false)
        .build();
    threshold_row.add_suffix(&threshold);
    group.add(&threshold_row);

    let direction_values = ["At or below", "At or above"];
    let direction_model = gtk4::StringList::new(&direction_values);
    let direction = gtk4::DropDown::builder().model(&direction_model).build();
    direction.set_selected(0);
    direction.set_valign(gtk4::Align::Center);
    let direction_row = adw::ActionRow::builder()
        .title("Threshold direction")
        .subtitle("Choose whether low or recovered charge matches")
        .selectable(false)
        .build();
    direction_row.add_suffix(&direction);
    group.add(&direction_row);

    let ac_values = [
        "No AC requirement",
        "Require AC online",
        "Require battery power",
    ];
    let ac_model = gtk4::StringList::new(&ac_values);
    let ac_condition = gtk4::DropDown::builder().model(&ac_model).build();
    ac_condition.set_selected(0);
    ac_condition.set_valign(gtk4::Align::Center);
    let ac_row = adw::ActionRow::builder()
        .title("AC condition")
        .subtitle("Optional AC-online/battery-power requirement")
        .selectable(false)
        .build();
    ac_row.add_suffix(&ac_condition);
    group.add(&ac_row);

    let cooldown = gtk4::SpinButton::with_range(0.0, 86_400.0, 30.0);
    cooldown.set_value(300.0);
    cooldown.set_digits(0);
    cooldown.set_valign(gtk4::Align::Center);
    let cooldown_row = adw::ActionRow::builder()
        .title("Cooldown seconds")
        .subtitle("Suppress repeated same-profile applies")
        .selectable(false)
        .build();
    cooldown_row.add_suffix(&cooldown);
    group.add(&cooldown_row);

    let create = gtk4::Button::builder()
        .label("Create rule")
        .css_classes(["suggested-action", "pill"])
        .valign(gtk4::Align::Center)
        .build();
    create.set_sensitive(!profile_ids.is_empty());
    let create_row = adw::ActionRow::builder()
        .title("Create battery threshold rule")
        .subtitle(if profile_ids.is_empty() {
            "Create at least one hardware profile before creating rules"
        } else {
            "Saves a daemon-owned rule through normal validation"
        })
        .selectable(false)
        .build();
    create_row.add_suffix(&create);
    group.add(&create_row);

    let feedback = write_feedback_row("Create automation rule");
    group.add(&feedback);
    page.add(&group);

    let profile_ids_for_create = profile_ids.clone();
    create.connect_clicked(move |button| {
        let rule_id_value = rule_id.text().trim().to_owned();
        if rule_id_value.is_empty() {
            store_write_feedback_state(
                "Create automation rule",
                "Create error",
                "Failed: enter a rule ID",
            );
            return;
        }
        let label_value = label.text().trim().to_owned();
        if label_value.is_empty() {
            store_write_feedback_state(
                "Create automation rule",
                "Create error",
                "Failed: enter a label",
            );
            return;
        }
        let Some(profile_id) = selected_dropdown_value(&profile) else {
            store_write_feedback_state(
                "Create automation rule",
                "Create error",
                "Failed: select a target profile",
            );
            return;
        };
        if !profile_ids_for_create.contains(&profile_id) {
            store_write_feedback_state(
                "Create automation rule",
                "Create error",
                "Failed: selected profile no longer exists",
            );
            return;
        }

        let threshold_value = threshold.value_as_int();
        let when_below_or_equal = direction.selected() == 0;
        let require_ac_value = match ac_condition.selected() {
            1 => serde_json::Value::Bool(true),
            2 => serde_json::Value::Bool(false),
            _ => serde_json::Value::Null,
        };
        let cooldown_value = cooldown.value_as_int();

        button.set_sensitive(false);
        let button_for_recv = button.clone();
        spawn_dbus_call(
            move || {
                let mut rule_json = serde_json::json!({
                    "schema_version": 1,
                    "label": label_value,
                    "enabled": true,
                    "kind": "battery_profile_threshold",
                    "threshold_percent": threshold_value,
                    "profile_id": profile_id,
                    "when_below_or_equal": when_below_or_equal,
                    "cooldown_secs": cooldown_value
                });
                if !require_ac_value.is_null() {
                    rule_json["require_ac"] = require_ac_value;
                }
                make_client().and_then(|client| {
                    client.set_automation_rule(&rule_id_value, &rule_json.to_string())
                })
            },
            move |result| {
                button_for_recv.set_sensitive(true);
                match result {
                    Ok(_) => {
                        store_write_feedback_state(
                            "Create automation rule",
                            "Rule created",
                            "Battery threshold rule saved to the daemon.",
                        );
                        let _ = request_dashboard_refresh();
                    }
                    Err(error) => {
                        store_write_feedback_state(
                            "Create automation rule",
                            "Create error",
                            &format!("Failed: {error}"),
                        );
                    }
                }
            },
        );
    });
}

fn append_ac_router_rule_creator(page: &adw::PreferencesPage, bundle: &DiagnosticsBundle) {
    let group = adw::PreferencesGroup::new();
    group.set_title("Create AC Router Rule");
    group.set_description(Some(
        "Create daemon-owned AC online/offline routing rules from saved hardware profiles.",
    ));

    let mut profile_ids = bundle.hardware_profiles.keys().cloned().collect::<Vec<_>>();
    profile_ids.sort();
    let has_enough_profiles = profile_ids.len() >= 2;
    let default_ac_profile = profile_ids.first().map_or("", String::as_str);
    let default_battery_profile = profile_ids
        .get(1)
        .map_or(default_ac_profile, String::as_str);

    let rule_id = gtk4::Entry::builder()
        .text("custom_ac_router")
        .hexpand(true)
        .build();
    let rule_id_row = adw::ActionRow::builder()
        .title("AC router rule ID")
        .subtitle("Lowercase letters, numbers, dash, or underscore")
        .selectable(false)
        .build();
    rule_id_row.add_suffix(&rule_id);
    group.add(&rule_id_row);

    let label = gtk4::Entry::builder()
        .text("Custom AC router")
        .hexpand(true)
        .build();
    let label_row = adw::ActionRow::builder()
        .title("AC router label")
        .subtitle("Shown in saved automation rules")
        .selectable(false)
        .build();
    label_row.add_suffix(&label);
    group.add(&label_row);

    let ac_profile = profile_dropdown(&profile_ids, default_ac_profile);
    ac_profile.set_sensitive(has_enough_profiles);
    ac_profile.set_valign(gtk4::Align::Center);
    let ac_profile_row = adw::ActionRow::builder()
        .title("AC online profile")
        .subtitle(if has_enough_profiles {
            "Profile selected while AC is online"
        } else {
            "Create at least two hardware profiles first"
        })
        .selectable(false)
        .build();
    ac_profile_row.add_suffix(&ac_profile);
    group.add(&ac_profile_row);

    let battery_profile = profile_dropdown(&profile_ids, default_battery_profile);
    battery_profile.set_sensitive(has_enough_profiles);
    battery_profile.set_valign(gtk4::Align::Center);
    let battery_profile_row = adw::ActionRow::builder()
        .title("Battery power profile")
        .subtitle(if has_enough_profiles {
            "Profile selected while AC is offline"
        } else {
            "Create at least two hardware profiles first"
        })
        .selectable(false)
        .build();
    battery_profile_row.add_suffix(&battery_profile);
    group.add(&battery_profile_row);

    let cooldown = gtk4::SpinButton::with_range(0.0, 86_400.0, 30.0);
    cooldown.set_value(300.0);
    cooldown.set_digits(0);
    cooldown.set_valign(gtk4::Align::Center);
    let cooldown_row = adw::ActionRow::builder()
        .title("AC router cooldown seconds")
        .subtitle("Suppress repeated same-profile applies")
        .selectable(false)
        .build();
    cooldown_row.add_suffix(&cooldown);
    group.add(&cooldown_row);

    let create = gtk4::Button::builder()
        .label("Create AC router")
        .css_classes(["suggested-action", "pill"])
        .valign(gtk4::Align::Center)
        .build();
    create.set_sensitive(has_enough_profiles);
    let create_row = adw::ActionRow::builder()
        .title("Create AC profile router rule")
        .subtitle(if has_enough_profiles {
            "Saves a daemon-owned AC router rule through normal validation"
        } else {
            "Create at least two hardware profiles before creating an AC router"
        })
        .selectable(false)
        .build();
    create_row.add_suffix(&create);
    group.add(&create_row);

    let feedback = write_feedback_row("Create AC router rule");
    group.add(&feedback);
    page.add(&group);

    let profile_ids_for_create = profile_ids.clone();
    create.connect_clicked(move |button| {
        let rule_id_value = rule_id.text().trim().to_owned();
        if rule_id_value.is_empty() {
            store_write_feedback_state(
                "Create AC router rule",
                "Create error",
                "Failed: enter a rule ID",
            );
            return;
        }
        let label_value = label.text().trim().to_owned();
        if label_value.is_empty() {
            store_write_feedback_state(
                "Create AC router rule",
                "Create error",
                "Failed: enter a label",
            );
            return;
        }
        let Some(ac_profile_id) = selected_dropdown_value(&ac_profile) else {
            store_write_feedback_state(
                "Create AC router rule",
                "Create error",
                "Failed: select an AC online profile",
            );
            return;
        };
        let Some(battery_profile_id) = selected_dropdown_value(&battery_profile) else {
            store_write_feedback_state(
                "Create AC router rule",
                "Create error",
                "Failed: select a battery power profile",
            );
            return;
        };
        if ac_profile_id == battery_profile_id {
            store_write_feedback_state(
                "Create AC router rule",
                "Create error",
                "Failed: AC and battery profiles must be different",
            );
            return;
        }
        if !profile_ids_for_create.contains(&ac_profile_id)
            || !profile_ids_for_create.contains(&battery_profile_id)
        {
            store_write_feedback_state(
                "Create AC router rule",
                "Create error",
                "Failed: selected profile no longer exists",
            );
            return;
        }

        let cooldown_value = cooldown.value_as_int();
        button.set_sensitive(false);
        let button_for_recv = button.clone();
        spawn_dbus_call(
            move || {
                let rule_json = serde_json::json!({
                    "schema_version": 1,
                    "label": label_value,
                    "enabled": true,
                    "kind": "ac_profile_router",
                    "ac_profile_id": ac_profile_id,
                    "battery_profile_id": battery_profile_id,
                    "cooldown_secs": cooldown_value
                })
                .to_string();
                make_client()
                    .and_then(|client| client.set_automation_rule(&rule_id_value, &rule_json))
            },
            move |result| {
                button_for_recv.set_sensitive(true);
                match result {
                    Ok(_) => {
                        store_write_feedback_state(
                            "Create AC router rule",
                            "Rule created",
                            "AC router rule saved to the daemon.",
                        );
                        let _ = request_dashboard_refresh();
                    }
                    Err(error) => {
                        store_write_feedback_state(
                            "Create AC router rule",
                            "Create error",
                            &format!("Failed: {error}"),
                        );
                    }
                }
            },
        );
    });
}

fn append_fast_charge_rule_creator(page: &adw::PreferencesPage, bundle: &DiagnosticsBundle) {
    let group = adw::PreferencesGroup::new();
    group.set_title("Create Fast Charge Rule");
    group.set_description(Some(
        "Create daemon-owned fast-charge-until-threshold rules from saved hardware profiles.",
    ));

    let mut profile_ids = bundle.hardware_profiles.keys().cloned().collect::<Vec<_>>();
    profile_ids.sort();
    let has_enough_profiles = profile_ids.len() >= 2;
    let default_fast_profile = profile_ids.first().map_or("", String::as_str);
    let default_protect_profile = profile_ids
        .get(1)
        .map_or(default_fast_profile, String::as_str);

    let rule_id = gtk4::Entry::builder()
        .text("custom_fast_charge_until")
        .hexpand(true)
        .build();
    let rule_id_row = adw::ActionRow::builder()
        .title("Fast-charge rule ID")
        .subtitle("Lowercase letters, numbers, dash, or underscore")
        .selectable(false)
        .build();
    rule_id_row.add_suffix(&rule_id);
    group.add(&rule_id_row);

    let label = gtk4::Entry::builder()
        .text("Custom fast charge until threshold")
        .hexpand(true)
        .build();
    let label_row = adw::ActionRow::builder()
        .title("Fast-charge label")
        .subtitle("Shown in saved automation rules")
        .selectable(false)
        .build();
    label_row.add_suffix(&label);
    group.add(&label_row);

    let fast_profile = profile_dropdown(&profile_ids, default_fast_profile);
    fast_profile.set_sensitive(has_enough_profiles);
    fast_profile.set_valign(gtk4::Align::Center);
    let fast_profile_row = adw::ActionRow::builder()
        .title("Fast-charge profile")
        .subtitle(if has_enough_profiles {
            "Profile selected while battery is below the threshold"
        } else {
            "Create at least two hardware profiles first"
        })
        .selectable(false)
        .build();
    fast_profile_row.add_suffix(&fast_profile);
    group.add(&fast_profile_row);

    let protect_profile = profile_dropdown(&profile_ids, default_protect_profile);
    protect_profile.set_sensitive(has_enough_profiles);
    protect_profile.set_valign(gtk4::Align::Center);
    let protect_profile_row = adw::ActionRow::builder()
        .title("Protect profile")
        .subtitle(if has_enough_profiles {
            "Profile selected when battery reaches the threshold"
        } else {
            "Create at least two hardware profiles first"
        })
        .selectable(false)
        .build();
    protect_profile_row.add_suffix(&protect_profile);
    group.add(&protect_profile_row);

    let threshold = gtk4::SpinButton::with_range(1.0, 100.0, 1.0);
    threshold.set_value(80.0);
    threshold.set_digits(0);
    threshold.set_valign(gtk4::Align::Center);
    let threshold_row = adw::ActionRow::builder()
        .title("Fast-charge threshold")
        .subtitle("Percent, 1-100")
        .selectable(false)
        .build();
    threshold_row.add_suffix(&threshold);
    group.add(&threshold_row);

    let require_ac = gtk4::Switch::new();
    require_ac.set_active(true);
    require_ac.set_valign(gtk4::Align::Center);
    let require_ac_row = adw::ActionRow::builder()
        .title("Require AC online")
        .subtitle("Skip when the machine is running on battery")
        .selectable(false)
        .build();
    require_ac_row.add_suffix(&require_ac);
    group.add(&require_ac_row);

    let cooldown = gtk4::SpinButton::with_range(0.0, 86_400.0, 30.0);
    cooldown.set_value(300.0);
    cooldown.set_digits(0);
    cooldown.set_valign(gtk4::Align::Center);
    let cooldown_row = adw::ActionRow::builder()
        .title("Fast-charge cooldown seconds")
        .subtitle("Suppress repeated same-profile applies")
        .selectable(false)
        .build();
    cooldown_row.add_suffix(&cooldown);
    group.add(&cooldown_row);

    let create = gtk4::Button::builder()
        .label("Create fast-charge rule")
        .css_classes(["suggested-action", "pill"])
        .valign(gtk4::Align::Center)
        .build();
    create.set_sensitive(has_enough_profiles);
    let create_row = adw::ActionRow::builder()
        .title("Create fast-charge threshold rule")
        .subtitle(if has_enough_profiles {
            "Saves a daemon-owned fast-charge rule through normal validation"
        } else {
            "Create at least two hardware profiles before creating a fast-charge rule"
        })
        .selectable(false)
        .build();
    create_row.add_suffix(&create);
    group.add(&create_row);

    let feedback = write_feedback_row("Create fast-charge rule");
    group.add(&feedback);
    page.add(&group);

    let profile_ids_for_create = profile_ids.clone();
    create.connect_clicked(move |button| {
        let rule_id_value = rule_id.text().trim().to_owned();
        if rule_id_value.is_empty() {
            store_write_feedback_state(
                "Create fast-charge rule",
                "Create error",
                "Failed: enter a rule ID",
            );
            return;
        }
        let label_value = label.text().trim().to_owned();
        if label_value.is_empty() {
            store_write_feedback_state(
                "Create fast-charge rule",
                "Create error",
                "Failed: enter a label",
            );
            return;
        }
        let Some(fast_profile_id) = selected_dropdown_value(&fast_profile) else {
            store_write_feedback_state(
                "Create fast-charge rule",
                "Create error",
                "Failed: select a fast-charge profile",
            );
            return;
        };
        let Some(protect_profile_id) = selected_dropdown_value(&protect_profile) else {
            store_write_feedback_state(
                "Create fast-charge rule",
                "Create error",
                "Failed: select a protect profile",
            );
            return;
        };
        if fast_profile_id == protect_profile_id {
            store_write_feedback_state(
                "Create fast-charge rule",
                "Create error",
                "Failed: fast-charge and protect profiles must be different",
            );
            return;
        }
        if !profile_ids_for_create.contains(&fast_profile_id)
            || !profile_ids_for_create.contains(&protect_profile_id)
        {
            store_write_feedback_state(
                "Create fast-charge rule",
                "Create error",
                "Failed: selected profile no longer exists",
            );
            return;
        }

        let threshold_value = threshold.value_as_int();
        let require_ac_value = require_ac.is_active();
        let cooldown_value = cooldown.value_as_int();
        button.set_sensitive(false);
        let button_for_recv = button.clone();
        spawn_dbus_call(
            move || {
                let rule_json = serde_json::json!({
                    "schema_version": 1,
                    "label": label_value,
                    "enabled": true,
                    "kind": "fast_charge_until_threshold",
                    "threshold_percent": threshold_value,
                    "fast_charge_profile_id": fast_profile_id,
                    "protect_profile_id": protect_profile_id,
                    "require_ac": require_ac_value,
                    "cooldown_secs": cooldown_value
                })
                .to_string();
                make_client()
                    .and_then(|client| client.set_automation_rule(&rule_id_value, &rule_json))
            },
            move |result| {
                button_for_recv.set_sensitive(true);
                match result {
                    Ok(_) => {
                        store_write_feedback_state(
                            "Create fast-charge rule",
                            "Rule created",
                            "Fast-charge threshold rule saved to the daemon.",
                        );
                        let _ = request_dashboard_refresh();
                    }
                    Err(error) => {
                        store_write_feedback_state(
                            "Create fast-charge rule",
                            "Create error",
                            &format!("Failed: {error}"),
                        );
                    }
                }
            },
        );
    });
}

fn append_integrated_gpu_on_battery_starter(group: &adw::PreferencesGroup) {
    let row = adw::ActionRow::builder()
        .title("Integrated GPU on battery starter")
        .subtitle(
            "Creates an integrated-GPU battery profile and maps AC unplugged to it; GPU switch still uses reboot-gated daemon policy",
        )
        .selectable(false)
        .build();
    let create = gtk4::Button::builder()
        .label("Create iGPU")
        .css_classes(["suggested-action", "pill"])
        .valign(gtk4::Align::Center)
        .build();
    row.add_suffix(&create);
    group.add(&row);

    let feedback = write_feedback_row("Integrated GPU on battery starter");
    group.add(&feedback);

    create.connect_clicked(move |button| {
        button.set_sensitive(false);
        feedback.set_title("Creating iGPU profile");
        feedback.set_subtitle("Saving daemon-owned GPU battery profile and AC-unplugged trigger...");

        let button_for_recv = button.clone();
        let feedback_for_recv = feedback.clone();
        spawn_dbus_call(
            || {
                let client = make_client()?;
                let integrated_battery = serde_json::json!({
                    "schema_version": 1,
                    "label": "Integrated GPU on battery",
                    "actions": {
                        "gpu_mode": "integrated"
                    }
                })
                .to_string();

                client.set_hardware_profile("integrated_gpu_on_battery", &integrated_battery)?;
                client
                    .set_hardware_profile_trigger("ac_disconnected", "integrated_gpu_on_battery")?;
                Ok(())
            },
            move |result| {
                button_for_recv.set_sensitive(true);
                match result {
                    Ok(()) => {
                        feedback_for_recv.set_title("iGPU profile ready");
                        feedback_for_recv.set_subtitle(
                            "Saved integrated_gpu_on_battery profile and AC-unplugged trigger; applying it remains GPU-policy/reboot gated.",
                        );
                        store_write_feedback_state(
                            "Integrated GPU on battery starter",
                            "iGPU profile ready",
                            "Saved integrated_gpu_on_battery profile and AC-unplugged trigger; applying it remains GPU-policy/reboot gated.",
                        );
                        let _ = request_dashboard_refresh();
                    }
                    Err(error) => {
                        let subtitle = format!("Failed: {error}");
                        feedback_for_recv.set_title("Create error");
                        feedback_for_recv.set_subtitle(&subtitle);
                        store_write_feedback_state(
                            "Integrated GPU on battery starter",
                            "Create error",
                            &subtitle,
                        );
                    }
                }
            },
        );
    });
}

fn append_resume_balanced_profile_starter(group: &adw::PreferencesGroup) {
    let row = adw::ActionRow::builder()
        .title("Resume balanced profile starter")
        .subtitle(
            "Creates a balanced resume profile and maps logind resume to it for post-sleep state repair",
        )
        .selectable(false)
        .build();
    let create = gtk4::Button::builder()
        .label("Create resume")
        .css_classes(["suggested-action", "pill"])
        .valign(gtk4::Align::Center)
        .build();
    row.add_suffix(&create);
    group.add(&row);

    let feedback = write_feedback_row("Resume balanced profile starter");
    group.add(&feedback);

    create.connect_clicked(move |button| {
        button.set_sensitive(false);
        feedback.set_title("Creating resume profile");
        feedback.set_subtitle("Saving daemon-owned resume profile and trigger mapping...");

        let button_for_recv = button.clone();
        let feedback_for_recv = feedback.clone();
        spawn_dbus_call(
            || {
                let client = make_client()?;
                let resume_profile = serde_json::json!({
                    "schema_version": 1,
                    "label": "Resume balanced repair",
                    "actions": {
                        "platform_profile": "balanced"
                    }
                })
                .to_string();

                client.set_hardware_profile("resume_balanced_repair", &resume_profile)?;
                client.set_hardware_profile_trigger("resume", "resume_balanced_repair")?;
                Ok(())
            },
            move |result| {
                button_for_recv.set_sensitive(true);
                match result {
                    Ok(()) => {
                        feedback_for_recv.set_title("Resume profile ready");
                        feedback_for_recv.set_subtitle(
                            "Saved resume_balanced_repair profile and resume trigger; execution still uses daemon write gates after logind resume.",
                        );
                        store_write_feedback_state(
                            "Resume balanced profile starter",
                            "Resume profile ready",
                            "Saved resume_balanced_repair profile and resume trigger; execution still uses daemon write gates after logind resume.",
                        );
                        let _ = request_dashboard_refresh();
                    }
                    Err(error) => {
                        let subtitle = format!("Failed: {error}");
                        feedback_for_recv.set_title("Create error");
                        feedback_for_recv.set_subtitle(&subtitle);
                        store_write_feedback_state(
                            "Resume balanced profile starter",
                            "Create error",
                            &subtitle,
                        );
                    }
                }
            },
        );
    });
}

fn append_gpu_reboot_completion_starter(group: &adw::PreferencesGroup) {
    let row = adw::ActionRow::builder()
        .title("GPU reboot repair starter")
        .subtitle(
            "Creates a balanced post-GPU-switch repair profile and maps GPU reboot completion to it",
        )
        .selectable(false)
        .build();
    let create = gtk4::Button::builder()
        .label("Create GPU repair")
        .css_classes(["suggested-action", "pill"])
        .valign(gtk4::Align::Center)
        .build();
    row.add_suffix(&create);
    group.add(&row);

    let feedback = write_feedback_row("GPU reboot repair starter");
    group.add(&feedback);

    create.connect_clicked(move |button| {
        button.set_sensitive(false);
        feedback.set_title("Creating GPU repair profile");
        feedback.set_subtitle("Saving daemon-owned GPU reboot completion profile and trigger...");

        let button_for_recv = button.clone();
        let feedback_for_recv = feedback.clone();
        spawn_dbus_call(
            || {
                let client = make_client()?;
                let repair_profile = serde_json::json!({
                    "schema_version": 1,
                    "label": "GPU reboot completed balanced repair",
                    "actions": {
                        "platform_profile": "balanced"
                    }
                })
                .to_string();

                client.set_hardware_profile("gpu_reboot_completed_balanced", &repair_profile)?;
                client.set_hardware_profile_trigger(
                    "gpu_mode_reboot_completed",
                    "gpu_reboot_completed_balanced",
                )?;
                Ok(())
            },
            move |result| {
                button_for_recv.set_sensitive(true);
                match result {
                    Ok(()) => {
                        feedback_for_recv.set_title("GPU repair profile ready");
                        feedback_for_recv.set_subtitle(
                            "Saved gpu_reboot_completed_balanced profile and GPU reboot-completion trigger; execution still uses daemon write gates.",
                        );
                        store_write_feedback_state(
                            "GPU reboot repair starter",
                            "GPU repair profile ready",
                            "Saved gpu_reboot_completed_balanced profile and GPU reboot-completion trigger; execution still uses daemon write gates.",
                        );
                        let _ = request_dashboard_refresh();
                    }
                    Err(error) => {
                        let subtitle = format!("Failed: {error}");
                        feedback_for_recv.set_title("Create error");
                        feedback_for_recv.set_subtitle(&subtitle);
                        store_write_feedback_state(
                            "GPU reboot repair starter",
                            "Create error",
                            &subtitle,
                        );
                    }
                }
            },
        );
    });
}

fn append_profile_change_tuning_starter(group: &adw::PreferencesGroup) {
    let row = adw::ActionRow::builder()
        .title("Fn+Q tuning repair starter")
        .subtitle(
            "Creates a profile-change repair profile for balanced CPU tuning and maps external platform-profile changes to it",
        )
        .selectable(false)
        .build();
    let create = gtk4::Button::builder()
        .label("Create repair")
        .css_classes(["suggested-action", "pill"])
        .valign(gtk4::Align::Center)
        .build();
    row.add_suffix(&create);
    group.add(&row);

    let feedback = write_feedback_row("Fn+Q tuning repair starter");
    group.add(&feedback);

    create.connect_clicked(move |button| {
        button.set_sensitive(false);
        feedback.set_title("Creating repair profile");
        feedback.set_subtitle(
            "Saving daemon-owned platform-profile change response profile and trigger mapping...",
        );

        let button_for_recv = button.clone();
        let feedback_for_recv = feedback.clone();
        spawn_dbus_call(
            || {
                let client = make_client()?;
                let repair_profile = serde_json::json!({
                    "schema_version": 1,
                    "label": "Fn+Q balanced CPU repair",
                    "actions": {
                        "cpu_governor": "powersave",
                        "cpu_epp": "balance_performance",
                        "cpu_boost": "1"
                    }
                })
                .to_string();

                client.set_hardware_profile("fnq_balanced_cpu_repair", &repair_profile)?;
                client.set_hardware_profile_trigger(
                    "platform_profile_changed",
                    "fnq_balanced_cpu_repair",
                )?;
                Ok(())
            },
            move |result| {
                button_for_recv.set_sensitive(true);
                match result {
                    Ok(()) => {
                        feedback_for_recv.set_title("Repair profile ready");
                        feedback_for_recv.set_subtitle(
                            "Saved fnq_balanced_cpu_repair profile and platform_profile_changed trigger; execution remains policy/read-back gated.",
                        );
                        store_write_feedback_state(
                            "Fn+Q tuning repair starter",
                            "Repair profile ready",
                            "Saved fnq_balanced_cpu_repair profile and platform_profile_changed trigger; execution remains policy/read-back gated.",
                        );
                        let _ = request_dashboard_refresh();
                    }
                    Err(error) => {
                        let subtitle = format!("Failed: {error}");
                        feedback_for_recv.set_title("Create error");
                        feedback_for_recv.set_subtitle(&subtitle);
                        store_write_feedback_state(
                            "Fn+Q tuning repair starter",
                            "Create error",
                            &subtitle,
                        );
                    }
                }
            },
        );
    });
}

fn append_rgb_breathing_profile_starter(group: &adw::PreferencesGroup) {
    let row = adw::ActionRow::builder()
        .title("RGB breathing profile starter")
        .subtitle(
            "Creates an RGB breathing hardware profile for preview/review only; applying still waits for RGB backend evidence",
        )
        .selectable(false)
        .build();
    let create = gtk4::Button::builder()
        .label("Create RGB")
        .css_classes(["suggested-action", "pill"])
        .valign(gtk4::Align::Center)
        .build();
    row.add_suffix(&create);
    group.add(&row);

    let feedback = write_feedback_row("RGB breathing profile starter");
    group.add(&feedback);

    create.connect_clicked(move |button| {
        button.set_sensitive(false);
        feedback.set_title("Creating RGB profile");
        feedback.set_subtitle("Saving daemon-owned RGB profile for preview/review...");

        let button_for_recv = button.clone();
        let feedback_for_recv = feedback.clone();
        spawn_dbus_call(
            || {
                let client = make_client()?;
                let rgb_profile = serde_json::json!({
                    "schema_version": 1,
                    "label": "RGB breathing blue",
                    "actions": {
                        "keyboard_rgb": {
                            "effect": "Breathing",
                            "colors": {
                                "left_side": "#333333",
                                "left_center": "#333333",
                                "right_center": "#333333",
                                "right_side": "#333333"
                            },
                            "brightness": 40,
                            "speed": 30
                        }
                    }
                })
                .to_string();

                client.set_hardware_profile("rgb_breathing_blue", &rgb_profile)?;
                Ok(())
            },
            move |result| {
                button_for_recv.set_sensitive(true);
                match result {
                    Ok(()) => {
                        feedback_for_recv.set_title("RGB profile ready");
                        feedback_for_recv.set_subtitle(
                            "Saved rgb_breathing_blue profile; preview it before applying. RGB execution remains evidence-gated.",
                        );
                        store_write_feedback_state(
                            "RGB breathing profile starter",
                            "RGB profile ready",
                            "Saved rgb_breathing_blue profile; preview it before applying. RGB execution remains evidence-gated.",
                        );
                        let _ = request_dashboard_refresh();
                    }
                    Err(error) => {
                        let subtitle = format!("Failed: {error}");
                        feedback_for_recv.set_title("Create error");
                        feedback_for_recv.set_subtitle(&subtitle);
                        store_write_feedback_state(
                            "RGB breathing profile starter",
                            "Create error",
                            &subtitle,
                        );
                    }
                }
            },
        );
    });
}

fn append_co_experimental_profile_starter(group: &adw::PreferencesGroup) {
    let row = adw::ActionRow::builder()
        .title("CO experimental profile starter")
        .subtitle(
            "Creates a Curve Optimizer -10 hardware profile for preview only; applying remains policy-gated and write-only",
        )
        .selectable(false)
        .build();
    let create = gtk4::Button::builder()
        .label("Create CO")
        .css_classes(["suggested-action", "pill"])
        .valign(gtk4::Align::Center)
        .build();
    row.add_suffix(&create);
    group.add(&row);

    let feedback = write_feedback_row("CO experimental profile starter");
    group.add(&feedback);

    create.connect_clicked(move |button| {
        button.set_sensitive(false);
        feedback.set_title("Creating CO profile");
        feedback.set_subtitle("Saving daemon-owned Curve Optimizer profile for preview/review...");

        let button_for_recv = button.clone();
        let feedback_for_recv = feedback.clone();
        spawn_dbus_call(
            || {
                let client = make_client()?;
                let co_profile = serde_json::json!({
                    "schema_version": 1,
                    "label": "CO experimental -10",
                    "actions": {
                        "curve_optimizer_all_core": "-10"
                    }
                })
                .to_string();

                client.set_hardware_profile("co_experimental_minus10", &co_profile)?;
                Ok(())
            },
            move |result| {
                button_for_recv.set_sensitive(true);
                match result {
                    Ok(()) => {
                        feedback_for_recv.set_title("CO profile ready");
                        feedback_for_recv.set_subtitle(
                            "Saved co_experimental_minus10 profile; preview it before applying. CO remains write-only and policy-gated.",
                        );
                        store_write_feedback_state(
                            "CO experimental profile starter",
                            "CO profile ready",
                            "Saved co_experimental_minus10 profile; preview it before applying. CO remains write-only and policy-gated.",
                        );
                        let _ = request_dashboard_refresh();
                    }
                    Err(error) => {
                        let subtitle = format!("Failed: {error}");
                        feedback_for_recv.set_title("Create error");
                        feedback_for_recv.set_subtitle(&subtitle);
                        store_write_feedback_state(
                            "CO experimental profile starter",
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
        "Map supported triggers to saved hardware profiles. Resume and GPU reboot-completion triggers run from the daemon; manual runs use the same daemon gates and stop on the first non-applied action.",
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
                .subtitle(format!(
                    "{profile_id} · {}",
                    automation_profile_action_summary(profile)
                ))
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

fn append_recent_platform_profile_changes(page: &adw::PreferencesPage, bundle: &DiagnosticsBundle) {
    if bundle.recent_platform_profile_changes.is_empty() {
        return;
    }

    let group = adw::PreferencesGroup::new();
    group.set_title("Recent Platform Profile Changes");
    group.set_description(Some(
        "External profile changes observed by the daemon, including Fn+Q or firmware-side changes.",
    ));

    for event in bundle.recent_platform_profile_changes.iter().rev().take(5) {
        group.add(
            &adw::ActionRow::builder()
                .title(format!(
                    "{} -> {}",
                    event.previous_profile, event.current_profile
                ))
                .subtitle(format!(
                    "{} · unix {}",
                    event.source, event.timestamp_unix_secs
                ))
                .selectable(false)
                .build(),
        );
    }

    page.add(&group);
}

fn append_recent_desktop_power_profile_changes(
    page: &adw::PreferencesPage,
    bundle: &DiagnosticsBundle,
) {
    if bundle.recent_desktop_power_profile_changes.is_empty() {
        return;
    }

    let group = adw::PreferencesGroup::new();
    group.set_title("Recent Desktop Power Profile Changes");
    group.set_description(Some(
        "Desktop PowerProfiles changes observed by the daemon, including KDE or GNOME power mode changes.",
    ));

    for event in bundle
        .recent_desktop_power_profile_changes
        .iter()
        .rev()
        .take(5)
    {
        group.add(
            &adw::ActionRow::builder()
                .title(format!(
                    "{} -> {}",
                    event.previous_profile, event.current_profile
                ))
                .subtitle(format!(
                    "{} · unix {}",
                    event.source, event.timestamp_unix_secs
                ))
                .selectable(false)
                .build(),
        );
    }

    page.add(&group);
}

fn append_persisted_automation_rules(page: &adw::PreferencesPage, bundle: &DiagnosticsBundle) {
    let group = adw::PreferencesGroup::new();
    group.set_title("Saved Automation Rules");
    group.add(&section_note(
        "Rules are daemon-owned. Save updates the daemon state; preview refreshes telemetry without writing; test run applies through the normal daemon write path.",
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

    let profile_ids = bundle.hardware_profiles.keys().cloned().collect::<Vec<_>>();

    for (rule_id, rule) in &bundle.automation_rules {
        let expander = adw::ExpanderRow::new();
        expander.set_title(rule.label.as_str());
        expander.set_subtitle(&automation_rule_subtitle(
            rule_id,
            rule.enabled,
            bundle.last_automation_rule_apply.get(rule_id),
        ));

        let enabled = gtk4::Switch::new();
        enabled.set_active(rule.enabled);
        enabled.set_valign(gtk4::Align::Center);
        expander.add_suffix(&enabled);

        if let AutomationRuleKind::AcProfileRouter {
            ac_profile_id,
            battery_profile_id,
            cooldown_secs,
        } = &rule.kind
        {
            expander.add_row(
                &adw::ActionRow::builder()
                    .title("Rule type")
                    .subtitle("AC profile router")
                    .selectable(false)
                    .build(),
            );

            let ac_profile = profile_dropdown(&profile_ids, ac_profile_id);
            ac_profile.set_valign(gtk4::Align::Center);
            ac_profile.set_sensitive(!profile_ids.is_empty());
            let ac_row = adw::ActionRow::builder()
                .title("AC profile")
                .subtitle(ac_profile_id.as_str())
                .selectable(false)
                .build();
            ac_row.add_suffix(&ac_profile);
            expander.add_row(&ac_row);

            let battery_profile = profile_dropdown(&profile_ids, battery_profile_id);
            battery_profile.set_valign(gtk4::Align::Center);
            battery_profile.set_sensitive(!profile_ids.is_empty());
            let battery_row = adw::ActionRow::builder()
                .title("Battery profile")
                .subtitle(battery_profile_id.as_str())
                .selectable(false)
                .build();
            battery_row.add_suffix(&battery_profile);
            expander.add_row(&battery_row);

            let cooldown = gtk4::SpinButton::with_range(0.0, 86_400.0, 30.0);
            cooldown.set_value(*cooldown_secs as f64);
            cooldown.set_digits(0);
            cooldown.set_valign(gtk4::Align::Center);
            let cooldown_row = adw::ActionRow::builder()
                .title("Cooldown seconds")
                .subtitle(cooldown_secs.to_string())
                .selectable(false)
                .build();
            cooldown_row.add_suffix(&cooldown);
            expander.add_row(&cooldown_row);

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
            let save = gtk4::Button::builder()
                .label("Save")
                .css_classes(["suggested-action", "pill"])
                .valign(gtk4::Align::Center)
                .build();
            save.set_sensitive(profile_ids.len() >= 2);
            let delete = gtk4::Button::builder()
                .label("Delete")
                .css_classes(["destructive-action", "pill"])
                .valign(gtk4::Align::Center)
                .build();
            let action_row = adw::ActionRow::builder()
                .title("Rule actions")
                .subtitle(if profile_ids.len() >= 2 {
                    "Preview, apply once, save edits, or delete this daemon-owned AC router rule"
                } else {
                    "Create at least two hardware profiles before saving edits"
                })
                .selectable(false)
                .build();
            action_row.add_suffix(&preview);
            action_row.add_suffix(&run);
            action_row.add_suffix(&save);
            action_row.add_suffix(&delete);
            expander.add_row(&action_row);
            group.add(&expander);

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
                        make_client()
                            .and_then(|client| client.apply_automation_rule(&rule_id_for_call))
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

            let rule_id_for_save = rule_id.clone();
            let label_for_save = rule.label.clone();
            let profile_ids_for_save = profile_ids.clone();
            save.connect_clicked(move |button| {
                let Some(ac_profile_id) = selected_dropdown_value(&ac_profile) else {
                    store_write_feedback_state(
                        "Automation rule",
                        "Save error",
                        "Failed: select an AC profile",
                    );
                    return;
                };
                let Some(battery_profile_id) = selected_dropdown_value(&battery_profile) else {
                    store_write_feedback_state(
                        "Automation rule",
                        "Save error",
                        "Failed: select a battery profile",
                    );
                    return;
                };
                if ac_profile_id == battery_profile_id {
                    store_write_feedback_state(
                        "Automation rule",
                        "Save error",
                        "Failed: AC and battery profiles must be different",
                    );
                    return;
                }
                if !profile_ids_for_save.contains(&ac_profile_id)
                    || !profile_ids_for_save.contains(&battery_profile_id)
                {
                    store_write_feedback_state(
                        "Automation rule",
                        "Save error",
                        "Failed: selected profile no longer exists",
                    );
                    return;
                }
                let enabled_value = enabled.is_active();
                let cooldown_value = cooldown.value_as_int();

                button.set_sensitive(false);
                let button_for_recv = button.clone();
                let rule_id_for_call = rule_id_for_save.clone();
                let label_for_call = label_for_save.clone();
                spawn_dbus_call(
                    move || {
                        let rule_json = serde_json::json!({
                            "schema_version": 1,
                            "label": label_for_call,
                            "enabled": enabled_value,
                            "kind": "ac_profile_router",
                            "ac_profile_id": ac_profile_id,
                            "battery_profile_id": battery_profile_id,
                            "cooldown_secs": cooldown_value
                        })
                        .to_string();
                        make_client().and_then(|client| {
                            client.set_automation_rule(&rule_id_for_call, &rule_json)
                        })
                    },
                    move |result| {
                        button_for_recv.set_sensitive(true);
                        match result {
                            Ok(_) => {
                                store_write_feedback_state(
                                    "Automation rule",
                                    "Saved",
                                    "AC router edits saved to the daemon.",
                                );
                                let _ = request_dashboard_refresh();
                            }
                            Err(error) => {
                                store_write_feedback_state(
                                    "Automation rule",
                                    "Save error",
                                    &format!("Failed: {error}"),
                                );
                            }
                        }
                    },
                );
            });

            let rule_id_for_delete = rule_id.clone();
            delete.connect_clicked(move |button| {
                button.set_sensitive(false);
                let button_for_recv = button.clone();
                let rule_id_for_call = rule_id_for_delete.clone();
                spawn_dbus_call(
                    move || {
                        make_client()
                            .and_then(|client| client.remove_automation_rule(&rule_id_for_call))
                    },
                    move |result| {
                        button_for_recv.set_sensitive(true);
                        match result {
                            Ok(_) => {
                                store_write_feedback_state(
                                    "Automation rule",
                                    "Deleted",
                                    "Rule removed from the daemon.",
                                );
                                let _ = request_dashboard_refresh();
                            }
                            Err(error) => {
                                store_write_feedback_state(
                                    "Automation rule",
                                    "Delete error",
                                    &format!("Failed: {error}"),
                                );
                            }
                        }
                    },
                );
            });

            continue;
        }

        if let AutomationRuleKind::BatteryProfileThreshold {
            threshold_percent,
            profile_id,
            when_below_or_equal,
            require_ac,
            cooldown_secs,
        } = &rule.kind
        {
            expander.add_row(
                &adw::ActionRow::builder()
                    .title("Rule type")
                    .subtitle("Battery profile threshold")
                    .selectable(false)
                    .build(),
            );

            let profile = profile_dropdown(&profile_ids, profile_id);
            profile.set_valign(gtk4::Align::Center);
            profile.set_sensitive(!profile_ids.is_empty());
            let profile_row = adw::ActionRow::builder()
                .title("Profile")
                .subtitle(profile_id.as_str())
                .selectable(false)
                .build();
            profile_row.add_suffix(&profile);
            expander.add_row(&profile_row);

            let threshold = gtk4::SpinButton::with_range(1.0, 100.0, 1.0);
            threshold.set_value((*threshold_percent).into());
            threshold.set_digits(0);
            threshold.set_valign(gtk4::Align::Center);
            let threshold_row = adw::ActionRow::builder()
                .title("Battery threshold")
                .subtitle(threshold_percent.to_string())
                .selectable(false)
                .build();
            threshold_row.add_suffix(&threshold);
            expander.add_row(&threshold_row);

            let direction_values = ["At or below", "At or above"];
            let direction_model = gtk4::StringList::new(&direction_values);
            let direction = gtk4::DropDown::builder().model(&direction_model).build();
            direction.set_selected(if *when_below_or_equal { 0 } else { 1 });
            direction.set_valign(gtk4::Align::Center);
            let direction_row = adw::ActionRow::builder()
                .title("Threshold direction")
                .subtitle(if *when_below_or_equal {
                    "At or below"
                } else {
                    "At or above"
                })
                .selectable(false)
                .build();
            direction_row.add_suffix(&direction);
            expander.add_row(&direction_row);

            let ac_values = [
                "No AC requirement",
                "Require AC online",
                "Require battery power",
            ];
            let ac_model = gtk4::StringList::new(&ac_values);
            let ac_condition = gtk4::DropDown::builder().model(&ac_model).build();
            ac_condition.set_selected(match require_ac {
                None => 0,
                Some(true) => 1,
                Some(false) => 2,
            });
            ac_condition.set_valign(gtk4::Align::Center);
            let ac_condition_row = adw::ActionRow::builder()
                .title("AC condition")
                .subtitle(match require_ac {
                    Some(true) => "Require AC online".to_owned(),
                    Some(false) => "Require battery power".to_owned(),
                    None => "No AC requirement".to_owned(),
                })
                .selectable(false)
                .build();
            ac_condition_row.add_suffix(&ac_condition);
            expander.add_row(&ac_condition_row);

            let cooldown = gtk4::SpinButton::with_range(0.0, 86_400.0, 30.0);
            cooldown.set_value(*cooldown_secs as f64);
            cooldown.set_digits(0);
            cooldown.set_valign(gtk4::Align::Center);
            let cooldown_row = adw::ActionRow::builder()
                .title("Cooldown seconds")
                .subtitle(cooldown_secs.to_string())
                .selectable(false)
                .build();
            cooldown_row.add_suffix(&cooldown);
            expander.add_row(&cooldown_row);

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
            let save = gtk4::Button::builder()
                .label("Save")
                .css_classes(["suggested-action", "pill"])
                .valign(gtk4::Align::Center)
                .build();
            save.set_sensitive(!profile_ids.is_empty());
            let delete = gtk4::Button::builder()
                .label("Delete")
                .css_classes(["destructive-action", "pill"])
                .valign(gtk4::Align::Center)
                .build();
            let action_row = adw::ActionRow::builder()
                .title("Rule actions")
                .subtitle(if profile_ids.is_empty() {
                    "Create at least one hardware profile before saving edits"
                } else {
                    "Preview, apply once, save edits, or delete this daemon-owned battery threshold rule"
                })
                .selectable(false)
                .build();
            action_row.add_suffix(&preview);
            action_row.add_suffix(&run);
            action_row.add_suffix(&save);
            action_row.add_suffix(&delete);
            expander.add_row(&action_row);
            group.add(&expander);

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
                        make_client()
                            .and_then(|client| client.apply_automation_rule(&rule_id_for_call))
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

            let rule_id_for_save = rule_id.clone();
            let label_for_save = rule.label.clone();
            let profile_ids_for_save = profile_ids.clone();
            save.connect_clicked(move |button| {
                let Some(profile_id) = selected_dropdown_value(&profile) else {
                    store_write_feedback_state(
                        "Automation rule",
                        "Save error",
                        "Failed: select a profile",
                    );
                    return;
                };
                if !profile_ids_for_save.contains(&profile_id) {
                    store_write_feedback_state(
                        "Automation rule",
                        "Save error",
                        "Failed: selected profile no longer exists",
                    );
                    return;
                }
                let enabled_value = enabled.is_active();
                let threshold_value = threshold.value_as_int();
                let when_below_or_equal = direction.selected() == 0;
                let require_ac_value = match ac_condition.selected() {
                    1 => serde_json::Value::Bool(true),
                    2 => serde_json::Value::Bool(false),
                    _ => serde_json::Value::Null,
                };
                let cooldown_value = cooldown.value_as_int();

                button.set_sensitive(false);
                let button_for_recv = button.clone();
                let rule_id_for_call = rule_id_for_save.clone();
                let label_for_call = label_for_save.clone();
                spawn_dbus_call(
                    move || {
                        let mut rule_json = serde_json::json!({
                            "schema_version": 1,
                            "label": label_for_call,
                            "enabled": enabled_value,
                            "kind": "battery_profile_threshold",
                            "threshold_percent": threshold_value,
                            "profile_id": profile_id,
                            "when_below_or_equal": when_below_or_equal,
                            "cooldown_secs": cooldown_value
                        });
                        if !require_ac_value.is_null() {
                            rule_json["require_ac"] = require_ac_value;
                        }
                        make_client().and_then(|client| {
                            client.set_automation_rule(&rule_id_for_call, &rule_json.to_string())
                        })
                    },
                    move |result| {
                        button_for_recv.set_sensitive(true);
                        match result {
                            Ok(_) => {
                                store_write_feedback_state(
                                    "Automation rule",
                                    "Saved",
                                    "Battery threshold rule edits saved to the daemon.",
                                );
                                let _ = request_dashboard_refresh();
                            }
                            Err(error) => {
                                store_write_feedback_state(
                                    "Automation rule",
                                    "Save error",
                                    &format!("Failed: {error}"),
                                );
                            }
                        }
                    },
                );
            });

            let rule_id_for_delete = rule_id.clone();
            delete.connect_clicked(move |button| {
                button.set_sensitive(false);
                let button_for_recv = button.clone();
                let rule_id_for_call = rule_id_for_delete.clone();
                spawn_dbus_call(
                    move || {
                        make_client()
                            .and_then(|client| client.remove_automation_rule(&rule_id_for_call))
                    },
                    move |result| {
                        button_for_recv.set_sensitive(true);
                        match result {
                            Ok(_) => {
                                store_write_feedback_state(
                                    "Automation rule",
                                    "Deleted",
                                    "Rule removed from the daemon.",
                                );
                                let _ = request_dashboard_refresh();
                            }
                            Err(error) => {
                                store_write_feedback_state(
                                    "Automation rule",
                                    "Delete error",
                                    &format!("Failed: {error}"),
                                );
                            }
                        }
                    },
                );
            });

            continue;
        }

        let AutomationRuleKind::FastChargeUntilThreshold {
            threshold_percent,
            fast_charge_profile_id,
            protect_profile_id,
            require_ac,
            cooldown_secs,
        } = &rule.kind
        else {
            unreachable!("all automation rule kinds are handled before fast-charge editor");
        };

        let threshold = gtk4::SpinButton::with_range(1.0, 100.0, 1.0);
        threshold.set_value((*threshold_percent).into());
        threshold.set_digits(0);
        threshold.set_valign(gtk4::Align::Center);
        let threshold_row = adw::ActionRow::builder()
            .title("Battery threshold")
            .subtitle("Switch from fast charge to protect profile at or above this percent")
            .selectable(false)
            .build();
        threshold_row.add_suffix(&threshold);
        expander.add_row(&threshold_row);

        let require_ac_switch = gtk4::Switch::new();
        require_ac_switch.set_active(*require_ac);
        require_ac_switch.set_valign(gtk4::Align::Center);
        let require_ac_row = adw::ActionRow::builder()
            .title("Require AC")
            .subtitle("Skip the rule unless the AC adapter is online")
            .selectable(false)
            .build();
        require_ac_row.add_suffix(&require_ac_switch);
        expander.add_row(&require_ac_row);

        let fast_profile = profile_dropdown(&profile_ids, fast_charge_profile_id);
        fast_profile.set_valign(gtk4::Align::Center);
        fast_profile.set_sensitive(!profile_ids.is_empty());
        let fast_row = adw::ActionRow::builder()
            .title("Fast-charge profile")
            .subtitle("Profile applied while battery is below the threshold")
            .selectable(false)
            .build();
        fast_row.add_suffix(&fast_profile);
        expander.add_row(&fast_row);

        let protect_profile = profile_dropdown(&profile_ids, protect_profile_id);
        protect_profile.set_valign(gtk4::Align::Center);
        protect_profile.set_sensitive(!profile_ids.is_empty());
        let protect_row = adw::ActionRow::builder()
            .title("Protect profile")
            .subtitle("Profile applied when battery reaches the threshold")
            .selectable(false)
            .build();
        protect_row.add_suffix(&protect_profile);
        expander.add_row(&protect_row);

        if let Some(summary) = advanced_cpu_profile_summary(
            bundle.hardware_profiles.get(fast_charge_profile_id),
            bundle.ryzen_backend_status.as_ref(),
        ) {
            expander.add_row(
                &adw::ActionRow::builder()
                    .title("Fast-charge advanced CPU actions")
                    .subtitle(summary.as_str())
                    .selectable(false)
                    .build(),
            );
        }
        if let Some(summary) = advanced_cpu_profile_summary(
            bundle.hardware_profiles.get(protect_profile_id),
            bundle.ryzen_backend_status.as_ref(),
        ) {
            expander.add_row(
                &adw::ActionRow::builder()
                    .title("Protect advanced CPU actions")
                    .subtitle(summary.as_str())
                    .selectable(false)
                    .build(),
            );
        }

        let cooldown = gtk4::SpinButton::with_range(0.0, 86_400.0, 30.0);
        cooldown.set_value(*cooldown_secs as f64);
        cooldown.set_digits(0);
        cooldown.set_valign(gtk4::Align::Center);
        let cooldown_row = adw::ActionRow::builder()
            .title("Cooldown seconds")
            .subtitle("Minimum delay between automatic observer runs for this rule")
            .selectable(false)
            .build();
        cooldown_row.add_suffix(&cooldown);
        expander.add_row(&cooldown_row);

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
        let save = gtk4::Button::builder()
            .label("Save")
            .css_classes(["suggested-action", "pill"])
            .valign(gtk4::Align::Center)
            .build();
        save.set_sensitive(profile_ids.len() >= 2);
        let delete = gtk4::Button::builder()
            .label("Delete")
            .css_classes(["destructive-action", "pill"])
            .valign(gtk4::Align::Center)
            .build();

        let action_row = adw::ActionRow::builder()
            .title("Rule actions")
            .subtitle(if profile_ids.len() >= 2 {
                "Preview, apply once, save edits, or delete this rule"
            } else {
                "Create at least two hardware profiles before saving edits"
            })
            .selectable(false)
            .build();
        action_row.add_suffix(&preview);
        action_row.add_suffix(&run);
        action_row.add_suffix(&save);
        action_row.add_suffix(&delete);
        expander.add_row(&action_row);
        group.add(&expander);

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

        let rule_id_for_save = rule_id.clone();
        let label_for_save = rule.label.clone();
        let profile_ids_for_save = profile_ids.clone();
        save.connect_clicked(move |button| {
            let Some(fast_profile_id) = selected_dropdown_value(&fast_profile) else {
                store_write_feedback_state(
                    "Automation rule",
                    "Save error",
                    "Failed: select a fast-charge profile",
                );
                return;
            };
            let Some(protect_profile_id) = selected_dropdown_value(&protect_profile) else {
                store_write_feedback_state(
                    "Automation rule",
                    "Save error",
                    "Failed: select a protect profile",
                );
                return;
            };
            if fast_profile_id == protect_profile_id {
                store_write_feedback_state(
                    "Automation rule",
                    "Save error",
                    "Failed: fast-charge and protect profiles must be different",
                );
                return;
            }
            if !profile_ids_for_save.contains(&fast_profile_id)
                || !profile_ids_for_save.contains(&protect_profile_id)
            {
                store_write_feedback_state(
                    "Automation rule",
                    "Save error",
                    "Failed: selected profile no longer exists",
                );
                return;
            }
            let enabled_value = enabled.is_active();
            let threshold_value = threshold.value_as_int();
            let require_ac_value = require_ac_switch.is_active();
            let cooldown_value = cooldown.value_as_int();

            button.set_sensitive(false);
            let button_for_recv = button.clone();
            let rule_id_for_call = rule_id_for_save.clone();
            let label_for_call = label_for_save.clone();
            spawn_dbus_call(
                move || {
                    let rule_json = serde_json::json!({
                        "schema_version": 1,
                        "label": label_for_call,
                        "enabled": enabled_value,
                        "kind": "fast_charge_until_threshold",
                        "threshold_percent": threshold_value,
                        "fast_charge_profile_id": fast_profile_id,
                        "protect_profile_id": protect_profile_id,
                        "require_ac": require_ac_value,
                        "cooldown_secs": cooldown_value
                    })
                    .to_string();
                    make_client().and_then(|client| {
                        client.set_automation_rule(&rule_id_for_call, &rule_json)
                    })
                },
                move |result| {
                    button_for_recv.set_sensitive(true);
                    match result {
                        Ok(_) => {
                            store_write_feedback_state(
                                "Automation rule",
                                "Saved",
                                "Rule edits saved to the daemon.",
                            );
                            let _ = request_dashboard_refresh();
                        }
                        Err(error) => {
                            store_write_feedback_state(
                                "Automation rule",
                                "Save error",
                                &format!("Failed: {error}"),
                            );
                        }
                    }
                },
            );
        });

        let rule_id_for_delete = rule_id.clone();
        delete.connect_clicked(move |button| {
            button.set_sensitive(false);
            let button_for_recv = button.clone();
            let rule_id_for_call = rule_id_for_delete.clone();
            spawn_dbus_call(
                move || {
                    make_client()
                        .and_then(|client| client.remove_automation_rule(&rule_id_for_call))
                },
                move |result| {
                    button_for_recv.set_sensitive(true);
                    match result {
                        Ok(_) => {
                            store_write_feedback_state(
                                "Automation rule",
                                "Deleted",
                                "Rule removed from the daemon.",
                            );
                            let _ = request_dashboard_refresh();
                        }
                        Err(error) => {
                            store_write_feedback_state(
                                "Automation rule",
                                "Delete error",
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

fn profile_dropdown(profile_ids: &[String], selected_profile_id: &str) -> gtk4::DropDown {
    let values = profile_ids.iter().map(String::as_str).collect::<Vec<_>>();
    let model = gtk4::StringList::new(&values);
    let chooser = gtk4::DropDown::builder().model(&model).build();
    if let Some(position) = profile_ids
        .iter()
        .position(|profile_id| profile_id == selected_profile_id)
    {
        chooser.set_selected(position as u32);
    }
    chooser
}

fn automation_profile_action_summary(profile: &HardwareProfile) -> String {
    let mut actions = Vec::new();
    if let Some(value) = &profile.actions.platform_profile {
        actions.push(format!("platform={value}"));
    }
    if let Some(value) = &profile.actions.battery_charge_type {
        actions.push(format!("charge={value}"));
    }
    if let Some(value) = &profile.actions.gpu_mode {
        actions.push(format!("gpu={value}"));
    }
    if let Some(request) = &profile.actions.keyboard_rgb {
        actions.push(format!(
            "rgb={} {}",
            request.effect,
            request
                .colors
                .values()
                .next()
                .map(String::as_str)
                .unwrap_or("no-color")
        ));
    }
    if let Some(value) = &profile.actions.cpu_governor {
        actions.push(format!("governor={value}"));
    }
    if let Some(value) = &profile.actions.cpu_epp {
        actions.push(format!("EPP={value}"));
    }
    if let Some(value) = &profile.actions.cpu_boost {
        actions.push(format!("boost={value}"));
    }
    if let Some(value) = &profile.actions.conservation_mode {
        actions.push(format!("conservation={value}"));
    }
    if let Some(value) = &profile.actions.amd_gpu_dpm_force_level {
        actions.push(format!("AMD DPM={value}"));
    }
    if let Some(value) = &profile.actions.curve_optimizer_all_core {
        actions.push(format!("CO={value}"));
    }
    for (attribute_id, value) in &profile.actions.firmware_attributes {
        actions.push(format!("{attribute_id}={value}"));
    }
    if actions.is_empty() {
        "no actions".to_owned()
    } else {
        actions.join(", ")
    }
}

fn advanced_cpu_profile_summary(
    profile: Option<&HardwareProfile>,
    ryzen_status: Option<&RyzenBackendStatus>,
) -> Option<String> {
    let profile = profile?;
    let mut actions = Vec::new();
    if let Some(value) = &profile.actions.cpu_governor {
        actions.push(format!("governor={value}"));
    }
    if let Some(value) = &profile.actions.cpu_epp {
        actions.push(format!("EPP={value}"));
    }
    if let Some(value) = &profile.actions.cpu_boost {
        actions.push(format!("boost={value}"));
    }
    if let Some(value) = &profile.actions.curve_optimizer_all_core {
        let readback = ryzen_status
            .map(|status| match status.curve_optimizer_readback_status {
                CurveOptimizerReadbackStatus::WriteOnly => "write-only",
                CurveOptimizerReadbackStatus::Verified => "read-back available",
                CurveOptimizerReadbackStatus::Failed => "read-back failed",
            })
            .unwrap_or("read-back unknown");
        actions.push(format!("CO={value} ({readback})"));
    }
    if actions.is_empty() {
        None
    } else {
        Some(format!(
            "{}. These run through profile apply with daemon policy; bad CO values can destabilize the system.",
            actions.join(", ")
        ))
    }
}

fn automation_rule_subtitle(
    rule_id: &str,
    enabled: bool,
    last_run: Option<&legion_common::AutomationRuleApplyRun>,
) -> String {
    let enabled_label = if enabled { "enabled" } else { "disabled" };
    let Some(last_run) = last_run else {
        return format!("{rule_id} · {enabled_label} · never run");
    };
    let action_label = match &last_run.profile_run {
        Some(run) if run.completed => {
            format!(
                "applied {} ({} result(s))",
                run.profile_id,
                run.results.len()
            )
        }
        Some(run) => format!("stopped {} ({})", run.profile_id, run.message),
        None if last_run.evaluation.matched => "matched but no profile run".to_owned(),
        None => "skipped".to_owned(),
    };
    format!(
        "{rule_id} · {enabled_label} · last: {action_label} · {}",
        last_run.evaluation.reason
    )
}

fn trigger_label(trigger_id: &str) -> String {
    match trigger_id {
        "ac_connected" => "AC plugged in".to_owned(),
        "ac_disconnected" => "AC unplugged".to_owned(),
        "resume" => "Resume from sleep".to_owned(),
        "platform_profile_changed" => "Platform profile changed".to_owned(),
        "desktop_power_profile_changed" => "Desktop power profile changed".to_owned(),
        "gpu_mode_reboot_completed" => "GPU reboot completed".to_owned(),
        "manual" => "Manual test trigger".to_owned(),
        other => other.replace('_', " "),
    }
}
