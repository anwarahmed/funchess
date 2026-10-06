//! Drawing. The board is as large as the window allows: pixel-art pieces made of
//! half-block characters when there is room, chess symbols (or letters) when not.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color as TermColor, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph, Widget, Wrap};

use crate::app::{App, Click, Geometry, MenuItem, Opponent, Prompt, Screen};
use crate::chess::{Board, Color, Kind, Piece, file_of, rank_of, sq};
use crate::theme::{Pieces, Rgb};
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
const SMALL: [[&str; 8]; 6] = [
    ["........", "...##...", "..####..", "..####..", "...##...", "..####..", ".######.", "........"],
    ["..#.#...", "..####..", ".#+####.", "#######.", "##..###.", "...####.", "..#####.", "........"],
    ["...##...", "..####..", "..#+##..", "..####..", "...##...", "..####..", ".######.", "........"],
    ["........", "##.##.##", "########", ".######.", ".######.", ".######.", "########", "........"],
    ["#.#..#.#", "#.#..#.#", "########", ".######.", "..####..", "..####..", ".######.", "........"],
    ["...##...", "..####..", "...##...", ".######.", ".######.", "..####..", ".######.", "........"],
];
#[rustfmt::skip]
const MEDIUM: [[&str; 10]; 6] = [
    ["..........", "....##....", "...####...", "...####...", "....##....", "...####...", "....##....", "...####...", "..######..", ".........."],
    ["..........", "...#.##...", "..######..", ".##+#####.", ".########.", ".###.####.", "....####..", "...#####..", "..#######.", ".........."],
    ["....##....", "...####...", "..###+##..", "..##+###..", "..######..", "...####...", "....##....", "...####...", "..######..", ".........."],
    ["..........", ".##.##.##.", ".########.", "..######..", "..######..", "..######..", "..######..", ".########.", ".########.", ".........."],
    ["..........", ".#..##..#.", ".#..##..#.", ".##.##.##.", ".########.", "..######..", "..######..", "..++++++..", ".########.", ".........."],
    ["....##....", "..######..", "....##....", "..##..##..", ".###++###.", ".########.", "..######..", "..++++++..", ".########.", ".........."],
];
#[rustfmt::skip]
const LARGE: [[&str; 12]; 6] = [
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
            let (x, y) = centered(buf, area, "Join a game", lines);
            app.buttons.push((Rect::new(x, y + row, 13, 1), Click::Key(KeyCode::Enter, false)));
            app.buttons.push((Rect::new(x + 17, y + row, 8, 1), Click::Key(KeyCode::Esc, false)));
        }
        Screen::Connecting(text) => {
            let mut lines: Vec<Line> =
                text.iter().map(|l| if l.starts_with("funchess ") { Line::styled(l.clone(), bold()) } else { Line::raw(l.clone()) }).collect();
            lines.extend([Line::raw(""), Line::styled("Esc cancel", dim())]);
            let row = lines.len() as u16 - 1;
            let (x, y) = centered(buf, area, "Network game", lines);
            app.buttons.push((Rect::new(x, y + row, 10, 1), Click::Key(KeyCode::Esc, false)));
        }
        Screen::Game => game(buf, area, app),
    }
}

/// A titled block of lines in the middle of the screen. Returns where the first line starts.
fn centered(buf: &mut Buffer, area: Rect, title: &str, lines: Vec<Line>) -> (u16, u16) {
    let width = lines.iter().map(Line::width).max().unwrap_or(0).max(title.len()) as u16;
    let height = lines.len() as u16 + 2;
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    buf.set_stringn(x, y, title, area.width as usize, bold().fg(TermColor::Cyan));
    for (i, line) in lines.iter().enumerate() {
        let row = y + 2 + i as u16;
        if row < area.bottom() {
            buf.set_line(x, row, line, area.width.saturating_sub(x - area.x));
        }
    }
    (x, y + 2)
}

fn menu(buf: &mut Buffer, area: Rect, app: &mut App) {
    let mut lines = Vec::new();
    let mut rows = Vec::new();
    for (i, item) in MenuItem::ALL.into_iter().enumerate() {
        if matches!(item, MenuItem::Level | MenuItem::Quit) {
            lines.push(Line::raw(""));
        }
        let text = match item {
            MenuItem::Computer => "1  Play the computer".to_string(),
            MenuItem::Local => "2  Two players, this computer".to_string(),
            MenuItem::Host => "3  Host a game for another computer".to_string(),
            MenuItem::Join => "4  Join a game on another computer".to_string(),
            MenuItem::Level => format!("   Computer level   < {} >", app.level.name()),
            MenuItem::Side => format!("   Your color       < {} >", app.side.name()),
            MenuItem::Theme => format!("^T Theme            < {} >", app.theme().name),
            MenuItem::Pieces => format!("^P Pieces           < {} >", app.pieces.name()),
            MenuItem::Quit => "^Q Quit".to_string(),
        };
        let chosen = i == app.menu_item;
        let style = if chosen { bold().fg(TermColor::Cyan) } else { Style::new() };
        rows.push(lines.len() as u16);
        lines.push(Line::styled(format!("{} {text}", if chosen { ">" } else { " " }), style));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(app.menu_error.clone(), Style::new().fg(TermColor::Red)));
    lines.push(Line::styled("Up/Down choose   Enter start   Left/Right change", dim()));
    lines.push(Line::styled("^ means the Ctrl key", dim()));
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
    let (left, top) = centered(buf, area, "funchess   chess in the terminal", lines);
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
                target: false,
                cursor: false,
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
    let checked = board.in_check(board.turn).then(|| board.king_square(board.turn));
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
                ground = mix(ground, theme.check, 0.75);
            }
            let cell = Cell {
                x: bx + col * cell_w,
                y: by + row * cell_h,
                w: cell_w,
                h: cell_h,
                ground,
                piece: board.at(square),
                target: targets.contains(&square),
                cursor: app.cursor == square && app.prompt.is_none(),
            };
            cell.draw(buf, app);
        }
        // Rank numbers down the side, file letters along the bottom.
        let rank = if white_below { 8 - row } else { row + 1 };
        buf.set_string(bx - 2, by + row * cell_h + (cell_h - 1) / 2, rank.to_string(), dim());
        let file = (b'a' + if white_below { row } else { 7 - row } as u8) as char;
        buf.set_string(bx + row * cell_w + cell_w / 2, by + board_h, file.to_string(), dim());
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
    side_panel(buf, Rect::new(bx + board_w + GAP, by, RIGHT_WIDTH, board_h), app, left == 0);

    if let Some(prompt) = app.prompt {
        popup(buf, area, app, prompt);
    }
}

struct Cell {
    x: u16,
    y: u16,
    w: u16,
    h: u16,
    ground: Rgb,
    piece: Option<Piece>,
    target: bool,
    cursor: bool,
}

impl Cell {
    fn draw(&self, buf: &mut Buffer, app: &App) {
        if self.h >= 4 && app.pieces.is_pixel_art() { self.pixels(buf, app) } else { self.symbols(buf, app) }
    }

    /// Pixel art: every character is two pixels, the upper drawn as the foreground
    /// of a half block and the lower as its background.
    fn pixels(&self, buf: &mut Buffer, app: &App) {
        // The square is `n` pixels each way. The art comes in three sizes, each of which
        // can be doubled or tripled; the largest that fits is centered in the square.
        let n = self.w as i32;
        let (size, scale) = [(12, 1), (10, 1), (8, 1), (12, 2), (10, 2), (8, 2), (12, 3), (10, 3), (8, 3)]
            .into_iter()
            .filter(|&(size, scale)| size * scale <= n)
            .max_by_key(|&(size, scale)| size * scale)
            .unwrap_or((8, 1));
        let margin = (n - size * scale) / 2;
        let art: Option<&[&str]> = self.piece.map(|p| match size {
            12 => &LARGE[p.kind as usize][..],
            10 => &MEDIUM[p.kind as usize][..],
            _ => &SMALL[p.kind as usize][..],
        });
        let at = |x: i32, y: i32| -> u8 {
            let (ax, ay) = ((x - margin).div_euclid(scale), (y - margin).div_euclid(scale));
            match art {
                Some(rows) if (0..size).contains(&ax) && (0..size).contains(&ay) => rows[ay as usize].as_bytes()[ax as usize],
                _ => b'.',
            }
        };
        let theme = app.theme();
        let white = self.piece.is_some_and(|p| p.color == Color::White);
        let (body, shade) = if white { (theme.white, theme.white_shade) } else { (theme.black, theme.black_shade) };
        let clear = |x: i32, y: i32| at(x, y) == b'.';
        let color = |x: i32, y: i32| -> Rgb {
            match at(x, y) {
                b'+' => shade,
                b'#' if app.pieces == Pieces::Shaded => {
                    // Lit from the top left: edges facing the light are bright, the
                    // opposite edges dark, and the rest in between.
                    let (bright, middle, dim) = if white {
                        (theme.white, mix(theme.white, theme.white_shade, 0.4), theme.white_shade)
                    } else {
                        (theme.black_shade, mix(theme.black, theme.black_shade, 0.35), theme.black)
                    };
                    if clear(x + 1, y) || clear(x, y + 1) {
                        dim
                    } else if clear(x - 1, y) || clear(x, y - 1) {
                        bright
                    } else {
                        middle
                    }
                }
                b'#' => body,
                _ => {
                    let edge = x == 0 || y == 0 || x == n - 1 || y == n - 1;
                    let middle = |v: i32| (v - n / 2).abs().min((v - n / 2 + 1).abs()) < n / 6;
                    let beside_piece = !(clear(x + 1, y) && clear(x - 1, y) && clear(x, y + 1) && clear(x, y - 1));
                    if self.cursor && edge {
                        theme.cursor
                    } else if self.target && (if self.piece.is_some() { edge } else { middle(x) && middle(y) }) {
                        theme.target
                    } else if beside_piece && app.pieces == Pieces::Outlined {
                        if white { theme.black } else { mix(self.ground, theme.white, 0.8) }
                    } else if beside_piece && white {
                        // White pieces get a soft dark rim so they stand out on a light square.
                        mix(self.ground, (0, 0, 0), 0.5)
                    } else {
                        self.ground
                    }
                }
            }
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
const COMMANDS: [(&str, &str, char, bool); 9] = [
    ("?", "Help", '?', false),
    ("^U", "Undo", 'u', true),
    ("^N", "New game", 'n', true),
    ("^F", "Flip board", 'f', true),
    ("^R", "Resign", 'r', true),
    ("^D", "Offer draw", 'd', true),
    ("^T", "Theme", 't', true),
    ("^P", "Pieces", 'p', true),
    ("^Q", "Menu", 'q', true),
];

/// Right of the board: each player beside their own side of it, and between them
/// what is going on, the moves if they are not shown on the left, and the commands.
fn side_panel(buf: &mut Buffer, area: Rect, app: &mut App, with_moves: bool) {
    let board = app.game.board;
    // Small boards get one line per player and no blank lines; large ones get room.
    let (compact, roomy) = (area.height < 16, area.height >= 24);
    let player = |color: Color| -> Vec<Line<'static>> {
        let who = match app.opponent {
            Opponent::Local => String::new(),
            Opponent::Computer(_) | Opponent::Remote if color == app.me => "You".into(),
            Opponent::Computer(level) => format!("Computer ({})", level.name()),
            Opponent::Remote => "Opponent".into(),
        };
        let to_move = board.turn == color && !app.over();
        let name = format!("{} {:<6} {who}", if to_move { ">" } else { " " }, color.name());
        let taken: String = lost(&board, color.other()).into_iter().map(|p| symbol(app, p, false)).flat_map(|c| [c, ' ']).collect();
        let lead = material(&board, color) - material(&board, color.other());
        let lead = if lead > 0 { format!("+{lead}") } else { String::new() };
        let mut lines = vec![Line::styled(name, if to_move { bold().fg(TermColor::Cyan) } else { bold() })];
        if !compact {
            lines.push(Line::raw(format!("  {taken}{lead}")));
        }
        lines
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
    let mut y = area.y;
    for line in player(app.bottom().other()) {
        buf.set_line(area.x, y, &line, area.width);
        y += 1;
    }
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
    let near = player(app.bottom());
    let mut bottom = area.bottom() - near.len() as u16;
    for (i, line) in near.iter().enumerate() {
        buf.set_line(area.x, bottom + i as u16, line, area.width);
    }
    bottom -= u16::from(roomy);
    let columns: u16 = if area.height >= 32 { 1 } else { 2 };
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
                "  Ctrl-U  take back a move",
                "  Ctrl-R  resign",
                "  Ctrl-D  offer a draw",
                "  Ctrl-N  new game (rematch in a network game)",
                "  Ctrl-F  flip the board (Tab works too)",
                "  Ctrl-T  next color theme",
                "  Ctrl-P  next way of drawing the pieces",
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
            (Prompt::Promotion { .. }, "") => {}
            (Prompt::Promotion { .. }, text) if text.starts_with("Esc") => app.buttons.push((whole, Click::Key(KeyCode::Esc, false))),
            (Prompt::Promotion { .. }, text) => app.buttons.push((whole, key(text.chars().next().unwrap_or('q')))),
            _ => {}
        }
    }
    let block = Block::bordered().title(format!(" {title} ")).border_style(Style::new().fg(TermColor::Cyan)).padding(ratatui::widgets::Padding::horizontal(1));
    Paragraph::new(lines.into_iter().map(Line::raw).collect::<Vec<_>>()).block(block).render(rect, buf);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Settings, THEMES};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
            let mut app = App::new(true, Settings::default());
            render(&mut app, width, height);
            app.start_local();
            keys(&mut app, "e2e4e7e5g1");
            render(&mut app, width, height);
            for prompt in [Prompt::Help, Prompt::Resign, Prompt::DrawOffered, Prompt::Leave, Prompt::NewGame, Prompt::Promotion { from: 0, to: 0 }] {
                app.prompt = Some(prompt);
                render(&mut app, width, height);
            }
        }
    }

    #[test]
    fn the_board_grows_with_the_window() {
        let mut app = App::new(true, Settings::default());
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
        let mut app = App::new(true, Settings::default());
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
        app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        let buf = render(&mut app, 80, 24);
        assert_eq!(symbol_on(&app, &buf, "e4"), "♟");
        assert!(text(&buf).contains("1. e4"));
        // Flipped: Black's king is now on the bottom row of the board.
        let g = app.geometry.unwrap();
        assert_eq!(cell_origin(&app, sq(4, 7)).1, g.y + 7 * g.cell_h);

        app.pieces = Pieces::Letters;
        let buf = render(&mut app, 80, 24);
        assert_eq!(symbol_on(&app, &buf, "e8"), "k");
        assert_eq!(symbol_on(&app, &buf, "e1"), "K");
    }

    #[test]
    fn pixel_art_pieces_are_drawn_in_their_own_colors() {
        let mut app = App::new(true, Settings::default());
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

    #[test]
    fn menu_lines_can_be_clicked() {
        let mut app = App::new(true, Settings::default());
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
        let mut app = App::new(true, Settings::default());
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

        // Every command fits somewhere, and the help closes on a click.
        render(&mut app, 132, 52);
        assert_eq!(app.buttons.len(), 9);
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
        let mut app = App::new(true, Settings::default());
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
        let mut app = App::new(true, Settings::default());
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
}
