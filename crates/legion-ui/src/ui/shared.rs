use crate::LegionControlClient;
use adw::prelude::*;
use anyhow::{anyhow, Result};
use legion_common::{WriteDryRunPlan, WriteExecutionResult, WriteExecutionStatus};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;

static BUS_ADDRESS: OnceLock<String> = OnceLock::new();

pub(crate) fn install_bus_address(address: &str) {
    let _ = BUS_ADDRESS.set(address.to_owned());
}

pub(crate) fn make_client() -> Result<LegionControlClient> {
    match BUS_ADDRESS.get() {
        Some(address) => LegionControlClient::address(address),
        None => LegionControlClient::system(),
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum PillTone {
    Neutral,
    Good,
    Warning,
    Error,
}

thread_local! {
    pub(crate) static DASHBOARD_REFRESH_HOOK: RefCell<Option<Rc<dyn Fn()>>> = RefCell::new(None);
    pub(crate) static WRITE_FEEDBACK_STATE: RefCell<HashMap<&'static str, WriteFeedbackState>> =
        RefCell::new(HashMap::new());
}

#[derive(Clone)]
pub(crate) struct WriteFeedbackState {
    pub title: String,
    pub subtitle: String,
}

pub(crate) fn default_write_feedback_state() -> WriteFeedbackState {
    WriteFeedbackState {
        title: "Apply result".to_owned(),
        subtitle:
            "No write attempted yet. If a request is blocked, the daemon will report why here."
                .to_owned(),
    }
}

pub(crate) fn load_write_feedback_state(capability_label: &'static str) -> WriteFeedbackState {
    WRITE_FEEDBACK_STATE.with(|state| {
        state
            .borrow()
            .get(capability_label)
            .cloned()
            .unwrap_or_else(default_write_feedback_state)
    })
}

pub(crate) fn store_write_feedback_state(
    capability_label: &'static str,
    title: &str,
    subtitle: &str,
) {
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

pub(crate) fn request_dashboard_refresh() -> bool {
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

pub fn write_feedback_title(result: Option<&WriteExecutionResult>) -> &'static str {
    match result.map(|result| result.status) {
        None => "Apply result",
        Some(WriteExecutionStatus::Applied) => "Applied and verified",
        Some(WriteExecutionStatus::BlockedByPolicy) => "Apply blocked by policy",
        Some(WriteExecutionStatus::BlockedByAuthorization) => "Apply blocked by authorization",
        Some(WriteExecutionStatus::Failed) => "Apply failed readback",
    }
}

pub fn write_feedback_subtitle(result: Option<&WriteExecutionResult>) -> String {
    match result {
        None => "No write attempted yet. If a request is blocked, the daemon will report why here."
            .to_owned(),
        Some(result) => {
            let readback = result
                .readback_value
                .as_deref()
                .map(|value| format!(" Read-back: {value}."))
                .unwrap_or_default();
            let guidance = match result.status {
                WriteExecutionStatus::Applied => {
                    " The daemon wrote the value and verified it with read-back."
                }
                WriteExecutionStatus::BlockedByPolicy => {
                    " The daemon is running without the matching --enable-*-write flag."
                }
                WriteExecutionStatus::BlockedByAuthorization => {
                    " Polkit denied the request or no authorization agent approved it."
                }
                WriteExecutionStatus::Failed => {
                    if result.message.contains("restored previous value") {
                        " The daemon restored the previous value because read-back did not match the request."
                    } else {
                        " The daemon did not confirm the requested value; check the message before retrying."
                    }
                }
            };
            format!("{}{}{}", result.message, readback, guidance)
        }
    }
}

pub(crate) fn write_feedback_row(capability_label: &'static str) -> adw::ActionRow {
    let state = load_write_feedback_state(capability_label);
    adw::ActionRow::builder()
        .title(&state.title)
        .subtitle(&state.subtitle)
        .selectable(false)
        .build()
}

pub(crate) fn clone_result<T: Clone>(result: &Result<T>) -> Result<T> {
    match result {
        Ok(value) => Ok(value.clone()),
        Err(error) => Err(anyhow!(error.to_string())),
    }
}

pub(crate) fn info_row(title: &str, value: &str) -> adw::ActionRow {
    adw::ActionRow::builder()
        .title(title)
        .subtitle(value)
        .selectable(false)
        .build()
}

pub(crate) fn section_note(text: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.set_wrap(true);
    label.set_xalign(0.0);
    label.set_halign(gtk4::Align::Fill);
    label.set_margin_top(4);
    label.set_margin_bottom(6);
    label.set_margin_start(12);
    label.set_margin_end(12);
    label.add_css_class("dim-label");
    label
}

pub(crate) fn status_pill(label: &str, tone: PillTone) -> gtk4::Label {
    let pill = gtk4::Label::new(Some(label));
    pill.set_halign(gtk4::Align::Start);
    pill.set_valign(gtk4::Align::Center);
    pill.add_css_class("caption");
    pill.add_css_class("pill");
    match tone {
        PillTone::Neutral => pill.add_css_class("dim-label"),
        PillTone::Good => pill.add_css_class("success"),
        PillTone::Warning => pill.add_css_class("warning"),
        PillTone::Error => pill.add_css_class("error"),
    }
    pill
}

pub(crate) fn state_tile(title: &str, value: &str, detail: &str, tone: PillTone) -> gtk4::Box {
    let tile = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    tile.set_hexpand(true);
    tile.set_margin_top(6);
    tile.set_margin_bottom(6);
    tile.set_margin_start(6);
    tile.set_margin_end(6);
    tile.add_css_class("card");

    let title_label = gtk4::Label::new(Some(title));
    title_label.set_xalign(0.0);
    title_label.set_margin_top(12);
    title_label.set_margin_start(12);
    title_label.set_margin_end(12);
    title_label.add_css_class("caption");
    title_label.add_css_class("dim-label");
    tile.append(&title_label);

    let value_label = gtk4::Label::new(Some(value));
    value_label.set_xalign(0.0);
    value_label.set_wrap(true);
    value_label.set_margin_start(12);
    value_label.set_margin_end(12);
    value_label.add_css_class("title-3");
    tile.append(&value_label);

    let detail_label = status_pill(detail, tone);
    detail_label.set_margin_start(12);
    detail_label.set_margin_end(12);
    detail_label.set_margin_bottom(12);
    tile.append(&detail_label);

    tile
}

pub(crate) fn append_error(page: &adw::PreferencesPage, error: &anyhow::Error) {
    let group = adw::PreferencesGroup::new();
    group.set_title("Daemon connection lost");
    group.set_description(Some("The GTK dashboard cannot communicate with the hardware daemon. Ensure legion-control-daemon is running."));

    let row = adw::ActionRow::builder()
        .title("Error details")
        .subtitle(error.to_string())
        .selectable(true)
        .build();
    group.add(&row);

    let refresh = gtk4::Button::builder()
        .label("Try to reconnect")
        .halign(gtk4::Align::Center)
        .margin_top(12)
        .css_classes(["suggested-action", "pill"])
        .build();
    refresh.connect_clicked(|_| {
        let _ = request_dashboard_refresh();
    });

    let box_ = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    box_.append(&refresh);
    group.add(&box_);

    page.add(&group);
}

pub(crate) fn selected_dropdown_value(chooser: &gtk4::DropDown) -> Option<String> {
    chooser
        .selected_item()
        .and_then(|item| item.downcast::<gtk4::StringObject>().ok())
        .map(|selected| selected.string().to_string())
}

pub(crate) fn render_dry_run_plan_summary(plan: &WriteDryRunPlan) -> String {
    format!(
        "{} -> {} via {} - reboot required {} - read-back required {}",
        plan.previous_value,
        plan.requested_value,
        plan.path,
        plan.reboot_required,
        plan.readback_required
    )
}

pub(crate) fn render_dry_run_recovery_summary(plan: &WriteDryRunPlan) -> String {
    if plan.rollback_instructions.is_empty() {
        "No rollback instructions were provided by the daemon.".to_owned()
    } else {
        plan.rollback_instructions.join(" ")
    }
}

pub(crate) fn spawn_dbus_call<F, T, G>(execute: F, on_result: G)
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
    G: FnOnce(Result<T>) + 'static,
{
    let (sender, receiver) = futures_channel::oneshot::channel();
    std::thread::spawn(move || {
        let res = execute();
        let _ = sender.send(res);
    });
    gtk4::glib::MainContext::default().spawn_local(async move {
        if let Ok(res) = receiver.await {
            on_result(res);
        }
    });
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_write_controls<F, G>(
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
    F: Fn(&str) -> Result<WriteExecutionResult> + Send + Sync + 'static,
    G: Fn(&WriteExecutionResult) + 'static,
{
    let group = adw::PreferencesGroup::new();
    group.set_title(title);

    let Some(choices) = choices else {
        group.add(&info_row(
            capability_label,
            "Unavailable: this hardware capability was not detected, so quick apply is disabled.",
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
    apply.add_css_class("suggested-action");
    apply.add_css_class("pill");
    apply.set_valign(gtk4::Align::Center);

    let row = adw::ActionRow::builder()
        .title(chooser_title)
        .subtitle(if can_apply {
            "Choose a value, then apply. The daemon verifies read-back."
        } else {
            "No runtime choices detected."
        })
        .selectable(false)
        .build();
    row.add_suffix(&chooser);
    row.add_suffix(&apply);
    row.set_activatable_widget(Some(&chooser));
    group.add(&row);

    let feedback_row = write_feedback_row(capability_label);
    group.add(&feedback_row);

    let feedback_row_for_click = feedback_row.clone();
    let execute = std::sync::Arc::new(execute);
    let on_result = std::rc::Rc::new(on_result);
    let apply_for_click = apply.clone();
    let chooser_for_click = chooser.clone();

    apply.connect_clicked(move |_| {
        let Some(selected) = chooser_for_click
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
        feedback_row_for_click.set_title("Apply in progress");
        feedback_row_for_click
            .set_subtitle("Request sent to the daemon; waiting for policy/auth/write result...");

        apply_for_click.set_sensitive(false);
        chooser_for_click.set_sensitive(false);

        let (sender, receiver) = futures_channel::oneshot::channel();
        let execute_clone = execute.clone();
        std::thread::spawn(move || {
            let result = execute_clone(&requested);
            let _ = sender.send(result);
        });

        let feedback_row_for_recv = feedback_row_for_click.clone();
        let apply_for_recv = apply_for_click.clone();
        let chooser_for_recv = chooser_for_click.clone();
        let on_result_clone = on_result.clone();

        gtk4::glib::MainContext::default().spawn_local(async move {
            let result = receiver.await;

            apply_for_recv.set_sensitive(true);
            chooser_for_recv.set_sensitive(true);

            match result {
                Ok(Ok(res)) => {
                    let title = write_feedback_title(Some(&res));
                    let subtitle = write_feedback_subtitle(Some(&res));
                    feedback_row_for_recv.set_title(title);
                    feedback_row_for_recv.set_subtitle(&subtitle);
                    store_write_feedback_state(capability_label, title, &subtitle);
                    on_result_clone(&res);
                }
                Ok(Err(error)) => {
                    feedback_row_for_recv.set_title("Apply error");
                    let subtitle = format!("Failed - daemon call could not be completed: {error}");
                    feedback_row_for_recv.set_subtitle(&subtitle);
                    store_write_feedback_state(capability_label, "Apply error", &subtitle);
                    let _ = request_dashboard_refresh();
                }
                Err(_) => {
                    feedback_row_for_recv.set_title("Apply error");
                    feedback_row_for_recv.set_subtitle(
                        "Failed - background task was cancelled before the daemon returned.",
                    );
                    store_write_feedback_state(
                        capability_label,
                        "Apply error",
                        "Failed - background task was cancelled before the daemon returned.",
                    );
                    let _ = request_dashboard_refresh();
                }
            }
        });
    });

    group
}

#[cfg(test)]
pub(crate) fn clear_dashboard_refresh_hook() {
    DASHBOARD_REFRESH_HOOK.with(|slot| {
        *slot.borrow_mut() = None;
    });
}

pub(crate) fn install_dashboard_refresh_hook(hook: Rc<dyn Fn()>) {
    DASHBOARD_REFRESH_HOOK.with(|slot| {
        *slot.borrow_mut() = Some(hook);
    });
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
