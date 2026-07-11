use anyhow::Result;
use clap::Subcommand as _Subcommand;
use serde::Serialize;
use std::collections::BTreeMap;

use crate::core::scheme::{Scheme, SCHEME_VARIANTS};

/// `caelestia scheme list/get/set/preview` — native implementation that reads
/// from the shipped `schemes/` data tree and mirrors the Python reference. The
/// `set` subcommand persists via the atomic JSON helper and writes the current
/// `scheme.json` snapshot under `~/.local/state/caelestia/`. The `preview`
/// subcommand emits a single JSON object to stdout without touching any state.
pub fn run(args: SchemeActionArgs) -> Result<()> {
    match args.action {
        SchemeAction::List(opts) => list(opts),
        SchemeAction::Get(opts) => get(opts),
        SchemeAction::Set(opts) => set(opts),
        SchemeAction::Preview(opts) => preview(opts),
    }
}

#[derive(Debug, clap::Subcommand, Clone)]
pub enum SchemeAction {
    List(ListOptions),
    Get(GetOptions),
    Set(SetOptions),
    Preview(PreviewOptions),
}

#[derive(Debug, Clone, clap::Args)]
pub struct ListOptions {
    #[arg(short = 'n', long)]
    pub names: bool,
    #[arg(short = 'f', long)]
    pub flavours: bool,
    #[arg(short = 'm', long)]
    pub modes: bool,
    #[arg(short = 'v', long)]
    pub variants: bool,
}

#[derive(Debug, Clone, clap::Args)]
pub struct GetOptions {
    #[arg(short = 'n', long)]
    pub name: bool,
    #[arg(short = 'f', long)]
    pub flavour: bool,
    #[arg(short = 'm', long)]
    pub mode: bool,
    #[arg(short = 'v', long)]
    pub variant: bool,
}

#[derive(Debug, Clone, clap::Args)]
pub struct SetOptions {
    #[arg(long)]
    pub notify: bool,
    #[arg(long)]
    pub random: bool,
    #[arg(short = 'n', long)]
    pub name: Option<String>,
    #[arg(short = 'f', long)]
    pub flavour: Option<String>,
    #[arg(short = 'm', long)]
    pub mode: Option<String>,
    #[arg(short = 'v', long)]
    pub variant: Option<String>,
}

#[derive(Debug, Clone, clap::Args)]
pub struct PreviewOptions {
    /// material variant to generate the preview for (e.g. tonalspot, monochrome)
    #[arg(long, value_name = "VARIANT")]
    pub variant: String,
    /// override the scheme name; defaults to the current `scheme.json` name
    #[arg(long, value_name = "NAME")]
    pub name: Option<String>,
    /// override the scheme flavour
    #[arg(long, value_name = "FLAVOUR")]
    pub flavour: Option<String>,
    /// override the scheme mode (dark|light)
    #[arg(long, value_name = "MODE")]
    pub mode: Option<String>,
}

#[derive(Debug, Clone, clap::Args)]
pub struct SchemeActionArgs {
    #[command(subcommand)]
    pub action: SchemeAction,
}

fn list(opts: ListOptions) -> Result<()> {
    let selected = [opts.names, opts.flavours, opts.modes, opts.variants]
        .iter()
        .filter(|v| **v)
        .count();
    let multi = selected > 1;

    if opts.names {
        let names = Scheme::names();
        if multi {
            print!("Names: ");
        }
        for name in &names {
            print!("{name} ");
        }
        println!();
    }
    if opts.flavours {
        let name = Scheme::load()
            .map(|s| s.name)
            .unwrap_or_else(|_| "catppuccin".into());
        let flavours = Scheme::flavours(&name);
        if multi {
            print!("Flavours: ");
        }
        for flavour in &flavours {
            print!("{flavour} ");
        }
        println!();
    }
    if opts.modes {
        let scheme = Scheme::load()?;
        let modes = Scheme::modes(&scheme.name, &scheme.flavour);
        if multi {
            print!("Modes: ");
        }
        for mode in &modes {
            print!("{mode} ");
        }
        println!();
    }
    if opts.variants {
        if multi {
            print!("Variants: ");
        }
        for v in SCHEME_VARIANTS {
            print!("{v} ");
        }
        println!();
    }
    if !opts.names && !opts.flavours && !opts.modes && !opts.variants {
        let scheme = Scheme::load()?;
        #[derive(Serialize)]
        struct Entry {
            colours: BTreeMap<String, String>,
        }
        let mut out = BTreeMap::new();
        for name in Scheme::names() {
            let mut flavours = BTreeMap::new();
            for flavour in Scheme::flavours(&name) {
                for mode in Scheme::modes(&name, &flavour) {
                    let scheme_path =
                        crate::core::scheme::Scheme::colours_path_for(&name, &flavour, &mode);
                    let colours = crate::core::scheme::read_colours_from_file(&scheme_path);
                    if !colours.is_empty() {
                        flavours.insert(format!("{flavour}/{mode}"), colours);
                    }
                }
            }
            if !flavours.is_empty() {
                out.insert(name, flavours);
            }
        }
        let _ = Entry {
            colours: BTreeMap::new(),
        };
        let _ = scheme;
        println!("{}", serde_json::to_string_pretty(&out)?);
    }
    Ok(())
}

fn get(opts: GetOptions) -> Result<()> {
    let scheme = Scheme::load()?;
    if !opts.name && !opts.flavour && !opts.mode && !opts.variant {
        println!("{scheme}");
        return Ok(());
    }
    if opts.name {
        println!("{}", scheme.name);
    }
    if opts.flavour {
        println!("{}", scheme.flavour);
    }
    if opts.mode {
        println!("{}", scheme.mode);
    }
    if opts.variant {
        println!("{}", scheme.variant);
    }
    Ok(())
}

fn set(opts: SetOptions) -> Result<()> {
    let mut scheme = Scheme::load()?;
    if opts.random {
        let names = Scheme::names();
        let name = pick(&names, &scheme.name);
        let flavours = Scheme::flavours(&name);
        let flavour = pick(&flavours, &scheme.flavour);
        let modes = Scheme::modes(&name, &flavour);
        let mode = pick(&modes, &scheme.mode);
        scheme.name = name;
        scheme.flavour = flavour;
        scheme.mode = mode;
        scheme.colours = crate::core::scheme::read_colours_from_file(&scheme.colours_path());
        scheme.save()?;
        println!("switched scheme to {scheme}");
        return Ok(());
    }
    if let Some(name) = opts.name.clone() {
        scheme.name = name;
    }
    if let Some(flavour) = opts.flavour.clone() {
        scheme.flavour = flavour;
    }
    if let Some(mode) = opts.mode.clone() {
        scheme.mode = mode;
    }
    if let Some(variant) = opts.variant.clone() {
        scheme.variant = variant;
    }
    scheme.colours = crate::core::scheme::read_colours_from_file(&scheme.colours_path());
    scheme.save()?;
    if opts.notify {
        let _ = crate::util::notify::notify(&[
            "Scheme updated",
            &format!(
                "{}/{}/{} ({})",
                scheme.name, scheme.flavour, scheme.mode, scheme.variant
            ),
        ]);
    }
    println!("{scheme}");
    Ok(())
}

fn pick(options: &[String], current: &str) -> String {
    if options.is_empty() {
        return current.to_owned();
    }
    let next =
        (current.as_bytes().first().copied().unwrap_or_default() as usize + 1) % options.len();
    options[next].clone()
}

/// Emits a single JSON object describing the colour palette that the M3
/// generator would produce for the requested variant. Crucially, it never
/// writes to disk, never spawns theme hooks, and never notifies the user; the
/// shell drives any side effects downstream.
fn preview(opts: PreviewOptions) -> Result<()> {
    let current = Scheme::load()?;
    let name = opts.name.unwrap_or(current.name);
    let flavour = opts.flavour.unwrap_or(current.flavour);
    let mode = opts.mode.unwrap_or(current.mode);
    if !SCHEME_VARIANTS.contains(&opts.variant.as_str()) {
        anyhow::bail!(
            "unknown variant `{}`; expected one of {}",
            opts.variant,
            SCHEME_VARIANTS.join(", ")
        );
    }
    let path = crate::core::scheme::Scheme::colours_path_for(&name, &flavour, &mode);
    let stored = crate::core::scheme::read_colours_from_file(&path);
    if stored.is_empty() {
        anyhow::bail!(
            "no stored palette for {name}/{flavour}/{mode}; run `caelestia scheme set -m {mode}` first"
        );
    }
    let payload = serde_json::json!({
        "name": name,
        "flavour": flavour,
        "mode": mode,
        "variant": opts.variant,
        "colours": stored,
    });
    println!("{}", serde_json::to_string(&payload)?);
    Ok(())
}
