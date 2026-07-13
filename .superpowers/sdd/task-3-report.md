# Task 3 Report: Wallpaper smart selection and post-hook compatibility

## Changes

- Added `SmartOptions` cache payload serialized as `{ "variant": String, "mode": String }` in each wallpaper cache's `smart.json`.
- Added cache reuse and malformed/unreadable-cache recomputation.
- Ported Python `utils/colourfulness.py` exactly:
  - `rg = abs(r - g)`
  - `yb = abs(0.5 * (r + g) - b)`
  - population standard deviations
  - score `sqrt(std_rg² + std_yb²) + 0.3 * sqrt(mean_rg² + mean_yb²)`
  - `< 10 neutral`, `< 20 content`, otherwise `tonalspot`.
- Added 1x1 Lanczos thumbnail averaging and Material HCT tone selection (`tone > 60` => `light`, otherwise `dark`).
- Applied smart mode/variant in both print palette and wallpaper mutation flows unless `--no-smart` is set.
- Added exact post-hook environment names and JSON colour payload.
- Added `wallpaper.postHook` string handling via `sh -c`, inherited process environment plus hook variables, suppressed stderr, ignored nonzero status, and contextualized spawn errors.
- Kept Python implementation and Task 2 behavior intact outside smart-enabled paths.

## TDD evidence

Initial targeted run failed with missing `smart_options`, `colourfulness_variant`, and `post_hook_env` symbols. After implementation, smart tests passed. A separate post-hook execution test first failed with missing `execute_post_hook`, then passed after extraction/implementation.

Added tests for:

- first smart calculation and exact cache payload;
- valid cache reuse without reading source image;
- malformed cache recomputation;
- all three Python colourfulness variant bands;
- exact hook environment values and colour JSON;
- inherited environment and ignored nonzero hook exit.

## Commands and results

- `nix develop -c cargo test wallpaper::tests::smart -- --nocapture` — RED: expected missing-function compile failures; GREEN: 3 passed.
- `nix develop -c cargo test wallpaper::tests -- --nocapture` — 15 passed before final hook execution test.
- `nix develop -c cargo test wallpaper::tests::post_hook_ignores -- --nocapture` — RED then GREEN: 1 passed.
- `nix develop -c cargo fmt` — passed.
- `nix develop -c cargo test --all-targets --all-features` — exit 0; groups: 42 + 3 + 2 + 2 + 4 tests passed, 0 failed.
- `git diff --check` — passed.

## Self-review

- Verified smart inference runs only for dynamic persisted schemes during mutation and honors `no_smart`; print output remains dynamic and honors `no_smart` as Python does.
- Verified hook runs only after successful scheme application, receives canonical original wallpaper and generated thumbnail paths, and non-string/missing config is ignored.
- Verified nonzero hook exits do not turn a successful wallpaper change into failure.
- Verified no material adapter or Python removal was needed.

## Concerns

- Full suite retains two pre-existing compiler warnings: unused `clap::Subcommand` import and unused `socket2_stream`; Task 3 introduces no new warning.

---

# Task 3 quality fix: corrupt thumbnail cache self-recovery

## Finding addressed

`generate_thumbnail()` previously treated any existing `thumbnail.jpg` as a valid cache hit. A zero-byte, partial, or otherwise undecodable cached file therefore returned success without regeneration, and downstream palette/smart processing failed while opening the corrupt thumbnail.

## Fix

- Validate an existing cached thumbnail by decoding it and checking expected generated-thumbnail dimensions: nonzero, no side above 128 pixels, and one side exactly 128 pixels.
- Preserve the immediate return for valid cached thumbnails.
- Remove invalid cached thumbnails before regeneration.
- Decode and resize the source with the existing nearest-neighbor behavior.
- Write regenerated JPEG bytes to a unique sibling temporary path, then atomically rename that file into the cache path.
- Clean up the temporary file when encoding or installation fails and retain contextual errors for source/regeneration failures.

## Regression coverage and TDD evidence

Added `corrupt_cached_thumbnail_is_replaced_with_valid_image`, which creates a valid 160x90 source and corrupt bytes at the expected thumbnail path, invokes `generate_thumbnail()`, then verifies the cache path contains a decodable 128x72 image.

RED run before production change:

- `nix develop -c cargo test corrupt_cached_thumbnail_is_replaced_with_valid_image --all-features` — exit 101; regression failed because corrupt bytes remained at the cache path.

GREEN and verification runs:

- `nix develop -c cargo test subcommands::wallpaper::tests --all-features` — exit 0; 17 wallpaper tests passed, 0 failed.
- `nix develop -c cargo fmt --all -- --check` — exit 0.
- `nix develop -c cargo test --all-targets --all-features` — exit 0; test groups 43 + 3 + 2 + 2 + 4 passed, 0 failed.
- `git diff --check` — exit 0.

## Scope and concerns

Only `generate_thumbnail()`, its regression test, and this report changed. No unrelated wallpaper behavior changed. No known concerns.
