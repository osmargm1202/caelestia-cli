#![allow(dead_code)] // Pure interfaces are consumed by the Task 5 Hyprland runtime.

use anyhow::Result;
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;

use crate::cli::ResizerArgs;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MatchType {
    TitleContains,
    TitleExact,
    TitleRegex,
    InitialTitle,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Float,
    Center,
    Pip,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct WindowRule {
    pub name: String,
    #[serde(rename = "matchType")]
    pub match_type: MatchType,
    pub width: String,
    pub height: String,
    pub actions: Vec<Action>,
}

pub fn default_rules() -> Vec<WindowRule> {
    vec![
        WindowRule {
            name: "(Bitwarden".into(),
            match_type: MatchType::TitleContains,
            width: "20%".into(),
            height: "54%".into(),
            actions: vec![Action::Float, Action::Center],
        },
        WindowRule {
            name: "^[Pp]icture(-| )in(-| )[Pp]icture$".into(),
            match_type: MatchType::TitleRegex,
            width: String::new(),
            height: String::new(),
            actions: vec![Action::Pip],
        },
    ]
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WindowEvent {
    Title {
        address: String,
    },
    Open {
        address: String,
        workspace: String,
        class: String,
        title: String,
    },
}

fn valid_address(address: &str) -> bool {
    !address.is_empty() && address.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn rules_from_config(config: &Value) -> Vec<WindowRule> {
    let Some(value) = config.pointer("/resizer/rules") else {
        return default_rules();
    };
    match serde_json::from_value(value.clone()) {
        Ok(rules) => rules,
        Err(_) => {
            crate::util::io::warn("invalid config, falling back to default rules");
            default_rules()
        }
    }
}

pub fn load_rules() -> Vec<WindowRule> {
    rules_from_config(&crate::util::paths::get_config())
}

pub fn matches_rule(rule: &WindowRule, title: &str, initial_title: &str) -> bool {
    match rule.match_type {
        MatchType::InitialTitle => initial_title == rule.name,
        MatchType::TitleContains => title.contains(&rule.name),
        MatchType::TitleExact => title == rule.name,
        MatchType::TitleRegex => {
            Regex::new(&rule.name).is_ok_and(|pattern| pattern.is_match(title))
        }
    }
}

pub fn parse_event(event: &str) -> Option<WindowEvent> {
    if let Some(data) = event
        .strip_prefix("windowtitle>>>")
        .or_else(|| event.strip_prefix("windowtitle>>"))
    {
        let address = data.split(',').next()?.trim_start_matches('>');
        return valid_address(address).then(|| WindowEvent::Title {
            address: address.into(),
        });
    }

    let data = event
        .strip_prefix("openwindow>>>")
        .or_else(|| event.strip_prefix("openwindow>>"))?;
    let mut fields = data.splitn(4, ',');
    let address = fields.next()?.trim_start_matches('>');
    let workspace = fields.next()?;
    let class = fields.next()?;
    let title = fields.next()?;
    valid_address(address).then(|| WindowEvent::Open {
        address: address.into(),
        workspace: workspace.into(),
        class: class.into(),
        title: title.into(),
    })
}

pub fn legacy_resize(width: &str, height: &str, address: &str) -> String {
    format!("dispatch resizewindowpixel exact {width} {height},address:{address}")
}

pub fn legacy_move(x: i64, y: i64, address: &str) -> String {
    format!("dispatch movewindowpixel exact {x} {y},address:{address}")
}

pub fn legacy_float(address: &str) -> String {
    format!("dispatch togglefloating address:{address}")
}

pub fn legacy_center() -> &'static str {
    "dispatch centerwindow"
}

pub fn lua_resize(width: &str, height: &str, address: &str) -> String {
    format!(
        "dispatch hl.dsp.window.resize({{x = {width}, y = {height}, exact = true, window = \"address:{address}\"}})"
    )
}

pub fn lua_move(x: i64, y: i64, address: &str) -> String {
    format!("dispatch hl.dsp.window.move({{x = {x}, y = {y}, window = \"address:{address}\"}})")
}

pub fn lua_float(address: &str) -> String {
    format!("dispatch hl.dsp.window.float({{action = \"toggle\", window = \"address:{address}\"}})")
}

pub fn lua_center() -> &'static str {
    "dispatch hl.dsp.window.center()"
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowSize {
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MonitorGeometry {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub scale: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PipGeometry {
    pub width: i64,
    pub height: i64,
    pub x: i64,
    pub y: i64,
}

pub fn pip_geometry(window: WindowSize, monitor: MonitorGeometry) -> Option<PipGeometry> {
    if !window.width.is_finite()
        || window.width <= 0.0
        || !window.height.is_finite()
        || window.height <= 0.0
        || !monitor.x.is_finite()
        || !monitor.y.is_finite()
        || !monitor.width.is_finite()
        || monitor.width <= 0.0
        || !monitor.height.is_finite()
        || monitor.height <= 0.0
        || !monitor.scale.is_finite()
        || monitor.scale <= 0.0
    {
        return None;
    }

    let monitor_height = monitor.height / monitor.scale;
    let monitor_width = monitor.width / monitor.scale;
    let scale_factor = monitor_height / 4.0 / window.height;
    let scaled_width = window.width * scale_factor;
    let scaled_height = window.height * scale_factor;
    let offset = monitor_width.min(monitor_height) * 0.03;
    if !monitor_height.is_finite()
        || !monitor_width.is_finite()
        || !scale_factor.is_finite()
        || !scaled_width.is_finite()
        || !scaled_height.is_finite()
        || !offset.is_finite()
    {
        return None;
    }

    let width = (scaled_width as i64).max(200);
    let height = (scaled_height as i64).max(150);
    let x = monitor.x + monitor_width - width as f64 - offset;
    let y = monitor.y + monitor_height - height as f64 - offset;
    if !x.is_finite() || !y.is_finite() {
        return None;
    }

    Some(PipGeometry {
        width,
        height,
        x: x as i64,
        y: y as i64,
    })
}

/// Task 5 supplies live Hyprland lookups and daemon execution.
pub fn run(_args: ResizerArgs) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_rules_match_python() {
        let rules = default_rules();
        assert_eq!(rules[0].name, "(Bitwarden");
        assert_eq!(rules[0].match_type, MatchType::TitleContains);
        assert_eq!(rules[0].width, "20%");
        assert_eq!(rules[0].height, "54%");
        assert_eq!(rules[0].actions, vec![Action::Float, Action::Center]);
        assert_eq!(rules[1].match_type, MatchType::TitleRegex);
        assert_eq!(rules[1].actions, vec![Action::Pip]);
    }

    #[test]
    fn parses_title_events_with_both_separators() {
        assert_eq!(
            parse_event("windowtitle>>abc123,ignored"),
            Some(WindowEvent::Title {
                address: "abc123".into()
            })
        );
        assert_eq!(
            parse_event("windowtitle>>>DEADbeef,ignored"),
            Some(WindowEvent::Title {
                address: "DEADbeef".into()
            })
        );
    }

    #[test]
    fn parses_open_events_with_both_separators_and_title_commas() {
        let expected = Some(WindowEvent::Open {
            address: "abc123".into(),
            workspace: "2".into(),
            class: "firefox".into(),
            title: "A title, with comma".into(),
        });
        assert_eq!(
            parse_event("openwindow>>abc123,2,firefox,A title, with comma"),
            expected
        );
        assert_eq!(
            parse_event("openwindow>>>abc123,2,firefox,A title, with comma"),
            expected
        );
    }

    #[test]
    fn rejects_invalid_or_empty_addresses_without_panicking() {
        assert_eq!(parse_event("windowtitle>>not-hex,title"), None);
        assert_eq!(parse_event("windowtitle>>,title"), None);
        assert_eq!(parse_event("openwindow>>>xyz,1,class,title"), None);
        assert_eq!(parse_event("openwindow>>abc,too,few"), None);
    }

    #[test]
    fn deserializes_python_config_schema_and_falls_back_atomically() {
        let config = serde_json::json!({"resizer": {"rules": [{
            "name": "Vault", "matchType": "titleExact", "width": "640",
            "height": "480", "actions": ["float", "center"]
        }]}});
        let rules = rules_from_config(&config);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].match_type, MatchType::TitleExact);
        assert_eq!(rules[0].actions, vec![Action::Float, Action::Center]);

        let malformed = serde_json::json!({"resizer": {"rules": [{
            "name": "broken", "matchType": "unknown", "width": 10
        }]}});
        assert_eq!(rules_from_config(&malformed), default_rules());
        assert_eq!(rules_from_config(&serde_json::json!({})), default_rules());
    }

    #[test]
    fn all_python_match_types_match() {
        let rule = |name: &str, match_type: MatchType| WindowRule {
            name: name.into(),
            match_type,
            width: String::new(),
            height: String::new(),
            actions: vec![],
        };
        assert!(matches_rule(
            &rule("term", MatchType::TitleContains),
            "a term here",
            ""
        ));
        assert!(matches_rule(
            &rule("whole", MatchType::TitleExact),
            "whole",
            ""
        ));
        assert!(matches_rule(
            &rule("first", MatchType::InitialTitle),
            "changed",
            "first"
        ));
        assert!(matches_rule(
            &rule("^[Pp]ip$", MatchType::TitleRegex),
            "Pip",
            ""
        ));
        assert!(!matches_rule(
            &rule("[", MatchType::TitleRegex),
            "anything",
            ""
        ));
    }

    #[test]
    fn dispatcher_strings_match_python() {
        assert_eq!(
            legacy_resize("800", "600", "0xabc"),
            "dispatch resizewindowpixel exact 800 600,address:0xabc"
        );
        assert_eq!(
            legacy_move(10, 20, "0xabc"),
            "dispatch movewindowpixel exact 10 20,address:0xabc"
        );
        assert_eq!(
            legacy_float("0xabc"),
            "dispatch togglefloating address:0xabc"
        );
        assert_eq!(legacy_center(), "dispatch centerwindow");
        assert_eq!(lua_resize("800", "600", "0xabc"), "dispatch hl.dsp.window.resize({x = 800, y = 600, exact = true, window = \"address:0xabc\"})");
        assert_eq!(
            lua_move(10, 20, "0xabc"),
            "dispatch hl.dsp.window.move({x = 10, y = 20, window = \"address:0xabc\"})"
        );
        assert_eq!(
            lua_float("0xabc"),
            "dispatch hl.dsp.window.float({action = \"toggle\", window = \"address:0xabc\"})"
        );
        assert_eq!(lua_center(), "dispatch hl.dsp.window.center()");
    }

    #[test]
    fn pip_geometry_matches_python_fixture() {
        let geometry = pip_geometry(
            WindowSize {
                width: 1920.0,
                height: 1080.0,
            },
            MonitorGeometry {
                x: 100.0,
                y: 50.0,
                width: 3840.0,
                height: 2160.0,
                scale: 2.0,
            },
        )
        .expect("valid geometry");
        assert_eq!(
            geometry,
            PipGeometry {
                width: 480,
                height: 270,
                x: 1507,
                y: 827
            }
        );
    }

    #[test]
    fn pip_geometry_clamps_small_windows_like_python() {
        let geometry = pip_geometry(
            WindowSize {
                width: 100.0,
                height: 1000.0,
            },
            MonitorGeometry {
                x: 0.0,
                y: 0.0,
                width: 1000.0,
                height: 800.0,
                scale: 1.0,
            },
        )
        .expect("valid geometry");
        assert_eq!((geometry.width, geometry.height), (200, 200));
        assert_eq!((geometry.x, geometry.y), (776, 576));
    }

    #[test]
    fn pip_geometry_rejects_zero_scale_and_dimensions() {
        let window = WindowSize {
            width: 1920.0,
            height: 1080.0,
        };
        let monitor = MonitorGeometry {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            scale: 1.0,
        };

        assert_eq!(
            pip_geometry(
                window,
                MonitorGeometry {
                    scale: 0.0,
                    ..monitor
                }
            ),
            None
        );
        assert_eq!(
            pip_geometry(
                WindowSize {
                    height: 0.0,
                    ..window
                },
                monitor
            ),
            None
        );
        assert_eq!(
            pip_geometry(
                WindowSize {
                    width: 0.0,
                    ..window
                },
                monitor
            ),
            None
        );
        assert_eq!(
            pip_geometry(
                window,
                MonitorGeometry {
                    width: 0.0,
                    ..monitor
                }
            ),
            None
        );
        assert_eq!(
            pip_geometry(
                window,
                MonitorGeometry {
                    height: 0.0,
                    ..monitor
                }
            ),
            None
        );
    }

    #[test]
    fn pip_geometry_rejects_non_finite_inputs() {
        let window = WindowSize {
            width: 1920.0,
            height: 1080.0,
        };
        let monitor = MonitorGeometry {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            scale: 1.0,
        };

        assert_eq!(
            pip_geometry(
                WindowSize {
                    width: f64::NAN,
                    ..window
                },
                monitor
            ),
            None
        );
        assert_eq!(
            pip_geometry(
                window,
                MonitorGeometry {
                    scale: f64::INFINITY,
                    ..monitor
                }
            ),
            None
        );
        assert_eq!(
            pip_geometry(
                window,
                MonitorGeometry {
                    x: f64::NEG_INFINITY,
                    ..monitor
                }
            ),
            None
        );
        assert_eq!(
            pip_geometry(
                window,
                MonitorGeometry {
                    y: f64::NAN,
                    ..monitor
                }
            ),
            None
        );
    }
}
