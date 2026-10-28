//! Filesystem adapter for the shared Grimoire palette and existing theme.toml.
#[cfg(test)]
use ratatui::style::Color;
use std::path::PathBuf;
pub use wros_shell::grimoire_theme::*;

pub fn config_path() -> PathBuf {
    crate::home()
        .join(".config")
        .join("grimoire")
        .join("theme.toml")
}

pub fn load() -> Theme {
    load_from(&config_path())
}

pub fn load_from(path: &std::path::Path) -> Theme {
    std::fs::read_to_string(path).map_or_else(|_| default_theme(), |s| from_config(&s))
}

pub fn save(t: &Theme) -> std::io::Result<()> {
    save_to(&config_path(), t)
}

pub fn save_to(path: &std::path::Path, t: &Theme) -> std::io::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    grimoire_core::atomic::write_io(path, to_config(t).as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("grimoire-theme-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d.join("theme.toml")
    }

    #[test]
    fn a_custom_themes_picks_survive_saving() {
        let path = scratch("picks");
        let mut t = default_theme();
        t.name = "Custom".into();
        t.pick_world("Kanagawa".into());
        t.pick_look("Synthwave '84".into());
        save_to(&path, &t).unwrap();
        let back = load_from(&path);
        assert_eq!(back.world(), "Kanagawa");
        assert_eq!(back.look(), "Synthwave '84");
        assert_eq!(back, t);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn an_older_custom_theme_loads_as_it_always_did() {
        let path = scratch("old");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut file = String::from("name = \"Custom\"\n");
        for (i, role) in ROLES.iter().enumerate() {
            file.push_str(&format!(
                "{role} = \"{}\"\n",
                hex_of(default_theme().role(i))
            ));
        }
        std::fs::write(&path, &file).unwrap();
        let t = load_from(&path);
        assert_eq!(
            (t.pomodoro.as_deref(), t.visualizer.as_deref()),
            (None, None)
        );
        assert_eq!((t.world(), t.look()), (CUSTOM_WORLD, ORIGINAL_LOOK));
        // A name no preset has any more is no pick at all.
        std::fs::write(
            &path,
            file + "pomodoro = \"Atlantis\"\nvisualizer = \"Nope\"\n",
        )
        .unwrap();
        let t = load_from(&path);
        assert_eq!((t.world(), t.look()), (CUSTOM_WORLD, ORIGINAL_LOOK));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn presets_draw_their_own_world_and_look() {
        for p in presets() {
            assert_eq!(
                (p.pomodoro.as_deref(), p.visualizer.as_deref()),
                (None, None)
            );
            assert_eq!(p.world(), p.name);
            assert_eq!(p.look(), p.name);
        }
        // Picking the default stores no pick; every preset can be borrowed.
        let mut t = default_theme();
        t.name = "Custom".into();
        t.pick_world(CUSTOM_WORLD.into());
        t.pick_look(ORIGINAL_LOOK.into());
        assert_eq!(
            (t.pomodoro.as_deref(), t.visualizer.as_deref()),
            (None, None)
        );
        assert_eq!(pomodoro_choices().len(), presets().len());
        assert_eq!(visualizer_choices().len(), presets().len() + 1);
        let c = visualizer_choices();
        assert_eq!(
            step_choice(&c, ORIGINAL_LOOK, false),
            "Rainbow",
            "round the list"
        );
    }

    #[test]
    fn every_preset_is_named_and_distinct() {
        let p = presets();
        assert_eq!(p.len(), 19);
        assert!(p.iter().all(|t| !t.name.is_empty()));
        for i in 0..p.len() {
            for j in i + 1..p.len() {
                assert_ne!(p[i].name, p[j].name);
                assert_ne!(p[i].accent, p[j].accent, "{} vs {}", p[i].name, p[j].name);
            }
        }
    }

    /// Lost Forest has to stay green-led with cyan only as a rare contrast —
    /// cheap guard against the palette drifting back to cyan-dominant.
    #[test]
    fn lost_forest_is_green_led_with_cyan_only_as_contrast() {
        let t = presets()
            .into_iter()
            .find(|t| t.name == "Lost Forest")
            .expect("Lost Forest missing");

        let chan = |c: Color| match c {
            Color::Rgb(r, g, b) => (r as i32, g as i32, b as i32),
            _ => panic!("themes must be truecolor"),
        };

        // Accent is what the eye sees most, so it must read green: green
        // clearly ahead of red, and NOT cyan (blue must stay below green).
        let (r, g, b) = chan(t.accent);
        assert!(g > r + 30, "accent {:?} is not green enough", t.accent);
        assert!(g > b + 30, "accent {:?} has drifted to cyan", t.accent);

        // Every structural and woodland role is green-dominant.
        for (name, c) in [
            ("text", t.text),
            ("dim", t.dim),
            ("border", t.border),
            ("selection", t.sel),
            ("foliage", t.foliage),
            ("turf", t.turf),
        ] {
            let (r, g, b) = chan(c);
            assert!(g > r && g > b, "{name} {c:?} should be green-dominant");
        }

        // Foliage must sit darker than the accent or the tree pane blurs into it.
        let lum = |c| {
            let (r, g, b) = chan(c);
            r + g + b
        };
        assert!(
            lum(t.foliage) < lum(t.accent),
            "foliage should be darker than accent"
        );

        // Cyan survives, but only where it is rare: the moon and the flowers.
        for (name, c) in [("moon", t.moon), ("bloom", t.bloom)] {
            let (r, g, b) = chan(c);
            assert!(b > r + 30 && g > r + 30, "{name} {c:?} should read cyan");
        }

        // Warning has to stay warm or it vanishes into the trees.
        let (r, g, b) = chan(t.warn);
        assert!(r > g && r > b, "warning {:?} should be warm", t.warn);
    }

    /// The rainbow's roles really are a spectrum: every colourful role a
    /// different hue, spread right round the wheel.
    #[test]
    fn rainbow_roles_span_the_spectrum() {
        let t = presets()
            .into_iter()
            .find(|t| t.is_rainbow())
            .expect("Rainbow missing");
        let hue_of = |c: Color| {
            let Color::Rgb(r, g, b) = c else {
                panic!("truecolor")
            };
            let (r, g, b) = (r as f32, g as f32, b as f32);
            let (max, min) = (r.max(g).max(b), r.min(g).min(b));
            let d = max - min;
            let h = if max == r {
                60.0 * ((g - b) / d).rem_euclid(6.0)
            } else if max == g {
                60.0 * ((b - r) / d + 2.0)
            } else {
                60.0 * ((r - g) / d + 4.0)
            };
            (h / 30.0).round() as i32 % 12
        };
        let mut seen: Vec<i32> = [
            t.warn, t.bark, t.sun, t.foliage, t.moon, t.turf, t.accent, t.bloom,
        ]
        .into_iter()
        .map(hue_of)
        .collect();
        seen.sort();
        seen.dedup();
        assert!(seen.len() >= 7, "rainbow roles share hues: {seen:?}");
        assert!(presets().iter().filter(|t| t.is_rainbow()).count() == 1);
    }

    #[test]
    fn the_wheel_goes_round() {
        assert_eq!(hue(0.0), hue(360.0));
        let Color::Rgb(r, g, b) = hue(0.0) else {
            panic!()
        };
        assert!(r > g && r > b, "0° is red");
        let Color::Rgb(r, g, b) = hue(120.0) else {
            panic!()
        };
        assert!(g > r && g > b, "120° is green");
        let Color::Rgb(r, g, b) = hue(240.0) else {
            panic!()
        };
        assert!(b > r && b > g, "240° is blue");
    }

    #[test]
    fn hex_round_trips() {
        for t in presets() {
            for i in 0..ROLES.len() {
                let c = t.role(i);
                assert_eq!(parse_hex(&hex_of(c)), Some(c));
            }
        }
    }

    #[test]
    fn bad_hex_is_rejected() {
        for bad in ["", "#fff", "#gggggg", "12345", "#1234567", "nope"] {
            assert!(parse_hex(bad).is_none(), "{bad} should not parse");
        }
    }

    #[test]
    #[allow(clippy::needless_range_loop)]
    fn role_get_set_covers_every_index() {
        let mut t = default_theme();
        for i in 0..ROLES.len() {
            let c = parse_hex("#010203").unwrap();
            t.set_role(i, c);
            assert_eq!(t.role(i), c, "role {} ({}) did not round-trip", i, ROLES[i]);
        }
    }
}
