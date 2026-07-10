use std::env;
use std::path::PathBuf;

use serde_json::Value;

fn home() -> PathBuf {
    PathBuf::from(env::var("HOME").expect("HOME not set"))
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    env::var(var).map(PathBuf::from).unwrap_or_else(|_| home().join(fallback))
}

pub fn config_dir() -> PathBuf { xdg("XDG_CONFIG_HOME", ".config") }
pub fn state_dir() -> PathBuf { xdg("XDG_STATE_HOME", ".local/state") }
pub fn cache_dir() -> PathBuf { xdg("XDG_CACHE_HOME", ".cache") }
pub fn pictures_dir() -> PathBuf { xdg("XDG_PICTURES_DIR", "Pictures") }
pub fn videos_dir() -> PathBuf { xdg("XDG_VIDEOS_DIR", "Videos") }

pub fn c_config_dir() -> PathBuf { config_dir().join("caelestia") }
pub fn c_state_dir() -> PathBuf { state_dir().join("caelestia") }
pub fn c_cache_dir() -> PathBuf { cache_dir().join("caelestia") }

pub fn user_config_path() -> PathBuf { c_config_dir().join("cli.json") }

pub fn screenshots_dir() -> PathBuf {
    env::var("CAELESTIA_SCREENSHOTS_DIR").map(PathBuf::from)
        .unwrap_or_else(|_| pictures_dir().join("Screenshots"))
}
pub fn screenshots_cache_dir() -> PathBuf { c_cache_dir().join("screenshots") }

pub fn recordings_dir() -> PathBuf {
    env::var("CAELESTIA_RECORDINGS_DIR").map(PathBuf::from)
        .unwrap_or_else(|_| videos_dir().join("Recordings"))
}
pub fn recording_path() -> PathBuf { c_state_dir().join("record/recording.mp4") }
pub fn recording_notif_path() -> PathBuf { c_state_dir().join("record/notifid.txt") }

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
        assert_eq!(user_config_path(), PathBuf::from("/tmp/xdgtest-config/caelestia/cli.json"));
        assert_eq!(screenshots_dir(), PathBuf::from("/tmp/xdgtest-pics/Screenshots"));
        assert_eq!(recordings_dir(), PathBuf::from("/tmp/xdgtest-vids/Recordings"));
        assert_eq!(recording_path(), PathBuf::from("/tmp/xdgtest-state/caelestia/record/recording.mp4"));

        std::env::set_var("CAELESTIA_SCREENSHOTS_DIR", "/tmp/shots");
        std::env::set_var("CAELESTIA_RECORDINGS_DIR", "/tmp/recs");
        assert_eq!(screenshots_dir(), PathBuf::from("/tmp/shots"));
        assert_eq!(recordings_dir(), PathBuf::from("/tmp/recs"));
        std::env::remove_var("CAELESTIA_SCREENSHOTS_DIR");
        std::env::remove_var("CAELESTIA_RECORDINGS_DIR");
    }
}
