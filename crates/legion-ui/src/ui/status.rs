use crate::{capability_status_label, risk_level_label, GpuModePending, UiStatus};
use adw::prelude::*;
use anyhow::Result;

use super::shared::{append_error, info_row};

pub fn status_page(
    status: Result<UiStatus>,
    gpu_pending: Result<Option<GpuModePending>>,
) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    match status {
        Ok(status) => append_status(&page, &status, gpu_pending),
        Err(error) => append_error(&page, &error),
    }

    page
}

fn append_status(
    page: &adw::PreferencesPage,
    status: &UiStatus,
    gpu_pending: Result<Option<GpuModePending>>,
) {
    let group = adw::PreferencesGroup::new();
    group.set_title("Detected Hardware");
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
    page.add(&group);

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
    page.add(&capabilities);
}

fn render_gpu_pending_row(pending: Result<Option<GpuModePending>>) -> String {
    match pending {
        Ok(opt) => legion_common::format_gpu_mode_pending_summary(opt.as_ref()),
        Err(error) => format!("state unavailable - {error}"),
    }
}
