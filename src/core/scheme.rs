use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::util::paths::{atomic_dump, get_config, scheme_data_dir, scheme_path};

/// Supported scheme variants — mirrors the Python reference (`scheme_variants`).
pub const SCHEME_VARIANTS: &[&str] = &[
    "tonalspot",
    "vibrant",
    "expressive",
    "fidelity",
    "fruitsalad",
    "monochrome",
    "neutral",
    "rainbow",
    "content",
];

pub const SCHEME_MODES: &[&str] = &["dark", "light"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scheme {
    pub name: String,
    pub flavour: String,
    pub mode: String,
    pub variant: String,
    pub colours: HashMap<String, String>,
}

impl Scheme {
    /// Loads the persisted scheme, falling back to the Catppuccin/Mocha/Dark/Tonalspot
    /// defaults when no scheme file exists or it cannot be parsed (matches the Python
    /// `Scheme(None)` initialiser).
    pub fn load() -> Result<Self> {
        if let Ok(text) = std::fs::read_to_string(scheme_path()) {
            if let Ok(parsed) = serde_json::from_str::<Scheme>(&text) {
                return Ok(parsed);
            }
        }
        let mut default = Scheme {
            name: "catppuccin".into(),
            flavour: "mocha".into(),
            mode: "dark".into(),
            variant: "tonalspot".into(),
            colours: HashMap::new(),
        };
        default.colours = read_colours_from_file(&default.colours_path());
        Ok(default)
    }

    pub fn save(&self) -> Result<()> {
        atomic_dump(scheme_path(), self)
    }

    pub fn colours_path(&self) -> std::path::PathBuf {
        Self::colours_path_for(&self.name, &self.flavour, &self.mode)
    }

    pub fn colours_path_for(name: &str, flavour: &str, mode: &str) -> std::path::PathBuf {
        scheme_data_dir()
            .join(name)
            .join(flavour)
            .join(mode)
            .with_extension("txt")
    }

    pub fn names() -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(scheme_data_dir())
            .map_err(|_| ())
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let name = entry.file_name().to_string_lossy().to_string();
                entry.file_type().ok().filter(|t| t.is_dir())?;
                Some(name)
            })
            .collect();
        names.sort();
        names.push("dynamic".into());
        names
    }

    pub fn flavours(name: &str) -> Vec<String> {
        if name == "dynamic" {
            return vec!["default".into(), "hard".into()];
        }
        let dir = scheme_data_dir().join(name);
        let mut flavours: Vec<String> = std::fs::read_dir(dir)
            .map_err(|_| ())
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let name = entry.file_name().to_string_lossy().to_string();
                entry.file_type().ok().filter(|t| t.is_dir())?;
                Some(name)
            })
            .collect();
        flavours.sort();
        flavours
    }

    pub fn modes(name: &str, flavour: &str) -> Vec<String> {
        if name == "dynamic" {
            return SCHEME_MODES.iter().map(|s| s.to_string()).collect();
        }
        let dir = scheme_data_dir().join(name).join(flavour);
        let mut modes: Vec<String> = std::fs::read_dir(dir)
            .map_err(|_| ())
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.is_file() {
                    Some(path.file_stem()?.to_string_lossy().to_string())
                } else {
                    None
                }
            })
            .collect();
        modes.sort();
        modes
    }

    #[allow(dead_code)]
    pub fn extra_record_args(&self) -> Result<Vec<String>> {
        let cfg = get_config();
        if let Some(extra) = cfg.get("record").and_then(|r| r.get("extraArgs")) {
            if let Some(arr) = extra.as_array() {
                return arr
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .map(str::to_owned)
                            .ok_or_else(|| anyhow::anyhow!("record.extraArgs must be strings"))
                    })
                    .collect();
            }
            return Err(anyhow::anyhow!(
                "Config option 'record.extraArgs' should be an array"
            ));
        }
        Ok(Vec::new())
    }
}

pub fn read_colours_from_file(path: &Path) -> HashMap<String, String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    text.lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let mut parts = line.splitn(2, ' ');
            let key = parts.next()?.trim().to_string();
            let value = parts.next()?.trim().to_string();
            if key.is_empty() || value.is_empty() {
                None
            } else {
                Some((key, value))
            }
        })
        .collect()
}

impl std::fmt::Display for Scheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Current scheme:\n    Name: {}\n    Flavour: {}\n    Mode: {}\n    Variant: {}\n    Colours: {}\n",
            self.name, self.flavour, self.mode, self.variant,
            self.colours.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_constants_match_python_reference() {
        let python =
            "tonalspot vibrant expressive fidelity fruitsalad monochrome neutral rainbow content";
        let parsed: Vec<&str> = python.split_whitespace().collect();
        assert_eq!(parsed, SCHEME_VARIANTS);
    }

    #[test]
    fn modes_constants_match_python_reference() {
        assert_eq!(SCHEME_MODES, &["dark", "light"]);
    }

    #[test]
    fn colours_path_layout_matches_python_reference() {
        let scheme = Scheme {
            name: "catppuccin".into(),
            flavour: "mocha".into(),
            mode: "dark".into(),
            variant: "tonalspot".into(),
            colours: HashMap::new(),
        };
        let path = scheme.colours_path();
        assert!(path.ends_with("catppuccin/mocha/dark.txt"));
    }
}
