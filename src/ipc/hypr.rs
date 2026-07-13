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

#[allow(dead_code)] // consumed by record.rs (Task 8)
pub fn socket2_path() -> PathBuf {
    socket_base().join(".socket2.sock")
}

pub fn socket2_stream() -> Result<UnixStream> {
    UnixStream::connect(socket2_path()).context("cannot connect to Hyprland socket2")
}

fn send(msg: &str) -> Result<String> {
    let path = socket_base().join(".socket.sock");
    let mut sock = UnixStream::connect(&path).with_context(|| {
        format!("cannot connect to Hyprland socket {path:?} (is Hyprland running?)")
    })?;
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
pub fn message_raw(msg: &str) -> Result<String> {
    send(msg)
}

#[allow(dead_code)] // consumed by screenshot.rs (Task 7)
pub fn batch(msgs: &[String]) -> Result<String> {
    send(&format!("[[BATCH]]{}", msgs.join(";")))
}

pub fn is_lua_config() -> bool {
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
        "resizewindowpixel" => {
            let [exact, width, target] = args else {
                return None;
            };
            if exact != "exact" {
                return None;
            }
            let (height, address) = target.split_once(",address:")?;
            Some(format!(
                r#"hl.dsp.window.resize({{x = {width}, y = {height}, exact = true, window = "address:{address}"}})"#
            ))
        }
        "movewindowpixel" => {
            let [exact, x, target] = args else {
                return None;
            };
            if exact != "exact" {
                return None;
            }
            let (y, address) = target.split_once(",address:")?;
            Some(format!(
                r#"hl.dsp.window.move({{x = {x}, y = {y}, window = "address:{address}"}})"#
            ))
        }
        "togglefloating" => {
            let address = args.first()?.strip_prefix("address:")?;
            Some(format!(
                r#"hl.dsp.window.float({{action = "toggle", window = "address:{address}"}})"#
            ))
        }
        _ => None,
    }
}

pub fn format_dispatch(dispatcher: &str, args: &[String], lua_config: bool) -> String {
    if lua_config {
        if let Some(lua) = lua_dispatch(dispatcher, args) {
            return format!("dispatch {lua}");
        }
    }

    format!("dispatch {dispatcher} {}", args.join(" "))
        .trim_end()
        .to_string()
}

pub fn dispatch(dispatcher: &str, args: &[String]) -> Result<bool> {
    let req = format_dispatch(dispatcher, args, is_lua_config());
    Ok(message_raw(&req)? == "ok")
}

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

        assert_eq!(
            format_dispatch(
                "resizewindowpixel",
                &["exact".into(), "800".into(), "600,address:0xabc".into()],
                false,
            ),
            "dispatch resizewindowpixel exact 800 600,address:0xabc"
        );
        assert_eq!(
            format_dispatch(
                "resizewindowpixel",
                &["exact".into(), "800".into(), "600,address:0xabc".into()],
                true,
            ),
            r#"dispatch hl.dsp.window.resize({x = 800, y = 600, exact = true, window = "address:0xabc"})"#
        );
        assert_eq!(
            format_dispatch(
                "movewindowpixel",
                &["exact".into(), "10".into(), "20,address:0xabc".into()],
                false,
            ),
            "dispatch movewindowpixel exact 10 20,address:0xabc"
        );
        assert_eq!(
            format_dispatch(
                "movewindowpixel",
                &["exact".into(), "10".into(), "20,address:0xabc".into()],
                true,
            ),
            r#"dispatch hl.dsp.window.move({x = 10, y = 20, window = "address:0xabc"})"#
        );
        assert_eq!(
            format_dispatch("togglefloating", &["address:0xabc".into()], false),
            "dispatch togglefloating address:0xabc"
        );
        assert_eq!(
            format_dispatch("togglefloating", &["address:0xabc".into()], true),
            r#"dispatch hl.dsp.window.float({action = "toggle", window = "address:0xabc"})"#
        );
    }

    #[test]
    fn message_speaks_hyprland_protocol() {
        use std::io::{Read, Write};
        use std::os::unix::net::UnixListener;

        let mut env = crate::test_support::EnvGuard::new();
        let dir = std::env::temp_dir().join(format!("hypr-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("hypr/testsig")).unwrap();
        let sock_path = dir.join("hypr/testsig/.socket.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();

        let socket2_path = dir.join("hypr/testsig/.socket2.sock");
        let listener2 = UnixListener::bind(&socket2_path).unwrap();

        let handle = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0u8; 256];
            let n = s.read(&mut buf).unwrap();
            assert_eq!(&buf[..n], b"j/monitors");
            s.write_all(b"[{\"focused\":true}]").unwrap();
            // connection close = EOF terminates the response
        });

        env.set("XDG_RUNTIME_DIR", &dir);
        env.set("HYPRLAND_INSTANCE_SIGNATURE", "testsig");
        let v = message_json("monitors").unwrap();
        assert!(v[0]["focused"].as_bool().unwrap());
        handle.join().unwrap();

        let event_handle = std::thread::spawn(move || {
            let (mut s, _) = listener2.accept().unwrap();
            s.write_all(b"openwindow>>0xabc\n").unwrap();
        });
        let mut stream = socket2_stream().unwrap();
        let mut line = String::new();
        stream.read_to_string(&mut line).unwrap();
        assert_eq!(line, "openwindow>>0xabc\n");
        event_handle.join().unwrap();
    }
}
