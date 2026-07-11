//! Golden parity harness: regenerate colour schemes with both Rust and the Python
//! reference implementation and compare the JSON output.
//!
//! Tests are skipped unless the env var `CAELESTIA_GOLDEN_DIR` points at a directory
//! of sample wallpapers, and `CAELESTIA_PYTHONPATH` points at the Python reference
//! tree (`python-ref/src`). The harness reads scheme variants/flavours/modes from
//! the env vars `CAELESTIA_GOLDEN_VARIANTS`, `CAELESTIA_GOLDEN_FLAVOURS`, and
//! `CAELESTIA_GOLDEN_MODES` (comma-separated).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

// The binary crate doesn't expose its modules; reach into the public via env var
// shell-only test entry. The harness runs the helper Rust binary instead via
// `target/debug/caelestia ...` for the parity comparison.

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

fn samples() -> Option<Vec<PathBuf>> {
    let dir = env("CAELESTIA_GOLDEN_DIR")?;
    let path = PathBuf::from(dir);
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&path).ok()?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("png") {
            out.push(path);
        }
    }
    out.sort();
    Some(out)
}

fn parse_list(env_key: &str, default: &[&str]) -> Vec<String> {
    env(env_key)
        .map(|v| v.split(',').map(|s| s.trim().to_owned()).collect())
        .unwrap_or_else(|| default.iter().map(|s| s.to_string()).collect())
}

fn run_python(image: &Path, variant: &str, flavour: &str, mode: &str) -> BTreeMap<String, String> {
    let pythonpath = env("CAELESTIA_PYTHONPATH").expect("CAELESTIA_PYTHONPATH");
    let script = r#"
import importlib.util, json, os, sys, types

# Synthesize minimal `caelestia` package structure so the material helpers import
# without triggering the real subcommand module graph.
pkg = types.ModuleType("caelestia")
pkg.__path__ = [sys.argv[5]]
sys.modules["caelestia"] = pkg

utils_pkg = types.ModuleType("caelestia.utils")
utils_pkg.__path__ = [sys.argv[5] + "/caelestia/utils"]
sys.modules["caelestia.utils"] = utils_pkg

# Stub the few symbols the material helpers reach for.
paths_stub = types.ModuleType("caelestia.utils.paths")
_Path = __import__("pathlib").Path
_CACHE = _Path("/tmp/caelestia-golden-cache")
paths_stub.compute_hash = lambda path: format("{:016x}", abs(hash(str(path))) & ((1 << 64) - 1))
paths_stub.scheme_cache_dir = lambda: _CACHE
paths_stub.scheme_data_dir = lambda: _CACHE
paths_stub.wallpaper_thumbnail_path = lambda: _Path(sys.argv[1])
sys.modules["caelestia.utils.paths"] = paths_stub

score_pkg = types.ModuleType("caelestia.utils.material")
score_pkg.__path__ = [sys.argv[5] + "/caelestia/utils/material"]
sys.modules["caelestia.utils.material"] = score_pkg

spec = importlib.util.spec_from_file_location("caelestia_material", sys.argv[5] + "/caelestia/utils/material/__init__.py")
mod = importlib.util.module_from_spec(spec); spec.loader.exec_module(mod)
spec2 = importlib.util.spec_from_file_location("caelestia_gen", sys.argv[5] + "/caelestia/utils/material/generator.py")
gen = importlib.util.module_from_spec(spec2); spec2.loader.exec_module(gen)

image, variant, flavour, mode = sys.argv[1:5]
primary = mod.get_score_for_image(image, paths_stub.scheme_cache_dir())
scheme = type("S", (), {"variant": variant, "flavour": flavour, "mode": mode, "name": "x"})()
colours = gen.gen_scheme(scheme, primary)
# Strip the `0x` prefix and zero-pad to 6 hex chars per the Python output format.
out = {k: format(int(v, 16) & 0xFFFFFF, "06x") for k, v in colours.items()}
print(json.dumps(out, sort_keys=True))
"#;
    let script_path = std::env::temp_dir().join("caelestia-golden-script.py");
    std::fs::write(&script_path, script).unwrap();
    let python = env("CAELESTIA_PYTHON_BIN").unwrap_or_else(|| "python3".to_string());
    let extra_paths = env("CAELESTIA_PYTHON_EXTRA").unwrap_or_default();
    let combined = if extra_paths.is_empty() {
        pythonpath.clone()
    } else {
        format!("{extra_paths}:{pythonpath}")
    };
    let output = Command::new(&python)
        .arg(&script_path)
        .arg(image)
        .arg(variant)
        .arg(flavour)
        .arg(mode)
        .arg(&pythonpath)
        .env("PYTHONPATH", &combined)
        .output()
        .expect("failed to spawn python golden harness");
    assert!(
        output.status.success(),
        "python golden harness failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&text).expect("invalid JSON from python harness")
}

fn run_rust(image: &Path, variant: &str, flavour: &str, mode: &str) -> BTreeMap<String, String> {
    // Defer to the compiled Rust binary via a JSON helper subcommand. Until the
    // parity subcommand is wired in, fall back to a placeholder invocation that
    // the user can swap out via the `CAELESTIA_GOLDEN_BIN` env var.
    let bin = env("CAELESTIA_GOLDEN_BIN")
        .unwrap_or_else(|| "target/debug/caelestia".to_string());
    let output = Command::new(&bin)
        .args([
            "golden", "--image", image.to_str().unwrap(), "--variant", variant,
            "--flavour", flavour, "--mode", mode,
        ])
        .output()
        .expect("failed to spawn rust golden harness");
    if !output.status.success() {
        eprintln!(
            "rust golden harness failed (status={}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        return BTreeMap::new();
    }
    let text = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&text).unwrap_or_default()
}

fn compare(a: &BTreeMap<String, String>, b: &BTreeMap<String, String>) -> Vec<String> {
    let mut diffs = Vec::new();
    for (key, lhs) in a {
        match b.get(key) {
            Some(rhs) => {
                let l = u32::from_str_radix(lhs, 16).unwrap_or(0);
                let r = u32::from_str_radix(rhs, 16).unwrap_or(0);
                let lr = (l >> 16) & 0xFF;
                let lg = (l >> 8) & 0xFF;
                let lb = l & 0xFF;
                let rr = (r >> 16) & 0xFF;
                let rg = (r >> 8) & 0xFF;
                let rb = r & 0xFF;
                let delta = lr.abs_diff(rr).max(lg.abs_diff(rg)).max(lb.abs_diff(rb));
                if delta > 1 {
                    diffs.push(format!("{key}: rust={lhs} python={rhs} (delta={delta})"));
                }
            }
            None => diffs.push(format!("{key}: rust={lhs} missing in python")),
        }
    }
    for key in b.keys() {
        if !a.contains_key(key) {
            diffs.push(format!("{key}: present in python, missing in rust"));
        }
    }
    diffs
}

#[test]
fn golden_parity_stub() {
    // Individual golden tests are listed below; we keep this stub so the binary
    // always links something even when both env vars are missing.
    let rust: BTreeMap<String, String> = BTreeMap::new();
    let python: BTreeMap<String, String> = BTreeMap::new();
    assert_eq!(compare(&rust, &python), Vec::<String>::new());
}

#[test]
fn golden_parity_across_samples() {
    let Some(samples) = samples() else {
        eprintln!(
            "skipping golden parity test (set CAELESTIA_GOLDEN_DIR to a directory with .png samples)"
        );
        return;
    };
    if samples.is_empty() {
        eprintln!("golden parity: CAELESTIA_GOLDEN_DIR contains no png files");
        return;
    }
    let variants = parse_list("CAELESTIA_GOLDEN_VARIANTS", &["tonalspot", "monochrome"]);
    let flavours = parse_list("CAELESTIA_GOLDEN_FLAVOURS", &["default", "hard"]);
    let modes = parse_list("CAELESTIA_GOLDEN_MODES", &["dark", "light"]);

    let mut failures = 0;
    for image in &samples {
        for variant in &variants {
            for flavour in &flavours {
                for mode in &modes {
                    let rust = run_rust(image, variant, flavour, mode);
                    let python = run_python(image, variant, flavour, mode);
                    let diffs = compare(&rust, &python);
                    if !diffs.is_empty() {
                        failures += 1;
                        eprintln!(
                            "MISMATCH image={} variant={} flavour={} mode={}",
                            image.display(),
                            variant,
                            flavour,
                            mode
                        );
                        for d in diffs {
                            eprintln!("  - {d}");
                        }
                    }
                }
            }
        }
    }
    assert_eq!(failures, 0, "golden parity mismatches: {failures}");
}