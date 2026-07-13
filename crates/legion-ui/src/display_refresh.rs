use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::PathBuf,
    process::{Command, Output},
};

const CONFIG_SCHEMA_VERSION: u32 = 1;
const CONFIG_FILE: &str = "display-refresh.json";
const KSCREEN_DOCTOR: &str = "kscreen-doctor";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "mode", content = "hz", rename_all = "snake_case")]
pub enum DisplayRefreshPreference {
    Keep,
    Highest,
    Lowest,
    Hertz(u32),
}

impl DisplayRefreshPreference {
    pub fn label(&self) -> String {
        match self {
            Self::Keep => "Keep current".to_owned(),
            Self::Highest => "Highest available".to_owned(),
            Self::Lowest => "Lowest available".to_owned(),
            Self::Hertz(hz) => format!("{hz} Hz"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DisplayRefreshAutomation {
    pub schema_version: u32,
    pub platform_profiles: BTreeMap<String, DisplayRefreshPreference>,
}

impl Default for DisplayRefreshAutomation {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            platform_profiles: [
                ("balanced".to_owned(), DisplayRefreshPreference::Highest),
                ("performance".to_owned(), DisplayRefreshPreference::Highest),
                ("max-power".to_owned(), DisplayRefreshPreference::Highest),
                ("low-power".to_owned(), DisplayRefreshPreference::Keep),
                ("custom".to_owned(), DisplayRefreshPreference::Keep),
            ]
            .into_iter()
            .collect(),
        }
    }
}

impl DisplayRefreshAutomation {
    pub fn preference_for(&self, platform_profile: &str) -> DisplayRefreshPreference {
        self.platform_profiles
            .get(platform_profile)
            .cloned()
            .unwrap_or(DisplayRefreshPreference::Keep)
    }
}

#[derive(Debug, Deserialize)]
struct KscreenConfiguration {
    outputs: Vec<KscreenOutput>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KscreenOutput {
    connected: bool,
    enabled: bool,
    current_mode_id: String,
    modes: Vec<KscreenMode>,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KscreenMode {
    id: String,
    refresh_rate: f64,
    size: KscreenSize,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct KscreenSize {
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ModeChange {
    output_name: String,
    previous_mode_id: String,
    requested_mode_id: String,
    requested_hz: u32,
}

pub fn display_refresh_config_path() -> Result<PathBuf> {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(path).join("ratvantage").join(CONFIG_FILE));
    }
    let home = env::var_os("HOME").ok_or_else(|| anyhow!("HOME is not set"))?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("ratvantage")
        .join(CONFIG_FILE))
}

pub fn load_display_refresh_automation() -> Result<DisplayRefreshAutomation> {
    let path = display_refresh_config_path()?;
    load_display_refresh_automation_from(&path)
}

fn load_display_refresh_automation_from(
    path: &std::path::Path,
) -> Result<DisplayRefreshAutomation> {
    if !path.exists() {
        return Ok(DisplayRefreshAutomation::default());
    }
    let contents =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let automation: DisplayRefreshAutomation = serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    if automation.schema_version != CONFIG_SCHEMA_VERSION {
        bail!(
            "unsupported display refresh schema version {}",
            automation.schema_version
        );
    }
    Ok(automation)
}

pub fn save_display_refresh_automation(automation: &DisplayRefreshAutomation) -> Result<()> {
    let path = display_refresh_config_path()?;
    save_display_refresh_automation_to(automation, &path)
}

fn save_display_refresh_automation_to(
    automation: &DisplayRefreshAutomation,
    path: &std::path::Path,
) -> Result<()> {
    if automation.schema_version != CONFIG_SCHEMA_VERSION {
        bail!(
            "unsupported display refresh schema version {}",
            automation.schema_version
        );
    }
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("display refresh config has no parent directory"))?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    let temporary = path.with_extension("json.tmp");
    let contents = serde_json::to_vec_pretty(automation)?;
    fs::write(&temporary, contents)
        .with_context(|| format!("failed to write {}", temporary.display()))?;
    fs::rename(&temporary, path)
        .with_context(|| format!("failed to replace {}", path.display()))?;
    Ok(())
}

pub fn available_internal_refresh_rates() -> Result<Vec<u32>> {
    if !is_kde_session() {
        return Ok(Vec::new());
    }
    let configuration = query_kscreen()?;
    let mut rates = BTreeSet::new();
    for output in configuration
        .outputs
        .iter()
        .filter(|output| output.connected && output.enabled && is_internal_output(&output.name))
    {
        let Some(current) = output
            .modes
            .iter()
            .find(|mode| mode.id == output.current_mode_id)
        else {
            continue;
        };
        for mode in output.modes.iter().filter(|mode| mode.size == current.size) {
            rates.insert(mode.refresh_rate.round() as u32);
        }
    }
    Ok(rates.into_iter().collect())
}

pub fn refresh_preference_for_platform(platform_profile: &str) -> Result<DisplayRefreshPreference> {
    Ok(load_display_refresh_automation()?.preference_for(platform_profile))
}

pub fn apply_display_refresh_preference(
    preference: &DisplayRefreshPreference,
) -> Result<Option<String>> {
    if matches!(preference, DisplayRefreshPreference::Keep) {
        return Ok(None);
    }
    if !is_kde_session() {
        return Ok(None);
    }
    let before = query_kscreen()?;
    let changes = plan_mode_changes(&before, preference)?;
    if changes.is_empty() {
        return Ok(None);
    }

    let requested_args = changes
        .iter()
        .map(|change| {
            format!(
                "output.{}.mode.{}",
                change.output_name, change.requested_mode_id
            )
        })
        .collect::<Vec<_>>();
    let apply = run_kscreen(requested_args.iter().map(String::as_str))?;
    if !apply.status.success() {
        rollback_mode_changes(&changes);
        bail!("KScreen rejected refresh change: {}", output_detail(&apply));
    }

    let after = query_kscreen()?;
    let mismatch = changes.iter().find(|change| {
        after
            .outputs
            .iter()
            .find(|output| output.name == change.output_name)
            .is_none_or(|output| output.current_mode_id != change.requested_mode_id)
    });
    if let Some(change) = mismatch {
        rollback_mode_changes(&changes);
        bail!(
            "refresh read-back mismatch on {}: requested mode {}",
            change.output_name,
            change.requested_mode_id
        );
    }

    Ok(Some(
        changes
            .iter()
            .map(|change| format!("{}={} Hz", change.output_name, change.requested_hz))
            .collect::<Vec<_>>()
            .join(", "),
    ))
}

fn query_kscreen() -> Result<KscreenConfiguration> {
    let output = run_kscreen(["--json"])?;
    if !output.status.success() {
        bail!("failed to query KScreen: {}", output_detail(&output));
    }
    serde_json::from_slice(&output.stdout).context("failed to parse KScreen output")
}

fn run_kscreen<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Output> {
    Command::new(KSCREEN_DOCTOR)
        .args(args)
        .output()
        .context("failed to run kscreen-doctor")
}

fn rollback_mode_changes(changes: &[ModeChange]) {
    let rollback_args = changes
        .iter()
        .map(|change| {
            format!(
                "output.{}.mode.{}",
                change.output_name, change.previous_mode_id
            )
        })
        .collect::<Vec<_>>();
    let _ = run_kscreen(rollback_args.iter().map(String::as_str));
}

fn plan_mode_changes(
    configuration: &KscreenConfiguration,
    preference: &DisplayRefreshPreference,
) -> Result<Vec<ModeChange>> {
    let mut changes = Vec::new();
    for output in configuration
        .outputs
        .iter()
        .filter(|output| output.connected && output.enabled && is_internal_output(&output.name))
    {
        let current = output
            .modes
            .iter()
            .find(|mode| mode.id == output.current_mode_id)
            .ok_or_else(|| anyhow!("current mode missing for {}", output.name))?;
        let candidates = output
            .modes
            .iter()
            .filter(|mode| mode.size == current.size)
            .collect::<Vec<_>>();
        let requested = match preference {
            DisplayRefreshPreference::Keep => None,
            DisplayRefreshPreference::Highest => candidates
                .into_iter()
                .max_by(|left, right| left.refresh_rate.total_cmp(&right.refresh_rate)),
            DisplayRefreshPreference::Lowest => candidates
                .into_iter()
                .min_by(|left, right| left.refresh_rate.total_cmp(&right.refresh_rate)),
            DisplayRefreshPreference::Hertz(hz) => candidates.into_iter().min_by(|left, right| {
                (left.refresh_rate - f64::from(*hz))
                    .abs()
                    .total_cmp(&(right.refresh_rate - f64::from(*hz)).abs())
            }),
        };
        let Some(requested) = requested else {
            continue;
        };
        if let DisplayRefreshPreference::Hertz(hz) = preference {
            if (requested.refresh_rate - f64::from(*hz)).abs() > 1.0 {
                bail!(
                    "{} does not support {hz} Hz at its current resolution",
                    output.name
                );
            }
        }
        if requested.id != current.id {
            changes.push(ModeChange {
                output_name: output.name.clone(),
                previous_mode_id: current.id.clone(),
                requested_mode_id: requested.id.clone(),
                requested_hz: requested.refresh_rate.round() as u32,
            });
        }
    }
    Ok(changes)
}

fn is_internal_output(name: &str) -> bool {
    name.starts_with("eDP-") || name.starts_with("LVDS-") || name.starts_with("DSI-")
}

fn is_kde_session() -> bool {
    env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .any(|desktop| desktop.eq_ignore_ascii_case("KDE"))
}

fn output_detail(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if stderr.is_empty() {
        format!("exit status {}", output.status)
    } else {
        stderr
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIGURATION: &str = r#"{
      "outputs": [{
        "connected": true,
        "enabled": true,
        "currentModeId": "60",
        "name": "eDP-1",
        "modes": [
          {"id":"60","refreshRate":60.03,"size":{"width":2560,"height":1600}},
          {"id":"165","refreshRate":165.04,"size":{"width":2560,"height":1600}},
          {"id":"other","refreshRate":240.0,"size":{"width":1920,"height":1080}}
        ]
      }]
    }"#;

    #[test]
    fn defaults_high_refresh_for_non_low_power_platform_modes() {
        let automation = DisplayRefreshAutomation::default();
        for profile in ["balanced", "performance", "max-power"] {
            assert_eq!(
                automation.preference_for(profile),
                DisplayRefreshPreference::Highest
            );
        }
        assert_eq!(
            automation.preference_for("low-power"),
            DisplayRefreshPreference::Keep
        );
    }

    #[test]
    fn highest_refresh_preserves_current_resolution() {
        let configuration: KscreenConfiguration = serde_json::from_str(CONFIGURATION).unwrap();
        let changes = plan_mode_changes(&configuration, &DisplayRefreshPreference::Highest)
            .expect("highest mode plan");
        assert_eq!(
            changes,
            vec![ModeChange {
                output_name: "eDP-1".to_owned(),
                previous_mode_id: "60".to_owned(),
                requested_mode_id: "165".to_owned(),
                requested_hz: 165,
            }]
        );
    }

    #[test]
    fn fixed_refresh_rejects_unavailable_rate() {
        let configuration: KscreenConfiguration = serde_json::from_str(CONFIGURATION).unwrap();
        let error = plan_mode_changes(&configuration, &DisplayRefreshPreference::Hertz(120))
            .expect_err("120 Hz should be unavailable");
        assert!(error.to_string().contains("does not support 120 Hz"));
    }

    #[test]
    fn automation_overrides_round_trip_atomically() {
        let directory =
            std::env::temp_dir().join(format!("ratvantage-display-refresh-{}", std::process::id()));
        let path = directory.join("display-refresh.json");
        let mut automation = DisplayRefreshAutomation::default();
        automation
            .platform_profiles
            .insert("balanced".to_owned(), DisplayRefreshPreference::Hertz(60));
        save_display_refresh_automation_to(&automation, &path).expect("save override");
        let loaded = load_display_refresh_automation_from(&path).expect("load override");
        assert_eq!(loaded, automation);
        assert!(!path.with_extension("json.tmp").exists());
        let _ = std::fs::remove_dir_all(directory);
    }
}
