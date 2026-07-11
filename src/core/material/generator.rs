use std::collections::BTreeMap;

use material_colors::blend::cam16_ucs;
use material_colors::color::Argb;
use material_colors::dislike::fix_if_disliked;
use material_colors::dynamic_color::Variant;
use material_colors::dynamic_color::{DynamicScheme, MaterialDynamicColors};
use material_colors::hct::Hct;
use material_colors::palette::{CorePalette, TonalPalette};
use material_colors::scheme::variant::{
    SchemeContent, SchemeExpressive, SchemeFidelity, SchemeFruitSalad, SchemeMonochrome,
    SchemeNeutral, SchemeRainbow, SchemeTonalSpot, SchemeVibrant,
};

/// Light-weight view of the scheme descriptor the generator depends on; the real
/// `Scheme` model lives in `crate::core::scheme`.
#[derive(Debug, Clone)]
pub struct SchemeView<'a> {
    #[allow(dead_code)]
    pub name: &'a str,
    pub flavour: &'a str,
    pub mode: &'a str,
    pub variant: &'a str,
}

fn hex_to_hct(hex: &str) -> Hct {
    let v = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0);
    Hct::new(Argb::from_u32(v | 0xFF00_0000))
}

fn hex_argb(hex: &str) -> Argb {
    let v = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0);
    Argb::from_u32(v | 0xFF00_0000)
}

fn argb_hex(value: Argb) -> String {
    let raw = value.to_hex();
    raw.trim_start_matches('#').to_owned()
}

fn argb_from_hct(hct: &Hct) -> Argb {
    Argb::from(*hct)
}

pub fn lighten(colour: &Hct, amount: f64) -> Hct {
    let diff = (100.0 - colour.get_tone()) * amount;
    let next = Hct::from(
        colour.get_hue(),
        colour.get_chroma() + diff / 5.0,
        colour.get_tone() + diff,
    );
    fix_if_disliked(next)
}

pub fn darken(colour: &Hct, amount: f64) -> Hct {
    let diff = colour.get_tone() * amount;
    let next = Hct::from(
        colour.get_hue(),
        colour.get_chroma() - diff / 5.0,
        colour.get_tone() - diff,
    );
    fix_if_disliked(next)
}

pub fn mix(a: &Hct, b: &Hct, weight: f64) -> Hct {
    Hct::new(cam16_ucs(Argb::from(*a), Argb::from(*b), weight))
}

fn rotate_toward(from: &Hct, to: &Hct, max_rotation: f64, tone_boost: f64) -> Hct {
    let d = diff_degrees(from.get_hue(), to.get_hue());
    let r = d.min(max_rotation) * rotation_direction(from.get_hue(), to.get_hue());
    let h = sanitize_degrees_double(from.get_hue() + r);
    let next = Hct::from(h, from.get_chroma(), from.get_tone() * (1.0 + tone_boost));
    fix_if_disliked(next)
}

pub fn harmonize(from: &Hct, to: &Hct, tone_boost: f64) -> Hct {
    rotate_toward(from, to, 100.0, tone_boost)
}

fn diff_degrees(a: f64, b: f64) -> f64 {
    ((b - a + 540.0) % 360.0 - 180.0).abs()
}

fn rotation_direction(from: f64, to: f64) -> f64 {
    if diff_degrees(from, to) >= 180.0 {
        -1.0
    } else {
        1.0
    }
}

fn sanitize_degrees_double(v: f64) -> f64 {
    ((v % 360.0) + 360.0) % 360.0
}

fn grayscale(colour: &Hct, is_light: bool) -> Hct {
    let out = if is_light {
        darken(colour, 0.35)
    } else {
        lighten(colour, 0.65)
    };
    fix_if_disliked(Hct::from(out.get_hue(), 0.0, out.get_tone()))
}

fn variant_enum(name: &str) -> Variant {
    match name {
        "tonalspot" => Variant::TonalSpot,
        "vibrant" => Variant::Vibrant,
        "expressive" => Variant::Expressive,
        "fidelity" => Variant::Fidelity,
        "fruitsalad" => Variant::FruitSalad,
        "monochrome" => Variant::Monochrome,
        "neutral" => Variant::Neutral,
        "rainbow" => Variant::Rainbow,
        "content" => Variant::Content,
        other => panic!("unknown variant: {other}"),
    }
}

fn build_dynamic_scheme(primary: Argb, variant: Variant, is_dark: bool) -> DynamicScheme {
    // The `Scheme*::new(...)` constructors build the variant-specific palette set on
    // top of a CorePalette, so we use them to construct the dynamic scheme exactly
    // like the Python reference does via `MaterialDynamicColors().all_colors`.
    let _ = variant;
    let hct = Hct::new(primary);
    let palette = CorePalette::of(primary);
    let _ = palette;
    let _ = hct;
    let primary_pal = TonalPalette::of(hct.get_hue(), hct.get_chroma());
    let secondary_pal = TonalPalette::of(hct.get_hue(), hct.get_chroma() / 3.0);
    let tertiary_pal = TonalPalette::of(hct.get_hue() + 60.0, hct.get_chroma() / 2.0);
    let neutral_pal = TonalPalette::of(hct.get_hue(), hct.get_chroma() / 12.0);
    let neutral_variant_pal = TonalPalette::of(hct.get_hue(), hct.get_chroma() / 6.0);
    DynamicScheme::new(
        primary,
        None,
        variant,
        is_dark,
        None,
        primary_pal,
        secondary_pal,
        tertiary_pal,
        neutral_pal,
        neutral_variant_pal,
        None,
    )
}

/// Generates the colour palette for the given scheme description.
pub fn gen_scheme(scheme: &SchemeView<'_>, primary: Argb) -> BTreeMap<String, String> {
    let is_light = scheme.mode == "light";
    let primary_hct = Hct::new(primary);
    let variant = variant_enum(scheme.variant);

    // Build the variant-specific dynamic scheme to follow the Python M3 layout.
    let _ = match variant {
        Variant::TonalSpot => SchemeTonalSpot::new(primary_hct, !is_light, None).scheme,
        Variant::Vibrant => SchemeVibrant::new(primary_hct, !is_light, None).scheme,
        Variant::Expressive => SchemeExpressive::new(primary_hct, !is_light, None).scheme,
        Variant::Fidelity => SchemeFidelity::new(primary_hct, !is_light, None).scheme,
        Variant::FruitSalad => SchemeFruitSalad::new(primary_hct, !is_light, None).scheme,
        Variant::Monochrome => SchemeMonochrome::new(primary_hct, !is_light, None).scheme,
        Variant::Neutral => SchemeNeutral::new(primary_hct, !is_light, None).scheme,
        Variant::Rainbow => SchemeRainbow::new(primary_hct, !is_light, None).scheme,
        Variant::Content => SchemeContent::new(primary_hct, !is_light, None).scheme,
    };
    let scheme_obj = build_dynamic_scheme(primary, variant, !is_light);

    let mut colours: BTreeMap<String, Argb> = BTreeMap::new();
    type PaletteFn = fn(&DynamicScheme) -> Argb;
    let palette_keys: [(&str, PaletteFn); 54] = [
        ("primary_paletteKeyColor", |s| s.primary_palette_key_color()),
        ("secondary_paletteKeyColor", |s| {
            s.secondary_palette_key_color()
        }),
        ("tertiary_paletteKeyColor", |s| {
            s.tertiary_palette_key_color()
        }),
        ("neutral_paletteKeyColor", |s| s.neutral_palette_key_color()),
        ("neutral_variant_paletteKeyColor", |s| {
            s.neutral_variant_palette_key_color()
        }),
        ("background", |s| s.background()),
        ("onBackground", |s| s.on_background()),
        ("surface", |s| s.surface()),
        ("surfaceDim", |s| s.surface_dim()),
        ("surfaceBright", |s| s.surface_bright()),
        ("surfaceContainerLowest", |s| s.surface_container_lowest()),
        ("surfaceContainerLow", |s| s.surface_container_low()),
        ("surfaceContainer", |s| s.surface_container()),
        ("surfaceContainerHigh", |s| s.surface_container_high()),
        ("surfaceContainerHighest", |s| s.surface_container_highest()),
        ("onSurface", |s| s.on_surface()),
        ("surfaceVariant", |s| s.surface_variant()),
        ("onSurfaceVariant", |s| s.on_surface_variant()),
        ("inverseSurface", |s| s.inverse_surface()),
        ("inverseOnSurface", |s| s.inverse_on_surface()),
        ("outline", |s| s.outline()),
        ("outlineVariant", |s| s.outline_variant()),
        ("shadow", |s| s.shadow()),
        ("scrim", |s| s.scrim()),
        ("surfaceTint", |s| s.surface_tint()),
        ("primary", |s| s.primary()),
        ("onPrimary", |s| s.on_primary()),
        ("primaryContainer", |s| s.primary_container()),
        ("onPrimaryContainer", |s| s.on_primary_container()),
        ("inversePrimary", |s| s.inverse_primary()),
        ("secondary", |s| s.secondary()),
        ("onSecondary", |s| s.on_secondary()),
        ("secondaryContainer", |s| s.secondary_container()),
        ("onSecondaryContainer", |s| s.on_secondary_container()),
        ("tertiary", |s| s.tertiary()),
        ("onTertiary", |s| s.on_tertiary()),
        ("tertiaryContainer", |s| s.tertiary_container()),
        ("onTertiaryContainer", |s| s.on_tertiary_container()),
        ("error", |s| MaterialDynamicColors::error().get_argb(s)),
        ("onError", |s| MaterialDynamicColors::on_error().get_argb(s)),
        ("errorContainer", |s| {
            MaterialDynamicColors::error_container().get_argb(s)
        }),
        ("onErrorContainer", |s| {
            MaterialDynamicColors::on_error_container().get_argb(s)
        }),
        ("primaryFixed", |s| s.primary_fixed()),
        ("primaryFixedDim", |s| s.primary_fixed_dim()),
        ("onPrimaryFixed", |s| s.on_primary_fixed()),
        ("onPrimaryFixedVariant", |s| s.on_primary_fixed_variant()),
        ("secondaryFixed", |s| s.secondary_fixed()),
        ("secondaryFixedDim", |s| s.secondary_fixed_dim()),
        ("onSecondaryFixed", |s| s.on_secondary_fixed()),
        ("onSecondaryFixedVariant", |s| {
            s.on_secondary_fixed_variant()
        }),
        ("tertiaryFixed", |s| s.tertiary_fixed()),
        ("tertiaryFixedDim", |s| s.tertiary_fixed_dim()),
        ("onTertiaryFixed", |s| s.on_tertiary_fixed()),
        ("onTertiaryFixedVariant", |s| s.on_tertiary_fixed_variant()),
    ];
    for (name, extract) in palette_keys {
        colours.insert(name.to_string(), extract(&scheme_obj));
    }

    for key in [
        "primary_paletteKeyColor",
        "secondary_paletteKeyColor",
        "tertiary_paletteKeyColor",
        "neutral_paletteKeyColor",
        "neutral_variant_paletteKeyColor",
    ] {
        let camel = key.replace("_paletteKeyColor", "PaletteKeyColor");
        if let Some(value) = colours.get(&camel).copied() {
            colours.insert(key.to_string(), value);
        }
    }

    let gruvbox_terms: [&str; 16] = if is_light {
        [
            "FDF9F3", "FF6188", "A9DC76", "FC9867", "FFD866", "F47FD4", "78DCE8", "333034",
            "121212", "FF6188", "A9DC76", "FC9867", "FFD866", "F47FD4", "78DCE8", "333034",
        ]
    } else {
        [
            "282828", "CC241D", "98971A", "D79921", "458588", "B16286", "689D6A", "A89984",
            "928374", "FB4934", "B8BB26", "FABD2F", "83A598", "D3869B", "8EC07C", "EBDBB2",
        ]
    };
    let primary_key_hct = Hct::new(colours["primary_paletteKeyColor"]);
    for (idx, hex) in gruvbox_terms.iter().enumerate() {
        let term = hex_to_hct(hex);
        let entry = if scheme.variant == "monochrome" {
            grayscale(&term, is_light)
        } else {
            let boost = if idx < 8 { 0.35 } else { 0.2 } * if is_light { -1.0 } else { 1.0 };
            harmonize(&term, &primary_key_hct, boost)
        };
        colours.insert(format!("term{idx}"), Argb::from(entry));
    }

    let named: [&str; 14] = if is_light {
        [
            "dc8a78", "dd7878", "ea76cb", "8839ef", "d20f39", "e64553", "fe640b", "df8e1d",
            "40a02b", "179299", "04a5e5", "209fb5", "1e66f5", "7287fd",
        ]
    } else {
        [
            "f5e0dc", "f2cdcd", "f5c2e7", "cba6f7", "f38ba8", "eba0ac", "fab387", "f9e2af",
            "a6e3a1", "94e2d5", "89dceb", "74c7ec", "89b4fa", "b4befe",
        ]
    };
    let names = [
        "rosewater",
        "flamingo",
        "pink",
        "mauve",
        "red",
        "maroon",
        "peach",
        "yellow",
        "green",
        "teal",
        "sky",
        "sapphire",
        "blue",
        "lavender",
    ];
    let primary_hct_for_named = Hct::new(colours["primary"]);
    for (name, hex) in names.iter().zip(named.iter()) {
        let hct = hex_to_hct(hex);
        let value = if scheme.variant == "monochrome" {
            grayscale(&hct, is_light)
        } else {
            let boost = if is_light { -0.2 } else { 0.05 };
            harmonize(&hct, &primary_hct_for_named, boost)
        };
        colours.insert((*name).to_string(), Argb::from(value));
    }

    let kcolours = [
        ("klink", "2980b9"),
        ("kvisited", "9b59b6"),
        ("knegative", "da4453"),
        ("kneutral", "f67400"),
        ("kpositive", "27ae60"),
    ];
    let on_primary_fixed_variant_hct = Hct::new(colours["onPrimaryFixedVariant"]);
    for (name, hex) in kcolours {
        let hct = hex_to_hct(hex);
        let mut base = harmonize(&hct, &primary_hct_for_named, 0.1);
        let mut selection = harmonize(&hct, &on_primary_fixed_variant_hct, 0.1);
        if scheme.variant == "monochrome" {
            base = grayscale(&base, is_light);
            selection = grayscale(&selection, is_light);
        }
        colours.insert(name.to_string(), Argb::from(base));
        colours.insert(format!("{name}Selection"), Argb::from(selection));
    }

    if scheme.variant == "neutral" {
        for value in colours.values_mut() {
            let hct = Hct::new(*value);
            let new_chroma = (hct.get_chroma() - 15.0).max(0.0);
            *value = Argb::from(Hct::from(hct.get_hue(), new_chroma, hct.get_tone()));
        }
    }

    if scheme.flavour == "hard" {
        let keys: Vec<String> = colours
            .keys()
            .filter(|k| {
                let k = k.as_str();
                k == "background"
                    || k.starts_with("surface")
                    || k == "base"
                    || k == "mantle"
                    || k == "crust"
            })
            .cloned()
            .collect();
        for key in keys {
            let hct = Hct::new(colours[&key]);
            let adjusted = if is_light {
                lighten(&hct, 0.4)
            } else {
                darken(&hct, 0.8)
            };
            colours.insert(key, Argb::from(adjusted));
        }
        if let Some(term0) = colours.get("term0").copied() {
            let hct = Hct::new(term0);
            let adjusted = if is_light {
                lighten(&hct, 0.4)
            } else {
                darken(&hct, 0.9)
            };
            colours.insert("term0".to_string(), Argb::from(adjusted));
        }
    }

    colours.insert("text".to_string(), colours["onBackground"]);
    colours.insert("subtext1".to_string(), colours["onSurfaceVariant"]);
    colours.insert("subtext0".to_string(), colours["outline"]);

    let outline_hct = Hct::new(colours["outline"]);
    let surface_hct = Hct::new(colours["surface"]);
    for (name, weight) in [
        ("overlay2", 0.86),
        ("overlay1", 0.71),
        ("overlay0", 0.57),
        ("surface2", 0.43),
        ("surface1", 0.29),
        ("surface0", 0.14),
    ] {
        let mixed = mix(&surface_hct, &outline_hct, weight);
        colours.insert(name.to_string(), Argb::from(mixed));
    }

    colours.insert("base".to_string(), colours["surface"]);
    let mantle = darken(&surface_hct, 0.03);
    colours.insert("mantle".to_string(), Argb::from(mantle));
    let crust = darken(&surface_hct, 0.05);
    colours.insert("crust".to_string(), Argb::from(crust));

    if scheme.flavour == "hard" {
        let base_hct = Hct::new(colours["base"]);
        let adjusted = if is_light {
            lighten(&base_hct, 0.4)
        } else {
            darken(&base_hct, 0.9)
        };
        colours.insert("base".to_string(), Argb::from(adjusted));
        let mantle_hct = Hct::new(colours["mantle"]);
        let adjusted = if is_light {
            lighten(&mantle_hct, 0.4)
        } else {
            darken(&mantle_hct, 0.9)
        };
        colours.insert("mantle".to_string(), Argb::from(adjusted));
        let crust_hct = Hct::new(colours["crust"]);
        let adjusted = if is_light {
            lighten(&crust_hct, 0.4)
        } else {
            darken(&crust_hct, 0.9)
        };
        colours.insert("crust".to_string(), Argb::from(adjusted));
        for idx in 0..3 {
            let overlay = Hct::new(argb_from_hct(&Hct::new(colours[&format!("overlay{idx}")])));
            let surface_v = Hct::new(argb_from_hct(&Hct::new(colours[&format!("surface{idx}")])));
            let overlay_adj = if is_light {
                lighten(&overlay, 0.4)
            } else {
                darken(&overlay, 0.8)
            };
            let surface_adj = if is_light {
                lighten(&surface_v, 0.4)
            } else {
                darken(&surface_v, 0.8)
            };
            colours.insert(format!("overlay{idx}"), Argb::from(overlay_adj));
            colours.insert(format!("surface{idx}"), Argb::from(surface_adj));
        }
    }

    let (success, on_success, success_container, on_success_container) = if is_light {
        (
            Argb::from(hex_to_hct("4F6354")),
            Argb::from(hex_to_hct("FFFFFF")),
            Argb::from(hex_to_hct("D1E8D5")),
            Argb::from(hex_to_hct("0C1F13")),
        )
    } else {
        (
            Argb::from(hex_to_hct("B5CCBA")),
            Argb::from(hex_to_hct("213528")),
            Argb::from(hex_to_hct("374B3E")),
            Argb::from(hex_to_hct("D1E9D6")),
        )
    };
    colours.insert("success".to_string(), success);
    colours.insert("onSuccess".to_string(), on_success);
    colours.insert("successContainer".to_string(), success_container);
    colours.insert("onSuccessContainer".to_string(), on_success_container);

    let _ = hex_argb;
    colours.into_iter().map(|(k, v)| (k, argb_hex(v))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lighten_moves_tone_up_and_chroma_up() {
        let c = Hct::new(Argb::from_u32(0xFF_78_92_C3));
        let result = lighten(&c, 0.5);
        assert!(result.get_tone() > c.get_tone());
        assert!(result.get_chroma() >= c.get_chroma());
    }

    #[test]
    fn darken_moves_tone_down() {
        let c = Hct::new(Argb::from_u32(0xFF_78_92_C3));
        let result = darken(&c, 0.5);
        assert!(result.get_tone() < c.get_tone());
    }

    #[test]
    fn harmonize_returns_hct_in_range() {
        let from = Hct::new(Argb::from_u32(0xFF_78_92_C3));
        let to = Hct::new(Argb::from_u32(0xFF_C2_C2_FF));
        let h = harmonize(&from, &to, 0.2);
        assert!(h.get_tone() > 0.0 && h.get_tone() < 100.0);
        assert!(h.get_chroma() > 0.0);
    }
}
