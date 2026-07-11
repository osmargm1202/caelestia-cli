use anyhow::Result;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

pub mod generator;
pub mod score;

pub use generator::gen_scheme;
pub use score::score;

/// Cached colour payload matching the Python reference output exactly.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
pub struct ColourMap {
    #[serde(flatten)]
    pub colours: BTreeMap<String, String>,
}

/// Loads the cached colour JSON for a given image/scheme/variant/flavour/mode tuple.
#[allow(dead_code)]
pub fn get_colours_for_image(
    image: &Path,
    scheme_name: &str,
    variant: &str,
    flavour: &str,
    mode: &str,
) -> Result<ColourMap> {
    let cache = crate::util::paths::scheme_cache_dir();
    let key = crate::util::paths::compute_hash(image);
    let base = cache.join(key);
    let path = base
        .join(variant)
        .join(flavour)
        .join(mode)
        .with_extension("json");

    if let Ok(text) = std::fs::read_to_string(&path) {
        let map: BTreeMap<String, String> = serde_json::from_str(&text)?;
        return Ok(ColourMap { colours: map });
    }

    let primary = score(image)?;
    let scheme = generator::SchemeView {
        name: scheme_name,
        flavour,
        mode,
        variant,
    };
    let colours = gen_scheme(&scheme, primary);

    std::fs::create_dir_all(path.parent().expect("scheme cache parent"))?;
    std::fs::write(&path, serde_json::to_string(&colours)?)?;

    Ok(ColourMap { colours })
}
