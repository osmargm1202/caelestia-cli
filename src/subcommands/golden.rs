use std::path::Path;

use anyhow::Result;

use crate::cli::GoldenArgs;
use crate::core::material::{gen_scheme, generator::SchemeView, score};

/// Internal subcommand that regenerates a colour scheme JSON from an image,
/// variant/flavour/mode tuple. It is exercised by the golden parity harness and
/// not part of the public CLI surface.
pub fn run(args: GoldenArgs) -> Result<()> {
    let image = Path::new(&args.image);
    let primary = score(image)?;
    let view = SchemeView {
        name: "x",
        flavour: &args.flavour,
        mode: &args.mode,
        variant: &args.variant,
    };
    let colours = gen_scheme(&view, primary);
    println!("{}", serde_json::to_string(&colours)?);
    Ok(())
}
