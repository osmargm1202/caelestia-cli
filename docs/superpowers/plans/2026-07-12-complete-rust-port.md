# Complete Rust Port Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace every remaining Python-backed `caelestia` command with Rust and remove `python-ref` plus all Python runtime packaging.

**Architecture:** Extend the existing Rust `ipc::hypr`, `core::scheme`, `core::material`, and `util::paths` modules. Add native `wallpaper` and `resizer` command modules whose pure parsing/state/geometry functions are unit-testable; keep external filesystem, process, and Unix-socket boundaries narrow. Finally make Clap own all invocation parsing and delete the Python fallback/package tree.

**Tech Stack:** Rust 2021, Clap 4, anyhow, serde/serde_json, image, material-colors, Unix sockets, Nix `buildRustPackage`, ffmpeg.

## Global Constraints

- NixOS and Hyprland only; `install` and `update` are removed commands.
- Preserve supported CLI flags, XDG paths, state file names, symlink names, JSON rule schema, and post-hook variables.
- Preserve image extensions `jpg jpeg png webp tif tiff gif` and video extensions `mp4 webm mkv avi mov wmv flv`.
- Use ffmpeg only for video frame extraction; it remains a runtime dependency.
- `wallpaper` and `resizer` must not require Python at runtime.
- Do not alter shell IPC contract or existing native command behavior.
- Every task runs `nix develop -c cargo test --all-targets --all-features` before commit.

---

### Task 1: Extend paths and Hyprland primitives

**Files:**
- Modify: `src/util/paths.rs`
- Modify: `src/ipc/hypr.rs`
- Modify: `src/ipc/mod.rs`
- Test: unit tests within `src/util/paths.rs` and `src/ipc/hypr.rs`

**Interfaces:**
- Produces `wallpaper_path_path()`, `wallpaper_link_path()`, `wallpaper_thumbnail_path()`, and `wallpapers_cache_dir()` returning `PathBuf` under the existing Caelestia XDG roots.
- Produces `pub fn is_lua_config() -> bool`, `pub fn socket2_stream() -> Result<UnixStream>`, and `pub fn batch(msgs: &[String]) -> Result<String>` for resizer.
- Consumes existing `message_json`, `message_raw`, and `compute_hash`.

- [ ] **Step 1: Add failing path-layout tests**

Add tests that set XDG homes and assert the Python-compatible paths. Use exact expected suffixes taken from `python-ref/src/caelestia/utils/paths.py`:

```rust
assert!(wallpaper_path_path().ends_with("caelestia/wallpaper/path.txt"));
assert!(wallpaper_link_path().ends_with("caelestia/wallpaper/current"));
assert!(wallpaper_thumbnail_path().ends_with("caelestia/wallpaper/thumbnail.jpg"));
assert!(wallpapers_cache_dir().ends_with("caelestia/wallpapers"));
```

- [ ] **Step 2: Run the path tests to confirm failure**

Run:

```bash
nix develop -c cargo test util::paths::tests -- --nocapture
```

Expected: compilation failure because wallpaper path helpers do not exist.

- [ ] **Step 3: Implement path helpers**

Add public helpers in `src/util/paths.rs` using existing `c_state_dir()` and `c_cache_dir()`:

```rust
pub fn wallpaper_path_path() -> PathBuf { c_state_dir().join("wallpaper/path.txt") }
pub fn wallpaper_link_path() -> PathBuf { c_state_dir().join("wallpaper/current") }
pub fn wallpaper_thumbnail_path() -> PathBuf { c_state_dir().join("wallpaper/thumbnail.jpg") }
pub fn wallpapers_cache_dir() -> PathBuf { c_cache_dir().join("wallpapers") }
```

Verify exact Python names before committing; adjust literals if the Python path helpers use different names.

- [ ] **Step 4: Add socket2 and public Lua-mode tests**

Use a temporary `XDG_RUNTIME_DIR/hypr/testsig/.socket2.sock` listener. Assert `socket2_stream()` connects and reads a line. Add a pure dispatcher formatter test for `resizewindowpixel`, `movewindowpixel`, `togglefloating`, and Lua equivalents used by resizer.

- [ ] **Step 5: Implement Hypr helpers**

Expose `is_lua_config`, add:

```rust
pub fn socket2_stream() -> Result<UnixStream> {
    UnixStream::connect(socket2_path()).context("cannot connect to Hyprland socket2")
}
```

Keep `batch` request framing exactly `[[BATCH]]` plus semicolon-separated messages.

- [ ] **Step 6: Run focused and full tests**

Run:

```bash
nix develop -c cargo test util::paths::tests ipc::hypr::tests -- --nocapture
nix develop -c cargo test --all-targets --all-features
```

Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/util/paths.rs src/ipc/hypr.rs src/ipc/mod.rs
git commit -m "feat: add wallpaper paths and Hyprland socket primitives"
```

### Task 2: Implement native wallpaper state, media conversion, and palette generation

**Files:**
- Create: `src/subcommands/wallpaper.rs`
- Modify: `src/subcommands/mod.rs`
- Modify: `src/cli.rs`
- Modify: `src/core/scheme.rs`
- Test: unit tests in `src/subcommands/wallpaper.rs`

**Interfaces:**
- Produces `pub fn run(args: WallpaperArgs) -> anyhow::Result<()>`.
- Adds `Native::Wallpaper(WallpaperArgs)` and Clap flags `--print`, `--file`, `--random`, `--no-filter`, `--threshold`, `--no-smart` matching Python semantics.
- Consumes `Scheme`, `core::material::get_colours_for_image`, path helpers, `image`, and `ffmpeg` process execution.

- [ ] **Step 1: Write failing CLI and extension tests**

Add Clap parsing tests for:

```rust
Cli::try_parse_from(["caelestia", "wallpaper", "--file", "/tmp/a.png"])
Cli::try_parse_from(["caelestia", "wallpaper", "--random", "/walls", "--no-filter", "--threshold", "0.8", "--no-smart"])
```

Add pure tests for accepted images/videos and rejected extension:

```rust
assert!(is_valid_image(Path::new("a.webp")));
assert!(is_valid_video(Path::new("a.mkv")));
assert!(!is_valid_wallpaper(Path::new("a.txt")));
```

- [ ] **Step 2: Confirm tests fail**

Run:

```bash
nix develop -c cargo test wallpaper -- --nocapture
```

Expected: missing `WallpaperArgs`, `Native::Wallpaper`, and helpers.

- [ ] **Step 3: Implement non-side-effect helpers**

Implement `is_valid_image`, `is_valid_video`, `is_valid_wallpaper`, cache key lookup via `compute_hash`, `thumbnail_path`, and random candidate filtering. Image filtering opens dimensions with `image::image_dimensions`; video candidates bypass dimension filtering. Exclude the current wall if alternatives remain.

Implement media source conversion:

```rust
fn converted_source(wall: &Path, cache: &Path) -> Result<PathBuf>
```

GIF writes `first_frame.png` using `image`. Video creates the same output through:

```text
ffmpeg -y -loglevel error -i <wall> -vframes 1 -vf scale=512:-1 <first_frame.png>
```

Skip conversion when cache output exists.

- [ ] **Step 4: Test conversion command construction and random filtering**

Inject/process-wrap the command builder so a test checks every ffmpeg argument without invoking ffmpeg. Use a temp directory with image-extension files plus a current wall; assert candidate selection never returns it when another candidate exists.

- [ ] **Step 5: Implement palette/state operations**

Implement `print_wallpaper_colours`, `set_wallpaper`, and `set_random`:

- canonicalize and validate source;
- atomically write the current-wall state text and replace state symlinks;
- generate/reuse 128×128 JPEG thumbnail;
- load scheme, apply smart mode/variant only for `dynamic` unless `--no-smart`;
- obtain palette from `get_colours_for_image` and persist scheme state;
- retain current scheme mode/variant in this task; Task 3 exclusively adds smart mode/variant selection and post-hook behavior;
- invoke the existing native scheme/theme application mechanism; extract it from `scheme.rs` if necessary rather than duplicating output logic.

Match JSON output for `--print`: `name`, `flavour`, `mode`, `variant`, `colours`.

- [ ] **Step 6: Test state and print behavior**

Use temporary XDG paths. Assert `--file` writes state content and symlinks; assert `--print` returns valid JSON and does not mutate state; assert invalid file and empty random set return errors.

- [ ] **Step 7: Run full tests and commit**

```bash
nix develop -c cargo test --all-targets --all-features
git add src/subcommands/wallpaper.rs src/subcommands/mod.rs src/cli.rs src/core/scheme.rs
git commit -m "feat: port wallpaper command to Rust"
```

### Task 3: Complete wallpaper smart selection and post-hook compatibility

**Files:**
- Modify: `src/subcommands/wallpaper.rs`
- Modify: `src/core/material/mod.rs` only if an image thumbnail adapter is needed
- Test: `src/subcommands/wallpaper.rs`

**Interfaces:**
- Produces cached `smart.json` containing `{ "variant": String, "mode": String }`.
- Produces hook environment exactly named `WALLPAPER_PATH`, `SCHEME_NAME`, `SCHEME_FLAVOUR`, `SCHEME_MODE`, `SCHEME_VARIANT`, `SCHEME_COLOURS`, `THUMBNAIL_PATH`.
- Consumes `wallpaper.postHook` from `get_config()`.

- [ ] **Step 1: Write failing smart-cache and hook-environment tests**

Create a deterministic tiny image. Assert first smart calculation writes `smart.json`; second call reuses valid JSON. Test a hook environment builder directly:

```rust
let env = post_hook_env(&wall, &thumb, &scheme)?;
assert_eq!(env.get("WALLPAPER_PATH"), Some(&wall.display().to_string()));
assert!(env.contains_key("SCHEME_COLOURS"));
```

- [ ] **Step 2: Confirm test failure**

Run:

```bash
nix develop -c cargo test wallpaper::tests::smart -- --nocapture
```

Expected: missing smart/cache and hook functions.

- [ ] **Step 3: Implement smart strategy and cache**

Use image pixel/Material HCT analysis to select light mode when tone is greater than 60. Port the existing colourfulness variant selection exactly from Python `utils/colourfulness.py`; do not invent a new heuristic. Serialize cache via `serde_json`, tolerate unreadable/malformed cache by recomputing.

- [ ] **Step 4: Implement hook spawn**

When config path `wallpaper.postHook` is a string, spawn:

```rust
Command::new("sh").arg("-c").arg(hook).envs(post_hook_env(...)).status()
```

Do not fail a successful wallpaper change if hook exits nonzero; suppress hook stderr as Python did. Return spawn failures with context.

- [ ] **Step 5: Run test suite and commit**

```bash
nix develop -c cargo test --all-targets --all-features
git add src/subcommands/wallpaper.rs src/core/material/mod.rs
git commit -m "feat: preserve wallpaper smart mode and post hooks"
```

### Task 4: Implement resizer models, matching, commands, and PiP geometry

**Files:**
- Create: `src/subcommands/resizer.rs`
- Modify: `src/subcommands/mod.rs`
- Modify: `src/cli.rs`
- Test: unit tests in `src/subcommands/resizer.rs`

**Interfaces:**
- Produces `pub fn run(args: ResizerArgs) -> Result<()>`.
- Produces `WindowRule { name, match_type, width, height, actions }`, `MatchType`, `Action`, pure `parse_event`, `matches_rule`, dispatcher formatters, and PiP geometry.
- Adds `Native::Resizer(ResizerArgs)` with `--daemon`, positional `pip`, and active/custom rule inputs compatible with Python parser.

- [ ] **Step 1: Write failing model and event parser tests**

Test defaults:

```rust
assert_eq!(default_rules()[0].name, "(Bitwarden");
assert_eq!(default_rules()[1].match_type, MatchType::TitleRegex);
```

Test `windowtitle>>id,...`, `windowtitle>>>id,...`, `openwindow>>id,ws,class,title`, and `openwindow>>>id,ws,class,title`. Assert invalid non-hex addresses are rejected without panic.

- [ ] **Step 2: Confirm failure**

```bash
nix develop -c cargo test resizer -- --nocapture
```

Expected: missing native command/model/module.

- [ ] **Step 3: Implement config model and matching**

Deserialize `resizer.rules` through serde with fields `name`, `matchType`, `width`, `height`, `actions`. On missing config use defaults. On malformed config warn and use defaults. Implement exact, contains, initial-title, and Rust-regex matching; add `regex` dependency only for `titleRegex` compatibility.

- [ ] **Step 4: Implement dispatcher and PiP pure functions**

Generate exact legacy/Lua strings:

```text
legacy resize: dispatch resizewindowpixel exact <width> <height>,address:<address>
legacy move:   dispatch movewindowpixel exact <x> <y>,address:<address>
legacy float:  dispatch togglefloating address:<address>
legacy center: dispatch centerwindow
```

Lua output must match Python field names and `exact = true`. Implement PiP geometry: scaled height is one quarter monitor logical height; preserve aspect ratio; clamp 200×150; bottom-right placement with 3% minimum-dimension margin. Unit-test a known monitor/window fixture.

- [ ] **Step 5: Run full tests and commit**

```bash
nix develop -c cargo test --all-targets --all-features
git add src/subcommands/resizer.rs src/subcommands/mod.rs src/cli.rs Cargo.toml Cargo.lock
git commit -m "feat: add native resizer rules and PiP geometry"
```

### Task 5: Implement resizer active mode and socket2 daemon

**Files:**
- Modify: `src/subcommands/resizer.rs`
- Test: `src/subcommands/resizer.rs`

**Interfaces:**
- Consumes `ipc::hypr::{message_json, batch, socket2_stream, is_lua_config}`.
- Produces active/matching-window behavior and a daemon which continues after malformed individual events.

- [ ] **Step 1: Write failing Unix socket daemon test**

Create temporary socket2 server that emits a valid event, malformed event, then second valid event. Inject its environment path. Assert daemon handler receives both valid events and records warning/error for the malformed event without returning early.

- [ ] **Step 2: Confirm failure**

```bash
nix develop -c cargo test resizer::tests::daemon -- --nocapture
```

Expected: daemon event loop absent.

- [ ] **Step 3: Implement Hyprland lookup and actions**

Request `clients`, `activewindow`, `workspaces`, and `monitors` as JSON. Apply actions in this order: float when necessary; PiP exclusively when selected; otherwise resize, then center if selected. Use a per-address `HashMap<String, Instant>` to suppress events within one second.

For active mode, `pattern == "active"` targets only active window. For ordinary custom rule mode, request clients and apply to every matching entry. Return clear errors for invalid user rule regex; daemon-config regex errors warn and skip the rule.

- [ ] **Step 4: Implement stream loop**

Use `BufReader::read_line` on `socket2_stream()`. Trim each line, dispatch `windowtitle`/`openwindow`, and continue on parser/action errors. Socket connect/read EOF is the only daemon-ending failure.

- [ ] **Step 5: Run tests and commit**

```bash
nix develop -c cargo test --all-targets --all-features
git add src/subcommands/resizer.rs
git commit -m "feat: port resizer daemon and active modes"
```

### Task 6: Remove Python dispatcher and Python package tree

**Files:**
- Modify: `src/main.rs`
- Modify: `src/cli.rs`
- Modify: `src/subcommands/mod.rs`
- Modify: `tests/delegation.rs`
- Delete: `python-ref/`
- Modify: `default.nix`
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `completions/caelestia.fish`
- Modify: `README.md`

**Interfaces:**
- `main()` always parses `Cli`; no `delegate`, `native_subcommand`, `NATIVE`, `CommandExt`, or Python environment.
- `install`/`update` are absent from `Native`; Clap errors with unknown-subcommand status 2.
- Nix package contains no Python environment/copy/import/wrapper logic.

- [ ] **Step 1: Write failing no-delegation tests**

Replace delegation tests with CLI-surface tests:

```rust
assert!(Cli::try_parse_from(["caelestia", "--help"]).is_ok());
assert!(Cli::try_parse_from(["caelestia", "--version"]).is_ok());
assert!(Cli::try_parse_from(["caelestia", "install"]).is_err());
assert!(Cli::try_parse_from(["caelestia", "update"]).is_err());
```

Add a subprocess test asserting `caelestia --version` does not require `CAELESTIA_PYTHON` or `PYTHONPATH`.

- [ ] **Step 2: Confirm test failure against fallback**

```bash
nix develop -c cargo test delegation cli -- --nocapture
```

Expected: current top-level flags delegate and Python helper tests remain.

- [ ] **Step 3: Make all parsing native**

Remove fallback code from `main.rs`; enable Clap's normal version flag by removing `disable_version_flag = true`. Add `Wallpaper` and `Resizer` to `Native` dispatch. Delete `install`/`update` enum variants if any are introduced. Ensure `--help`, `-h`, `--version`, and `-V` exit successfully without Python.

- [ ] **Step 4: Remove Python packaging**

Delete `python-ref/` and `tests/delegation.rs`. In `default.nix`, remove `python3` argument, `pythonEnv`, Python substitutions, copied `share/caelestia/python`, import check, and `CAELESTIA_PYTHON*` wrapper variables. Retain Rust source substitutions and runtime dependencies including ffmpeg.

- [ ] **Step 5: Update completion and docs**

Remove `install` and `update` from fish command/completion sections. Add any missing full `wallpaper`/`resizer` flags. Rewrite README migration/install text: NixOS source derivation via Cachix, Rust-native command surface, no Python wheel instructions.

- [ ] **Step 6: Verify no Python runtime residue**

Run:

```bash
if rg -n 'python-ref|CAELESTIA_PYTHON|pythonEnv|python3\.withPackages|delegat' \
  src default.nix flake.nix tests completions README.md; then
  exit 1
fi
```

Expected: no matches. Python may remain only in historical committed docs outside the checked paths.

- [ ] **Step 7: Run full tests and Nix checks**

```bash
nix develop -c cargo test --all-targets --all-features
nix flake check --no-build
nix build .#default
result/bin/caelestia --version
if test -e result/share/caelestia/python; then exit 1; fi
```

Expected: all pass; build output has no Python tree.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor: complete native Rust CLI port"
```

### Task 7: Final parity validation and delivery

**Files:**
- Modify: `README.md` only if validation exposes stale command/documentation claims
- Test: full workspace and Nix output

**Interfaces:**
- Consumes all native command modules and Python-free package.
- Produces validated `main`-ready branch.

- [ ] **Step 1: Validate command surface**

```bash
nix develop -c cargo run -- --help
nix develop -c cargo run -- --version
! nix develop -c cargo run -- install
! nix develop -c cargo run -- update
```

Expected: help/version succeed; removed commands return Clap errors.

- [ ] **Step 2: Run full automated suite**

```bash
nix develop -c cargo fmt --check
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo test --all-targets --all-features
nix flake check --no-build
nix build .#default
```

Expected: all exit 0.

- [ ] **Step 3: Review scope and commit any fixes**

```bash
git status --short
git diff main...HEAD --stat
git diff main...HEAD -- default.nix src/main.rs src/cli.rs
```

Expected: only planned Rust/Nix/test/completion/doc changes; no generated files.

- [ ] **Step 4: Commit final validation fixes, if any**

```bash
git add README.md
git commit -m "docs: document Python-free NixOS CLI"
```

Run this only when Task 7 changes README; otherwise do not create an empty commit.
