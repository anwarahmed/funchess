//! Drawing. The board is as large as the window allows: pixel-art pieces made of
//! half-block characters when there is room, chess symbols (or letters) when not.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color as TermColor, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph, Widget, Wrap};

use crate::app::{App, Click, Geometry, MenuItem, Opponent, Prompt, QUICK_WIN, Screen};
use crate::chess::{Board, Color, Kind, Outcome, Piece, file_of, rank_of, sq};
use crate::engine::Level;
use crate::fx;
use crate::theme::{self, Pieces, Rgb};
use ratatui::crossterm::event::KeyCode;

/// The players, the state of the game and the commands, to the right of the board.
const RIGHT_WIDTH: u16 = 30;
/// The title and the moves, to the left of the board when the window is wide enough.
const LEFT_WIDTH: u16 = 22;
const GAP: u16 = 2;
/// The tallest a square gets, in rows. Beyond this the pieces would only get blockier.
const MAX_CELL_HEIGHT: u16 = 12;

// Pixel art: `#` is the piece, `+` a shaded detail, `.` shows the square through.
#[rustfmt::skip]
static SMALL: [[&str; 8]; 6] = [
    ["........", "...##...", "..####..", "..####..", "...##...", "..####..", ".######.", "........"],
    ["..#.#...", "..####..", ".#+####.", "#######.", "##..###.", "...####.", "..#####.", "........"],
    ["...##...", "..####..", "..#+##..", "..####..", "...##...", "..####..", ".######.", "........"],
    ["........", "##.##.##", "########", ".######.", ".######.", ".######.", "########", "........"],
    ["#.#..#.#", "#.#..#.#", "########", ".######.", "..####..", "..####..", ".######.", "........"],
    ["...##...", "..####..", "...##...", ".######.", ".######.", "..####..", ".######.", "........"],
];
#[rustfmt::skip]
static MEDIUM: [[&str; 10]; 6] = [
    ["..........", "....##....", "...####...", "...####...", "....##....", "...####...", "....##....", "...####...", "..######..", ".........."],
    ["..........", "...#.##...", "..######..", ".##+#####.", ".########.", ".###.####.", "....####..", "...#####..", "..#######.", ".........."],
    ["....##....", "...####...", "..###+##..", "..##+###..", "..######..", "...####...", "....##....", "...####...", "..######..", ".........."],
    ["..........", ".##.##.##.", ".########.", "..######..", "..######..", "..######..", "..######..", ".########.", ".########.", ".........."],
    ["..........", ".#..##..#.", ".#..##..#.", ".##.##.##.", ".########.", "..######..", "..######..", "..++++++..", ".########.", ".........."],
    ["....##....", "..######..", "....##....", "..##..##..", ".###++###.", ".########.", "..######..", "..++++++..", ".########.", ".........."],
];
#[rustfmt::skip]
static LARGE: [[&str; 12]; 6] = [
    [
        "............", ".....##.....", "....####....", "....####....", ".....##.....", "....####....",
        ".....##.....", ".....##.....", "....####....", "...######...", "..########..", "............",
    ],
    [
        "............", "....#.##....", "...######...", "..##+#####..", ".#########..", ".####.####..",
        ".##..#####..", "....#####...", "...#####....", "...######...", "..########..", "............",
    ],
    [
        ".....##.....", "....####....", "...###+##...", "...##+###...", "...######...", "....####....",
        ".....##.....", "....####....", "....####....", "...######...", "..########..", "............",
    ],
    [
        "............", "..##.##.##..", "..##.##.##..", "..########..", "...######...", "...######...",
        "...######...", "...######...", "...######...", "..########..", ".##########.", "............",
    ],
    [
        "............", ".#...##...#.", ".#...##...#.", ".##.####.##.", ".##########.", "..########..",
        "...######...", "...######...", "..########..", "..++++++++..", ".##########.", "............",
    ],
    [
        ".....##.....", "...######...", ".....##.....", ".....##.....", "..###..###..", ".####++####.",
        ".##########.", "..########..", "...######...", "..++++++++..", ".##########.", "............",
    ],
];

// The pieces a player has captured, on the tray beside the board: 5 pixels by 6.
#[rustfmt::skip]
const TINY: [(Kind, [&str; 6]); 5] = [
    (Kind::Queen,  ["..#..", "#.#.#", "#####", ".###.", ".###.", "#####"]),
    (Kind::Rook,   ["#.#.#", "#####", ".###.", ".###.", ".###.", "#####"]),
    (Kind::Bishop, ["..#..", ".###.", ".#.#.", ".###.", "..#..", "#####"]),
    (Kind::Knight, [".##..", "####.", "#.##.", "..##.", ".###.", "#####"]),
    (Kind::Pawn,   [".....", "..#..", ".###.", "..#..", ".###.", "#####"]),
];
/// How wide a place on the tray is: the piece, and a column for how many of them.
const TRAY_SLOT: u16 = 6;

// The computer has a face, a different one at each level: its name, the color of its
// head (`h`) and of its ears, beak, bolts or horns (`a`), and the head itself, 12
// pixels each way. The eyes and the mouth are drawn over it to suit its mood.
#[rustfmt::skip]
const FACES: [(&str, Rgb, Rgb, [&str; 12]); 4] = [
    ("Chick", (250, 212, 70), (240, 140, 40), [
        ".....aa.....", "...hhhhhh...", "..hhhhhhhh..", ".hhhhhhhhhh.", ".hhhhhhhhhh.", ".hhhhhhhhhh.",
        ".hhhhaahhhh.", ".hhhhhhhhhh.", ".hhhhhhhhhh.", "..hhhhhhhh..", "...hhhhhh...", "............",
    ]),
    ("Cat", (240, 150, 60), (250, 175, 185), [
        ".hh......hh.", ".hah....hah.", ".hhhhhhhhhh.", "hhhhhhhhhhhh", "hhhhhhhhhhhh", "hhhhhhhhhhhh",
        "hhhhhaahhhhh", "hhhhhhhhhhhh", "hhhhhhhhhhhh", ".hhhhhhhhhh.", "..hhhhhhhh..", "............",
    ]),
    ("Robot", (150, 172, 195), (235, 80, 80), [
        ".....aa.....", ".....hh.....", "hhhhhhhhhhhh", "hhhhhhhhhhhh", "hhhhhhhhhhhh", "hhhhhhhhhhhh",
        "ahhhhhhhhhha", "hhhhhhhhhhhh", "hhhhhhhhhhhh", "hhhhhhhhhhhh", ".hhhhhhhhhh.", "............",
    ]),
    ("Dragon", (95, 190, 105), (250, 220, 90), [
        "a..........a", "aa........aa", ".hhhhhhhhhh.", "hhhhhhhhhhhh", "hhhhhhhhhhhh", "hhhhhhhhhhhh",
        "hhhhhhhhhhhh", "hhhhhhhhhhhh", "hhhhhhhhhhhh", ".hhhhhhhhhh.", "..hhhhhhhh..", "............",
    ]),
];

/// What the computer's face shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mood {
    Calm,
    Thinking,
    Happy,
    Surprised,
    Sad,
}

/// The name of the character the computer is at this level.
fn character(level: Level) -> &'static str {
    FACES[level as usize].0
}

/// How the computer feels about the game as it stands, and what it says. It is worked
/// out from the position and the last move alone, so a take-back takes it back too.
fn mood(app: &App) -> (Mood, String) {
    let board = &app.game.board;
    let computer = app.me.other();
    if let Some(outcome) = app.game.outcome {
        return match outcome.winner() {
            Some(c) if c == app.me => (Mood::Sad, "You got me! Well played!".into()),
            Some(_) => (Mood::Happy, "I won! Good game!".into()),
            None => (Mood::Calm, "A draw. Good game!".into()),
        };
    }
    let Some(last) = app.game.history.last() else {
        return (Mood::Calm, "Let's play!".into());
    };
    let took = last.before.is_capture(last.mv).then(|| last.before.at(last.mv.to).map_or(Kind::Pawn, |p| p.kind).name());
    match (last.before.turn == computer, took) {
        (true, _) if board.in_check(app.me) => (Mood::Happy, "Check!".into()),
        (true, Some(kind)) => (Mood::Happy, format!("I got your {kind}!")),
        (true, None) => (Mood::Calm, "Your turn!".into()),
        (false, Some(kind)) => (Mood::Surprised, format!("Hey, my {kind}!")),
        (false, None) if board.in_check(computer) => (Mood::Surprised, "Uh oh, check!".into()),
        // The dots come one at a time while it thinks.
        (false, None) => (Mood::Thinking, format!("Hmm{}", ".".repeat(app.clock.as_millis() as usize / 400 % 4))),
    }
}

/// One pixel of the computer's face; nothing where the face is not.
fn face_pixel(level: Level, mood: Mood, x: usize, y: usize) -> Option<Rgb> {
    let (_, head, other, rows) = FACES[level as usize];
    // Two pixels each way for an eye, four by two for the mouth.
    let (eyes, mouth) = match mood {
        Mood::Calm => (["we", "ee"], ["hhhh", "hmmh"]),
        Mood::Thinking => (["we", "ww"], ["hhhh", "hhmm"]),
        Mood::Happy => (["ee", "hh"], ["mmmm", "hrrh"]),
        Mood::Surprised => (["ww", "we"], ["hmmh", "hmmh"]),
        Mood::Sad => (["hh", "ee"], ["hmmh", "mhhm"]),
    };
    let c = match (x, y) {
        (2..=3 | 8..=9, 4..=5) => eyes[y - 4].as_bytes()[(x - 2) % 6],
        (4..=7, 7..=8) => mouth[y - 7].as_bytes()[x - 4],
        _ => rows[y].as_bytes()[x],
    };
    match c {
        b'h' => Some(head),
        b'a' => Some(other),
        b'e' | b'm' => Some((30, 26, 34)),
        b'w' => Some((255, 255, 255)),
        b'r' => Some((235, 90, 100)),
        _ => None,
    }
}

/// The face, 12 columns by 6 rows, on whatever the terminal's own background is.
fn face(buf: &mut Buffer, app: &App, x: u16, y: u16, level: Level, mood: Mood) {
    for row in 0..6 {
        for col in 0..12 {
            let (top, bottom) = (face_pixel(level, mood, col, row * 2), face_pixel(level, mood, col, row * 2 + 1));
            let Some(cell) = buf.cell_mut((x + col as u16, y + row as u16)) else { continue };
            match (top, bottom) {
                (Some(top), Some(bottom)) => cell.set_char('▀').set_fg(paint(app, top)).set_bg(paint(app, bottom)),
                (Some(top), None) => cell.set_char('▀').set_fg(paint(app, top)),
                (None, Some(bottom)) => cell.set_char('▄').set_fg(paint(app, bottom)),
                (None, None) => cell,
            };
        }
    }
}

/// A tray three rows high holding the captured pieces, one of each kind with how many
/// there are of it. It is the color of a dark square, so both colors show on it.
fn tray(buf: &mut Buffer, app: &App, area: Rect, pieces: &[Piece]) {
    // An empty tray is not drawn: a bar of color with nothing on it means nothing.
    if pieces.is_empty() {
        return;
    }
    let shelf = paint(app, app.theme().dark);
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_char('▀').set_fg(shelf).set_bg(shelf);
            }
        }
    }
    let mut x = area.x;
    for (kind, art) in TINY {
        let Some(&piece) = pieces.iter().find(|p| p.kind == kind) else { continue };
        if x + TRAY_SLOT > area.right() {
            break;
        }
        let ink = paint(app, if piece.color == Color::White { app.theme().white } else { app.theme().black });
        for (py, row) in art.iter().enumerate() {
            for (px, _) in row.bytes().enumerate().filter(|&(_, c)| c == b'#') {
                if let Some(cell) = buf.cell_mut((x + px as u16, area.y + py as u16 / 2)) {
                    if py % 2 == 0 {
                        cell.set_fg(ink)
                    } else {
                        cell.set_bg(ink)
                    };
                }
            }
        }
        let count = pieces.iter().filter(|p| p.kind == kind).count();
        if count > 1
            && let Some(cell) = buf.cell_mut((x + TRAY_SLOT - 1, area.y + 2))
        {
            cell.set_char(char::from_digit(count as u32, 10).unwrap_or('+')).set_fg(paint(app, app.theme().white)).set_style(bold());
        }
        x += TRAY_SLOT;
    }
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    (f(a.0, b.0), f(a.1, b.1), f(a.2, b.2))
}

/// The nearest of the 256 standard colors, for terminals without full color
/// (Terminal.app on macOS before Tahoe, for one).
fn nearest_256((r, g, b): Rgb) -> TermColor {
    const STEPS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let step = |v: u8| (0..6).min_by_key(|&i| (STEPS[i] as i32 - v as i32).abs()).unwrap_or(0);
    let (ri, gi, bi) = (step(r), step(g), step(b));
    let distance = |c: Rgb| (c.0 as i32 - r as i32).pow(2) + (c.1 as i32 - g as i32).pow(2) + (c.2 as i32 - b as i32).pow(2);
    let gray_index = ((r as i32 + g as i32 + b as i32) / 3 - 8).clamp(0, 230) / 10;
    let gray = (8 + gray_index * 10) as u8;
    if distance((gray, gray, gray)) < distance((STEPS[ri], STEPS[gi], STEPS[bi])) {
        TermColor::Indexed(232 + gray_index as u8)
    } else {
        TermColor::Indexed((16 + 36 * ri + 6 * gi + bi) as u8)
    }
}

fn paint(app: &App, c: Rgb) -> TermColor {
    if app.truecolor { TermColor::Rgb(c.0, c.1, c.2) } else { nearest_256(c) }
}

fn dim() -> Style {
    Style::new().fg(TermColor::DarkGray)
}

fn bold() -> Style {
    Style::new().add_modifier(Modifier::BOLD)
}

fn symbol(app: &App, piece: Piece, solid: bool) -> char {
    if app.pieces == Pieces::Letters {
        let c = piece.kind.letter();
        return if piece.color == Color::White { c } else { c.to_ascii_lowercase() };
    }
    // The outlined set is U+2654.., the solid set U+265A.., king first.
    let base = if solid || piece.color == Color::Black { 0x265A } else { 0x2654 };
    let order = [5, 4, 3, 2, 1, 0][piece.kind as usize];
    char::from_u32(base + order).unwrap_or('?')
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let buf = frame.buffer_mut();
    app.buttons.clear();
    match &app.screen {
        Screen::Menu => menu(buf, area, app),
        Screen::Join => {
            let lines = vec![
                Line::raw("Address of the computer hosting the game"),
                Line::raw(""),
                Line::styled(format!("> {}_", app.address), bold()),
                Line::raw(""),
                Line::styled("for example 192.168.1.20 or mybox.local:6464", dim()),
                Line::styled("Enter connect    Esc back", dim()),
            ];
            let row = lines.len() as u16 - 1;
            let (x, y) = centered(buf, area, "Join a game", 2, lines);
            app.buttons.push((Rect::new(x, y + row, 13, 1), Click::Key(KeyCode::Enter, false)));
            app.buttons.push((Rect::new(x + 17, y + row, 8, 1), Click::Key(KeyCode::Esc, false)));
        }
        Screen::Connecting(text) => {
            let mut lines: Vec<Line> = text
                .iter()
                .map(|l| if l.starts_with("funchess ") || l.starts_with("sudo ") { Line::styled(l.clone(), bold()) } else { Line::raw(l.clone()) })
                .collect();
            lines.extend([Line::raw(""), Line::styled("Esc cancel", dim())]);
            let row = lines.len() as u16 - 1;
            let (x, y) = centered(buf, area, "Network game", 2, lines);
            app.buttons.push((Rect::new(x, y + row, 10, 1), Click::Key(KeyCode::Esc, false)));
        }
        Screen::Game => game(buf, area, app),
    }
}

/// A titled block of lines in the middle of the screen. Returns where the first line starts.
/// `head` is the rows the title takes: 2 with an empty row under it, 1 without, 0 for no title.
fn centered(buf: &mut Buffer, area: Rect, title: &str, head: u16, lines: Vec<Line>) -> (u16, u16) {
    let width = lines.iter().map(Line::width).max().unwrap_or(0).max(title.len()) as u16;
    let height = lines.len() as u16 + head;
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    if head > 0 {
        buf.set_stringn(x, y, title, area.width as usize, bold().fg(TermColor::Cyan));
    }
    for (i, line) in lines.iter().enumerate() {
        let row = y + head + i as u16;
        if row < area.bottom() {
            buf.set_line(x, row, line, area.width.saturating_sub(x - area.x));
        }
    }
    (x, y + head)
}

fn menu(buf: &mut Buffer, area: Rect, app: &mut App) {
    // The nine choices always show. What a short window has rows left for is added in
    // this order: the error line, the keys, the title, then the empty rows between them.
    let spare = area.height.saturating_sub(MenuItem::ALL.len() as u16);
    let mut lines = Vec::new();
    let mut rows = Vec::new();
    for (i, item) in MenuItem::ALL.into_iter().enumerate() {
        if spare >= 6 && matches!(item, MenuItem::Level | MenuItem::Quit) {
            lines.push(Line::raw(""));
        }
        let text = match item {
            MenuItem::Computer => "1  Play the computer".to_string(),
            MenuItem::Local => "2  Two players, this computer".to_string(),
            MenuItem::Host => "3  Host a game for another computer".to_string(),
            MenuItem::Join => "4  Join a game on another computer".to_string(),
            MenuItem::Level => format!("   Computer level   < {} >", app.level.name()),
            MenuItem::Side => format!("   Your color       < {} >", app.side.name()),
            MenuItem::Theme => format!("T  Theme            < {} >", app.theme().name),
            MenuItem::Pieces => format!("P  Pieces           < {} >", app.pieces.name()),
            MenuItem::Quit => "Q  Quit".to_string(),
        };
        let chosen = i == app.menu_item;
        let style = if chosen { bold().fg(TermColor::Cyan) } else { Style::new() };
        rows.push(lines.len() as u16);
        lines.push(Line::styled(format!("{} {text}", if chosen { ">" } else { " " }), style));
    }
    if spare >= 7 {
        lines.push(Line::raw(""));
    }
    if spare >= 1 {
        lines.push(Line::styled(app.menu_error.clone(), Style::new().fg(TermColor::Red)));
    }
    if spare >= 2 {
        let keys = "Up/Down choose   Enter start   Left/Right change";
        lines.push(Line::styled(if area.width as usize >= keys.len() { keys } else { "↑↓ choose   Enter start   ←→ change" }, dim()));
    }
    // A strip of pieces under the menu, to show the theme and the piece style.
    let (cell_w, cell_h) = (8, 4);
    let sample = [
        (Color::White, Kind::King),
        (Color::White, Kind::Queen),
        (Color::White, Kind::Knight),
        (Color::Black, Kind::Rook),
        (Color::Black, Kind::Bishop),
        (Color::Black, Kind::Pawn),
    ];
    let strip_w = sample.len() as u16 * cell_w;
    let preview = (area.height >= lines.len() as u16 + 3 + cell_h && area.width >= strip_w).then_some(lines.len() as u16 + 1);
    if preview.is_some() {
        lines.extend(std::iter::repeat_n(Line::raw(""), cell_h as usize + 1));
    }
    let width = lines.iter().map(Line::width).max().unwrap_or(0) as u16;
    let (left, top) = centered(buf, area, "funchess   chess in the terminal", spare.saturating_sub(2).min(2), lines);
    for (i, row) in rows.into_iter().enumerate() {
        // The whole line chooses the item or steps a setting forward; its "<" steps back.
        app.buttons.push((Rect::new(left, top + row, width, 1), Click::Menu(i, true)));
        app.buttons.push((Rect::new(left + 21, top + row, 3, 1), Click::Menu(i, false)));
    }
    if let Some(row) = preview {
        let x0 = area.x + (area.width - strip_w) / 2;
        for (i, (color, kind)) in sample.into_iter().enumerate() {
            let ground = if i % 2 == 0 { app.theme().light } else { app.theme().dark };
            let cell = Cell {
                x: x0 + i as u16 * cell_w,
                y: top + row,
                w: cell_w,
                h: cell_h,
                ground,
                piece: Some(Piece { color, kind }),
                pose: Pose::Upright,
                target: false,
                cursor: false,
                hint: None,
            };
            cell.draw(buf, app);
        }
    }
}

/// How wide a square of the given height is. Pixel art is twice as wide as tall, which
/// is square on screen; a symbol needs an odd width to sit in the middle.
fn cell_width(app: &App, height: u16) -> u16 {
    if height >= 4 && app.pieces.is_pixel_art() { height * 2 } else { height * 2 + 1 }
}

fn game(buf: &mut Buffer, area: Rect, app: &mut App) {
    app.geometry = None;
    // The board is as tall as the window allows, leaving one row for the file letters,
    // as long as the panel beside it still fits.
    let width_with = |cell_w: u16| 2 + 8 * cell_w + GAP + RIGHT_WIDTH;
    let tallest = (area.height.saturating_sub(1) / 8).min(MAX_CELL_HEIGHT);
    let Some((cell_w, cell_h)) = (1..=tallest).rev().map(|h| (cell_width(app, h), h)).find(|&(w, _)| width_with(w) <= area.width) else {
        let need = format!("Please make the window at least {} x 9", width_with(cell_width(app, 1)));
        buf.set_stringn(area.x, area.y, need, area.width as usize, Style::new());
        return;
    };
    let (board_w, board_h) = (8 * cell_w, 8 * cell_h);
    let left = if width_with(cell_w) + LEFT_WIDTH + GAP <= area.width { LEFT_WIDTH + GAP } else { 0 };
    let x0 = area.x + (area.width - width_with(cell_w) - left) / 2;
    let by = area.y + (area.height - (board_h + 1)) / 2;
    let bx = x0 + left + 2;
    app.geometry = Some(Geometry { x: bx, y: by, cell_w, cell_h });

    let board = app.game.board;
    let white_below = app.bottom() == Color::White;
    let targets = app.targets();
    let last = app.game.last_move();
    // The last move, while it is being shown: the piece is still on its way, or it
    // has landed and what follows from that is happening.
    let clock = app.clock;
    let moving = app.fx.as_ref();
    let sliding = moving.filter(|fx| fx.sliding(clock));
    let checked = board.in_check(board.turn).then(|| board.king_square(board.turn)).filter(|_| sliding.is_none());
    let mated = matches!(app.game.outcome, Some(Outcome::Checkmate(_))).then(|| board.king_square(board.turn));
    let pulse = 0.5 + 0.5 * (clock.as_secs_f32() * 6.0).sin();
    let mut stage = Stage { x: bx, y: by, cell_w, cell_h, white_below, pixels: cell_h >= 4 && app.pieces.is_pixel_art(), grounds: [[(0, 0, 0); 8]; 8] };
    for row in 0..8u16 {
        for col in 0..8u16 {
            let square = if white_below { sq(col as u8, 7 - row as u8) } else { sq(7 - col as u8, row as u8) };
            let theme = app.theme();
            let mut ground = if (file_of(square) + rank_of(square)) % 2 == 1 { theme.light } else { theme.dark };
            if last.is_some_and(|m| m.from == square || m.to == square) {
                ground = mix(ground, theme.last_move, 0.55);
            }
            if app.selected == Some(square) {
                ground = mix(ground, theme.selected, 0.8);
            }
            if checked == Some(square) {
                // Redder with every shake of the king.
                ground = mix(ground, theme.check, 0.75 + 0.25 * moving.map_or(0.0, |fx| fx.shake(clock).abs()));
            }
            stage.grounds[row as usize][col as usize] = ground;
            let (mut piece, mut pose) = (board.at(square), Pose::Upright);
            match moving {
                // The position is already the one after the move. Until the piece
                // arrives its square holds what was there before, a pawn taken in
                // passing is still beside it, and a castling rook is on its way too.
                Some(fx) if sliding.is_some() => {
                    if square == fx.to || fx.captured.is_some_and(|(at, _)| at == square) {
                        piece = fx.captured.filter(|&(at, _)| at == square).map(|(_, taken)| taken);
                    } else if fx.rook.is_some_and(|(_, to)| to == square) {
                        piece = None;
                    }
                }
                Some(fx) if fx.checked == Some(square) => {
                    pose = match fx.fall(clock) {
                        Some(t) if t >= 1.0 => Pose::Fallen,
                        Some(t) => Pose::Leaning(t),
                        None => Pose::Shifted((fx.shake(clock) * 1.4).round() as i32),
                    }
                }
                Some(_) => {}
                None if mated == Some(square) => pose = Pose::Fallen,
                None => {}
            }
            let cell = Cell {
                x: bx + col * cell_w,
                y: by + row * cell_h,
                w: cell_w,
                h: cell_h,
                ground,
                piece,
                pose,
                target: targets.contains(&square),
                cursor: app.cursor == square && app.prompt.is_none(),
                hint: app.hint.filter(|m| m.from == square || m.to == square).map(|_| pulse),
            };
            cell.draw(buf, app);
        }
        // Rank numbers down the side, file letters along the bottom.
        let rank = if white_below { 8 - row } else { row + 1 };
        buf.set_string(bx - 2, by + row * cell_h + (cell_h - 1) / 2, rank.to_string(), dim());
        let file = (b'a' + if white_below { row } else { 7 - row } as u8) as char;
        buf.set_string(bx + row * cell_w + cell_w / 2, by + board_h, file.to_string(), dim());
    }

    // Over the squares: whatever is on its way, or flying.
    if let Some(fx) = moving {
        if sliding.is_some() {
            let (file, rank, lift) = fx.place(clock);
            if let (Some((file, rank)), Some((_, to))) = (fx.rook_place(clock), fx.rook) {
                let (col, row) = stage.spot(file, rank);
                stage.piece(buf, app, board.at(to).unwrap_or(fx.piece), col, row);
            }
            let (col, row) = stage.spot(file, rank);
            stage.piece(buf, app, fx.piece, col, row - lift);
        }
        if let (Some(p), Some((at, taken))) = (fx.burst(clock), fx.captured) {
            stage.burst(buf, app, taken, stage.spot(file_of(at) as f32, rank_of(at) as f32), p);
        }
        if let Some(p) = fx.sparkle(clock) {
            let (col, row) = stage.spot(file_of(fx.to) as f32, rank_of(fx.to) as f32);
            for (dx, dy) in (0..fx::SPARKS).filter_map(|k| fx::spark(k, p)) {
                stage.dot(buf, app, col + 0.5 + dx, row + 0.5 + dy, theme::SPARK);
            }
        }
    }
    if let Some(seconds) = app.party.and_then(|start| clock.checked_sub(start)) {
        let pieces = if stage.pixels { 7 * cell_w as u32 } else { 2 * cell_w as u32 + 12 };
        for (x, y, color) in (0..pieces).filter_map(|i| fx::confetti(i, seconds.as_secs_f32(), 8.0, 8.0)) {
            stage.dot(buf, app, x, y, theme::CONFETTI[color as usize % theme::CONFETTI.len()]);
        }
    }

    if left > 0 {
        let area = Rect::new(x0, by, LEFT_WIDTH, board_h);
        buf.set_stringn(area.x, area.y, "funchess", area.width as usize, bold().fg(TermColor::Cyan));
        let mode = match app.opponent {
            Opponent::Computer(_) => "against the computer",
            Opponent::Local => "two players",
            Opponent::Remote => "network game",
        };
        buf.set_stringn(area.x, area.y + 1, mode, area.width as usize, dim());
        moves(buf, Rect::new(area.x, area.y + 3, area.width, area.height.saturating_sub(3)), app);
    }
    // Beside the smallest board the panel also takes the row of the file letters: the
    // commands need six rows there, and the two players and the state of the game three.
    let panel_h = if cell_h == 1 { board_h + 1 } else { board_h };
    side_panel(buf, Rect::new(bx + board_w + GAP, by, RIGHT_WIDTH, panel_h), app, left == 0);

    // The result waits until the move that brought it has been shown.
    if let Some(prompt) = app.prompt.filter(|&p| !(p == Prompt::GameOver && app.fx.is_some())) {
        popup(buf, area, app, prompt);
    }
}

/// Where the board is on screen, for drawing over its squares: a piece between two of
/// them, or the bits of one. Places are in squares from the top left corner, with
/// fractions. Pixel art is drawn a pixel at a time; a board of symbols gets characters.
struct Stage {
    x: u16,
    y: u16,
    cell_w: u16,
    cell_h: u16,
    white_below: bool,
    pixels: bool,
    /// The color of each square as it was drawn, by row and column.
    grounds: [[Rgb; 8]; 8],
}

impl Stage {
    /// The column and row at which a file and rank are drawn.
    fn spot(&self, file: f32, rank: f32) -> (f32, f32) {
        if self.white_below { (file, 7.0 - rank) } else { (7.0 - file, rank) }
    }

    /// Colors one pixel of the board, which is `8 * cell_w` of them each way.
    fn put(&self, buf: &mut Buffer, app: &App, x: i32, y: i32, color: Rgb) {
        let side = 8 * self.cell_w as i32;
        if !(0..side).contains(&x) || !(0..side).contains(&y) {
            return;
        }
        if let Some(cell) = buf.cell_mut((self.x + x as u16, self.y + y as u16 / 2)) {
            if y % 2 == 0 {
                cell.set_fg(paint(app, color))
            } else {
                cell.set_bg(paint(app, color))
            };
        }
    }

    /// The character cell of a board of symbols that a place falls in, if it is on the board.
    fn cell<'a>(&self, buf: &'a mut Buffer, col: f32, row: f32) -> Option<&'a mut ratatui::buffer::Cell> {
        let (x, y) = ((col * self.cell_w as f32).floor() as i32, (row * self.cell_h as f32).floor() as i32);
        let inside = (0..8 * self.cell_w as i32).contains(&x) && (0..8 * self.cell_h as i32).contains(&y);
        inside.then(|| buf.cell_mut((self.x + x as u16, self.y + y as u16))).flatten()
    }

    /// A piece with its square's top left corner at this column and row.
    fn piece(&self, buf: &mut Buffer, app: &App, piece: Piece, col: f32, row: f32) {
        if !self.pixels {
            // Where `Cell::symbols` puts it, for a square that is somewhere between two.
            let middle = (col + (self.cell_w / 2) as f32 / self.cell_w as f32, row + ((self.cell_h - 1) / 2) as f32 / self.cell_h as f32);
            let ink = if piece.color == Color::White { app.theme().white } else { app.theme().black };
            if let Some(cell) = self.cell(buf, middle.0 + 0.5 / self.cell_w as f32, middle.1 + 0.5 / self.cell_h as f32) {
                cell.set_char(symbol(app, piece, true)).set_style(bold().fg(paint(app, ink)));
            }
            return;
        }
        let n = self.cell_w as i32;
        let sprite = Sprite::new(piece, n, Pose::Upright);
        let (x0, y0) = ((col * n as f32).round() as i32, (row * n as f32).round() as i32);
        for y in 0..n {
            for x in 0..n {
                let under = self.grounds[((y0 + y) / n).clamp(0, 7) as usize][((x0 + x) / n).clamp(0, 7) as usize];
                if let Some(color) = sprite.color(app, x, y, under) {
                    self.put(buf, app, x0 + x, y0 + y, color);
                }
            }
        }
    }

    /// One bit of something flying: a pixel on the smallest pixel art and four on any
    /// larger, or a star in an empty character of a board of symbols.
    fn dot(&self, buf: &mut Buffer, app: &App, col: f32, row: f32, color: Rgb) {
        if !self.pixels {
            if let Some(cell) = self.cell(buf, col, row).filter(|cell| cell.symbol() == " ") {
                cell.set_char('*').set_fg(paint(app, color));
            }
            return;
        }
        let n = self.cell_w as f32;
        let (x, y) = ((col * n).round() as i32, (row * n).round() as i32);
        let size = if self.cell_w >= 10 { 2 } else { 1 };
        for (dx, dy) in (0..size).flat_map(|dx| (0..size).map(move |dy| (dx, dy))) {
            self.put(buf, app, x + dx, y + dy, color);
        }
    }

    /// A captured piece flying apart from the square at `spot`, `p` of the way through.
    fn burst(&self, buf: &mut Buffer, app: &App, piece: Piece, spot: (f32, f32), p: f32) {
        if !self.pixels {
            let ink = if piece.color == Color::White { app.theme().white } else { app.theme().black };
            for (dx, dy) in (0..10).filter_map(|i| fx::fragment(i, p)) {
                self.dot(buf, app, spot.0 + 0.5 + dx * 1.5, spot.1 + 0.5 + dy * 1.5, ink);
            }
            return;
        }
        // Every pixel of the piece goes its own way.
        let n = self.cell_w as i32;
        let sprite = Sprite::new(piece, n, Pose::Upright);
        let (x0, y0) = ((spot.0 * n as f32).round() as i32, (spot.1 * n as f32).round() as i32);
        for y in 0..n {
            for x in 0..n {
                if let (false, Some((dx, dy))) = (sprite.clear(x, y), fx::fragment((y * n + x) as u32, p))
                    && let Some(color) = sprite.color(app, x, y, (0, 0, 0))
                {
                    self.put(buf, app, x0 + x + (dx * n as f32).round() as i32, y0 + y + (dy * n as f32).round() as i32, color);
                }
            }
        }
    }
}

/// How a piece stands in its square.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Pose {
    Upright,
    /// Pushed to one side by this many pixels of its art: a king shaking in check.
    Shifted(i32),
    /// On its way over, from 0 upright to 1 nearly flat.
    Leaning(f32),
    /// Flat on its side: a king that was mated.
    Fallen,
}

/// A piece's pixel art, fitted to a square `n` pixels each way. The art comes in three
/// sizes, each of which can be doubled or tripled; the largest that fits is centered.
struct Sprite {
    piece: Piece,
    rows: &'static [&'static str],
    size: i32,
    scale: i32,
    margin: i32,
    pose: Pose,
}

impl Sprite {
    fn new(piece: Piece, n: i32, pose: Pose) -> Sprite {
        let (size, scale) = [(12, 1), (10, 1), (8, 1), (12, 2), (10, 2), (8, 2), (12, 3), (10, 3), (8, 3)]
            .into_iter()
            .filter(|&(size, scale)| size * scale <= n)
            .max_by_key(|&(size, scale)| size * scale)
            .unwrap_or((8, 1));
        let rows: &[&str] = match size {
            12 => &LARGE[piece.kind as usize],
            10 => &MEDIUM[piece.kind as usize],
            _ => &SMALL[piece.kind as usize],
        };
        Sprite { piece, rows, size, scale, margin: (n - size * scale) / 2, pose }
    }

    /// What the art has at a pixel of the square: `#`, `+`, or `.` for nothing.
    fn at(&self, x: i32, y: i32) -> u8 {
        let full = self.size * self.scale;
        let (mut x, mut y) = (x - self.margin, y - self.margin);
        match self.pose {
            Pose::Upright => {}
            Pose::Shifted(by) => x -= by * self.scale,
            // The higher up, the further over.
            Pose::Leaning(t) => x -= (t * 0.75 * (full - 1 - y) as f32).round() as i32,
            // A quarter turn, the top of the piece to the right.
            Pose::Fallen => (x, y) = (y, full - 1 - x),
        }
        let (ax, ay) = (x.div_euclid(self.scale), y.div_euclid(self.scale));
        if (0..self.size).contains(&ax) && (0..self.size).contains(&ay) { self.rows[ay as usize].as_bytes()[ax as usize] } else { b'.' }
    }

    fn clear(&self, x: i32, y: i32) -> bool {
        self.at(x, y) == b'.'
    }

    /// The color of a pixel of the square where the piece is, or its rim. Elsewhere
    /// nothing, and the square shows. `ground` is the color the rim is seen against.
    fn color(&self, app: &App, x: i32, y: i32, ground: Rgb) -> Option<Rgb> {
        let theme = app.theme();
        let white = self.piece.color == Color::White;
        let clear = |x: i32, y: i32| self.clear(x, y);
        match self.at(x, y) {
            b'+' => Some(if white { theme.white_shade } else { theme.black_shade }),
            b'#' if app.pieces == Pieces::Shaded => {
                // Lit from the top left: edges facing the light are bright, the
                // opposite edges dark, and the rest in between.
                let (bright, middle, dim) = if white {
                    (theme.white, mix(theme.white, theme.white_shade, 0.4), theme.white_shade)
                } else {
                    (theme.black_shade, mix(theme.black, theme.black_shade, 0.35), theme.black)
                };
                Some(if clear(x + 1, y) || clear(x, y + 1) {
                    dim
                } else if clear(x - 1, y) || clear(x, y - 1) {
                    bright
                } else {
                    middle
                })
            }
            b'#' => Some(if white { theme.white } else { theme.black }),
            _ if clear(x + 1, y) && clear(x - 1, y) && clear(x, y + 1) && clear(x, y - 1) => None,
            _ if app.pieces == Pieces::Outlined => Some(if white { theme.black } else { mix(ground, theme.white, 0.8) }),
            // White pieces get a soft dark rim so they stand out on a light square.
            _ if white => Some(mix(ground, (0, 0, 0), 0.5)),
            _ => None,
        }
    }
}

struct Cell {
    x: u16,
    y: u16,
    w: u16,
    h: u16,
    ground: Rgb,
    piece: Option<Piece>,
    pose: Pose,
    target: bool,
    cursor: bool,
    /// Framed as one of the two squares of a hint, the frame this bright (0 to 1).
    hint: Option<f32>,
}

impl Cell {
    fn draw(&self, buf: &mut Buffer, app: &App) {
        if self.h >= 4 && app.pieces.is_pixel_art() { self.pixels(buf, app) } else { self.symbols(buf, app) }
    }

    /// Pixel art: every character is two pixels, the upper drawn as the foreground
    /// of a half block and the lower as its background.
    fn pixels(&self, buf: &mut Buffer, app: &App) {
        // The square is `n` pixels each way.
        let n = self.w as i32;
        let theme = app.theme();
        let sprite = self.piece.map(|piece| Sprite::new(piece, n, self.pose));
        let color = |x: i32, y: i32| -> Rgb {
            // The marks go on the square, never over the piece.
            if sprite.as_ref().is_none_or(|s| s.clear(x, y)) {
                let inset = x.min(y).min(n - 1 - x).min(n - 1 - y);
                let middle = |v: i32| (v - n / 2).abs().min((v - n / 2 + 1).abs()) < n / 6;
                if self.cursor && inset == 0 {
                    return theme.cursor;
                } else if let Some(bright) = self.hint.filter(|_| inset <= (n / 8).max(1)) {
                    return mix(self.ground, theme::HINT, 0.5 + 0.5 * bright);
                } else if self.target && (if self.piece.is_some() { inset == 0 } else { middle(x) && middle(y) }) {
                    return theme.target;
                }
            }
            sprite.as_ref().and_then(|s| s.color(app, x, y, self.ground)).unwrap_or(self.ground)
        };
        for cy in 0..self.h {
            for cx in 0..self.w {
                let (top, bottom) = (color(cx as i32, cy as i32 * 2), color(cx as i32, cy as i32 * 2 + 1));
                if let Some(cell) = buf.cell_mut((self.x + cx, self.y + cy)) {
                    cell.set_char('▀').set_fg(paint(app, top)).set_bg(paint(app, bottom));
                }
            }
        }
    }

    /// One chess symbol in the middle of the square.
    fn symbols(&self, buf: &mut Buffer, app: &App) {
        let ground = if self.target { mix(self.ground, app.theme().target, 0.6) } else { self.ground };
        let ground = self.hint.map_or(ground, |bright| mix(ground, theme::HINT, 0.35 + 0.4 * bright));
        let style = Style::new().bg(paint(app, ground));
        for cy in 0..self.h {
            for cx in 0..self.w {
                if let Some(cell) = buf.cell_mut((self.x + cx, self.y + cy)) {
                    cell.set_char(' ').set_style(style);
                }
            }
        }
        let (mid_x, mid_y) = (self.x + self.w / 2, self.y + (self.h - 1) / 2);
        if let Some(piece) = self.piece {
            let ink = if piece.color == Color::White { app.theme().white } else { app.theme().black };
            if let Some(cell) = buf.cell_mut((mid_x, mid_y)) {
                cell.set_char(symbol(app, piece, true)).set_style(style.fg(paint(app, ink)).add_modifier(Modifier::BOLD));
            }
        }
        if self.cursor {
            let mark = style.fg(paint(app, app.theme().cursor)).add_modifier(Modifier::BOLD);
            for (x, c) in [(self.x, '['), (self.x + self.w - 1, ']')] {
                if let Some(cell) = buf.cell_mut((x, mid_y)) {
                    cell.set_char(c).set_style(mark);
                }
            }
        }
    }
}

/// The pieces of one color that are no longer on the board, most valuable first.
fn lost(board: &Board, color: Color) -> Vec<Piece> {
    let mut out = Vec::new();
    for (kind, start) in [(Kind::Queen, 1usize), (Kind::Rook, 2), (Kind::Bishop, 2), (Kind::Knight, 2), (Kind::Pawn, 8)] {
        let piece = Piece { color, kind };
        let now = board.squares.iter().filter(|&&p| p == Some(piece)).count();
        out.extend(std::iter::repeat_n(piece, start.saturating_sub(now)));
    }
    out
}

fn material(board: &Board, color: Color) -> i32 {
    let worth = |k| match k {
        Kind::Pawn => 1,
        Kind::Knight | Kind::Bishop => 3,
        Kind::Rook => 5,
        Kind::Queen => 9,
        Kind::King => 0,
    };
    board.squares.iter().flatten().filter(|p| p.color == color).map(|p| worth(p.kind)).sum()
}

/// The moves so far, two to a line, the latest at the bottom.
fn moves(buf: &mut Buffer, area: Rect, app: &App) {
    let pairs: Vec<String> = app
        .game
        .history
        .chunks(2)
        .enumerate()
        .map(|(i, pair)| format!("{:>3}. {:<8} {}", i + 1, pair[0].san, pair.get(1).map_or("", |p| p.san.as_str())))
        .collect();
    for (i, text) in pairs.iter().skip(pairs.len().saturating_sub(area.height as usize)).enumerate() {
        buf.set_stringn(area.x, area.y + i as u16, text, area.width as usize, Style::new());
    }
}

/// Every command, with its key. Each is drawn as a button that can be clicked.
const COMMANDS: [(&str, &str, char, bool); 12] = [
    ("?", "Help", '?', false),
    ("^G", "Hint", 'g', true),
    ("^U", "Undo", 'u', true),
    ("^N", "New game", 'n', true),
    ("^F", "Flip board", 'f', true),
    ("^R", "Resign", 'r', true),
    ("^D", "Offer draw", 'd', true),
    ("^T", "Theme", 't', true),
    ("^P", "Pieces", 'p', true),
    ("^S", "Sound", 's', true),
    ("^A", "Animations", 'a', true),
    ("^Q", "Menu", 'q', true),
];

/// Right of the board: each player beside their own side of it, and between them
/// what is going on, the moves if they are not shown on the left, and the commands.
fn side_panel(buf: &mut Buffer, area: Rect, app: &mut App, with_moves: bool) {
    let board = app.game.board;
    // Small boards get one line per player and no blank lines; large ones get room,
    // and those with pixel art get the captured pieces on trays and the computer's face.
    let (compact, roomy) = (area.height < 16, area.height >= 24);
    let rich = app.pieces.is_pixel_art() && area.height >= 32;
    let rows = |color: Color| -> u16 {
        let computer = matches!(app.opponent, Opponent::Computer(_)) && color != app.me;
        match (compact, rich) {
            (true, _) => 1,
            (false, false) => 2,
            (false, true) if computer => PLAYER_ROWS_WITH_FACE,
            (false, true) => PLAYER_ROWS_WITH_TRAY,
        }
    };
    let status = match app.game.outcome {
        Some(outcome) => outcome.describe(),
        None if app.is_thinking() => "The computer is thinking...".into(),
        None if board.in_check(board.turn) => format!("{} is in check", board.turn.name()),
        None => format!("{} to move", board.turn.name()),
    };
    let status_style = if app.over() { bold().fg(TermColor::Yellow) } else { Style::new() };
    let note = Style::new().fg(TermColor::Yellow);

    // From the top: the player at the far side, then the state of the game.
    let (far, near) = (app.bottom().other(), app.bottom());
    let mut y = area.y;
    player(buf, Rect::new(area.x, y, area.width, rows(far)), app, far);
    y += rows(far);
    y += u16::from(roomy);
    if compact && !app.message.is_empty() && !app.over() {
        buf.set_stringn(area.x, y, &app.message, area.width as usize, note);
        y += 1;
    } else {
        buf.set_stringn(area.x, y, &status, area.width as usize, status_style);
        y += 1;
        if !compact {
            let rows = if roomy { 2 } else { 1 };
            Paragraph::new(app.message.as_str()).wrap(Wrap { trim: true }).style(note).render(Rect::new(area.x, y, area.width, rows), buf);
            y += rows;
        }
    }
    y += u16::from(roomy);

    // From the bottom: the player at the near side, then the commands above them.
    let mut bottom = area.bottom() - rows(near);
    player(buf, Rect::new(area.x, bottom, area.width, rows(near)), app, near);
    bottom -= u16::from(roomy);
    let columns: u16 = if area.height >= 40 { 1 } else { 2 };
    let rows = (COMMANDS.len() as u16).div_ceil(columns);
    let column_width = area.width / columns;
    bottom -= rows;
    for (i, (key, name, letter, ctrl)) in COMMANDS.into_iter().enumerate() {
        let (x, row) = (area.x + (i as u16 / rows) * column_width, bottom + i as u16 % rows);
        buf.set_string(x, row, key, Style::new().fg(TermColor::Cyan));
        buf.set_string(x + 3, row, name, Style::new());
        if app.prompt.is_none() {
            app.buttons.push((Rect::new(x, row, 3 + name.len() as u16, 1), Click::Key(KeyCode::Char(letter), ctrl)));
        }
    }
    bottom -= u16::from(roomy);

    if with_moves && bottom > y {
        moves(buf, Rect::new(area.x, y, area.width, bottom - y), app);
    }
}

/// The rows a player takes in the panel with a tray, and with the computer's face too.
const PLAYER_ROWS_WITH_TRAY: u16 = 4;
const PLAYER_ROWS_WITH_FACE: u16 = 10;

/// One player's part of the panel, as detailed as it has rows: the name; from two
/// rows, what they have captured, as symbols; from four, as small pieces on a tray;
/// and from ten, for the computer, its face and what it is saying.
fn player(buf: &mut Buffer, area: Rect, app: &App, color: Color) {
    let board = app.game.board;
    let who = match app.opponent {
        Opponent::Local => String::new(),
        Opponent::Computer(_) | Opponent::Remote if color == app.me => "You".into(),
        Opponent::Computer(level) => format!("{} ({})", character(level), level.name()),
        Opponent::Remote => "Opponent".into(),
    };
    let to_move = board.turn == color && !app.over();
    let taken = lost(&board, color.other());
    let lead = material(&board, color) - material(&board, color.other());
    let lead = if lead > 0 { format!("+{lead}") } else { String::new() };
    let with_tray = area.height >= PLAYER_ROWS_WITH_TRAY;
    let name = format!("{} {:<6} {who}{}", if to_move { ">" } else { " " }, color.name(), if with_tray { format!("  {lead}") } else { String::new() });
    buf.set_stringn(area.x, area.y, name.trim_end(), area.width as usize, if to_move { bold().fg(TermColor::Cyan) } else { bold() });
    if !with_tray {
        if area.height >= 2 {
            let taken: String = taken.into_iter().map(|p| symbol(app, p, false)).flat_map(|c| [c, ' ']).collect();
            buf.set_stringn(area.x, area.y + 1, format!("  {taken}{lead}"), area.width as usize, Style::new());
        }
        return;
    }
    let mut y = area.y + 1;
    if let Opponent::Computer(level) = app.opponent
        && color != app.me
        && area.height >= PLAYER_ROWS_WITH_FACE
    {
        let (mood, says) = mood(app);
        face(buf, app, area.x + 2, y, level, mood);
        Paragraph::new(says).wrap(Wrap { trim: true }).render(Rect::new(area.x + 16, y + 1, area.width.saturating_sub(16), 4), buf);
        y += 6;
    }
    tray(buf, app, Rect::new(area.x, y, area.width, 3), &taken);
}

fn popup(buf: &mut Buffer, area: Rect, app: &mut App, prompt: Prompt) {
    let turn = app.game.board.turn;
    let local = app.opponent == Opponent::Local;
    let (title, lines): (&str, Vec<String>) = match prompt {
        Prompt::Promotion { .. } => {
            ("Promote the pawn to", vec!["q  Queen".into(), "r  Rook".into(), "b  Bishop".into(), "n  Knight".into(), String::new(), "Esc  cancel".into()])
        }
        Prompt::Resign if local => ("Resign", vec![format!("{} resigns?", turn.name()), String::new(), "y  yes     n  no".into()]),
        Prompt::Resign => ("Resign", vec!["Give up this game?".into(), String::new(), "y  yes     n  no".into()]),
        Prompt::DrawOffered if local => {
            ("Draw", vec![format!("{} offers a draw.", turn.name()), format!("Does {} accept?", turn.other().name()), String::new(), "y  yes     n  no".into()])
        }
        Prompt::DrawOffered => ("Draw", vec!["Your opponent offers a draw.".into(), "Accept?".into(), String::new(), "y  yes     n  no".into()]),
        Prompt::Leave => ("Leave", vec!["Leave this game unfinished?".into(), String::new(), "y  yes     n  no".into()]),
        Prompt::NewGame => ("New game", vec!["Abandon this game and start again?".into(), String::new(), "y  yes     n  no".into()]),
        Prompt::GameOver => ("Game over", game_over(app, area.height.saturating_sub(2) as usize)),
        Prompt::Help => (
            "Help",
            [
                "Move a piece: pick it, then pick where it goes.",
                "",
                "  Mouse        click the piece, click the square",
                "  Arrow keys   move the blue marker; Enter picks",
                "  Typing       the two squares, as in  e2 e4",
                "  Esc          put the piece back",
                "",
                "  Ctrl-G  show a good move (a hint)",
                "  Ctrl-U  take back a move",
                "  Ctrl-R  resign",
                "  Ctrl-D  offer a draw",
                "  Ctrl-N  new game (rematch in a network game)",
                "  Ctrl-F  flip the board (Tab works too)",
                "  Ctrl-T  next color theme",
                "  Ctrl-P  next way of drawing the pieces",
                "  Ctrl-S  sound on or off",
                "  Ctrl-A  animations on or off",
                "  Ctrl-Q  back to the menu",
                "  Ctrl-C  quit at once",
                "",
                "Press any key, or click",
            ]
            .map(String::from)
            .to_vec(),
        ),
    };
    let width = (lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as u16 + 4).min(area.width);
    let height = (lines.len() as u16 + 2).min(area.height);
    let rect = Rect::new(area.x + (area.width - width) / 2, area.y + (area.height - height) / 2, width, height);
    Clear.render(rect, buf);
    // Clicks: anywhere on the help closes it; elsewhere each answer is its own button.
    const ANSWERS: &str = "y  yes     n  no";
    let key = |c| Click::Key(KeyCode::Char(c), false);
    for (i, line) in lines.iter().enumerate() {
        let (x, y) = (rect.x + 2, rect.y + 1 + i as u16);
        let whole = Rect::new(x, y, width.saturating_sub(4), 1);
        match (prompt, line.as_str()) {
            (Prompt::Help, _) => app.buttons.push((rect, Click::Key(KeyCode::Esc, false))),
            (_, ANSWERS) => app.buttons.extend([(Rect::new(x, y, 6, 1), key('y')), (Rect::new(x + 11, y, 5, 1), key('n'))]),
            (Prompt::GameOver, text) => {
                let click = match text.split_once("  ") {
                    Some(("Enter", _)) => Some(Click::Key(KeyCode::Enter, false)),
                    Some((name, _)) => {
                        name.strip_prefix("Ctrl-").and_then(|c| c.chars().next()).map(|c| Click::Key(KeyCode::Char(c.to_ascii_lowercase()), true))
                    }
                    None => None,
                };
                if let Some(click) = click.filter(|_| y < rect.bottom().saturating_sub(1)) {
                    app.buttons.push((whole, click));
                }
            }
            (Prompt::Promotion { .. }, "") => {}
            (Prompt::Promotion { .. }, text) if text.starts_with("Esc") => app.buttons.push((whole, Click::Key(KeyCode::Esc, false))),
            (Prompt::Promotion { .. }, text) => app.buttons.push((whole, key(text.chars().next().unwrap_or('q')))),
            _ => {}
        }
    }
    let block = Block::bordered().title(format!(" {title} ")).border_style(Style::new().fg(TermColor::Cyan)).padding(ratatui::widgets::Padding::horizontal(1));
    let mut lines: Vec<Line> = lines.into_iter().map(Line::raw).collect();
    if prompt == Prompt::GameOver
        && let Some(first) = lines.first_mut()
    {
        *first = std::mem::take(first).style(bold());
    }
    Paragraph::new(lines).block(block).render(rect, buf);
}

/// What the box shown at the end of a game says: who won and why, a few facts about
/// the game, and what can be done next. With fewer than `rows` to fill, the facts go
/// first and the result and the commands stay.
fn game_over(app: &App, rows: usize) -> Vec<String> {
    let Some(outcome) = app.game.outcome else { return Vec::new() };
    let local = app.opponent == Opponent::Local;
    let winner = outcome.winner();
    // Beating the computer earns stars, and the first one missing says how to get it.
    let stars = app.stars();
    let (full, empty) = if app.pieces == Pieces::Letters { ("*", "-") } else { ("★", "☆") };
    let headline = match winner {
        None => "Draw".to_string(),
        Some(c) if local => format!("{} wins", c.name()),
        Some(c) if c == app.me => format!("You win!  {}", stars.unwrap_or_default().map(|won| if won { full } else { empty }).join(" ")).trim_end().to_string(),
        Some(_) => "You lose".to_string(),
    };
    let next_star = stars.map(|stars| match stars {
        [_, false, _] => "Next star: no hints or take-backs".to_string(),
        [_, _, false] => format!("Next star: win in {QUICK_WIN} moves or fewer"),
        _ => "All three stars!".to_string(),
    });
    let reason = match outcome {
        Outcome::Checkmate(_) => "Checkmate".to_string(),
        Outcome::Resigned(c) if local => format!("{} resigned", c.other().name()),
        Outcome::Resigned(c) if c == app.me => "Your opponent resigned".to_string(),
        Outcome::Resigned(_) => "You resigned".to_string(),
        Outcome::Abandoned(_) => "Your opponent left the game".to_string(),
        Outcome::Stalemate => format!("Stalemate: {} cannot move", app.game.board.turn.name()),
        Outcome::Repetition => "The same position, three times".to_string(),
        Outcome::FiftyMoves => "Fifty moves, no capture or pawn move".to_string(),
        Outcome::InsufficientMaterial => "Too few pieces left to mate".to_string(),
        Outcome::DrawAgreed => "Agreed by both players".to_string(),
    };
    let score = match winner {
        Some(Color::White) => "1-0",
        Some(Color::Black) => "0-1",
        None => "½-½",
    };
    let played = match app.game.history.last() {
        None => format!("{score} before a move was played"),
        Some(last) => {
            let moves = app.game.history.len().div_ceil(2);
            format!("{score} after {moves} move{} ({})", if moves == 1 { "" } else { "s" }, last.san)
        }
    };
    let ahead: i32 = (0..64)
        .filter_map(|s| app.game.board.at(s))
        .map(|p| {
            let worth = [1, 3, 3, 5, 9, 0][p.kind as usize];
            if p.color == Color::White { worth } else { -worth }
        })
        .sum();
    let material = match ahead {
        0 => "Material: even".to_string(),
        n => format!("Material: {} +{}", if n > 0 { Color::White } else { Color::Black }.name(), n.abs()),
    };

    // Each line with how soon it goes when the window is short: 0 never.
    let mut lines = vec![(0, headline), (0, reason)];
    // Whatever is being said beside the board, which this box may cover: why the
    // connection closed, or that the other player wants a rematch.
    if !app.message.is_empty() {
        lines.push((1, app.message.clone()));
    }
    lines.extend([(5, String::new()), (2, played), (3, material)]);
    if let Opponent::Computer(level) = app.opponent {
        lines.push((4, format!("Computer level: {}", level.name())));
    }
    lines.extend(next_star.map(|line| (3, line)));
    lines.push((5, String::new()));
    match app.opponent {
        Opponent::Remote if app.connected() => lines.push((0, "Ctrl-N  rematch, colors swapped".into())),
        Opponent::Remote => {}
        _ => lines.extend([(0, "Ctrl-N  new game".into()), (0, "Ctrl-U  take the last move back".into())]),
    }
    lines.extend([(0, "Ctrl-Q  back to the menu".into()), (0, "Enter   look at the board".into())]);
    for drop in (1..=5).rev() {
        if lines.len() > rows {
            lines.retain(|&(when, _)| when != drop);
        }
    }
    lines.into_iter().map(|(_, line)| line).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Settings, THEMES};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    /// The program with nothing taking time to be shown: a piece is where it went at once.
    fn still() -> App {
        App::new(true, Settings { animations: false, ..Settings::default() })
    }

    fn render(app: &mut App, width: u16, height: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        terminal.backend().buffer().clone()
    }

    fn text(buf: &Buffer) -> String {
        let area = buf.area;
        (0..area.height).map(|y| (0..area.width).map(|x| buf[(x, y)].symbol()).collect::<String>() + "\n").collect()
    }

    /// Where a square was drawn: the inverse of `App::square_at`.
    fn cell_origin(app: &App, square: crate::chess::Square) -> (u16, u16) {
        let g = app.geometry.unwrap();
        let (col, row) = if app.bottom() == Color::White { (file_of(square), 7 - rank_of(square)) } else { (7 - file_of(square), rank_of(square)) };
        (g.x + col as u16 * g.cell_w, g.y + row as u16 * g.cell_h)
    }

    fn keys(app: &mut App, typed: &str) {
        for c in typed.chars() {
            app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
    }

    #[test]
    fn the_art_is_the_right_shape() {
        for piece in SMALL {
            assert!(piece.iter().all(|row| row.len() == 8), "{piece:?}");
        }
        for piece in MEDIUM {
            assert!(piece.iter().all(|row| row.len() == 10), "{piece:?}");
        }
        for piece in LARGE {
            assert!(piece.iter().all(|row| row.len() == 12), "{piece:?}");
        }
    }

    #[test]
    fn every_window_size_draws_without_spilling() {
        for (width, height) in [(20, 5), (58, 9), (60, 12), (80, 24), (92, 28), (100, 36), (132, 52), (209, 48), (200, 60), (400, 130), (59, 40), (300, 11)] {
            let mut app = still();
            render(&mut app, width, height);
            app.start_local();
            keys(&mut app, "e2e4e7e5g1");
            render(&mut app, width, height);
            for prompt in
                [Prompt::Help, Prompt::Resign, Prompt::DrawOffered, Prompt::Leave, Prompt::NewGame, Prompt::GameOver, Prompt::Promotion { from: 0, to: 0 }]
            {
                app.prompt = Some(prompt);
                render(&mut app, width, height);
            }
        }
    }

    #[test]
    fn the_board_grows_with_the_window() {
        let mut app = still();
        app.start_local();
        for (width, height, cell) in [
            (58, 9, (3, 1)),
            (80, 24, (5, 2)),
            (92, 28, (7, 3)),
            (100, 36, (8, 4)),
            (209, 48, (10, 5)),
            (132, 52, (12, 6)),
            (300, 80, (18, 9)),
            (400, 120, (24, 12)),
            (80, 60, (5, 2)),
        ] {
            render(&mut app, width, height);
            let g = app.geometry.unwrap();
            assert_eq!((g.cell_w, g.cell_h), cell, "{width}x{height}");
        }
        render(&mut app, 57, 12);
        assert!(app.geometry.is_none());
        render(&mut app, 80, 8);
        assert!(app.geometry.is_none());
        // Symbols need an odd width to sit in the middle of a tall square.
        app.pieces = Pieces::Symbols;
        render(&mut app, 209, 48);
        assert_eq!(app.geometry.map(|g| (g.cell_w, g.cell_h)), Some((11, 5)));
    }

    #[test]
    fn pieces_sit_where_the_position_says() {
        let mut app = still();
        app.start_local();
        let buf = render(&mut app, 80, 24);
        let symbol_on = |app: &App, buf: &Buffer, name: &str| {
            let (x, y) = cell_origin(app, crate::chess::parse_square(name).unwrap());
            buf[(x + 2, y)].symbol().to_string()
        };
        assert_eq!(symbol_on(&app, &buf, "e1"), "♚");
        assert_eq!(symbol_on(&app, &buf, "d8"), "♛");
        assert_eq!(symbol_on(&app, &buf, "e4"), " ");
        let shown = text(&buf);
        assert!(shown.contains("White to move") && shown.contains("^Q Menu"), "{shown}");

        keys(&mut app, "e2e4");
        let buf = render(&mut app, 80, 24);
        assert_eq!(symbol_on(&app, &buf, "e4"), "♟");
        assert!(text(&buf).contains("1. e4"));
        // Black is to move in a two-player game: Black's king is now on the bottom row.
        let g = app.geometry.unwrap();
        assert_eq!(cell_origin(&app, sq(4, 7)).1, g.y + 7 * g.cell_h);
        // Flipped by hand, it is back at the top.
        app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        render(&mut app, 80, 24);
        assert_eq!(cell_origin(&app, sq(4, 7)).1, app.geometry.unwrap().y);
        app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

        app.pieces = Pieces::Letters;
        let buf = render(&mut app, 80, 24);
        assert_eq!(symbol_on(&app, &buf, "e8"), "k");
        assert_eq!(symbol_on(&app, &buf, "e1"), "K");
    }

    #[test]
    fn pixel_art_pieces_are_drawn_in_their_own_colors() {
        let mut app = still();
        app.start_local();
        let buf = render(&mut app, 100, 36);
        let colors_in = |name: &str| {
            let (x0, y0) = cell_origin(&app, crate::chess::parse_square(name).unwrap());
            let mut seen = Vec::new();
            for y in y0..y0 + 4 {
                for x in x0..x0 + 8 {
                    seen.extend([buf[(x, y)].fg, buf[(x, y)].bg]);
                }
            }
            seen
        };
        let rgb = |c: Rgb| TermColor::Rgb(c.0, c.1, c.2);
        assert!(colors_in("a1").contains(&rgb(THEMES[0].white)) && !colors_in("a1").contains(&rgb(THEMES[0].black)));
        assert!(colors_in("a8").contains(&rgb(THEMES[0].black)) && !colors_in("a8").contains(&rgb(THEMES[0].white)));
        assert!(colors_in("a4").iter().all(|&c| c == rgb(THEMES[0].light)));
        assert!(colors_in("b4").iter().all(|&c| c == rgb(THEMES[0].dark)));
        // The marker starts on e2 and frames that square.
        assert!(colors_in("e2").contains(&rgb(THEMES[0].cursor)));
    }

    #[test]
    fn without_full_color_only_the_256_palette_is_used() {
        let mut app = App::new(false, Settings::default());
        app.start_local();
        let buf = render(&mut app, 100, 36);
        assert!(buf.content.iter().all(|c| !matches!(c.fg, TermColor::Rgb(..)) && !matches!(c.bg, TermColor::Rgb(..))));
        assert_eq!(nearest_256((255, 255, 255)), TermColor::Indexed(231));
        assert_eq!(nearest_256((0, 0, 0)), TermColor::Indexed(16));
        assert_eq!(nearest_256((128, 128, 128)), TermColor::Indexed(244));
    }

    fn click(app: &mut App, column: u16, row: u16) {
        use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
        app.on_mouse(MouseEvent { kind: MouseEventKind::Down(MouseButton::Left), column, row, modifiers: KeyModifiers::NONE });
    }

    /// Where some text is on screen: the column and row of its first character.
    fn find(buf: &Buffer, needle: &str) -> (u16, u16) {
        for (row, line) in text(buf).lines().enumerate() {
            if let Some(byte) = line.find(needle) {
                return (line[..byte].chars().count() as u16, row as u16);
            }
        }
        panic!("{needle:?} is not on screen:\n{}", text(buf));
    }

    /// A short window drops the empty rows, then the title and the keys, before any choice.
    #[test]
    fn the_menu_fits_a_small_window() {
        let mut app = still();
        app.menu_error = "Could not connect".into();
        let shown = text(&render(&mut app, 39, 12));
        for part in ["funchess", "1  Play the computer", "4  Join a game", "P  Pieces", "Q  Quit", "Could not connect", "Enter start   ←→ change"] {
            assert!(shown.contains(part), "{part} is missing from\n{shown}");
        }
        // Every choice is still there, and can be clicked, in the shortest window a game fits in.
        let buf = render(&mut app, 39, 9);
        let shown = text(&buf);
        assert!(shown.contains("1  Play the computer") && shown.contains("Q  Quit"), "{shown}");
        let (x, y) = find(&buf, "Q  Quit");
        click(&mut app, x, y);
        assert!(app.quit);
        // With room to spare nothing changes.
        let mut app = still();
        let shown = text(&render(&mut app, 80, 24));
        assert!(shown.contains("Up/Down choose   Enter start   Left/Right change"), "{shown}");
    }

    #[test]
    fn the_result_box_says_who_won_and_why() {
        use crate::chess::Outcome;
        use crate::engine::Level;
        let mut app = still();
        app.start_local();
        keys(&mut app, "f2f3e7e5g2g4d8h4");
        let shown = text(&render(&mut app, 80, 24));
        for part in ["Black wins", "Checkmate", "0-1 after 2 moves (Qh4#)", "Material: even", "Ctrl-U  take the last move back"] {
            assert!(shown.contains(part), "{part} is missing from\n{shown}");
        }
        // In the shortest window a game fits in, the facts go and the rest stays clickable.
        let buf = render(&mut app, 62, 9);
        let shown = text(&buf);
        for part in ["Black wins", "Checkmate", "Ctrl-N  new game", "Ctrl-Q  back to the menu", "Enter   look at the board"] {
            assert!(shown.contains(part), "{part} is missing from\n{shown}");
        }
        assert!(!shown.contains("Material"), "{shown}");
        let (x, y) = find(&buf, "Ctrl-N  new game");
        click(&mut app, x, y);
        assert!(app.prompt.is_none() && app.game.history.is_empty());

        // Against the computer it is about the player, and a capture shows in the material.
        app.opponent = Opponent::Computer(Level::Easy);
        keys(&mut app, "e2e4");
        app.game.board = Board::from_fen("4k3/8/8/8/8/8/8/4K2R b - - 0 30").unwrap();
        for (outcome, parts) in [
            (Outcome::Resigned(Color::White), ["You win!", "Your opponent resigned", "Material: White +5", "Computer level: Easy"]),
            (Outcome::Checkmate(Color::Black), ["You lose", "Checkmate", "0-1 after 1 move (e4)", "Ctrl-N  new game"]),
            (Outcome::Stalemate, ["Draw", "Stalemate: Black cannot move", "½-½ after 1 move (e4)", "Ctrl-Q"]),
        ] {
            app.game.outcome = Some(outcome);
            app.prompt = Some(Prompt::GameOver);
            let shown = text(&render(&mut app, 80, 24));
            for part in parts {
                assert!(shown.contains(part), "{part} is missing from\n{shown}");
            }
        }
    }

    #[test]
    fn menu_lines_can_be_clicked() {
        let mut app = still();
        let buf = render(&mut app, 80, 24);
        assert!(text(&buf).contains("> 1  Play the computer"));

        // A setting steps forward when its line is clicked and back from its "<".
        let (x, y) = find(&buf, "Theme");
        click(&mut app, x, y);
        assert_eq!((app.theme, app.menu_item), (1, 6));
        let (x, y) = find(&buf, "< forest");
        click(&mut app, x, y);
        click(&mut app, x, y);
        assert_eq!(app.theme, THEMES.len() - 1);
        let (_, y) = find(&buf, "Computer level");
        click(&mut app, 0, y);
        assert_eq!(app.level, crate::engine::Level::Easy, "a click beside the line does nothing");

        let (x, y) = find(&buf, "Two players");
        click(&mut app, x, y);
        assert!(matches!(app.screen, Screen::Game) && app.opponent == Opponent::Local);
    }

    #[test]
    fn commands_and_answers_can_be_clicked() {
        let mut app = still();
        app.start_local();
        keys(&mut app, "e2e4");
        let buf = render(&mut app, 80, 24);
        let ((ux, uy), (rx, ry)) = (find(&buf, "^U Undo"), find(&buf, "^R Resign"));
        click(&mut app, ux + 3, uy);
        assert!(app.game.history.is_empty());

        keys(&mut app, "e2e4");
        click(&mut app, rx, ry);
        assert_eq!(app.prompt, Some(Prompt::Resign));
        let buf = render(&mut app, 80, 24);
        // With a question open the commands are not clickable, the answers are.
        click(&mut app, ux, uy);
        assert_eq!((app.prompt, app.game.history.len()), (Some(Prompt::Resign), 1));
        let (x, y) = find(&buf, "n  no");
        click(&mut app, x, y);
        assert_eq!(app.prompt, None);
        render(&mut app, 80, 24);
        click(&mut app, rx, ry);
        let buf = render(&mut app, 80, 24);
        let (x, y) = find(&buf, "y  yes");
        click(&mut app, x + 5, y);
        assert_eq!(app.game.outcome, Some(crate::chess::Outcome::Resigned(Color::White)));

        // The result is announced in a box whose lines are the only buttons.
        let buf = render(&mut app, 80, 24);
        let shown = text(&buf);
        for part in ["Game over", "White wins", "Black resigned", "1-0 after 1 move (e4)", "Material: even", "Ctrl-N  new game", "Ctrl-Q  back to the menu"] {
            assert!(shown.contains(part), "{part} is missing from\n{shown}");
        }
        assert_eq!(app.buttons.len(), 4);
        click(&mut app, 1, 1);
        assert_eq!(app.prompt, Some(Prompt::GameOver));
        let (x, y) = find(&buf, "Enter   look at the board");
        click(&mut app, x, y);
        assert!(app.prompt.is_none() && app.over());

        // Every command fits somewhere, and the help closes on a click.
        render(&mut app, 132, 52);
        assert_eq!(app.buttons.len(), 12);
        let buf = render(&mut app, 60, 12);
        assert!(text(&buf).contains("^D Offer draw"));
        click(&mut app, find(&buf, "?  Help").0, find(&buf, "?  Help").1);
        assert_eq!(app.prompt, Some(Prompt::Help));
        render(&mut app, 60, 12);
        click(&mut app, 30, 6);
        assert_eq!(app.prompt, None);
    }

    #[test]
    fn promotion_pieces_can_be_clicked() {
        let mut app = still();
        app.start_local();
        app.game.board = Board::from_fen("7k/P7/8/8/8/8/8/K7 w - - 0 1").unwrap();
        keys(&mut app, "a7a8");
        let buf = render(&mut app, 80, 24);
        let (x, y) = find(&buf, "r  Rook");
        click(&mut app, x + 4, y);
        assert_eq!(app.game.history.last().unwrap().san, "a8=R+");
    }

    #[test]
    fn the_other_screens_have_clickable_ways_out() {
        let mut app = still();
        app.screen = Screen::Join;
        let buf = render(&mut app, 80, 24);
        let (x, y) = find(&buf, "Esc back");
        click(&mut app, x, y);
        assert!(matches!(app.screen, Screen::Menu));

        app.screen = Screen::Connecting(vec!["Connecting...".into()]);
        let buf = render(&mut app, 80, 24);
        let (x, y) = find(&buf, "Esc cancel");
        click(&mut app, x, y);
        assert!(matches!(app.screen, Screen::Menu));
    }

    /// The colors in one square of a board of pixel art.
    fn colors_of(app: &App, buf: &Buffer, name: &str) -> Vec<TermColor> {
        let (g, (x0, y0)) = (app.geometry.unwrap(), cell_origin(app, crate::chess::parse_square(name).unwrap()));
        (y0..y0 + g.cell_h).flat_map(|y| (x0..x0 + g.cell_w).flat_map(move |x| [buf[(x, y)].fg, buf[(x, y)].bg])).collect()
    }

    fn rgb(c: Rgb) -> TermColor {
        TermColor::Rgb(c.0, c.1, c.2)
    }

    const MS: std::time::Duration = std::time::Duration::from_millis(1);

    #[test]
    fn a_moving_piece_is_drawn_on_its_way() {
        let white = rgb(THEMES[0].white);
        let mut app = App::new(true, Settings::default());
        app.start_local();
        keys(&mut app, "e2e4");
        // It has been played, and is still where it was.
        let buf = render(&mut app, 100, 36);
        assert!(text(&buf).contains("1. e4"));
        assert!(colors_of(&app, &buf, "e2").contains(&white) && !colors_of(&app, &buf, "e4").contains(&white));
        // Half way there it is over e3, and the board has not turned to Black yet.
        app.clock += 105 * MS;
        let buf = render(&mut app, 100, 36);
        assert_eq!(app.bottom(), Color::White);
        assert!(colors_of(&app, &buf, "e3").contains(&white));
        assert!(!colors_of(&app, &buf, "e2").contains(&white) && !colors_of(&app, &buf, "e4").contains(&white));
        // Then it has arrived, and the board turns.
        app.clock += 500 * MS;
        app.tick();
        let buf = render(&mut app, 100, 36);
        assert_eq!(app.bottom(), Color::Black);
        assert!(colors_of(&app, &buf, "e4").contains(&white) && !colors_of(&app, &buf, "e3").contains(&white));

        // Symbols move a character at a time.
        let mut app = App::new(true, Settings::default());
        app.start_local();
        keys(&mut app, "e2e4");
        app.clock += 105 * MS;
        let buf = render(&mut app, 80, 24);
        let symbol_on = |name: &str| {
            let (x, y) = cell_origin(&app, crate::chess::parse_square(name).unwrap());
            buf[(x + 2, y)].symbol().to_string()
        };
        assert_eq!((symbol_on("e2"), symbol_on("e3"), symbol_on("e4")), (" ".to_string(), "♟".to_string(), " ".to_string()));
    }

    /// Everything that moves, at every moment, in windows of every size.
    #[test]
    fn moving_things_draw_in_every_window_size() {
        for (width, height) in [(58, 9), (80, 24), (92, 28), (100, 36), (132, 52), (209, 48), (400, 130), (300, 11)] {
            for (pieces, fen, moves) in [
                (Pieces::Shaded, "r1bqkb1r/pppp1ppp/2n2n2/4p2Q/2B1P3/8/PPPP1PPP/RNB1K1NR w KQkq - 0 1", "h5f7"),
                (Pieces::Symbols, "r1bqkb1r/pppp1ppp/2n2n2/4p2Q/2B1P3/8/PPPP1PPP/RNB1K1NR w KQkq - 0 1", "h5f7"),
                (Pieces::Outlined, "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1", "e1g1 e8c8"),
                (Pieces::Solid, "4k3/P7/8/3pP3/8/8/7p/4K1N1 w - d6 0 1", "e5d6 h2h1q a7a8q"),
                (Pieces::Letters, "4k3/8/8/8/8/8/8/1N2K2R w K - 0 1", "b1c3 e8d8 h1h8"),
            ] {
                let mut app = App::new(true, Settings::default());
                app.pieces = pieces;
                app.start_local();
                app.game.board = Board::from_fen(fen).unwrap();
                for m in moves.split(' ') {
                    keys(&mut app, m);
                    assert!(app.fx.is_some(), "{moves} on {fen}");
                    for _ in 0..16 {
                        render(&mut app, width, height);
                        app.clock += 90 * MS;
                    }
                }
                app.tick();
                for _ in 0..8 {
                    render(&mut app, width, height);
                    app.clock += 900 * MS;
                }
            }
        }
    }

    #[test]
    fn the_result_waits_for_the_last_move_and_a_win_gets_confetti() {
        let mut app = App::new(true, Settings::default());
        app.start_local();
        keys(&mut app, "f2f3e7e5g2g4d8h4");
        assert_eq!(app.prompt, Some(Prompt::GameOver));
        let confetti = |buf: &Buffer| buf.content.iter().any(|c| theme::CONFETTI[..5].iter().any(|&color| c.fg == rgb(color) || c.bg == rgb(color)));
        let buf = render(&mut app, 100, 36);
        assert!(!text(&buf).contains("Game over") && !confetti(&buf), "{}", text(&buf));
        assert!(app.buttons.is_empty(), "nothing to click until the box is there");
        // The queen arrives, the king shakes and falls, and then the box and the confetti.
        app.clock += 2500 * MS;
        app.tick();
        let buf = render(&mut app, 100, 36);
        assert!(text(&buf).contains("Game over") && text(&buf).contains("Black wins"), "{}", text(&buf));
        assert!(confetti(&buf));
        // On a board of symbols it is stars in the empty squares.
        assert!(text(&render(&mut app, 80, 24)).contains('*'));
        app.clock += fx::CONFETTI;
        app.tick();
        assert!(!confetti(&render(&mut app, 100, 36)) && !app.animating());
    }

    #[test]
    fn the_computer_has_a_face_and_says_how_it_feels() {
        let play = |app: &mut App, uci: &str| assert!(app.game.play(crate::chess::Move::parse_uci(uci).unwrap()));
        let mut app = still();
        app.level = Level::Easy;
        app.start_computer();
        assert_eq!(mood(&app), (Mood::Calm, "Let's play!".to_string()));
        // In a large window its face is there, in the color of its head, and what it says.
        let buf = render(&mut app, 132, 52);
        let shown = text(&buf);
        assert!(shown.contains("Cat (Easy)") && shown.contains("Let's play!"), "{shown}");
        assert!(buf.content.iter().any(|c| c.fg == rgb(FACES[1].1)));
        // A small one has only its name.
        let buf = render(&mut app, 80, 24);
        assert!(text(&buf).contains("Cat (Easy)") && !text(&buf).contains("Let's play!"));
        assert!(!buf.content.iter().any(|c| c.fg == rgb(FACES[1].1)));

        play(&mut app, "e2e4");
        assert_eq!(mood(&app).0, Mood::Thinking);
        play(&mut app, "d7d5");
        assert_eq!(mood(&app), (Mood::Calm, "Your turn!".to_string()));
        play(&mut app, "e4d5");
        assert_eq!(mood(&app), (Mood::Surprised, "Hey, my pawn!".to_string()));
        play(&mut app, "d8d5");
        assert_eq!(mood(&app), (Mood::Happy, "I got your pawn!".to_string()));
        play(&mut app, "d1e2");
        play(&mut app, "d5e5");
        play(&mut app, "g1f3");
        play(&mut app, "e5e2");
        assert_eq!(mood(&app), (Mood::Happy, "Check!".to_string()));
        app.game.outcome = Some(Outcome::Resigned(Color::White));
        assert_eq!(mood(&app).0, Mood::Sad);
        app.game.outcome = Some(Outcome::Resigned(Color::Black));
        assert_eq!(mood(&app).0, Mood::Happy);
        // Every face has every row, and every level has a face.
        for (level, (name, _, _, rows)) in Level::ALL.into_iter().zip(FACES) {
            assert!(character(level) == name && rows.iter().all(|row| row.len() == 12), "{name}");
        }
    }

    #[test]
    fn captured_pieces_are_on_a_tray_in_a_large_window() {
        let mut app = still();
        app.start_local();
        let shelf = rgb(THEMES[0].dark);
        // The colors in the three rows under a player's name.
        let under = |buf: &Buffer, name: &str| -> Vec<TermColor> {
            let (x, y) = find(buf, name);
            (y + 1..y + 4).flat_map(|row| (x..x + 20).flat_map(move |col| [buf[(col, row)].fg, buf[(col, row)].bg])).collect()
        };
        // Nothing captured: no tray.
        let buf = render(&mut app, 132, 52);
        assert!(!under(&buf, "White  ").contains(&shelf));
        keys(&mut app, "e2e4d7d5e4d5d8d5b1c3g8f6c3d5");
        let buf = render(&mut app, 132, 52);
        // White has a pawn and the queen, Black a pawn: each tray holds the other color.
        assert!(text(&buf).contains("White    +9"), "{}", text(&buf));
        let (white, black) = (rgb(THEMES[0].white), rgb(THEMES[0].black));
        let (whites, blacks) = (under(&buf, "White  "), under(&buf, "Black  "));
        assert!(whites.contains(&shelf) && whites.contains(&black) && !whites.contains(&white));
        assert!(blacks.contains(&shelf) && blacks.contains(&white) && !blacks.contains(&black));
        for (kind, art) in TINY {
            assert!(art.iter().all(|row| row.len() == 5), "{kind:?}");
        }
        // A small window lists them as symbols, as it always did.
        assert!(text(&render(&mut app, 80, 24)).contains("♛ ♟ +9"));
    }

    #[test]
    fn a_hint_frames_two_squares_and_the_stars_are_in_the_result() {
        let mut app = still();
        app.start_local();
        let buf = render(&mut app, 100, 36);
        let plain = colors_of(&app, &buf, "e4");
        app.hint = crate::chess::Move::parse_uci("e2e4");
        let buf = render(&mut app, 100, 36);
        assert!(colors_of(&app, &buf, "e4") != plain && colors_of(&app, &buf, "d4").iter().all(|&c| c == rgb(THEMES[0].dark)));
        let (x, y) = find(&buf, "^G Hint");
        app.hint = None;
        click(&mut app, x, y);
        assert_eq!(app.message, "Thinking of a hint...");

        // Mate in one against the computer, with no help and in no time: all three.
        let mut app = still();
        app.start_computer();
        app.game.board = Board::from_fen("6k1/5ppp/8/8/8/8/8/R3K3 w - - 0 1").unwrap();
        keys(&mut app, "a1a8");
        let shown = text(&render(&mut app, 80, 24));
        assert!(shown.contains("You win!  ★ ★ ★") && shown.contains("All three stars!"), "{shown}");
        app.pieces = Pieces::Letters;
        assert!(text(&render(&mut app, 80, 24)).contains("You win!  * * *"));
    }

    /// Twelve commands and both players, in the smallest window a game fits in.
    #[test]
    fn the_smallest_window_holds_every_command() {
        let mut app = still();
        app.start_local();
        let buf = render(&mut app, 58, 9);
        let shown = text(&buf);
        for (key, name, _, _) in COMMANDS {
            assert!(shown.contains(&format!("{key:<3}{name}")), "{key} {name} is missing from\n{shown}");
        }
        assert!(shown.contains("> White") && shown.contains("  Black") && shown.contains("White to move"), "{shown}");
        assert_eq!(app.buttons.len(), COMMANDS.len());
        let (x, y) = find(&buf, "^S Sound");
        click(&mut app, x, y);
        assert!(!app.sound);
        // What a command says is shown in place of the state of the game.
        assert!(text(&render(&mut app, 58, 9)).contains("Sound off"));
        let (x, y) = find(&buf, "^A Animations");
        click(&mut app, x, y);
        assert!(app.animations);
        assert!(text(&render(&mut app, 58, 9)).contains("Animations on"));
    }
}
