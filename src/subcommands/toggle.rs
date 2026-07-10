use std::path::Path;

use anyhow::Result;
use serde_json::{json, Value};

use crate::cli::ToggleArgs;
use crate::ipc::hypr;
use crate::util::paths::get_config;

/// Python is_subset: dict→recurse, str→substring, list→subset, other→eq.
fn is_subset(superset: &Value, subset: &Value) -> bool {
    let (Some(sup), Some(sub)) = (superset.as_object(), subset.as_object()) else {
        return false;
    };
    sub.iter().all(|(k, v)| match sup.get(k) {
        None => false,
        Some(sv) => match v {
            Value::Object(_) => is_subset(sv, v),
            Value::String(s) => sv.as_str().is_some_and(|x| x.contains(s.as_str())),
            Value::Array(a) => sv
                .as_array()
                .is_some_and(|x| a.iter().all(|i| x.contains(i))),
            other => sv == other,
        },
    })
}

fn default_config() -> Value {
    json!({
        "communication": {
            "discord": {
                "enable": true,
                "match": [{"class": "discord"}],
                "command": ["discord"],
                "move": true,
            },
            "whatsapp": {
                "enable": true,
                "match": [{"class": "whatsapp"}],
                "move": true,
            },
        },
        "music": {
            "spotify": {
                "enable": true,
                "match": [{"class": "Spotify"}, {"initialTitle": "Spotify"}, {"initialTitle": "Spotify Free"}],
                "command": ["spicetify", "watch", "-s"],
                "move": true,
            },
            "feishin": {"enable": true, "match": [{"class": "feishin"}], "move": true},
        },
        "sysmon": {
            "btop": {
                "enable": true,
                "match": [{"class": "btop", "title": "btop", "workspace": {"name": "special:sysmon"}}],
                "command": ["foot", "-a", "btop", "-T", "btop", "fish", "-C", "exec btop"],
            },
        },
        "todo": {
            "todoist": {"enable": true, "match": [{"class": "Todoist"}], "command": ["todoist"], "move": true},
        },
    })
}

/// DeepChainMap equivalent: user wins per-key, dicts merge recursively.
fn deep_merge(user: &Value, defaults: &Value) -> Value {
    match (user.as_object(), defaults.as_object()) {
        (Some(u), Some(d)) => {
            let mut out = serde_json::Map::new();
            for (k, dv) in d {
                out.insert(
                    k.clone(),
                    match u.get(k) {
                        Some(uv) => deep_merge(uv, dv),
                        None => dv.clone(),
                    },
                );
            }
            for (k, uv) in u {
                out.entry(k.clone()).or_insert_with(|| uv.clone());
            }
            Value::Object(out)
        }
        _ => user.clone(),
    }
}

fn merged_config(user_toggles: Value) -> Value {
    deep_merge(&user_toggles, &default_config())
}

/// Python shlex.quote: safe tokens pass through, anything else is wrapped
/// in single quotes with embedded `'` escaped as `'"'"'`.
fn shlex_quote(s: &str) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c);
    if !s.is_empty() && s.chars().all(safe) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', r#"'"'"'"#))
    }
}

fn shlex_join(args: &[&str]) -> String {
    args.iter()
        .map(|a| shlex_quote(a))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Python truthiness for JSON values ("enable"/"move" checks).
fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

/// shutil.which equivalent: executable file on PATH (or at the given path
/// when the command contains a slash).
fn which(cmd: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let is_exec = |p: &Path| {
        std::fs::metadata(p)
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    };
    if cmd.contains('/') {
        return is_exec(Path::new(cmd));
    }
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| is_exec(&dir.join(cmd))))
        .unwrap_or(false)
}

/// Lazily fetched, cached `hyprctl clients` — mirrors python get_clients().
struct Ctx {
    workspace: String,
    clients: Option<Vec<Value>>,
}

impl Ctx {
    fn clients(&mut self) -> Result<&[Value]> {
        if self.clients.is_none() {
            let v = hypr::message_json("clients")?;
            self.clients = Some(v.as_array().cloned().unwrap_or_default());
        }
        Ok(self.clients.as_deref().unwrap_or_default())
    }

    fn move_client(&mut self, matches: &[Value]) -> Result<()> {
        let special = format!("special:{}", self.workspace);
        let clients = self.clients()?;
        let mut dispatches = Vec::new();
        for client in clients {
            let ws_name = client["workspace"]["name"].as_str().unwrap_or_default();
            if matches.iter().any(|m| is_subset(client, m)) && ws_name != special {
                let addr = client["address"].as_str().unwrap_or_default();
                dispatches.push(format!("{special},address:{addr}"));
            }
        }
        for arg in dispatches {
            hypr::dispatch("movetoworkspacesilent", &[arg])?;
        }
        Ok(())
    }

    fn spawn_client(&mut self, matches: &[Value], spawn: &[&str]) -> Result<bool> {
        let runnable = spawn[0].ends_with(".desktop") || which(spawn[0]);
        if runnable
            && !self
                .clients()?
                .iter()
                .any(|c| matches.iter().any(|m| is_subset(c, m)))
        {
            hypr::dispatch(
                "exec",
                &[format!(
                    "[workspace special:{}] {}",
                    self.workspace,
                    shlex_join(spawn)
                )],
            )?;
            return Ok(true);
        }
        Ok(false)
    }

    fn handle_client_config(&mut self, client: &Value) -> Result<bool> {
        let matches = client
            .get("match")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let mut spawned = false;
        if let Some(cmd) = client.get("command").and_then(Value::as_array) {
            let cmd: Vec<&str> = cmd.iter().filter_map(Value::as_str).collect();
            if !cmd.is_empty() {
                spawned = self.spawn_client(&matches, &cmd)?;
            }
        }
        if truthy(client.get("move")) {
            self.move_client(&matches)?;
        }

        Ok(spawned)
    }
}

fn specialws() -> Result<()> {
    let monitors = hypr::message_json("monitors")?;
    let focused = monitors
        .as_array()
        .and_then(|ms| ms.iter().find(|m| m["focused"].as_bool().unwrap_or(false)));
    if let Some(monitor) = focused {
        let name = monitor["specialWorkspace"]["name"]
            .as_str()
            .unwrap_or_default();
        // python slices off the "special:" prefix with [8:]; empty → "special"
        let special = match name.get(8..) {
            Some(s) if !s.is_empty() => s,
            _ => "special",
        };
        hypr::dispatch("togglespecialworkspace", &[special.to_string()])?;
    }
    Ok(())
}

pub fn run(args: ToggleArgs) -> Result<()> {
    if args.workspace == "specialws" {
        return specialws();
    }

    let cfg = merged_config(
        get_config()
            .get("toggles")
            .cloned()
            .unwrap_or_else(|| json!({})),
    );

    let mut spawned = false;
    if let Some(clients) = cfg.get(&args.workspace).and_then(Value::as_object) {
        let mut ctx = Ctx {
            workspace: args.workspace.clone(),
            clients: None,
        };
        for client in clients.values() {
            if truthy(client.get("enable")) && ctx.handle_client_config(client)? {
                spawned = true;
            }
        }
    }

    if !spawned {
        hypr::dispatch("togglespecialworkspace", &[args.workspace])?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn is_subset_matches_python_semantics() {
        // string = substring match
        assert!(is_subset(
            &json!({"class": "discordcanary"}),
            &json!({"class": "discord"})
        ));
        assert!(!is_subset(
            &json!({"class": "firefox"}),
            &json!({"class": "discord"})
        ));
        // missing key
        assert!(!is_subset(&json!({}), &json!({"class": "x"})));
        // nested dict
        assert!(is_subset(
            &json!({"workspace": {"name": "special:sysmon", "id": -98}}),
            &json!({"workspace": {"name": "special:sysmon"}})
        ));
        // other values: equality
        assert!(is_subset(&json!({"pid": 42}), &json!({"pid": 42})));
        assert!(!is_subset(&json!({"pid": 41}), &json!({"pid": 42})));
    }

    #[test]
    fn user_config_overrides_defaults_deeply() {
        let user = json!({"communication": {"discord": {"enable": false}}});
        let cfg = merged_config(user);
        assert_eq!(cfg["communication"]["discord"]["enable"], json!(false));
        // untouched sibling keys survive from defaults
        assert_eq!(cfg["communication"]["discord"]["move"], json!(true));
        assert_eq!(cfg["communication"]["whatsapp"]["enable"], json!(true));
    }

    #[test]
    fn shlex_join_quotes_like_python() {
        assert_eq!(
            shlex_join(&["foo", "a b", "it's"]),
            r#"foo 'a b' 'it'"'"'s'"#
        );
        // safe chars stay unquoted, empty string gets quoted
        assert_eq!(shlex_join(&["a_@%+=:,./-1", ""]), "a_@%+=:,./-1 ''");
    }
}
