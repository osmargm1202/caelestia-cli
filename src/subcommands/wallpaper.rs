use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, clap::Args)]
pub struct WallpaperArgs {
    #[arg(short = 'p', long, value_name = "PATH", num_args = 0..=1, default_missing_value = "")]
    pub print: Option<String>,
    #[arg(short = 'f', long, value_name = "PATH")]
    pub file: Option<String>,
    #[arg(short = 'r', long, value_name = "DIRECTORY", num_args = 0..=1, default_missing_value = "")]
    pub random: Option<String>,
    #[arg(short = 'n', long)]
    pub no_filter: bool,
    #[arg(short = 't', long, default_value_t = 0.8)]
    pub threshold: f64,
    #[arg(short = 'N', long)]
    pub no_smart: bool,
}

pub fn run(args: WallpaperArgs) -> anyhow::Result<()> {
    if let Some(path) = args.print {
        let path = if path.is_empty() {
            std::fs::read_to_string(crate::util::paths::wallpaper_path_path())
                .context("no wallpaper set")?
        } else {
            path
        };
        println!("{}", wallpaper_json(Path::new(&path), args.no_smart)?);
    } else if let Some(path) = args.file {
        set_wallpaper(Path::new(&path), args.no_smart)?;
    } else if let Some(directory) = args.random {
        let directory = if directory.is_empty() {
            std::env::var_os("CAELESTIA_WALLPAPERS_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| crate::util::paths::pictures_dir().join("Wallpapers"))
        } else {
            PathBuf::from(directory)
        };
        set_random(&directory, args.no_filter, args.threshold, args.no_smart)?;
    } else if let Ok(path) = std::fs::read_to_string(crate::util::paths::wallpaper_path_path()) {
        println!("{path}");
    } else {
        println!("No wallpaper set");
    }
    Ok(())
}

fn extension_in(path: &Path, accepted: &[&str]) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| accepted.contains(&extension.to_ascii_lowercase().as_str()))
}

fn is_valid_image(path: &Path) -> bool {
    extension_in(path, &["jpg", "jpeg", "png", "webp", "tif", "tiff", "gif"])
}

fn is_valid_video(path: &Path) -> bool {
    extension_in(path, &["mp4", "webm", "mkv", "avi", "mov", "wmv", "flv"])
}

fn is_valid_wallpaper(path: &Path) -> bool {
    is_valid_image(path) || is_valid_video(path)
}

fn cache_path(source: &Path, cache_root: &Path) -> PathBuf {
    cache_root.join(crate::util::paths::compute_hash(source))
}

fn thumbnail_path(source: &Path, cache_root: &Path) -> PathBuf {
    cache_path(source, cache_root).join("thumbnail.jpg")
}

fn ffmpeg_command(source: &Path, output: &Path) -> Command {
    let mut command = Command::new("ffmpeg");
    command.args(["-y", "-loglevel", "error", "-i"]);
    command.arg(source);
    command.args(["-vframes", "1", "-vf", "scale=512:-1"]);
    command.arg(output);
    command
}

fn converted_source(wall: &Path, cache: &Path) -> Result<PathBuf> {
    let output = cache.join("first_frame.png");
    if output.exists() {
        return Ok(output);
    }
    std::fs::create_dir_all(cache)
        .with_context(|| format!("creating wallpaper cache {}", cache.display()))?;
    if wall
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gif"))
    {
        image::open(wall)
            .with_context(|| format!("opening GIF {}", wall.display()))?
            .to_rgb8()
            .save_with_format(&output, image::ImageFormat::Png)
            .with_context(|| format!("writing first GIF frame to {}", output.display()))?;
    } else if is_valid_video(wall) {
        let status = ffmpeg_command(wall, &output)
            .status()
            .with_context(|| format!("running ffmpeg for {}", wall.display()))?;
        if !status.success() {
            anyhow::bail!(
                "ffmpeg failed to extract first frame from {}",
                wall.display()
            );
        }
    } else {
        return Ok(wall.to_path_buf());
    }
    Ok(output)
}

fn collect_wallpapers(directory: &Path, walls: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(directory)
        .with_context(|| format!("reading wallpaper directory {}", directory.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_wallpapers(&path, walls)?;
        } else if path.is_file() && is_valid_wallpaper(&path) {
            walls.push(path);
        }
    }
    Ok(())
}

fn random_candidates(
    directory: &Path,
    current: Option<&Path>,
    no_filter: bool,
    filter_size: Option<(u32, u32)>,
    threshold: f64,
) -> Result<Vec<PathBuf>> {
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut walls = Vec::new();
    collect_wallpapers(directory, &mut walls)?;
    walls.sort();
    if !no_filter {
        if let Some((min_width, min_height)) = filter_size {
            walls.retain(|wall| {
                if is_valid_video(wall) {
                    return true;
                }
                image::image_dimensions(wall).is_ok_and(|(width, height)| {
                    f64::from(width) >= f64::from(min_width) * threshold
                        && f64::from(height) >= f64::from(min_height) * threshold
                })
            });
        }
    }
    if let Some(current) = current {
        if walls
            .iter()
            .filter(|wall| wall.as_path() != current)
            .count()
            > 0
        {
            walls.retain(|wall| wall.as_path() != current);
        }
    }
    Ok(walls)
}

fn canonical_wallpaper(wall: &Path) -> Result<PathBuf> {
    let canonical = wall
        .canonicalize()
        .with_context(|| format!("cannot resolve wallpaper {}", wall.display()))?;
    if !canonical.is_file() || !is_valid_wallpaper(&canonical) {
        anyhow::bail!("\"{}\" is not a valid wallpaper", canonical.display());
    }
    Ok(canonical)
}

fn source_for_palette(wall: &Path, cache: &Path) -> Result<PathBuf> {
    if wall
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gif"))
        || is_valid_video(wall)
    {
        converted_source(wall, cache)
    } else {
        Ok(wall.to_path_buf())
    }
}

fn generate_thumbnail(source: &Path, thumbnail: &Path) -> Result<()> {
    if thumbnail.exists() {
        return Ok(());
    }
    let image = image::open(source)
        .with_context(|| format!("opening wallpaper image {}", source.display()))?;
    let resized = image
        .resize(128, 128, image::imageops::FilterType::Nearest)
        .to_rgb8();
    if let Some(parent) = thumbnail.parent() {
        std::fs::create_dir_all(parent)?;
    }
    resized
        .save_with_format(thumbnail, image::ImageFormat::Jpeg)
        .with_context(|| format!("writing thumbnail {}", thumbnail.display()))
}

#[derive(Debug, Deserialize, Serialize)]
struct SmartOptions {
    variant: String,
    mode: String,
}

fn colourfulness_variant(image: &image::RgbImage) -> &'static str {
    let count = f64::from(image.width()) * f64::from(image.height());
    if count == 0.0 {
        return "neutral";
    }
    let (rg, yb): (Vec<_>, Vec<_>) = image
        .pixels()
        .map(|pixel| {
            let [r, g, b] = pixel.0.map(f64::from);
            ((r - g).abs(), (0.5 * (r + g) - b).abs())
        })
        .unzip();
    let mean = |values: &[f64]| values.iter().sum::<f64>() / count;
    let mean_rg = mean(&rg);
    let mean_yb = mean(&yb);
    let stddev = |values: &[f64], average: f64| {
        (values
            .iter()
            .map(|value| (value - average).powi(2))
            .sum::<f64>()
            / count)
            .sqrt()
    };
    let colourfulness = (stddev(&rg, mean_rg).powi(2) + stddev(&yb, mean_yb).powi(2)).sqrt()
        + 0.3 * (mean_rg.powi(2) + mean_yb.powi(2)).sqrt();
    if colourfulness < 10.0 {
        "neutral"
    } else if colourfulness < 20.0 {
        "content"
    } else {
        "tonalspot"
    }
}

fn smart_options(thumbnail: &Path, cache: &Path) -> Result<SmartOptions> {
    let options_path = cache.join("smart.json");
    if let Ok(text) = std::fs::read_to_string(&options_path) {
        if let Ok(options) = serde_json::from_str(&text) {
            return Ok(options);
        }
    }

    let image = image::open(thumbnail)
        .with_context(|| format!("opening thumbnail {}", thumbnail.display()))?
        .to_rgb8();
    let variant = colourfulness_variant(&image).to_owned();
    let average = image::DynamicImage::ImageRgb8(image)
        .resize(1, 1, image::imageops::FilterType::Lanczos3)
        .to_rgb8();
    let [red, green, blue] = average.get_pixel(0, 0).0;
    let tone = <material_colors::hct::Hct as From<material_colors::color::Argb>>::from(
        material_colors::color::Argb {
            alpha: 255,
            red,
            green,
            blue,
        },
    )
    .get_tone();
    let options = SmartOptions {
        variant,
        mode: if tone > 60.0 { "light" } else { "dark" }.to_owned(),
    };
    std::fs::create_dir_all(cache)?;
    std::fs::write(&options_path, serde_json::to_string(&options)?)
        .with_context(|| format!("writing smart wallpaper cache {}", options_path.display()))?;
    Ok(options)
}

fn post_hook_env(
    wall: &Path,
    thumbnail: &Path,
    scheme: &crate::core::scheme::Scheme,
) -> Result<BTreeMap<String, String>> {
    Ok(BTreeMap::from([
        ("WALLPAPER_PATH".into(), wall.display().to_string()),
        ("SCHEME_NAME".into(), scheme.name.clone()),
        ("SCHEME_FLAVOUR".into(), scheme.flavour.clone()),
        ("SCHEME_MODE".into(), scheme.mode.clone()),
        ("SCHEME_VARIANT".into(), scheme.variant.clone()),
        (
            "SCHEME_COLOURS".into(),
            serde_json::to_string(&scheme.colours).context("serialising post-hook colours")?,
        ),
        ("THUMBNAIL_PATH".into(), thumbnail.display().to_string()),
    ]))
}

fn execute_post_hook(
    hook: &str,
    wall: &Path,
    thumbnail: &Path,
    scheme: &crate::core::scheme::Scheme,
) -> Result<()> {
    Command::new("sh")
        .arg("-c")
        .arg(hook)
        .envs(post_hook_env(wall, thumbnail, scheme)?)
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("spawning wallpaper postHook: {hook}"))?;
    Ok(())
}

fn run_post_hook(
    wall: &Path,
    thumbnail: &Path,
    scheme: &crate::core::scheme::Scheme,
) -> Result<()> {
    let config = crate::util::paths::get_config();
    let Some(hook) = config
        .get("wallpaper")
        .and_then(|wallpaper| wallpaper.get("postHook"))
        .and_then(serde_json::Value::as_str)
    else {
        return Ok(());
    };
    execute_post_hook(hook, wall, thumbnail, scheme)
}

fn palette_scheme(wall: &Path, no_smart: bool) -> Result<crate::core::scheme::Scheme> {
    let canonical = canonical_wallpaper(wall)?;
    let cache_root = crate::util::paths::wallpapers_cache_dir();
    let cache = cache_path(&canonical, &cache_root);
    let palette_source = source_for_palette(&canonical, &cache)?;
    let thumbnail = cache.join("thumbnail.jpg");
    generate_thumbnail(&palette_source, &thumbnail)?;

    let current = crate::core::scheme::Scheme::load()?;
    let mut scheme = crate::core::scheme::Scheme {
        name: "dynamic".into(),
        flavour: current.flavour,
        mode: current.mode,
        variant: current.variant,
        colours: Default::default(),
    };
    if !no_smart {
        let options = smart_options(&thumbnail, &cache)?;
        scheme.mode = options.mode;
        scheme.variant = options.variant;
    }
    scheme.colours = crate::core::material::get_colours_for_image(
        &thumbnail,
        &scheme.name,
        &scheme.variant,
        &scheme.flavour,
        &scheme.mode,
    )?
    .colours
    .into_iter()
    .collect();
    Ok(scheme)
}

fn wallpaper_json(wall: &Path, no_smart: bool) -> Result<String> {
    serde_json::to_string(&palette_scheme(wall, no_smart)?).context("serialising wallpaper palette")
}

fn replace_symlink(target: &Path, link: &Path) -> Result<()> {
    let parent = link.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let file_name = link
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("symlink path has no file name: {}", link.display()))?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = (0_u32..)
        .find_map(|attempt| {
            let mut temporary_name = file_name.to_os_string();
            temporary_name.push(format!(".tmp-{}-{nonce}-{attempt}", std::process::id()));
            let temporary = parent.join(temporary_name);
            match std::os::unix::fs::symlink(target, &temporary) {
                Ok(()) => Some(Ok(temporary)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => None,
                Err(error) => Some(Err(error)),
            }
        })
        .transpose()
        .with_context(|| format!("creating temporary symlink for {}", link.display()))?
        .expect("unbounded temporary symlink attempts cannot be exhausted");

    if let Err(error) = std::fs::rename(&temporary, link) {
        let cleanup = std::fs::remove_file(&temporary);
        return match cleanup {
            Ok(()) => Err(error).with_context(|| {
                format!("renaming temporary symlink to {}", link.display())
            }),
            Err(cleanup_error) => Err(anyhow::anyhow!(
                "renaming temporary symlink to {} failed: {error}; cleanup of {} failed: {cleanup_error}",
                link.display(),
                temporary.display()
            )),
        };
    }
    Ok(())
}

fn write_current_wallpaper(wall: &Path) -> Result<()> {
    let path = crate::util::paths::wallpaper_path_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, wall.as_os_str().as_encoded_bytes())?;
    std::fs::rename(temporary, path)?;
    Ok(())
}

fn set_wallpaper(wall: &Path, no_smart: bool) -> Result<()> {
    let canonical = canonical_wallpaper(wall)?;
    let cache_root = crate::util::paths::wallpapers_cache_dir();
    let conversion_cache = cache_path(&canonical, &cache_root);
    let palette_source = source_for_palette(&canonical, &conversion_cache)?;
    let thumbnail = thumbnail_path(&palette_source, &cache_root);
    generate_thumbnail(&palette_source, &thumbnail)?;

    write_current_wallpaper(&canonical)?;
    replace_symlink(&canonical, &crate::util::paths::wallpaper_link_path())?;
    replace_symlink(&thumbnail, &crate::util::paths::wallpaper_thumbnail_path())?;

    let mut scheme = crate::core::scheme::Scheme::load()?;
    if scheme.name == "dynamic" {
        if !no_smart {
            let options = smart_options(&thumbnail, thumbnail.parent().expect("thumbnail cache"))?;
            scheme.mode = options.mode;
            scheme.variant = options.variant;
        }
        scheme.colours = crate::core::material::get_colours_for_image(
            &thumbnail,
            &scheme.name,
            &scheme.variant,
            &scheme.flavour,
            &scheme.mode,
        )?
        .colours
        .into_iter()
        .collect();
    } else {
        scheme.colours = crate::core::scheme::read_colours_from_file(&scheme.colours_path());
    }
    crate::core::scheme::apply_scheme(&scheme)?;
    run_post_hook(&canonical, &thumbnail, &scheme)
}

fn monitor_filter_size() -> Result<(u32, u32)> {
    let monitors = crate::ipc::hypr::message_json("monitors")?;
    let monitors = monitors
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Hyprland monitors response is not an array"))?;
    let width = monitors
        .iter()
        .filter_map(|monitor| monitor.get("width")?.as_u64())
        .min()
        .ok_or_else(|| anyhow::anyhow!("Hyprland returned no monitor widths"))?;
    let height = monitors
        .iter()
        .filter_map(|monitor| monitor.get("height")?.as_u64())
        .min()
        .ok_or_else(|| anyhow::anyhow!("Hyprland returned no monitor heights"))?;
    Ok((u32::try_from(width)?, u32::try_from(height)?))
}

fn set_random(directory: &Path, no_filter: bool, threshold: f64, no_smart: bool) -> Result<()> {
    let current = std::fs::read_to_string(crate::util::paths::wallpaper_path_path())
        .ok()
        .map(PathBuf::from);
    let filter_size = if no_filter {
        None
    } else {
        Some(monitor_filter_size()?)
    };
    let candidates = random_candidates(
        directory,
        current.as_deref(),
        no_filter,
        filter_size,
        threshold,
    )?;
    if candidates.is_empty() {
        anyhow::bail!("No valid wallpapers found");
    }
    let index = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as usize
        % candidates.len();
    set_wallpaper(&candidates[index], no_smart)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "caelestia-wallpaper-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn save_image(path: &Path, width: u32, height: u32) {
        image::RgbImage::from_pixel(width, height, image::Rgb([30, 60, 90]))
            .save(path)
            .unwrap();
    }

    #[test]
    fn smart_options_write_cache_and_reuse_valid_json() {
        let root = temp_dir("smart-cache");
        let thumbnail = root.join("thumbnail.png");
        image::RgbImage::from_pixel(2, 2, image::Rgb([245, 245, 245]))
            .save(&thumbnail)
            .unwrap();

        let calculated = smart_options(&thumbnail, &root).unwrap();
        assert_eq!(calculated.variant, "neutral");
        assert_eq!(calculated.mode, "light");
        let cache_path = root.join("smart.json");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(
                &std::fs::read_to_string(&cache_path).unwrap()
            )
            .unwrap(),
            serde_json::json!({ "variant": "neutral", "mode": "light" })
        );

        std::fs::write(&cache_path, r#"{"variant":"content","mode":"dark"}"#).unwrap();
        let cached = smart_options(&root.join("missing.png"), &root).unwrap();
        assert_eq!(cached.variant, "content");
        assert_eq!(cached.mode, "dark");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn smart_options_recompute_malformed_cache() {
        let root = temp_dir("smart-malformed");
        let thumbnail = root.join("thumbnail.png");
        image::RgbImage::from_pixel(1, 1, image::Rgb([0, 0, 0]))
            .save(&thumbnail)
            .unwrap();
        std::fs::write(root.join("smart.json"), "not json").unwrap();

        let options = smart_options(&thumbnail, &root).unwrap();

        assert_eq!(options.variant, "neutral");
        assert_eq!(options.mode, "dark");
        assert!(serde_json::from_str::<serde_json::Value>(
            &std::fs::read_to_string(root.join("smart.json")).unwrap()
        )
        .is_ok());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn smart_variant_matches_python_colourfulness_thresholds() {
        let neutral = image::RgbImage::from_pixel(1, 1, image::Rgb([20, 20, 20]));
        let content = image::RgbImage::from_pixel(1, 1, image::Rgb([50, 0, 0]));
        let tonalspot = image::RgbImage::from_pixel(1, 1, image::Rgb([255, 0, 0]));

        assert_eq!(colourfulness_variant(&neutral), "neutral");
        assert_eq!(colourfulness_variant(&content), "content");
        assert_eq!(colourfulness_variant(&tonalspot), "tonalspot");
    }

    #[test]
    fn post_hook_environment_matches_python_reference() {
        let wall = Path::new("/walls/current.png");
        let thumbnail = Path::new("/cache/thumbnail.jpg");
        let scheme = crate::core::scheme::Scheme {
            name: "dynamic".into(),
            flavour: "mocha".into(),
            mode: "light".into(),
            variant: "neutral".into(),
            colours: [("primary".into(), "#abcdef".into())].into(),
        };

        let env = post_hook_env(wall, thumbnail, &scheme).unwrap();

        assert_eq!(env.get("WALLPAPER_PATH"), Some(&wall.display().to_string()));
        assert_eq!(
            env.get("THUMBNAIL_PATH"),
            Some(&thumbnail.display().to_string())
        );
        assert_eq!(env.get("SCHEME_NAME").map(String::as_str), Some("dynamic"));
        assert_eq!(env.get("SCHEME_FLAVOUR").map(String::as_str), Some("mocha"));
        assert_eq!(env.get("SCHEME_MODE").map(String::as_str), Some("light"));
        assert_eq!(
            env.get("SCHEME_VARIANT").map(String::as_str),
            Some("neutral")
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&env["SCHEME_COLOURS"]).unwrap(),
            serde_json::json!({ "primary": "#abcdef" })
        );
    }

    #[test]
    fn post_hook_ignores_nonzero_exit_and_supplements_process_environment() {
        let root = temp_dir("post-hook");
        let output = root.join("hook-output");
        let scheme = crate::core::scheme::Scheme {
            name: "dynamic".into(),
            flavour: "mocha".into(),
            mode: "dark".into(),
            variant: "content".into(),
            colours: Default::default(),
        };
        let hook = format!(
            "printf '%s|%s' \"$WALLPAPER_PATH\" \"$PATH\" > '{}'; exit 7",
            output.display()
        );

        execute_post_hook(
            &hook,
            Path::new("/walls/current.png"),
            Path::new("/cache/thumbnail.jpg"),
            &scheme,
        )
        .unwrap();

        let written = std::fs::read_to_string(&output).unwrap();
        assert!(written.starts_with("/walls/current.png|"));
        assert!(written.len() > "/walls/current.png|".len());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn thumbnail_uses_nearest_neighbor_resampling() {
        let root = temp_dir("nearest-thumbnail");
        let source = root.join("source.png");
        let actual = root.join("actual.jpg");
        let expected = root.join("expected.jpg");
        let source_image = image::RgbImage::from_fn(257, 129, |x, y| {
            image::Rgb([
                (x.wrapping_mul(37) % 256) as u8,
                (y.wrapping_mul(73) % 256) as u8,
                (x.wrapping_mul(11).wrapping_add(y.wrapping_mul(19)) % 256) as u8,
            ])
        });
        source_image.save(&source).unwrap();
        image::DynamicImage::ImageRgb8(source_image)
            .resize(128, 128, image::imageops::FilterType::Nearest)
            .save_with_format(&expected, image::ImageFormat::Jpeg)
            .unwrap();

        generate_thumbnail(&source, &actual).unwrap();

        assert_eq!(
            std::fs::read(actual).unwrap(),
            std::fs::read(expected).unwrap()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn symlink_replacement_replaces_existing_regular_file() {
        let root = temp_dir("replace-regular-file");
        let target = root.join("target");
        let link = root.join("wallpaper");
        std::fs::write(&target, "target").unwrap();
        std::fs::write(&link, "existing").unwrap();

        replace_symlink(&target, &link).unwrap();

        assert_eq!(std::fs::read_link(&link).unwrap(), target);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_symlink_replacement_preserves_existing_regular_file() {
        let root = temp_dir("atomic-symlink");
        let link = root.join("wallpaper");
        std::fs::write(&link, "existing").unwrap();
        let invalid_target = PathBuf::from("x".repeat(5000));

        assert!(replace_symlink(&invalid_target, &link).is_err());
        assert_eq!(std::fs::read_to_string(&link).unwrap(), "existing");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wallpaper_extensions_match_python_reference() {
        assert!(is_valid_image(Path::new("a.webp")));
        assert!(is_valid_video(Path::new("a.mkv")));
        assert!(!is_valid_wallpaper(Path::new("a.txt")));
    }

    #[test]
    fn cache_and_thumbnail_paths_use_source_hash() {
        let source = Path::new("/tmp/a.png");
        let cache_root = Path::new("/cache/wallpapers");
        let cache = cache_path(source, cache_root);
        assert_eq!(
            cache,
            cache_root.join(crate::util::paths::compute_hash(source))
        );
        assert_eq!(
            thumbnail_path(source, cache_root),
            cache.join("thumbnail.jpg")
        );
    }

    #[test]
    fn ffmpeg_command_matches_python_reference() {
        let command = ffmpeg_command(
            Path::new("/walls/a.mkv"),
            Path::new("/cache/first_frame.png"),
        );
        assert_eq!(command.get_program(), "ffmpeg");
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            [
                "-y",
                "-loglevel",
                "error",
                "-i",
                "/walls/a.mkv",
                "-vframes",
                "1",
                "-vf",
                "scale=512:-1",
                "/cache/first_frame.png",
            ]
        );
    }

    #[test]
    fn random_candidates_filter_small_images_but_keep_videos() {
        let root = temp_dir("filter");
        let large = root.join("large.png");
        let small = root.join("small.png");
        let video = root.join("clip.mkv");
        save_image(&large, 100, 80);
        save_image(&small, 20, 20);
        std::fs::write(&video, []).unwrap();

        let candidates = random_candidates(&root, None, false, Some((100, 80)), 0.8).unwrap();
        assert!(candidates.contains(&large));
        assert!(!candidates.contains(&small));
        assert!(candidates.contains(&video));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn random_candidates_exclude_current_when_an_alternative_exists() {
        let root = temp_dir("current");
        let current = root.join("current.png");
        let other = root.join("other.png");
        save_image(&current, 2, 2);
        save_image(&other, 2, 2);

        let candidates = random_candidates(&root, Some(&current), true, None, 0.9).unwrap();
        assert_eq!(candidates, vec![other]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn set_wallpaper_writes_state_and_symlinks() {
        let root = temp_dir("state");
        let mut env = crate::test_support::EnvGuard::new();
        env.set("XDG_STATE_HOME", root.join("state"));
        env.set("XDG_CACHE_HOME", root.join("cache"));
        env.set("XDG_DATA_HOME", root.join("data"));
        let source = root.join("wall.png");
        save_image(&source, 160, 90);

        set_wallpaper(&source, true).unwrap();
        let canonical = source.canonicalize().unwrap();
        assert_eq!(
            std::fs::read_to_string(crate::util::paths::wallpaper_path_path()).unwrap(),
            canonical.to_string_lossy()
        );
        assert_eq!(
            std::fs::read_link(crate::util::paths::wallpaper_link_path()).unwrap(),
            canonical
        );
        let thumbnail = std::fs::read_link(crate::util::paths::wallpaper_thumbnail_path()).unwrap();
        assert!(thumbnail.exists());
        assert_eq!(image::image_dimensions(thumbnail).unwrap(), (128, 72));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn print_payload_is_json_and_does_not_mutate_state() {
        let root = temp_dir("print");
        let mut env = crate::test_support::EnvGuard::new();
        env.set("XDG_STATE_HOME", root.join("state"));
        env.set("XDG_CACHE_HOME", root.join("cache"));
        env.set("XDG_DATA_HOME", root.join("data"));
        let source = root.join("wall.png");
        save_image(&source, 4, 4);

        let payload = wallpaper_json(&source, true).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&payload).unwrap();
        for key in ["name", "flavour", "mode", "variant", "colours"] {
            assert!(parsed.get(key).is_some(), "missing {key}");
        }
        assert!(!crate::util::paths::wallpaper_path_path().exists());
        assert!(!crate::util::paths::scheme_path().exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_file_and_empty_random_set_error() {
        let root = temp_dir("errors");
        assert!(set_wallpaper(&root.join("missing.png"), true).is_err());
        assert!(set_random(&root, true, 0.9, true).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
