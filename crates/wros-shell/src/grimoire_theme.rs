//! Colour themes.
//!
//! Nineteen presets drawn from the palettes people already run in their
//! terminals — and one rainbow — plus a custom slot. A theme is a flat set of named roles — no
//! inheritance, no derivation — so the custom editor can just be a list of
//! swatches you type hex into.

use ratatui::style::Color;

/// Every colour the interface uses. Twelve roles, in the order the custom
/// editor lists them.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub name: String,
    pub accent: Color,
    pub text: Color,
    pub dim: Color,
    pub border: Color,
    pub sel: Color,
    pub warn: Color,
    pub sun: Color,
    pub moon: Color,
    pub foliage: Color,
    pub bark: Color,
    pub bloom: Color,
    pub turf: Color,
    /// The Pomodoro world to draw, by the name of the preset it belongs to;
    /// `None` is the theme's own ([`Theme::world`]). Only Custom sets it.
    pub pomodoro: Option<String>,
    /// The Visualizer look, the same way ([`Theme::look`]).
    pub visualizer: Option<String>,
}

/// What Custom draws when it hasn't chosen: the glade the house theme has,
/// and the Visualizer as it first was.
pub const CUSTOM_WORLD: &str = "Lost Forest";
pub const ORIGINAL_LOOK: &str = "Original";

/// The Pomodoro worlds a Custom theme can borrow: every preset's, by name.
pub fn pomodoro_choices() -> Vec<String> {
    presets().into_iter().map(|t| t.name).collect()
}

/// The Visualizer looks a Custom theme can borrow: the original, then every
/// preset's, by name.
pub fn visualizer_choices() -> Vec<String> {
    std::iter::once(ORIGINAL_LOOK.to_string())
        .chain(presets().into_iter().map(|t| t.name))
        .collect()
}

/// The choice after (or before) `current`, round the list.
pub fn step_choice(choices: &[String], current: &str, forward: bool) -> String {
    let n = choices.len().max(1);
    let i = choices.iter().position(|c| c == current).unwrap_or(0);
    let j = if forward {
        (i + 1) % n
    } else {
        (i + n - 1) % n
    };
    choices.get(j).cloned().unwrap_or_default()
}

/// Field order for the custom editor and for (de)serialisation.
pub const ROLES: [&str; 12] = [
    "accent",
    "text",
    "dim",
    "border",
    "selection",
    "warning",
    "sun",
    "moon",
    "foliage",
    "bark",
    "bloom",
    "turf",
];

impl Theme {
    /// Whose Pomodoro world this theme draws: its pick, else its own (a
    /// preset's name), and for Custom the glade.
    pub fn world(&self) -> &str {
        match &self.pomodoro {
            Some(p) => p,
            None if self.name == "Custom" => CUSTOM_WORLD,
            None => &self.name,
        }
    }

    /// Whose Visualizer look this theme draws: its pick, else its own, and
    /// for Custom the original.
    pub fn look(&self) -> &str {
        match &self.visualizer {
            Some(v) => v,
            None if self.name == "Custom" => ORIGINAL_LOOK,
            None => &self.name,
        }
    }

    /// Choose the Pomodoro world; the default is stored as no choice at all.
    pub fn pick_world(&mut self, name: String) {
        self.pomodoro = (name != CUSTOM_WORLD).then_some(name);
    }

    /// Choose the Visualizer look; the original is stored as no choice.
    pub fn pick_look(&mut self, name: String) {
        self.visualizer = (name != ORIGINAL_LOOK).then_some(name);
    }

    pub fn role(&self, i: usize) -> Color {
        match i {
            0 => self.accent,
            1 => self.text,
            2 => self.dim,
            3 => self.border,
            4 => self.sel,
            5 => self.warn,
            6 => self.sun,
            7 => self.moon,
            8 => self.foliage,
            9 => self.bark,
            10 => self.bloom,
            _ => self.turf,
        }
    }

    pub fn set_role(&mut self, i: usize, c: Color) {
        match i {
            0 => self.accent = c,
            1 => self.text = c,
            2 => self.dim = c,
            3 => self.border = c,
            4 => self.sel = c,
            5 => self.warn = c,
            6 => self.sun = c,
            7 => self.moon = c,
            8 => self.foliage = c,
            9 => self.bark = c,
            10 => self.bloom = c,
            _ => self.turf = c,
        }
    }
}

/// The one preset that isn't a flat palette.
pub const RAINBOW: &str = "Rainbow";

impl Theme {
    /// Drawn as a spectrum rather than in its accent: see `ui::paint_rainbow`.
    pub fn is_rainbow(&self) -> bool {
        self.name == RAINBOW
    }
}

/// A colour on the wheel (`hue` in degrees), bright enough to read on a dark
/// terminal without shouting.
pub fn hue(hue: f32) -> Color {
    let (s, l) = (0.9_f32, 0.68_f32);
    let h = hue.rem_euclid(360.0) / 60.0;
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let to = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Color::Rgb(to(r), to(g), to(b))
}

const fn rgb(hex: u32) -> Color {
    Color::Rgb(
        ((hex >> 16) & 0xff) as u8,
        ((hex >> 8) & 0xff) as u8,
        (hex & 0xff) as u8,
    )
}

pub fn hex_of(c: Color) -> String {
    match c {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        _ => "#000000".into(),
    }
}

pub fn parse_hex(s: &str) -> Option<Color> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(rgb(v))
}

macro_rules! theme {
    ($name:literal, $accent:expr, $text:expr, $dim:expr, $border:expr, $sel:expr, $warn:expr,
     $sun:expr, $moon:expr, $foliage:expr, $bark:expr, $bloom:expr, $turf:expr) => {
        Theme {
            name: String::new(), // filled by presets()
            accent: rgb($accent),
            text: rgb($text),
            dim: rgb($dim),
            border: rgb($border),
            sel: rgb($sel),
            warn: rgb($warn),
            sun: rgb($sun),
            moon: rgb($moon),
            foliage: rgb($foliage),
            bark: rgb($bark),
            bloom: rgb($bloom),
            turf: rgb($turf),
            pomodoro: None,
            visualizer: None,
        }
    };
}

/// The presets. Order is the picker order.
pub fn presets() -> Vec<Theme> {
    let mut v = vec![
        //       accent    text      dim       border    sel       warn      sun       moon      foliage   bark      bloom     turf
        (
            "Grimoire",
            theme!(
                "", 0xd6ad60, 0xdedcd4, 0x6c6c7a, 0x464654, 0x30303c, 0xc87864, 0xe8b65c, 0xacb6d4,
                0x5c7860, 0x705c48, 0xc68a9e, 0x465242
            ),
        ),
        (
            "Gruvbox Dark",
            theme!(
                "", 0xd79921, 0xebdbb2, 0x928374, 0x504945, 0x3c3836, 0xcc241d, 0xfabd2f, 0x83a598,
                0x98971a, 0xa87c4f, 0xd3869b, 0x4f5340
            ),
        ),
        (
            "Nord",
            theme!(
                "", 0x88c0d0, 0xd8dee9, 0x616e88, 0x3b4252, 0x434c5e, 0xbf616a, 0xebcb8b, 0x81a1c1,
                0xa3be8c, 0x8a7660, 0xb48ead, 0x4a5a4a
            ),
        ),
        (
            "Dracula",
            theme!(
                "", 0xbd93f9, 0xf8f8f2, 0x6272a4, 0x44475a, 0x44475a, 0xff5555, 0xf1fa8c, 0x8be9fd,
                0x50fa7b, 0x9b6a4a, 0xff79c6, 0x3d5a45
            ),
        ),
        (
            "Solarized Dark",
            theme!(
                "", 0xb58900, 0x93a1a1, 0x586e75, 0x0b3c47, 0x073642, 0xdc322f, 0xcb9b00, 0x268bd2,
                0x859900, 0x9c6a3c, 0xd33682, 0x445b3a
            ),
        ),
        (
            "Catppuccin Mocha",
            theme!(
                "", 0xcba6f7, 0xcdd6f4, 0x7f849c, 0x313244, 0x45475a, 0xf38ba8, 0xf9e2af, 0x89b4fa,
                0xa6e3a1, 0xb08968, 0xf5c2e7, 0x4c6b52
            ),
        ),
        (
            "Tokyo Night",
            theme!(
                "", 0x7aa2f7, 0xc0caf5, 0x565f89, 0x292e42, 0x33467c, 0xf7768e, 0xe0af68, 0x7dcfff,
                0x9ece6a, 0xa07a52, 0xbb9af7, 0x445a3c
            ),
        ),
        (
            "Everforest",
            theme!(
                "", 0xdbbc7f, 0xd3c6aa, 0x859289, 0x3d484d, 0x475258, 0xe67e80, 0xe69875, 0x7fbbb3,
                0xa7c080, 0x9c7a5c, 0xd699b6, 0x4a5a48
            ),
        ),
        // Deep woods. `accent` drives focused borders, titles, the open-scene
        // marker and the progress bar, so it has to be green or the whole
        // interface reads as whatever colour it is. Cyan is kept for the two
        // places it stays rare — the break-time moon and the flowers.
        (
            "Lost Forest",
            theme!(
                "", 0x7cc47f, 0xcdddc6, 0x5f7a63, 0x24332a, 0x2e4436, 0xd1745e, 0xd9c87e, 0x7fd4c8,
                0x4a8250, 0x5f4c3a, 0x68c2b4, 0x2f4a37
            ),
        ),
        // Rosé Pine: rose as the accent, pine for the trees (it has no green),
        // foam for the moon, iris for the flowers.
        (
            "Rosé Pine",
            theme!(
                "", 0xebbcba, 0xe0def4, 0x6e6a86, 0x403d52, 0x524f67, 0xeb6f92, 0xf6c177, 0x9ccfd8,
                0x31748f, 0x8f6f66, 0xc4a7e7, 0x2a3f4a
            ),
        ),
        (
            "One Dark",
            theme!(
                "", 0x61afef, 0xabb2bf, 0x5c6370, 0x3e4451, 0x3e4451, 0xe06c75, 0xe5c07b, 0x56b6c2,
                0x98c379, 0x9a7550, 0xc678dd, 0x3f5238
            ),
        ),
        // Monokai's pink leads, so the warning takes its orange.
        (
            "Monokai",
            theme!(
                "", 0xf92672, 0xf8f8f2, 0x75715e, 0x49483e, 0x49483e, 0xfd971f, 0xe6db74, 0x66d9ef,
                0xa6e22e, 0x8f6f3e, 0xae81ff, 0x4a5a2a
            ),
        ),
        (
            "Kanagawa",
            theme!(
                "", 0x7e9cd8, 0xdcd7ba, 0x727169, 0x363646, 0x2d4f67, 0xe46876, 0xe6c384, 0x7fb4ca,
                0x98bb6c, 0x8a6d4b, 0xd27e99, 0x3a4a34
            ),
        ),
        (
            "Ayu Mirage",
            theme!(
                "", 0xffcc66, 0xcccac2, 0x707a8c, 0x363c4a, 0x2f3b54, 0xff6666, 0xffd173, 0x73d0ff,
                0x87d96c, 0xa37a4c, 0xdfbfff, 0x3b4d35
            ),
        ),
        (
            "Night Owl",
            theme!(
                "", 0x7fdbca, 0xd6deeb, 0x637777, 0x1d3b53, 0x1d3b53, 0xef5350, 0xecc48d, 0x82aaff,
                0xaddb67, 0x9c7656, 0xc792ea, 0x2e4a3a
            ),
        ),
        (
            "Material Palenight",
            theme!(
                "", 0xc792ea, 0xa6accd, 0x676e95, 0x3a3f58, 0x444267, 0xf07178, 0xffcb6b, 0x89ddff,
                0xc3e88d, 0xa07b5c, 0xff9cac, 0x3d4a3a
            ),
        ),
        // Neon on a purple night: the ground is the grid's own violet.
        (
            "Synthwave '84",
            theme!(
                "", 0xff7edb, 0xf4eee4, 0x848bbd, 0x495495, 0x463465, 0xfe4450, 0xfede5d, 0x36f9f6,
                0x72f1b8, 0xb0735a, 0xf97e72, 0x34294f
            ),
        ),
        (
            "GitHub Dark",
            theme!(
                "", 0x58a6ff, 0xe6edf3, 0x7d8590, 0x30363d, 0x1c3a5e, 0xf85149, 0xd29922, 0x79c0ff,
                0x3fb950, 0x8b6a4a, 0xd2a8ff, 0x2a4a32
            ),
        ),
        // Every role its own hue, red to violet, so the clearing is a rainbow
        // by itself; the accent is then repainted as a slow-moving spectrum
        // wherever it's drawn (see `ui::paint_rainbow`).
        (
            RAINBOW,
            theme!(
                "", 0xa78bff, 0xf2f0ff, 0x8f8aa8, 0x4a4560, 0x3d3560, 0xff5f5f, 0xffd23c, 0x5fd7ff,
                0x5fe07a, 0xff9a3c, 0xff6ec7, 0x6a55d8
            ),
        ),
    ];
    v.iter_mut().for_each(|(n, t)| t.name = (*n).to_string());
    v.into_iter().map(|(_, t)| t).collect()
}

pub fn default_theme() -> Theme {
    presets().remove(0)
}

/// Parse Grimoire's existing theme.toml contents without filesystem access.
/// Named presets refresh from the shared palette; Custom preserves each role.
pub fn from_config(s: &str) -> Theme {
    let mut name = String::new();
    let mut fields: Vec<(String, String)> = Vec::new();
    for line in s.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let (k, v) = (k.trim().to_string(), v.trim().trim_matches('"').to_string());
        if k == "name" {
            name = v;
        } else {
            fields.push((k, v));
        }
    }

    if name != "Custom" {
        if let Some(t) = presets().into_iter().find(|t| t.name == name) {
            return t;
        }
        return default_theme();
    }

    let mut t = default_theme();
    t.name = "Custom".into();
    for (k, v) in fields {
        if let (Some(i), Some(c)) = (ROLES.iter().position(|r| *r == k), parse_hex(&v)) {
            t.set_role(i, c);
        } else if k == "pomodoro" && pomodoro_choices().contains(&v) {
            // A name no preset has any more is no choice: the default.
            t.pick_world(v);
        } else if k == "visualizer" && visualizer_choices().contains(&v) {
            t.pick_look(v);
        }
    }
    t
}
/// Encode the existing theme.toml format; all callers share the same roles.
pub fn to_config(t: &Theme) -> String {
    let mut out = format!("name = \"{}\"\n", t.name);
    if t.name == "Custom" {
        for (i, role) in ROLES.iter().enumerate() {
            out.push_str(&format!("{role} = \"{}\"\n", hex_of(t.role(i))));
        }
        if let Some(p) = &t.pomodoro {
            out.push_str(&format!("pomodoro = \"{p}\"\n"));
        }
        if let Some(v) = &t.visualizer {
            out.push_str(&format!("visualizer = \"{v}\"\n"));
        }
    }
    out
}
