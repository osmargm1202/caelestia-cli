use std::env;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::Value;

fn home() -> PathBuf {
    PathBuf::from(env::var("HOME").expect("HOME not set"))
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    env::var(var)
        .map(PathBuf::from)
        .unwrap_or_else(|_| home().join(fallback))
}

pub fn config_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config")
}
#[allow(dead_code)] // consumed by record.rs (Task 8)
pub fn state_dir() -> PathBuf {
    xdg("XDG_STATE_HOME", ".local/state")
}
pub fn cache_dir() -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache")
}
pub fn pictures_dir() -> PathBuf {
    xdg("XDG_PICTURES_DIR", "Pictures")
}
#[allow(dead_code)] // consumed by record.rs (Task 8)
pub fn videos_dir() -> PathBuf {
    xdg("XDG_VIDEOS_DIR", "Videos")
}

pub fn c_config_dir() -> PathBuf {
    config_dir().join("caelestia")
}
#[allow(dead_code)] // consumed by record.rs (Task 8)
pub fn c_state_dir() -> PathBuf {
    state_dir().join("caelestia")
}
pub fn c_cache_dir() -> PathBuf {
    cache_dir().join("caelestia")
}

pub fn user_config_path() -> PathBuf {
    c_config_dir().join("cli.json")
}

pub fn screenshots_dir() -> PathBuf {
    env::var("CAELESTIA_SCREENSHOTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| pictures_dir().join("Screenshots"))
}
pub fn screenshots_cache_dir() -> PathBuf {
    c_cache_dir().join("screenshots")
}

pub fn wallpaper_path_path() -> PathBuf {
    c_state_dir().join("wallpaper/path.txt")
}
pub fn wallpaper_link_path() -> PathBuf {
    c_state_dir().join("wallpaper/current")
}
pub fn wallpaper_thumbnail_path() -> PathBuf {
    c_state_dir().join("wallpaper/thumbnail.jpg")
}
pub fn wallpapers_cache_dir() -> PathBuf {
    c_cache_dir().join("wallpapers")
}

#[allow(dead_code)] // consumed by record.rs (Task 8)
pub fn recordings_dir() -> PathBuf {
    env::var("CAELESTIA_RECORDINGS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| videos_dir().join("Recordings"))
}
#[allow(dead_code)] // consumed by record.rs (Task 8)
pub fn recording_path() -> PathBuf {
    c_state_dir().join("record/recording.mp4")
}
#[allow(dead_code)] // consumed by record.rs (Task 8)
pub fn recording_notif_path() -> PathBuf {
    c_state_dir().join("record/notifid.txt")
}

pub fn scheme_path() -> PathBuf {
    c_state_dir().join("scheme.json")
}

pub fn scheme_data_dir() -> PathBuf {
    c_data_dir().join("schemes")
}

pub fn scheme_cache_dir() -> PathBuf {
    c_cache_dir().join("schemes")
}

pub fn c_data_dir() -> PathBuf {
    let raw = env::var("XDG_DATA_HOME").ok();
    let base = raw
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local/share"));
    base.join("caelestia")
}

pub fn compute_hash<P: AsRef<Path>>(path: P) -> String {
    let path = path.as_ref();
    let h = path.as_os_str().as_bytes().iter().fold(0u64, |acc, b| {
        acc.wrapping_mul(31).wrapping_add(u64::from(*b))
    });
    format!("{:016x}", h)
}

/// Writes `value` atomically to `path`: serialises to JSON, writes to a temp file in
/// the same directory, then renames into place. Mirrors `python paths.atomic_dump`.
pub fn atomic_dump<T: serde::Serialize>(path: PathBuf, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating parent dir for {}", path.display()))?;
    }
    let tmp = path.with_extension("tmp");
    let serialised = serde_json::to_string_pretty(value)
        .with_context(|| format!("serialising for atomic dump to {}", path.display()))?;
    std::fs::write(&tmp, format!("{serialised}\n"))
        .with_context(|| format!("writing temp file {}", tmp.display()))?;
    std::fs::rename(&tmp, &path)
        .with_context(|| format!("renaming {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

/// ~/.config/caelestia/cli.json, `{}` when absent, warning (not error) on
/// invalid JSON — mirrors python paths.get_config().
pub fn get_config() -> Value {
    match std::fs::read_to_string(user_config_path()) {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => {
                crate::util::io::warn("failed to parse config, invalid JSON");
                Value::Object(Default::default())
            }
        },
        Err(_) => Value::Object(Default::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Both env-mutating tests are merged into one so cargo's parallel test
    // threads (which share the process environment) cannot race each
    // other's XDG_*/CAELESTIA_* var reads.
    #[test]
    fn xdg_dirs_respect_env_and_overrides_win() {
        std::env::set_var("XDG_CACHE_HOME", "/tmp/xdgtest-cache");
        std::env::set_var("XDG_CONFIG_HOME", "/tmp/xdgtest-config");
        std::env::set_var("XDG_PICTURES_DIR", "/tmp/xdgtest-pics");
        std::env::set_var("XDG_VIDEOS_DIR", "/tmp/xdgtest-vids");
        std::env::set_var("XDG_STATE_HOME", "/tmp/xdgtest-state");
        assert_eq!(c_cache_dir(), PathBuf::from("/tmp/xdgtest-cache/caelestia"));
        assert_eq!(
            user_config_path(),
            PathBuf::from("/tmp/xdgtest-config/caelestia/cli.json")
        );
        assert_eq!(
            screenshots_dir(),
            PathBuf::from("/tmp/xdgtest-pics/Screenshots")
        );
        assert_eq!(
            recordings_dir(),
            PathBuf::from("/tmp/xdgtest-vids/Recordings")
        );
        assert_eq!(
            recording_path(),
            PathBuf::from("/tmp/xdgtest-state/caelestia/record/recording.mp4")
        );

        std::env::set_var("CAELESTIA_SCREENSHOTS_DIR", "/tmp/shots");
        std::env::set_var("CAELESTIA_RECORDINGS_DIR", "/tmp/recs");
        assert_eq!(screenshots_dir(), PathBuf::from("/tmp/shots"));
        assert_eq!(recordings_dir(), PathBuf::from("/tmp/recs"));

        assert!(wallpaper_path_path().ends_with("caelestia/wallpaper/path.txt"));
        assert!(wallpaper_link_path().ends_with("caelestia/wallpaper/current"));
        assert!(wallpaper_thumbnail_path().ends_with("caelestia/wallpaper/thumbnail.jpg"));
        assert!(wallpapers_cache_dir().ends_with("caelestia/wallpapers"));

        std::env::remove_var("CAELESTIA_SCREENSHOTS_DIR");
        std::env::remove_var("CAELESTIA_RECORDINGS_DIR");
    }
}
