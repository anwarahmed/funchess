//! How the game looks: the color themes, the ways of drawing the pieces, and the
//! file that remembers which of them the player picked.

use std::path::PathBuf;

use crate::engine::Level;

pub type Rgb = (u8, u8, u8);

pub struct Theme {
    pub name: &'static str,
    pub light: Rgb,
    pub dark: Rgb,
    /// The white pieces, and the darker tone for their details and shadows.
    pub white: Rgb,
    pub white_shade: Rgb,
    /// The black pieces, and the lighter tone for their details and highlights.
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
    light: (142, 170, 120),
    dark: (92, 124, 82),
    white: (255, 255, 255),
    white_shade: (165, 170, 185),
    black: (22, 20, 24),
    black_shade: (105, 105, 120),
    last_move: (236, 214, 84),
    selected: (255, 244, 150),
    check: (224, 62, 52),
    target: (60, 200, 215),
    cursor: (40, 110, 255),
};

pub const THEMES: [Theme; 12] = [
    BASE,
    Theme { name: "wood", light: (196, 158, 108), dark: (138, 94, 58), white_shade: (190, 175, 160), black: (30, 20, 14), black_shade: (120, 100, 85), ..BASE },
    Theme { name: "ocean", light: (128, 166, 204), dark: (66, 108, 160), target: (140, 235, 130), cursor: (255, 128, 40), ..BASE },
    Theme { name: "slate", light: (150, 155, 166), dark: (94, 99, 112), white: (248, 248, 250), black: (14, 14, 18), ..BASE },
    Theme { name: "plum", light: (196, 150, 190), dark: (126, 80, 140), white: (255, 250, 242), black: (36, 18, 46), black_shade: (125, 100, 140), ..BASE },
    Theme { name: "ruby", light: (196, 134, 130), dark: (134, 62, 70), black: (28, 14, 16), black_shade: (125, 95, 100), check: (130, 50, 230), ..BASE },
    // The colorful ones. Each changes whichever tints its own squares would swallow.
    Theme {
        name: "candy",
        light: (176, 236, 214),
        dark: (236, 112, 168),
        black: (48, 18, 52),
        black_shade: (140, 100, 150),
        check: (110, 40, 200),
        cursor: (60, 60, 235),
        ..BASE
    },
    Theme {
        name: "sunset",
        light: (255, 196, 112),
        dark: (168, 70, 130),
        black: (40, 16, 44),
        black_shade: (135, 100, 135),
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
        light: (255, 190, 160),
        dark: (20, 150, 150),
        black: (10, 40, 48),
        black_shade: (90, 130, 135),
        target: (255, 90, 200),
        cursor: (120, 40, 220),
        ..BASE
    },
    Theme {
        name: "citrus",
        light: (236, 236, 110),
        dark: (96, 210, 80),
        black: (30, 44, 16),
        black_shade: (110, 130, 90),
        last_move: (255, 150, 60),
        selected: (255, 255, 255),
        target: (200, 70, 220),
        ..BASE
    },
    Theme {
        name: "aurora",
        light: (150, 230, 200),
        dark: (90, 90, 200),
        black: (20, 18, 60),
        black_shade: (105, 105, 160),
        target: (255, 90, 200),
        cursor: (255, 128, 40),
        ..BASE
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
