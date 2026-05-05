use adw::prelude::*;

pub fn automations_page() -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    // ── Planning banner ──────────────────────────────────────────────────────
    let info_group = adw::PreferencesGroup::new();
    info_group.set_title("Automation Engine");
    info_group.set_description(Some(
        "Automations let RatVantage react to system events — AC plug-in, lid open, time of day, \
         thermal thresholds — and apply power profiles, fan presets, or battery modes \
         automatically. Rule execution requires daemon support, which is planned for a future \
         release. Rules defined here show the intended behavior and will activate once the \
         backend ships.",
    ));
    page.add(&info_group);

    // ── Seed rules ───────────────────────────────────────────────────────────
    let rules_group = adw::PreferencesGroup::new();
    rules_group.set_title("Automation Rules");
    rules_group.set_description(Some(
        "Toggle example rules on or off. Rule editor and persistent storage ship in a future release.",
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
        "One-click starting points — pre-fills a new rule with a common configuration",
    ));

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
