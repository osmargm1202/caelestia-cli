# Task 4 implementation report

## Scope

Implemented native resizer foundations while leaving live Hyprland queries, socket handling, rate limiting, and daemon execution to Task 5.

## Changes

- Added `WindowRule`, `MatchType`, and `Action` serde models using exact Python JSON names (`matchType`, camel-case match modes, lowercase actions).
- Preserved Python default rules and atomic fallback to defaults for absent or malformed `resizer.rules` configuration.
- Added pure matching for `titleContains`, `titleExact`, `initialTitle`, and Rust `regex`-backed `titleRegex`.
- Added pure `windowtitle` and `openwindow` parsing for both `>>` and `>>>`; invalid/empty non-hex addresses return `None` without panic.
- Added exact legacy and Lua dispatcher string formatters.
- Added pure PiP geometry preserving Python scaling, independent minimum clamps, logical monitor dimensions, 3% margin, and integer truncation.
- Added Python-compatible CLI forms: `--daemon`, `pip`, and optional pattern/match type/width/height/actions positionals.
- Registered native resizer dispatch with a Task 5 runtime seam (`run`).
- Added `regex` dependency.

## TDD evidence

1. Model/event tests failed with missing `default_rules`, types, and `parse_event` (exit 101).
2. Config/matching/dispatcher/geometry tests failed with missing interfaces (exit 101).
3. CLI parser test failed with missing `ResizerArgs`, `MatchTypeArg`, and `Native::Resizer` (exit 101).
4. Focused resizer tests passed after minimal implementations: 10 passed, 0 failed.

## Verification

Final verification command: `nix develop -c cargo test --all-targets --all-features`.

## Task 5 boundary

`run` intentionally exposes the native entry point without implementing active-window/client/monitor/workspace lookups or the event socket daemon. Pure models, parser, dispatcher formatters, and geometry are ready for that runtime.

---

## Task 4 quality finding follow-up

### Fix

- Changed `pip_geometry` to return `Option<PipGeometry>` so invalid geometry cannot reach dispatcher construction.
- Rejects non-finite window width/height, monitor position/dimensions, and scale.
- Rejects non-positive window dimensions, monitor dimensions, and scale before division or scaling.
- Rejects non-finite intermediate/output coordinates before integer casts.
- Preserved valid Python-compatible fixture and minimum-clamp results.

### Regression/TDD evidence

- Added zero scale, zero window width/height, zero monitor width/height, NaN, and infinity cases.
- RED: `nix develop -c cargo test resizer` failed because old API returned `PipGeometry` and invalid cases could not return `None`.
- GREEN: focused resizer run passed 12 tests, 0 failed.

### Verification

- `nix develop -c cargo test resizer` — passed, 12 tests, 0 failed.
- `nix develop -c cargo test` — passed, exit 0 (55 unit tests plus integration suites).
- `nix develop -c cargo fmt -- --check` — passed.
- Existing unrelated warnings remain in `scheme.rs` (unused import) and `ipc/hypr.rs` (dead code).
