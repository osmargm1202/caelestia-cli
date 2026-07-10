# Rust Migration — Phase 2: Util Core + Native Subcommands Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `toggle`, `shell`, `screenshot`, `record`, `search` native Rust (Hyprland IPC + subprocess orchestration), stub `clipboard`/`emoji` (replaced by the shell launcher), keeping every other subcommand delegated.

**Architecture:** New modules mirror the Python layout: `src/util/` (paths, io, notify), `src/ipc/hypr.rs` (Hyprland unix-socket protocol), `src/cli.rs` (clap tree for native subcommands only), `src/subcommands/*.rs`. `main.rs` gains a two-way dispatch: first arg is a native subcommand → clap-parse and run natively; anything else (including top-level flags like `-v`) → delegate to Python unchanged.

**Tech Stack:** Rust 2021: clap 4 (derive), serde/serde_json, anyhow. No async runtime — everything is blocking I/O.

**Reference implementation:** the Python sources under `python-ref/src/caelestia/` are the byte-level behavior spec. Each task names its reference file — read it before implementing.

**Frozen contract (spec §2.1):** the shell executes `caelestia record`, `caelestia record -p`, etc. — flags, output messages consumed by notifications, and state file paths must not change. CLI→shell IPC is `qs -c caelestia ipc call picker openFreeze/open/openSearch`.

**Nix patch coupling:** default.nix `postPatch` substitutes the literal `"qs", "-c", "caelestia"` in shell.py/screenshot.py and `'discord'`/`'["todoist"]'` in toggle.py. The Rust sources MUST contain the same literals so the same substitutions apply (Task 9 updates postPatch to cover the Rust files).

---

### Task 1: Cargo dependencies + `util/paths.rs`

**Reference:** `python-ref/src/caelestia/utils/paths.py`
**Files:**
- Modify: `Cargo.toml`
- Create: `src/util/mod.rs`, `src/util/paths.rs`
- Modify: `src/main.rs` (add `mod util;`)

- [ ] **Step 1: Add dependencies to `Cargo.toml`**

```toml
[dependencies]
anyhow = "1"
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

- [ ] **Step 2: Create `src/util/mod.rs`**

```rust
pub mod io;
pub mod notify;
pub mod paths;
```

(io/notify come in Tasks 2-3; create empty `src/util/io.rs` and `src/util/notify.rs` placeholders now so it compiles, or add the mod lines per-task — implementer's choice, but the final state must match.)

- [ ] **Step 3: Write failing tests in `src/util/paths.rs`**

Only the paths phase 2 needs (YAGNI — wallpaper/scheme paths arrive with their phases):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xdg_dirs_respect_env() {
        // These read env at call time so tests can override.
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
    }

    #[test]
    fn override_env_vars_win() {
        std::env::set_var("CAELESTIA_SCREENSHOTS_DIR", "/tmp/shots");
        std::env::set_var("CAELESTIA_RECORDINGS_DIR", "/tmp/recs");
        assert_eq!(screenshots_dir(), PathBuf::from("/tmp/shots"));
        assert_eq!(recordings_dir(), PathBuf::from("/tmp/recs"));
        std::env::remove_var("CAELESTIA_SCREENSHOTS_DIR");
        std::env::remove_var("CAELESTIA_RECORDINGS_DIR");
    }
}
```

NOTE: cargo runs tests in parallel threads sharing the environment — env-mutating tests can race. Put all env-mutating assertions in these two tests only, and have `override_env_vars_win` set/remove only vars the other test doesn't read; if flakiness appears, merge them into one test.

- [ ] **Step 4: Run tests, verify red** (`nix develop --command cargo test paths`)

- [ ] **Step 5: Implement `src/util/paths.rs`**

Functions (not statics) so env is read at call time — matches Python import-time semantics closely enough for a CLI process and makes testing sane:

```rust
use std::env;
use std::path::PathBuf;

use anyhow::Result;
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
```

(`Result` import only if needed; drop unused imports — clippy runs with `-D warnings`.)

- [ ] **Step 6: Green + clippy** (`cargo test paths`, `cargo clippy --all-targets -- -D warnings`)

- [ ] **Step 7: Commit** — `feat: add XDG path helpers (util/paths)`

---

### Task 2: `util/io.rs`

**Reference:** `python-ref/src/caelestia/utils/io.py` lines 1-56 ONLY (log/info/warn/error/fatal). The prompt/selection/pause half serves install/update — dead code for the port, do NOT port it.
**Files:**
- Create: `src/util/io.rs`

- [ ] **Step 1: Failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_matches_python() {
        assert_eq!(format_msg(33, true, "Warning: hi"), "\x1b[33m:: Warning: hi\x1b[0m");
        assert_eq!(format_msg(31, false, "Error: x"), "\x1b[31mError: x\x1b[0m");
    }
}
```

- [ ] **Step 2: Red, then implement**

```rust
const LOG_COLOUR: u8 = 2;
const INFO_COLOUR: u8 = 0;
const WARNING_COLOUR: u8 = 33;
const ERROR_COLOUR: u8 = 31;

pub fn format_msg(colour: u8, prefix: bool, msg: &str) -> String {
    format!("\x1b[{colour}m{}{msg}\x1b[0m", if prefix { ":: " } else { "" })
}

pub fn log(msg: &str) { println!("{}", format_msg(LOG_COLOUR, true, msg)); }
pub fn info(msg: &str) { println!("{}", format_msg(INFO_COLOUR, true, msg)); }
pub fn warn(msg: &str) { println!("{}", format_msg(WARNING_COLOUR, true, &format!("Warning: {msg}"))); }
pub fn error(msg: &str) { eprintln!("{}", format_msg(ERROR_COLOUR, true, &format!("Error: {msg}"))); }

pub fn fatal(msg: &str) -> ! {
    eprintln!("{}", format_msg(ERROR_COLOUR, true, &format!("Fatal: {msg}")));
    std::process::exit(1);
}
```

Allow `dead_code` where a helper isn't referenced yet (`#[allow(dead_code)]` on unused ones) — later phases consume them.

- [ ] **Step 3: Green + clippy + commit** — `feat: add terminal log helpers (util/io)`

---

### Task 3: `util/notify.rs`

**Reference:** `python-ref/src/caelestia/utils/notify.py`
**Files:**
- Create: `src/util/notify.rs`

- [ ] **Step 1: Implement (no unit tests — pure subprocess glue; behavior covered by smoke tests)**

```rust
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

/// notify-send wrapper; returns trimmed stdout (the action id / notif id).
pub fn notify(args: &[&str]) -> Result<String> {
    let out = Command::new("notify-send")
        .arg("-a")
        .arg("caelestia-cli")
        .args(args)
        .output()
        .context("failed to run notify-send")?;
    anyhow::ensure!(out.status.success(), "notify-send failed");
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn close_notification(id: &str) -> Result<()> {
    Command::new("gdbus")
        .args([
            "call", "--session",
            "--dest=org.freedesktop.Notifications",
            "--object-path=/org/freedesktop/Notifications",
            "--method=org.freedesktop.Notifications.CloseNotification",
            id,
        ])
        .stdout(Stdio::null())
        .status()
        .context("failed to run gdbus")?;
    Ok(())
}
```

- [ ] **Step 2: Build + clippy + commit** — `feat: add notify-send wrapper (util/notify)`

---

### Task 4: `ipc/hypr.rs` — Hyprland IPC

**Reference:** `python-ref/src/caelestia/utils/hypr.py` (72 lines — read all of it; the protocol details matter: `j/` JSON prefix, read-until-EOF, `[[BATCH]]`, lua dispatcher map, `status.configProvider` cache).
**Files:**
- Create: `src/ipc/mod.rs` (`pub mod hypr;`), `src/ipc/hypr.rs`
- Modify: `src/main.rs` (add `mod ipc;`)

- [ ] **Step 1: Failing unit tests** (lua string generation is pure; socket framing tested against an in-process `UnixListener`):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lua_dispatch_strings_match_python() {
        assert_eq!(
            lua_dispatch("togglespecialworkspace", &["sysmon".into()]),
            Some(r#"hl.dsp.workspace.toggle_special("sysmon")"#.to_string())
        );
        assert_eq!(
            lua_dispatch("togglespecialworkspace", &[]),
            Some("hl.dsp.workspace.toggle_special()".to_string())
        );
        assert_eq!(
            lua_dispatch("movetoworkspacesilent", &["special:comm,address:0xabc".into()]),
            Some(r#"hl.dsp.window.move({window = "address:0xabc", workspace = "special:comm", follow = false})"#.to_string())
        );
        assert_eq!(
            lua_dispatch("exec", &[r#"[workspace special:x] foo "bar" \ baz"#.into()]),
            Some(r#"hl.dsp.exec_cmd("[workspace special:x] foo \"bar\" \\ baz")"#.to_string())
        );
        assert_eq!(lua_dispatch("workspace", &["3".into()]), None);
    }

    #[test]
    fn message_speaks_hyprland_protocol() {
        use std::io::{Read, Write};
        use std::os::unix::net::UnixListener;

        let dir = std::env::temp_dir().join(format!("hypr-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("hypr/testsig")).unwrap();
        let sock_path = dir.join("hypr/testsig/.socket.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();

        let handle = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0u8; 256];
            let n = s.read(&mut buf).unwrap();
            assert_eq!(&buf[..n], b"j/monitors");
            s.write_all(b"[{\"focused\":true}]").unwrap();
            // connection close = EOF terminates the response
        });

        std::env::set_var("XDG_RUNTIME_DIR", &dir);
        std::env::set_var("HYPRLAND_INSTANCE_SIGNATURE", "testsig");
        let v = message_json("monitors").unwrap();
        assert!(v[0]["focused"].as_bool().unwrap());
        handle.join().unwrap();
    }
}
```

- [ ] **Step 2: Red, then implement**

```rust
use std::env;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use serde_json::Value;

fn socket_base() -> PathBuf {
    PathBuf::from(env::var("XDG_RUNTIME_DIR").unwrap_or_default())
        .join("hypr")
        .join(env::var("HYPRLAND_INSTANCE_SIGNATURE").unwrap_or_default())
}

pub fn socket2_path() -> PathBuf { socket_base().join(".socket2.sock") }

fn send(msg: &str) -> Result<String> {
    let path = socket_base().join(".socket.sock");
    let mut sock = UnixStream::connect(&path)
        .with_context(|| format!("cannot connect to Hyprland socket {path:?} (is Hyprland running?)"))?;
    sock.write_all(msg.as_bytes())?;
    let mut resp = String::new();
    sock.read_to_string(&mut resp)?;
    Ok(resp)
}

/// `j/`-prefixed JSON request, mirrors python hypr.message(msg).
pub fn message_json(msg: &str) -> Result<Value> {
    let resp = send(&format!("j/{msg}"))?;
    serde_json::from_str(&resp).with_context(|| format!("invalid JSON from hyprland for {msg:?}"))
}

/// Raw request, mirrors python hypr.message(msg, is_json=False).
pub fn message_raw(msg: &str) -> Result<String> { send(msg) }

pub fn batch(msgs: &[String]) -> Result<String> {
    send(&format!("[[BATCH]]{}", msgs.join(";")))
}

fn is_lua_config() -> bool {
    static CACHE: OnceLock<bool> = OnceLock::new();
    *CACHE.get_or_init(|| {
        message_json("status")
            .ok()
            .and_then(|v| Some(v.get("configProvider")? == "lua"))
            .unwrap_or(false)
    })
}

/// Lua translations for dispatchers, mirrors python DISPATCHER_MAP_LUA.
fn lua_dispatch(dispatcher: &str, args: &[String]) -> Option<String> {
    match dispatcher {
        "togglespecialworkspace" => Some(match args.first() {
            Some(a) => format!(r#"hl.dsp.workspace.toggle_special("{a}")"#),
            None => "hl.dsp.workspace.toggle_special()".to_string(),
        }),
        "movetoworkspacesilent" => {
            let arg = args.first()?;
            let (workspace, address) = arg.split_once(',')?;
            let address = address.replace("address:", "");
            Some(format!(
                r#"hl.dsp.window.move({{window = "address:{address}", workspace = "{workspace}", follow = false}})"#
            ))
        }
        "exec" => {
            let joined = args.join(" ").replace('\\', r"\\").replace('"', "\\\"");
            Some(format!(r#"hl.dsp.exec_cmd("{joined}")"#))
        }
        _ => None,
    }
}

pub fn dispatch(dispatcher: &str, args: &[String]) -> Result<bool> {
    let req = if is_lua_config() {
        lua_dispatch(dispatcher, args)
    } else {
        None
    };
    let req = match req {
        Some(lua) => format!("dispatch {lua}"),
        None => format!("dispatch {dispatcher} {}", args.join(" ")).trim_end().to_string(),
    };
    Ok(message_raw(&req)? == "ok")
}
```

Note `movetoworkspacesilent`: python does `a[0].split(",")[1]` for address and `[0]` for workspace — `split_once(',')` reproduces it for the only call shape used (`"special:X,address:0xY"`).

- [ ] **Step 3: Green + clippy + commit** — `feat: add Hyprland IPC client (ipc/hypr)`

---

### Task 5: clap CLI tree + native dispatch in `main.rs`

**Reference:** `python-ref/src/caelestia/parser.py` (flag names/help strings for the 5 native + 2 stub subcommands — copy help text verbatim).
**Files:**
- Create: `src/cli.rs`, `src/subcommands/mod.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: `src/cli.rs` with clap derive**

```rust
use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "caelestia", disable_version_flag = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Native,
}

#[derive(Subcommand)]
pub enum Native {
    /// start or message the shell
    Shell(ShellArgs),
    /// toggle a special workspace
    Toggle(ToggleArgs),
    /// take a screenshot
    Screenshot(ScreenshotArgs),
    /// start a screen recording
    Record(RecordArgs),
    /// search using a screen region
    Search,
    /// open clipboard history
    Clipboard(ClipboardArgs),
    /// emoji/glyph utilities
    Emoji(EmojiArgs),
}

#[derive(Args)]
pub struct ShellArgs {
    /// a message to send to the shell
    pub message: Vec<String>,
    /// start the shell detached
    #[arg(short, long)]
    pub daemon: bool,
    /// print all shell IPC commands
    #[arg(short, long)]
    pub show: bool,
    /// print the shell log
    #[arg(short, long)]
    pub log: bool,
    /// kill the shell
    #[arg(short, long)]
    pub kill: bool,
    /// log rules to apply
    #[arg(long, value_name = "RULES")]
    pub log_rules: Option<String>,
}

#[derive(Args)]
pub struct ToggleArgs {
    /// the workspace to toggle
    pub workspace: String,
}

#[derive(Args)]
pub struct ScreenshotArgs {
    /// take a screenshot of a region
    #[arg(short, long, num_args = 0..=1, default_missing_value = "slurp")]
    pub region: Option<String>,
    /// freeze the screen while selecting a region
    #[arg(short, long)]
    pub freeze: bool,
}

#[derive(Args)]
pub struct RecordArgs {
    /// record a region
    #[arg(short, long, num_args = 0..=1, default_missing_value = "slurp")]
    pub region: Option<String>,
    /// record audio
    #[arg(short, long)]
    pub sound: bool,
    /// pause/resume the recording
    #[arg(short, long)]
    pub pause: bool,
    /// copy recording path to clipboard
    #[arg(short, long)]
    pub clipboard: bool,
}

#[derive(Args)]
pub struct ClipboardArgs {
    /// delete from clipboard history
    #[arg(short, long)]
    pub delete: bool,
}

#[derive(Args)]
pub struct EmojiArgs {
    /// open the emoji/glyph picker
    #[arg(short, long)]
    pub picker: bool,
    /// fetch emoji/glyph data from remote
    #[arg(short, long)]
    pub fetch: bool,
}
```

- [ ] **Step 2: Update `main.rs` dispatch**

Replace `first_subcommand`/NATIVE logic:

```rust
mod cli;
mod ipc;
mod subcommands;
mod util;

use clap::Parser;

const NATIVE: &[&str] = &["shell", "toggle", "screenshot", "record", "search", "clipboard", "emoji"];

fn is_native(subcommand: &str) -> bool {
    NATIVE.contains(&subcommand)
}

/// argv[0] is the subcommand IFF it is not a flag. Top-level flags
/// (-v/-h) always delegate — python still owns those until phase 6.
fn native_subcommand(args: &[String]) -> Option<&str> {
    match args.first() {
        Some(first) if !first.starts_with('-') && is_native(first) => Some(first),
        _ => None,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if native_subcommand(&args).is_none() {
        delegate(&args);
    }

    let cli = cli::Cli::parse();
    let result = match cli.command {
        cli::Native::Shell(a) => subcommands::shell::run(a),
        cli::Native::Toggle(a) => subcommands::toggle::run(a),
        cli::Native::Screenshot(a) => subcommands::screenshot::run(a),
        cli::Native::Record(a) => subcommands::record::run(a),
        cli::Native::Search => subcommands::search::run(),
        cli::Native::Clipboard(a) => subcommands::clipboard::run(a),
        cli::Native::Emoji(a) => subcommands::emoji::run(a),
    };

    if let Err(e) = result {
        util::io::error(&format!("{e:#}"));
        std::process::exit(1);
    }
}
```

Keep `delegate()`/`compute_pythonpath()` unchanged. Update the existing unit tests: `no_native_subcommands_in_phase_1` becomes:

```rust
    #[test]
    fn native_set_matches_phase_2() {
        for sub in ["shell", "toggle", "screenshot", "record", "search", "clipboard", "emoji"] {
            assert!(is_native(sub), "{sub} must be native in phase 2");
        }
        for sub in ["scheme", "wallpaper", "resizer", "install", "update"] {
            assert!(!is_native(sub), "{sub} must still delegate in phase 2");
        }
    }

    #[test]
    fn top_level_flags_delegate() {
        let args: Vec<String> = vec!["-v".into()];
        assert_eq!(native_subcommand(&args), None);
        let args: Vec<String> = vec!["--version".into(), "toggle".into()];
        assert_eq!(native_subcommand(&args), None);
        let args: Vec<String> = vec!["toggle".into(), "comm".into()];
        assert_eq!(native_subcommand(&args), Some("toggle"));
        let args: Vec<String> = vec!["scheme".into(), "get".into()];
        assert_eq!(native_subcommand(&args), None);
    }
```

`tests/delegation.rs` keeps working: `scheme get -n` and `--version` both still delegate.

- [ ] **Step 3: `src/subcommands/mod.rs`** — one `pub mod` per subcommand file; create the files as stubs returning `Ok(())` in this task ONLY if needed to compile, then fill them in Tasks 6-8 (each task replaces its stub).

- [ ] **Step 4: Green (existing 5 tests + 2 new) + clippy + commit** — `feat: add clap tree and native dispatch for phase-2 subcommands`

NOTE: this task leaves native subcommands non-functional (empty stubs) until Tasks 6-8 land. That's fine mid-branch, but the branch must NOT merge until all phase-2 tasks are done.

---

### Task 6: `subcommands/toggle.rs` + `subcommands/shell.rs`

**Reference:** `python-ref/src/caelestia/subcommands/toggle.py` (the `is_subset` match semantics and `DeepChainMap` user-config merge are the meat — read carefully) and `python-ref/src/caelestia/subcommands/shell.py`.
**Files:**
- Create: `src/subcommands/toggle.rs`, `src/subcommands/shell.rs`

- [ ] **Step 1: Failing tests for the pure logic (in toggle.rs)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn is_subset_matches_python_semantics() {
        // string = substring match
        assert!(is_subset(&json!({"class": "discordcanary"}), &json!({"class": "discord"})));
        assert!(!is_subset(&json!({"class": "firefox"}), &json!({"class": "discord"})));
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
}
```

- [ ] **Step 2: Red, then implement toggle.rs**

Key pieces (complete the rest by transcribing toggle.py):

```rust
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
            Value::Array(a) => sv.as_array().is_some_and(|x| a.iter().all(|i| x.contains(i))),
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
                out.insert(k.clone(), match u.get(k) {
                    Some(uv) => deep_merge(uv, dv),
                    None => dv.clone(),
                });
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
```

IMPORTANT (Nix patch coupling): the literals `"discord"` (twice: match class + command) and `["todoist"]` must appear in the source exactly as above — default.nix substitutes them (Task 9).

`run(args: ToggleArgs)` transcribes `Command.run/specialws/get_clients/move_client/spawn_client/handle_client_config` — including:
- `specialws`: focused monitor's `specialWorkspace.name` minus the `"special:"` prefix (python slices `[8:]`), fallback `"special"`.
- `spawn_client` gating: `spawn[0].ends_with(".desktop") || which(spawn[0])` — for `which`, check PATH manually (`std::env::split_paths`) or shell out to `which`; PATH scan preferred (no dep). Spawn via `hypr.dispatch("exec", ...)` with the args shell-quoted like python `shlex.join` (implement a minimal `shlex_join`: quote args containing whitespace/special chars with single quotes, escaping embedded `'` as `'\''`).
- clients fetched once and cached (fetch eagerly, store in a local var — no need for Option gymnastics).

- [ ] **Step 3: Implement shell.rs** (transcription of shell.py):

```rust
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

use crate::cli::ShellArgs;
use crate::util::paths::c_cache_dir;

// default.nix substitutes this literal with "caelestia-shell" (same as
// the python patchPhase) — keep it byte-identical.
const SHELL_CMD: &[&str] = &["qs", "-c", "caelestia"];

fn shell_output(args: &[&str]) -> Result<String> {
    let out = Command::new(SHELL_CMD[0])
        .args(&SHELL_CMD[1..])
        .args(args)
        .output()
        .context("failed to run shell command")?;
    anyhow::ensure!(out.status.success(), "shell command failed");
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn filter_log(line: &str) -> bool {
    !line.contains(&format!("Cannot open: file://{}/imagecache/", c_cache_dir().display()))
}

pub fn run(args: ShellArgs) -> Result<()> {
    if args.show {
        print!("{}", shell_output(&["ipc", "show"])?);
    } else if args.log {
        let log = match &args.log_rules {
            Some(rules) => shell_output(&["log", "-r", rules])?,
            None => shell_output(&["log"])?,
        };
        for line in log.lines().filter(|l| filter_log(l)) {
            println!("{line}");
        }
    } else if args.kill {
        shell_output(&["kill"])?;
    } else if !args.message.is_empty() {
        let msg: Vec<&str> = std::iter::once("ipc").chain(std::iter::once("call"))
            .chain(args.message.iter().map(String::as_str)).collect();
        print!("{}", shell_output(&msg)?);
    } else {
        let mut cmd = Command::new(SHELL_CMD[0]);
        cmd.args(&SHELL_CMD[1..]).arg("-n");
        if let Some(rules) = &args.log_rules {
            cmd.args(["--log-rules", rules]);
        }
        if args.daemon {
            cmd.arg("-d");
            cmd.status().context("failed to start shell daemon")?;
        } else {
            let mut child = cmd.stdout(Stdio::piped()).spawn().context("failed to start shell")?;
            if let Some(stdout) = child.stdout.take() {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    if filter_log(&line) {
                        println!("{line}");
                    }
                }
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Green + clippy. Live smoke on Hyprland:** `cargo run -- toggle specialws` (toggles focused special workspace), `cargo run -- shell -s` (prints IPC list). Compare against `python3 -m caelestia` equivalents (PYTHONPATH=python-ref/src).

- [ ] **Step 5: Commit** — `feat: native toggle and shell subcommands`

---

### Task 7: `subcommands/screenshot.rs` + `subcommands/search.rs`

**Reference:** `python-ref/src/caelestia/subcommands/screenshot.py`, `search.py`.
**Files:**
- Create: `src/subcommands/screenshot.rs`, `src/subcommands/search.rs`

- [ ] **Step 1: screenshot.rs** (transcription; key points):

- `region == Some("slurp")` → run `["qs", "-c", "caelestia", "ipc", "call", "picker", openFreeze/open]` — reuse the `SHELL_CMD` literal pattern: define the same `const SHELL_CMD: &[&str] = &["qs", "-c", "caelestia"];` in this file too (the Nix substitution is per-file, matching the python patchPhase which patches shell.py AND screenshot.py).
- explicit region → `grim -l 0 -g <region trimmed> -` captured to memory, piped into spawned `swappy -f -` (stdin write, `start_new_session` → `.process_group(0)` via `std::os::unix::process::CommandExt`).
- fullscreen → `grim -o <focused monitor name> -` → `wl-copy` (stdin), write bytes to `screenshots_cache_dir()/<YYYYmmddHHMMSS>`, notify with actions open/save (use `util::notify::notify`), handle `open` (swappy detached) and `save` (rename into `screenshots_dir()` with `.png` suffix + second notify).
- Timestamp format identical: `chrono`? NO — avoid the dependency: format via `std::time::SystemTime`? Manual date math is error-prone; add tiny dep `chrono` is overkill for one timestamp... Decision: use `chrono` (widely standard, adds ~200ms compile). `#[arg]`s already defined. Add `chrono = { version = "0.4", default-features = false, features = ["clock"] }` to Cargo.toml and format with `Local::now().format("%Y%m%d%H%M%S")` (and `%Y%m%d_%H-%M-%S` for record).

- [ ] **Step 2: search.rs** (transcription):

```rust
use std::path::Path;
use std::process::Command;
use std::os::unix::process::CommandExt;

use anyhow::{Context, Result};

const SEARCH_PNG: &str = "/tmp/caelestia-search.png";
const SEARCH_DONE: &str = "/tmp/caelestia-search.done";

// default.nix substitutes this literal (see shell.rs).
const SHELL_CMD: &[&str] = &["qs", "-c", "caelestia"];

pub fn run() -> Result<()> {
    let _ = std::fs::remove_file(SEARCH_PNG);
    let _ = std::fs::remove_file(SEARCH_DONE);

    Command::new(SHELL_CMD[0])
        .args(&SHELL_CMD[1..])
        .args(["ipc", "call", "picker", "openSearch"])
        .process_group(0)
        .spawn()
        .context("failed to open search picker")?;

    let mut found = false;
    for _ in 0..100 {
        if Path::new(SEARCH_DONE).exists() {
            found = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    if !found {
        return Ok(());
    }
    let _ = std::fs::remove_file(SEARCH_DONE);

    let out = Command::new("curl")
        .args(["-sSf", "--connect-timeout", "5", "--max-time", "15",
               "-F", &format!("files[]=@{SEARCH_PNG}"), "https://uguu.se/upload"])
        .output()
        .context("failed to upload search image")?;
    anyhow::ensure!(out.status.success(), "upload failed");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    if let Some(url) = v["files"][0]["url"].as_str().filter(|u| !u.is_empty()) {
        Command::new("xdg-open")
            .arg(format!("https://lens.google.com/uploadbyurl?url={url}"))
            .process_group(0)
            .spawn()?;
    }
    Ok(())
}
```

NOTE search.py has no flags — clap variant is unit `Search`.

- [ ] **Step 3: Green + clippy. Live smoke:** `cargo run -- screenshot` (full-screen notify flow), `cargo run -- screenshot -r` (region picker opens). Commit — `feat: native screenshot and search subcommands`

---

### Task 8: `subcommands/record.rs` + stubs `clipboard.rs`/`emoji.rs`

**Reference:** `python-ref/src/caelestia/subcommands/record.py` (whole file — pause/stop/start state machine, region/refresh-rate logic, notification actions).
**Files:**
- Create: `src/subcommands/record.rs`, `src/subcommands/clipboard.rs`, `src/subcommands/emoji.rs`

- [ ] **Step 1: Failing unit test for the pure helper**

```rust
    #[test]
    fn rect_intersection_matches_python() {
        // a, b as (x, y, w, h)
        assert!(intersects((0, 0, 100, 100), (50, 50, 100, 100)));
        assert!(!intersects((0, 0, 10, 10), (20, 20, 5, 5)));
        // touching edges do NOT intersect (strict <)
        assert!(!intersects((0, 0, 10, 10), (10, 0, 10, 10)));
    }

    #[test]
    fn region_parses() {
        assert_eq!(parse_region("1920x1080+0+0").unwrap(), (0, 0, 1920, 1080));
        assert!(parse_region("garbage").is_err());
    }
```

- [ ] **Step 2: Red, then implement record.rs**

Transcription notes:
- `RECORDER = "gpu-screen-recorder"`; pause → `pkill -USR2 -f gpu-screen-recorder`; running check → `pidof` exit code.
- start: region via slurp `-f "%wx%h+%x+%y"` or explicit; parse with plain string splitting or a hand-rolled parser (`parse_region(&str) -> Result<(i64,i64,i64,i64)>` returning x,y,w,h) — NO regex crate for one pattern; max refresh-rate across intersecting monitors (`monitor["refreshRate"].as_f64().round() as i64`); fullscreen → focused monitor name + its refresh rate.
- `-a default_output` when `--sound`; `config["record"]["extraArgs"]` array appended (error message identical: `Config option 'record.extraArgs' should be an array`).
- spawn detached (`process_group(0)`), write notif id to `recording_notif_path()`, then `try_wait` loop ~1s: if the process exited nonzero within 1s → close notification + failure notify (python `proc.wait(1)`).
- stop: `pkill -f`, poll `pidof` at 100ms until gone, move file to `recordings_dir()/recording_<%Y%m%d_%H-%M-%S>.mp4` (`std::fs::rename`; if cross-device rename fails, copy+remove — python shutil.move does this), close start notif, `--clipboard` → `wl-copy --type text/uri-list` with `file://<path>\n` on stdin, action notify watch/open/delete (dbus-send ShowItems, fallback xdg-open parent; delete → remove file).

- [ ] **Step 3: Stubs clipboard.rs / emoji.rs**

```rust
use anyhow::Result;

use crate::cli::ClipboardArgs;

/// Removed in the NixOS fork: the shell launcher's clipboard UI
/// (C++ ClipboardCore) replaces the old cliphist+fuzzel picker.
pub fn run(_args: ClipboardArgs) -> Result<()> {
    anyhow::bail!("removed in this fork — use the shell launcher (clipboard tab) instead");
}
```

(emoji.rs analogous: "use the shell launcher (emoji picker) instead".)

- [ ] **Step 4: Green + clippy. Live smoke:** `cargo run -- record` (start; notif appears), `cargo run -- record` again (stop; file lands in recordings dir), `cargo run -- clipboard` (clean error + exit 1).

- [ ] **Step 5: Commit** — `feat: native record subcommand; stub clipboard/emoji (shell owns them)`

---

### Task 9: Nix postPatch for Rust sources + delegation removal check

**Files:**
- Modify: `default.nix` (postPatch)
- Modify: `README.md` (migration section: status update)

- [ ] **Step 1: Extend postPatch in default.nix**

After the existing python-ref substitutions add:

```nix
      # Same substitutions for the Rust sources (native subcommands).
      substituteInPlace src/subcommands/shell.rs \
        --replace-fail '"qs", "-c", "caelestia"' '"caelestia-shell"'
      substituteInPlace src/subcommands/screenshot.rs \
        --replace-fail '"qs", "-c", "caelestia"' '"caelestia-shell"'
      substituteInPlace src/subcommands/search.rs \
        --replace-fail '"qs", "-c", "caelestia"' '"caelestia-shell"'

      substituteInPlace src/subcommands/toggle.rs \
        --replace-fail '"discord"' '"${discordBin}"' \
        --replace-fail '["todoist"]' '["todoist.desktop"]'
```

CAREFUL — verify each pattern actually appears in the Rust sources exactly once/expected count before relying on it (`grep -c`). `--replace-fail 'discord'` unquoted in python vs `'"discord"'` quoted here: python toggle.py patch replaced bare `discord` (hitting class-match AND command); the Rust source uses the same strings inside `json!` — patching `"discord"` (with quotes) hits both `"class": "discord"` and `"command": ["discord"]` plus the key `"discord"` — same effective result as the python patch (which also renamed the key). Keep semantics identical to python patch.

- [ ] **Step 2: Update README migration section** — list which subcommands are now native vs delegated vs removed (clipboard/emoji → shell launcher).

- [ ] **Step 3: `git add -A && nix build`** — postPatch must not fail; smoke `./result/bin/caelestia toggle --help` (clap help now, not argparse) and `./result/bin/caelestia scheme get -n` (still delegated argparse path).

- [ ] **Step 4: Commit** — `build: patch Rust sources in postPatch, update migration status`

---

### Task 10: Final phase verification

- [ ] **Step 1: Full gate**

```bash
nix build
nix develop --command cargo test
nix develop --command cargo clippy --all-targets -- -D warnings
```

- [ ] **Step 2: Behavior parity sweep (live Hyprland session)** — for each native subcommand, output/behavior must match `PYTHONPATH=python-ref/src python3 -m caelestia <same args>`:

```bash
./result/bin/caelestia shell -s | head -5
./result/bin/caelestia toggle specialws        # toggles; run twice to restore
./result/bin/caelestia screenshot              # notify flow
./result/bin/caelestia record && sleep 2 && ./result/bin/caelestia record
./result/bin/caelestia clipboard; echo "exit=$?"   # clean error, exit 1
./result/bin/caelestia scheme list -n          # delegated, unchanged
```

- [ ] **Step 3: Latency proof** (the point of the migration):

```bash
time ./result/bin/caelestia clipboard 2>/dev/null   # native: ~5ms
time ./result/bin/caelestia scheme get -n           # delegated: ~150-200ms
```

- [ ] **Step 4: Commit any doc tweaks; phase done**

---

## Phase exit criteria

- `toggle`, `shell`, `screenshot`, `record`, `search` native with behavior parity vs python-ref on a live Hyprland session.
- `clipboard`/`emoji` fail loud with pointer to the shell launcher, exit 1.
- `scheme`, `wallpaper`, `resizer`, `install`, `update`, `-v`, no-args → still delegate byte-identically.
- All tests + clippy green; `nix build` green with extended postPatch.

**Next:** Phase 3 plan (colour engine + golden tests) written after this merges.
