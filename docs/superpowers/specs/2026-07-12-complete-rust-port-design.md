# Complete Rust Port Design

## Goal

Replace the remaining Python backend in `caelestia-cli` with native Rust while preserving the supported NixOS/Hyprland CLI contract. The final binary has no runtime Python dependency and no `python-ref` tree.

## Scope

### Keep and implement natively

- Existing Rust commands: `shell`, `toggle`, `screenshot`, `record`, `search`, `clipboard`, `emoji`, `scheme`.
- `wallpaper` with full current behavior:
  - print generated colors for a supplied wall;
  - set an image, GIF, or video wallpaper;
  - random selection from a directory, recursive scan, optional size filter, threshold, and current-wall exclusion;
  - GIF/video first-frame cache (`ffmpeg` for videos);
  - thumbnail cache and wallpaper state paths/symlinks;
  - dynamic scheme smart mode/variant selection;
  - color generation/application and configured `postHook` environment.
- `resizer` with full current behavior:
  - configured/default window rules;
  - daemon reading Hyprland socket2 events;
  - title/open event parsing for both `>>` and `>>>` separators;
  - 1-second per-window rate limiting;
  - active rule mode, matching-window rule mode, and `pip` mode;
  - Hyprland legacy and Lua config dispatcher strings.
- Native Clap top-level `--help`, `-h`, `--version`, and `-V`.

### Remove

- `install` and `update` commands. They are invalid for this NixOS-only CLI and become normal Clap unknown-subcommand errors.
- `python-ref/` source and packaged copy.
- Python wrapper environment (`CAELESTIA_PYTHON`, `CAELESTIA_PYTHONPATH`, `PYTHONPATH` handling).
- Python dependencies and import check from `default.nix`.
- Python delegation tests.

## Architecture

`main.rs` parses every invocation using Clap. It has no native-subcommand filter and no `exec` fallback. `cli.rs` exposes only supported Rust commands; unknown commands return Clap's standard exit status 2.

`src/subcommands/wallpaper.rs` owns wall validation, state/cache mutation, conversion process calls, random selection, and hook execution. It uses focused helper modules for filesystem paths, image work, color/scheme application, and process invocation. Persistent path names and data formats must remain byte-compatible with the existing shell contract.

`src/subcommands/resizer.rs` owns rule validation/matching, event parsing, and daemon lifecycle. A small Hypr IPC abstraction supplies command and JSON request transport, while a socket2 adapter streams events. Pure parsing/matching/dispatcher formatting stays independently testable without a running compositor.

## Data and Compatibility

- Preserve existing XDG paths, state text files, cache hash layout, symlink names, JSON configuration shape, rule names, match types, action names, and hook variables.
- Supported wallpaper extensions remain images (`jpg`, `jpeg`, `png`, `webp`, `tif`, `tiff`, `gif`) and videos (`mp4`, `webm`, `mkv`, `avi`, `mov`, `wmv`, `flv`).
- `ffmpeg` stays a Nix runtime dependency for video frame extraction.
- A malformed wallpaper/config/rule/event must produce a clear CLI diagnostic or safe no-op matching current command intent; daemon event failures must not terminate the daemon except unrecoverable socket connection failures.
- `resizer` custom config rule schema remains `{ name, matchType, width, height, actions }`; defaults remain Bitwarden float/center and Picture-in-Picture.

## Error Behavior

- Invalid user request (unknown command, invalid wallpaper path, empty random candidates, invalid explicit regex) exits nonzero with concise diagnostics.
- Runtime failures from `ffmpeg`, Hypr IPC, filesystem mutation, color application, or hook spawn carry contextual errors.
- A malformed daemon event, malformed config rule, missing individual window field, failed lookup, or invalid configured regex logs/warns and continues processing future events.
- `postHook` preserves shell execution and receives `WALLPAPER_PATH`, `SCHEME_NAME`, `SCHEME_FLAVOUR`, `SCHEME_MODE`, `SCHEME_VARIANT`, `SCHEME_COLOURS`, and `THUMBNAIL_PATH`.

## Test Strategy

- Unit tests: CLI command surface; removal of delegation; supported extensions; cache/state path construction; event parser; rule matching; rate limiter; dispatcher formatting; PiP coordinate calculations; hook environment construction.
- Process tests: a fake `ffmpeg` validates command arguments and output handling; hook script validates environment values.
- Socket tests: Unix test sockets validate Hypr IPC request framing and socket2 event stream handling without Hyprland.
- Regression tests: pre-existing native command tests continue passing.
- Nix checks: `nix develop -c cargo test --all-targets --all-features`, `nix flake check --no-build`, and a source package build. Assert `default.nix` has no Python wrapper/import/package dependency and output contains no `share/caelestia/python`.

## Delivery Sequence

1. Add reusable Rust Hypr request/socket and wallpaper filesystem/image helpers with focused tests.
2. Implement and test `wallpaper` parity.
3. Implement and test `resizer` parity.
4. Make all CLI parsing native; remove unsupported commands and Python delegation.
5. Remove Python source/dependencies and update Nix, README, completions, tests, and migration documentation.
6. Run full test/Nix validation, review, commit, and push.

## Non-Goals

- Reintroducing imperative installation/update behavior.
- Supporting non-NixOS platforms.
- Altering Caelestia shell IPC contracts.
- Changing scheme algorithms beyond what is required to remove Python code.
