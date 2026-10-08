//! How the game looks: the color themes, the ways of drawing the pieces, and the
//! file that remembers which of them the player picked.

use std::path::PathBuf;

use crate::engine::Level;

pub type Rgb = (u8, u8, u8);

pub struct Theme {
    pub name: &'static str,
    pub light: Rgb,
    pub dark: Rgb,
    /// The white pieces, and the darker tone for their details and shadows. They are
    /// a color, not white: white shaded with gray looked like a blur.
    pub white: Rgb,
    pub white_shade: Rgb,
    /// The black pieces, and the darker tone for theirs.
    pub black: Rgb,
    pub black_shade: Rgb,
    /// Tints for the squares of the last move, the picked-up piece and a king in check.
    pub last_move: Rgb,
    pub selected: Rgb,
    pub check: Rgb,
    /// Where the picked-up piece can go, and the marker moved with the arrow keys.
    pub target: Rgb,
    pub cursor: Rgb,
}

const BASE: Theme = Theme {
    name: "forest",
    white: (255, 236, 150),
    white_shade: (214, 170, 70),
    black: (150, 64, 190),
    black_shade: (84, 28, 120),
    light: (142, 170, 120),
    dark: (92, 124, 82),
    last_move: (236, 214, 84),
    selected: (255, 244, 150),
    check: (224, 62, 52),
    target: (60, 200, 215),
    cursor: (40, 110, 255),
};

pub const THEMES: [Theme; 16] = [
    BASE,
    Theme {
        name: "wood",
        white: (255, 244, 214),
        white_shade: (214, 184, 130),
        black: (24, 128, 140),
        black_shade: (8, 70, 84),
        light: (196, 158, 108),
        dark: (138, 94, 58),
        ..BASE
    },
    Theme {
        name: "ocean",
        white: (255, 226, 110),
        white_shade: (214, 160, 40),
        black: (190, 48, 136),
        black_shade: (112, 16, 84),
        light: (128, 166, 204),
        dark: (66, 108, 160),
        target: (140, 235, 130),
        cursor: (255, 128, 40),
        ..BASE
    },
    Theme {
        name: "slate",
        white: (255, 214, 90),
        white_shade: (206, 150, 30),
        black: (50, 104, 190),
        black_shade: (20, 54, 120),
        light: (150, 155, 166),
        dark: (94, 99, 112),
        ..BASE
    },
    Theme {
        name: "plum",
        white: (255, 246, 190),
        white_shade: (220, 186, 110),
        black: (30, 136, 104),
        black_shade: (10, 78, 62),
        light: (196, 150, 190),
        dark: (126, 80, 140),
        ..BASE
    },
    Theme {
        name: "ruby",
        white: (255, 240, 200),
        white_shade: (220, 180, 120),
        black: (40, 100, 160),
        black_shade: (14, 52, 96),
        light: (196, 134, 130),
        dark: (134, 62, 70),
        check: (130, 50, 230),
        ..BASE
    },
    // The colorful ones. Each changes whichever tints its own squares would swallow.
    Theme {
        name: "candy",
        white: (255, 250, 170),
        white_shade: (230, 190, 70),
        black: (30, 120, 170),
        black_shade: (12, 68, 108),
        light: (176, 236, 214),
        dark: (236, 112, 168),
        check: (110, 40, 200),
        cursor: (60, 60, 235),
        ..BASE
    },
    Theme {
        name: "sunset",
        white: (255, 252, 220),
        white_shade: (225, 190, 140),
        black: (88, 60, 180),
        black_shade: (42, 22, 110),
        light: (255, 196, 112),
        dark: (168, 70, 130),
        last_move: (110, 220, 255),
        selected: (255, 255, 235),
        target: (60, 230, 140),
        ..BASE
    },
    Theme {
        name: "neon",
        light: (84, 40, 150),
        dark: (30, 10, 84),
        white: (90, 240, 255),
        white_shade: (40, 150, 190),
        black: (255, 60, 170),
        black_shade: (150, 30, 110),
        last_move: (250, 230, 60),
        selected: (255, 255, 255),
        check: (255, 50, 40),
        target: (90, 255, 120),
        cursor: (255, 170, 40),
    },
    Theme {
        name: "tropical",
        white: (255, 250, 180),
        white_shade: (226, 190, 80),
        black: (170, 48, 116),
        black_shade: (98, 16, 66),
        light: (255, 190, 160),
        dark: (20, 150, 150),
        target: (255, 90, 200),
        cursor: (120, 40, 220),
        ..BASE
    },
    Theme {
        name: "citrus",
        white: (255, 255, 245),
        white_shade: (150, 200, 230),
        black: (36, 100, 180),
        black_shade: (12, 54, 112),
        light: (236, 236, 110),
        dark: (96, 210, 80),
        last_move: (255, 150, 60),
        selected: (255, 255, 255),
        target: (200, 70, 220),
        ..BASE
    },
    Theme {
        name: "aurora",
        white: (255, 248, 190),
        white_shade: (226, 188, 90),
        black: (160, 44, 140),
        black_shade: (92, 14, 82),
        light: (150, 230, 200),
        dark: (90, 90, 200),
        target: (255, 90, 200),
        cursor: (255, 128, 40),
        ..BASE
    },
    // The glowing ones, after neon: dark squares, and pieces in two bright colors, each
    // with a darker tone of itself for its details and shadows.
    Theme {
        name: "arcade",
        light: (44, 72, 156),
        dark: (16, 28, 88),
        white: (255, 176, 56),
        white_shade: (190, 104, 24),
        black: (110, 236, 116),
        black_shade: (40, 150, 72),
        last_move: (200, 120, 255),
        selected: (255, 255, 255),
        check: (255, 50, 40),
        target: (255, 110, 200),
        cursor: (90, 220, 255),
    },
    Theme {
        name: "lava",
        light: (124, 46, 42),
        dark: (62, 18, 26),
        white: (140, 226, 255),
        white_shade: (60, 150, 200),
        black: (255, 156, 44),
        black_shade: (180, 84, 20),
        last_move: (255, 240, 120),
        selected: (255, 255, 255),
        check: (190, 90, 255),
        target: (90, 255, 120),
        cursor: (60, 120, 255),
    },
    Theme {
        name: "galaxy",
        light: (50, 58, 100),
        dark: (16, 18, 46),
        white: (255, 240, 150),
        white_shade: (200, 170, 80),
        black: (200, 132, 255),
        black_shade: (120, 70, 190),
        last_move: (90, 200, 255),
        selected: (255, 255, 255),
        check: (255, 50, 40),
        target: (90, 255, 120),
        cursor: (255, 170, 40),
    },
    Theme {
        name: "lagoon",
        light: (24, 100, 104),
        dark: (8, 54, 66),
        white: (255, 250, 200),
        white_shade: (200, 180, 120),
        black: (255, 112, 124),
        black_shade: (170, 50, 84),
        last_move: (250, 230, 60),
        selected: (255, 255, 255),
        check: (190, 90, 255),
        target: (120, 255, 120),
        cursor: (255, 170, 40),
    },
];

/// The frame around the two squares of a hint, the same in every theme so that it is
/// never mistaken for the theme's own marks.
pub const HINT: Rgb = (255, 105, 215);
/// The sparks around a promoted pawn.
pub const SPARK: Rgb = (255, 232, 110);
pub const CONFETTI: [Rgb; 6] = [(255, 92, 92), (255, 196, 60), (110, 220, 110), (80, 190, 255), (200, 130, 255), (255, 255, 255)];

pub fn find_theme(name: &str) -> Option<usize> {
    THEMES.iter().position(|t| t.name.eq_ignore_ascii_case(name))
}

/// How the pieces are drawn. The first three are pixel art and need a window large
/// enough for it; in a smaller one they fall back to chess symbols.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pieces {
    /// Flat shapes.
    Solid,
    /// Flat shapes with a strong rim in the other color.
    Outlined,
    /// Shapes lit from the top left, so they look rounded.
    Shaded,
    /// The font's own chess symbols, at any window size.
    Symbols,
    /// Letters, for fonts without chess symbols.
    Letters,
}

impl Pieces {
    pub const ALL: [Pieces; 5] = [Pieces::Solid, Pieces::Outlined, Pieces::Shaded, Pieces::Symbols, Pieces::Letters];

    pub fn name(self) -> &'static str {
        match self {
            Pieces::Solid => "solid",
            Pieces::Outlined => "outlined",
            Pieces::Shaded => "shaded",
            Pieces::Symbols => "symbols",
            Pieces::Letters => "letters",
        }
    }

    pub fn parse(text: &str) -> Option<Pieces> {
        Pieces::ALL.into_iter().find(|p| p.name().eq_ignore_ascii_case(text))
    }

    pub fn is_pixel_art(self) -> bool {
        matches!(self, Pieces::Solid | Pieces::Outlined | Pieces::Shaded)
    }
}

/// What is remembered from one run to the next.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Settings {
    pub theme: usize,
    pub pieces: Pieces,
    pub level: Level,
    /// Whether to look for a newer release when the game starts.
    pub update: bool,
    /// Whether pieces slide, burst and fall, or simply appear where they went.
    pub animations: bool,
    pub sound: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { theme: 0, pieces: Pieces::Shaded, level: Level::Easy, update: true, animations: true, sound: true }
    }
}

impl Settings {
    /// `settings` in the state directory.
    pub fn path() -> PathBuf {
        crate::update::state_dir().join("settings")
    }

    /// Lines of `name=value`. Anything missing or not understood keeps its default.
    pub fn parse(text: &str) -> Settings {
        let mut settings = Settings::default();
        for line in text.lines() {
            match line.split_once('=').map(|(k, v)| (k.trim(), v.trim())) {
                Some(("theme", v)) => settings.theme = find_theme(v).unwrap_or(settings.theme),
                Some(("pieces", v)) => settings.pieces = Pieces::parse(v).unwrap_or(settings.pieces),
                Some(("level", v)) => settings.level = Level::parse(v).unwrap_or(settings.level),
                Some(("update", v)) => settings.update = v != "0",
                Some(("animations", v)) => settings.animations = v != "0",
                Some(("sound", v)) => settings.sound = v != "0",
                _ => {}
            }
        }
        settings
    }

    pub fn format(&self) -> String {
        format!(
            "theme={}\npieces={}\nlevel={}\nupdate={}\nanimations={}\nsound={}\n",
            THEMES[self.theme].name,
            self.pieces.name(),
            self.level.name().to_lowercase(),
            u8::from(self.update),
            u8::from(self.animations),
            u8::from(self.sound)
        )
    }

    pub fn load(path: &std::path::Path) -> Settings {
        Settings::parse(&std::fs::read_to_string(path).unwrap_or_default())
    }

    /// Failing to save is not worth interrupting a game for.
    pub fn save(&self, path: &std::path::Path) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, self.format());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_piece_color_has_a_darker_shade() {
        let luma = |(r, g, b): Rgb| 299 * r as u32 + 587 * g as u32 + 114 * b as u32;
        for theme in &THEMES {
            assert!(luma(theme.white_shade) < luma(theme.white), "{}", theme.name);
            assert!(luma(theme.black_shade) < luma(theme.black), "{}", theme.name);
            assert!(luma(theme.black) < luma(theme.white), "{}", theme.name);
        }
    }

    #[test]
    fn settings_survive_a_round_trip_and_bad_input() {
        let settings =
            Settings { theme: find_theme("ocean").unwrap(), pieces: Pieces::Outlined, level: Level::Hard, update: false, animations: false, sound: false };
        assert_eq!(Settings::parse(&settings.format()), settings);
        assert_eq!(Settings::parse("theme=nope\npieces\n=\nlevel=HARD\nextra=1"), Settings { level: Level::Hard, ..Settings::default() });

        let dir = std::env::temp_dir().join(format!("funchess-test-{}", std::process::id()));
        let path = dir.join("deeper/settings");
        assert_eq!(Settings::load(&path), Settings::default());
        settings.save(&path);
        assert_eq!(Settings::load(&path), settings);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn names_are_unique_and_found() {
        for (i, theme) in THEMES.iter().enumerate() {
            assert_eq!(find_theme(theme.name), Some(i));
        }
        for pieces in Pieces::ALL {
            assert_eq!(Pieces::parse(pieces.name()), Some(pieces));
        }
    }
}
