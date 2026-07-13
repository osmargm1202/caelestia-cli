#![allow(dead_code)] // Pure interfaces are consumed by the Task 5 Hyprland runtime.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;

use crate::cli::{MatchTypeArg, ResizerArgs};
use crate::ipc::hypr;

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

fn validate_user_rule(rule: &WindowRule) -> Result<()> {
    if matches!(rule.match_type, MatchType::TitleRegex) {
        Regex::new(&rule.name)
            .map_err(|error| anyhow::anyhow!("invalid regex pattern {:?}: {error}", rule.name))?;
    }
    Ok(())
}

#[derive(Default)]
struct RateLimiter {
    last_seen: HashMap<String, Instant>,
}

impl RateLimiter {
    fn suppressed_at(&mut self, address: &str, now: Instant) -> bool {
        if self
            .last_seen
            .get(address)
            .is_some_and(|last| now.duration_since(*last) < Duration::from_secs(1))
        {
            return true;
        }
        self.last_seen.insert(address.to_owned(), now);
        false
    }

    fn suppressed(&mut self, address: &str) -> bool {
        self.suppressed_at(address, Instant::now())
    }
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

fn address_with_prefix(address: &str) -> String {
    if address.starts_with("0x") {
        address.to_owned()
    } else {
        format!("0x{address}")
    }
}

fn client_by_address(address: &str) -> Result<Option<Value>> {
    let address = address_with_prefix(address);
    Ok(hypr::message_json("clients")?
        .as_array()
        .and_then(|clients| clients.iter().find(|client| client["address"] == address))
        .cloned())
}

fn command_resize(width: &str, height: &str, address: &str, lua: bool) -> String {
    if lua {
        lua_resize(width, height, address)
    } else {
        legacy_resize(width, height, address)
    }
}

fn command_move(x: i64, y: i64, address: &str, lua: bool) -> String {
    if lua {
        lua_move(x, y, address)
    } else {
        legacy_move(x, y, address)
    }
}

fn command_float(address: &str, lua: bool) -> String {
    if lua {
        lua_float(address)
    } else {
        legacy_float(address)
    }
}

fn command_center(lua: bool) -> String {
    if lua { lua_center() } else { legacy_center() }.to_owned()
}

fn apply_pip(address: &str, lua: bool) -> Result<()> {
    let address = address_with_prefix(address);
    let Some(window) = client_by_address(&address)? else {
        return Ok(());
    };
    if !window["floating"].as_bool().unwrap_or(false) {
        return Ok(());
    }
    let workspace_name = window.pointer("/workspace/name").and_then(Value::as_str);
    let workspaces = hypr::message_json("workspaces")?;
    let Some(workspace) = workspaces.as_array().and_then(|items| {
        items
            .iter()
            .find(|item| item["name"].as_str() == workspace_name)
    }) else {
        return Ok(());
    };
    let monitor_id = workspace["monitorID"].as_i64();
    let monitors = hypr::message_json("monitors")?;
    let Some(monitor) = monitors
        .as_array()
        .and_then(|items| items.iter().find(|item| item["id"].as_i64() == monitor_id))
    else {
        return Ok(());
    };
    let Some(size) = window["size"].as_array() else {
        return Ok(());
    };
    let Some(geometry) = pip_geometry(
        WindowSize {
            width: size.first().and_then(Value::as_f64).unwrap_or(0.0),
            height: size.get(1).and_then(Value::as_f64).unwrap_or(0.0),
        },
        MonitorGeometry {
            x: monitor["x"].as_f64().unwrap_or(f64::NAN),
            y: monitor["y"].as_f64().unwrap_or(f64::NAN),
            width: monitor["width"].as_f64().unwrap_or(0.0),
            height: monitor["height"].as_f64().unwrap_or(0.0),
            scale: monitor["scale"].as_f64().unwrap_or(0.0),
        },
    ) else {
        return Ok(());
    };
    hypr::batch(&[
        command_resize(
            &geometry.width.to_string(),
            &geometry.height.to_string(),
            &address,
            lua,
        ),
        command_move(geometry.x, geometry.y, &address, lua),
    ])?;
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActionStep {
    Float,
    Pip,
    Resize,
    Center,
}

fn action_plan(rule: &WindowRule, already_floating: bool) -> Vec<ActionStep> {
    let mut steps = Vec::new();
    if (rule.actions.contains(&Action::Float) || rule.actions.contains(&Action::Pip))
        && !already_floating
    {
        steps.push(ActionStep::Float);
    }
    if rule.actions.contains(&Action::Pip) {
        steps.push(ActionStep::Pip);
        return steps;
    }
    steps.push(ActionStep::Resize);
    if rule.actions.contains(&Action::Center) {
        steps.push(ActionStep::Center);
    }
    steps
}

fn apply_actions(address: &str, rule: &WindowRule, lua: bool) -> Result<()> {
    let address = address_with_prefix(address);
    let floating = if rule.actions.contains(&Action::Float) || rule.actions.contains(&Action::Pip) {
        client_by_address(&address)?
            .is_some_and(|window| window["floating"].as_bool().unwrap_or(false))
    } else {
        false
    };
    let plan = action_plan(rule, floating);
    if plan.contains(&ActionStep::Pip) {
        if plan.contains(&ActionStep::Float) {
            hypr::batch(&[command_float(&address, lua)])?;
        }
        return apply_pip(&address, lua);
    }
    let commands = plan
        .into_iter()
        .filter_map(|step| match step {
            ActionStep::Float => Some(command_float(&address, lua)),
            ActionStep::Resize => Some(command_resize(&rule.width, &rule.height, &address, lua)),
            ActionStep::Center => Some(command_center(lua)),
            ActionStep::Pip => None,
        })
        .collect::<Vec<_>>();
    hypr::batch(&commands)?;
    Ok(())
}

fn matching_rule<'a>(
    rules: &'a [WindowRule],
    title: &str,
    initial: &str,
) -> Option<&'a WindowRule> {
    for rule in rules {
        if matches!(rule.match_type, MatchType::TitleRegex) && Regex::new(&rule.name).is_err() {
            crate::util::io::warn(&format!("invalid regex pattern in rule {:?}", rule.name));
            continue;
        }
        if matches_rule(rule, title, initial) {
            return Some(rule);
        }
    }
    None
}

struct Resizer {
    rules: Vec<WindowRule>,
    limiter: RateLimiter,
    lua: bool,
}

impl Resizer {
    fn new() -> Self {
        Self {
            rules: load_rules(),
            limiter: RateLimiter::default(),
            lua: hypr::is_lua_config(),
        }
    }

    fn handle_event(&mut self, event: WindowEvent) -> Result<()> {
        let (address, title, initial) = match event {
            WindowEvent::Open { address, title, .. } => (address, title.clone(), title),
            WindowEvent::Title { address } => {
                let Some(window) = client_by_address(&address)? else {
                    return Ok(());
                };
                (
                    address,
                    window["title"].as_str().unwrap_or("").to_owned(),
                    window["initialTitle"].as_str().unwrap_or("").to_owned(),
                )
            }
        };
        let Some(rule) = matching_rule(&self.rules, &title, &initial).cloned() else {
            return Ok(());
        };
        if self.limiter.suppressed(&address) {
            return Ok(());
        }
        apply_actions(&address, &rule, self.lua)
    }
}

fn daemon_loop<R, H, W>(stream: R, mut handle: H, mut warn: W) -> Result<()>
where
    R: Read,
    H: FnMut(WindowEvent) -> Result<()>,
    W: FnMut(String),
{
    let mut reader = BufReader::new(stream);
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            bail!("Hyprland socket2 reached EOF");
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match parse_event(line) {
            Some(event) => {
                if let Err(error) = handle(event) {
                    warn(format!("failed to handle Hyprland event: {error:#}"));
                }
            }
            None if line.starts_with("windowtitle") || line.starts_with("openwindow") => {
                warn(format!("failed to parse Hyprland event: {line}"));
            }
            None => {}
        }
    }
}

fn parse_actions(actions: &str) -> Vec<Action> {
    actions
        .split(',')
        .filter_map(|action| match action.trim().to_ascii_lowercase().as_str() {
            "float" => Some(Action::Float),
            "center" => Some(Action::Center),
            "pip" => Some(Action::Pip),
            _ => None,
        })
        .collect()
}

fn match_type(value: MatchTypeArg) -> MatchType {
    match value {
        MatchTypeArg::TitleContains => MatchType::TitleContains,
        MatchTypeArg::TitleExact => MatchType::TitleExact,
        MatchTypeArg::TitleRegex => MatchType::TitleRegex,
        MatchTypeArg::InitialTitle => MatchType::InitialTitle,
    }
}

fn run_rule(args: ResizerArgs, lua: bool) -> Result<()> {
    let pattern = args
        .pattern
        .ok_or_else(|| anyhow::anyhow!("missing window pattern"))?;
    if pattern.eq_ignore_ascii_case("pip") {
        let active = hypr::message_json("activewindow")?;
        let address = active["address"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("no active window found"))?;
        return apply_pip(address, lua);
    }
    let rule = WindowRule {
        name: pattern,
        match_type: match_type(
            args.match_type
                .ok_or_else(|| anyhow::anyhow!("missing match type"))?,
        ),
        width: args.width.ok_or_else(|| anyhow::anyhow!("missing width"))?,
        height: args
            .height
            .ok_or_else(|| anyhow::anyhow!("missing height"))?,
        actions: parse_actions(
            args.actions
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("missing actions"))?,
        ),
    };
    validate_user_rule(&rule)?;
    if rule.name.eq_ignore_ascii_case("active") {
        let active = hypr::message_json("activewindow")?;
        let address = active["address"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("no active window found"))?;
        return apply_actions(address, &rule, lua);
    }
    let clients = hypr::message_json("clients")?;
    let mut first_error = None;
    for window in clients.as_array().into_iter().flatten() {
        if matches_rule(
            &rule,
            window["title"].as_str().unwrap_or(""),
            window["initialTitle"].as_str().unwrap_or(""),
        ) {
            if let Some(address) = window["address"].as_str() {
                if let Err(error) = apply_actions(address, &rule, lua) {
                    crate::util::io::error(&format!(
                        "failed to apply window actions for {address}: {error:#}"
                    ));
                    first_error.get_or_insert(error);
                }
            }
        }
    }
    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

pub fn run(args: ResizerArgs) -> Result<()> {
    if args.daemon {
        let mut resizer = Resizer::new();
        return daemon_loop(
            hypr::socket2_stream()?,
            |event| resizer.handle_event(event),
            |warning| crate::util::io::warn(&warning),
        );
    }
    if args.pattern.is_none() {
        crate::util::io::info("Resizer daemon - use --daemon to start, 'pip' for quick pip mode, or provide pattern, match_type, width, height, and actions for active mode");
        return Ok(());
    }
    let lua = hypr::is_lua_config();
    run_rule(args, lua)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_continues_after_malformed_event() {
        use std::io::Write;
        use std::os::unix::net::UnixListener;
        use std::sync::{Arc, Mutex};

        let mut env = crate::test_support::EnvGuard::new();
        let dir = std::env::temp_dir().join(format!(
            "resizer-daemon-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let socket_dir = dir.join("hypr/testsig");
        std::fs::create_dir_all(&socket_dir).unwrap();
        let listener = UnixListener::bind(socket_dir.join(".socket2.sock")).unwrap();
        env.set("XDG_RUNTIME_DIR", &dir);
        env.set("HYPRLAND_INSTANCE_SIGNATURE", "testsig");

        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .write_all(
                    b"openwindow>>abc123,1,firefox,First\nopenwindow>>not-hex,broken\nwindowtitle>>deadbeef,Second\n",
                )
                .unwrap();
        });

        let events = Arc::new(Mutex::new(Vec::new()));
        let warnings = Arc::new(Mutex::new(Vec::new()));
        let event_sink = Arc::clone(&events);
        let warning_sink = Arc::clone(&warnings);
        let result = daemon_loop(
            crate::ipc::hypr::socket2_stream().unwrap(),
            move |event| {
                let mut events = event_sink.lock().unwrap();
                events.push(event);
                if events.len() == 1 {
                    anyhow::bail!("synthetic action failure");
                }
                Ok(())
            },
            move |warning| warning_sink.lock().unwrap().push(warning),
        );

        server.join().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(result.is_err(), "EOF must end the daemon");
        assert_eq!(events.lock().unwrap().len(), 2);
        assert_eq!(warnings.lock().unwrap().len(), 2);
    }

    #[test]
    fn pip_action_plan_floats_when_needed_and_excludes_other_actions() {
        let rule = WindowRule {
            name: "pip".into(),
            match_type: MatchType::TitleExact,
            width: "800".into(),
            height: "600".into(),
            actions: vec![Action::Pip, Action::Center],
        };
        assert_eq!(
            action_plan(&rule, false),
            vec![ActionStep::Float, ActionStep::Pip]
        );
        assert_eq!(action_plan(&rule, true), vec![ActionStep::Pip]);
    }

    #[test]
    fn normal_action_plan_orders_float_resize_center() {
        let rule = WindowRule {
            name: "x".into(),
            match_type: MatchType::TitleExact,
            width: "800".into(),
            height: "600".into(),
            actions: vec![Action::Center, Action::Float],
        };
        assert_eq!(
            action_plan(&rule, false),
            vec![ActionStep::Float, ActionStep::Resize, ActionStep::Center]
        );
    }

    #[test]
    fn user_regex_reports_invalid_pattern() {
        let rule = WindowRule {
            name: "[".into(),
            match_type: MatchType::TitleRegex,
            width: "800".into(),
            height: "600".into(),
            actions: vec![],
        };
        assert!(validate_user_rule(&rule)
            .unwrap_err()
            .to_string()
            .contains("invalid regex pattern"));
    }

    #[test]
    fn rate_limiter_is_per_address_and_expires_after_one_second() {
        let start = std::time::Instant::now();
        let mut limiter = RateLimiter::default();
        assert!(!limiter.suppressed_at("abc", start));
        assert!(limiter.suppressed_at("abc", start + std::time::Duration::from_millis(999)));
        assert!(!limiter.suppressed_at("def", start + std::time::Duration::from_millis(999)));
        assert!(!limiter.suppressed_at("abc", start + std::time::Duration::from_secs(1)));
    }

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
