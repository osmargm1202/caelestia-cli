use std::path::Path;

use anyhow::{Context, Result};
use indexmap::IndexMap;
use material_colors::color::Argb;
use material_colors::image::{AsPixels, ImageReader};
use material_colors::quantize::{Quantizer, QuantizerCelebi};
use material_colors::score::Score;

// The material-colors `Score` API expects its private `IndexMap` alias which uses
// a specific ahash hasher; replicate it locally so the call type-checks.
type ScoreIndexMap<K, V> = IndexMap<K, V, core::hash::BuildHasherDefault<ahash::AHasher>>;

/// Computes the HCT-derived primary colour for the supplied image, matching the
/// Python `Score.score(ImageQuantizeCelebi(image, 1, 128))` flow: the image is
/// decoded, quantised to 128 representative colours, and ranked for chroma/coverage.
pub fn score<P: AsRef<Path>>(image: P) -> Result<Argb> {
    score_inner(image.as_ref())
}

fn score_inner(image: &Path) -> Result<Argb> {
    let decoded = ImageReader::open(image)
        .with_context(|| format!("failed to open image {}", image.display()))?;
    let pixels = decoded.as_pixels();
    if pixels.is_empty() {
        anyhow::bail!("image {} has no pixels", image.display());
    }
    let result = QuantizerCelebi::quantize(&pixels, 128);
    let population: ScoreIndexMap<Argb, u32> = result.color_to_count.into_iter().collect();
    let mut scored = Score::score(&population, None, None, None);
    if scored.is_empty() {
        scored = Score::score(&population, None, None, Some(false));
    }
    scored
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("scorer produced no primary colour"))
}
