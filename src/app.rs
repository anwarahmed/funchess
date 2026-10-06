//! What the program is doing and how it reacts to keys, clicks, the computer
//! opponent finishing its thinking, and messages from the other computer.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use crate::chess::{Color, Game, Kind, Move, Outcome, Square, file_of, rank_of, sq};
use crate::engine::{self, Level};
use crate::net::{self, Incoming, Link, Message, Pending};
use crate::theme::{Pieces, Settings, THEMES, Theme};

/// The computer's move is held back this long, so that it does not land the instant
/// the player lets go of their own piece.
const MIN_THINK: Duration = Duration::from_millis(450);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Opponent {
    Computer(Level),
    /// Both players at this keyboard.
    Local,
    /// The other player is on another computer.
    Remote,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    White,
    Black,
    Random,
}

impl Side {
    pub fn name(self) -> &'static str {
        match self {
            Side::White => "White",
            Side::Black => "Black",
            Side::Random => "Random",
        }
    }

    fn color(self) -> Color {
        match self {
            Side::White => Color::White,
            Side::Black => Color::Black,
            Side::Random => {
                if seed().is_multiple_of(2) {
                    Color::White
                } else {
                    Color::Black
                }
            }
        }
    }
}

fn seed() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuItem {
    Computer,
    Local,
    Host,
    Join,
    Level,
    Side,
    Theme,
    Pieces,
    Quit,
}

impl MenuItem {
    pub const ALL: [MenuItem; 9] = [
        MenuItem::Computer,
        MenuItem::Local,
        MenuItem::Host,
        MenuItem::Join,
        MenuItem::Level,
        MenuItem::Side,
        MenuItem::Theme,
        MenuItem::Pieces,
        MenuItem::Quit,
    ];
}

pub enum Screen {
    Menu,
    /// Typing the address of the host to join.
    Join,
    /// Waiting for the connection, with what to show meanwhile.
    Connecting(Vec<String>),
    Game,
}

/// A question that has to be answered before play goes on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Prompt {
    Promotion {
        from: Square,
        to: Square,
    },
    Resign,
    /// A draw was offered to whoever is asked: the other local player, or this
    /// player by the opponent over the network.
    DrawOffered,
    /// Leaving a game that is not over.
    Leave,
    NewGame,
    Help,
}

struct Thinking {
    rx: Receiver<Option<Move>>,
    stop: Arc<AtomicBool>,
    since: Instant,
}

impl Drop for Thinking {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// What a click on a button does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Click {
    /// The same as pressing a key, with Ctrl or without.
    Key(KeyCode, bool),
    /// Choosing a line of the main menu; for a setting, stepping it forward or back.
    Menu(usize, bool),
}

/// Where the board was drawn, for turning a click into a square.
#[derive(Clone, Copy)]
pub struct Geometry {
    pub x: u16,
    pub y: u16,
    pub cell_w: u16,
    pub cell_h: u16,
}

pub struct App {
    pub screen: Screen,
    pub quit: bool,
    pub truecolor: bool,
    /// Index into `THEMES`.
    pub theme: usize,
    pub pieces: Pieces,
    /// Where the look and the level are remembered; `None` remembers nothing.
    pub settings_path: Option<std::path::PathBuf>,

    pub menu_item: usize,
    pub level: Level,
    pub side: Side,
    pub port: u16,
    pub address: String,
    /// Why the last attempt to start a game failed.
    pub menu_error: String,
    /// Everything on screen that can be clicked, apart from the board's squares, as
    /// drawn in the last frame. Where two overlap, the later one is on top.
    pub buttons: Vec<(Rect, Click)>,

    pending: Option<Pending>,
    /// Whether the connection being made is one this side is hosting.
    hosting: bool,
    link: Option<Link>,

    pub game: Game,
    pub opponent: Opponent,
    /// The color this player has. With both players at one keyboard, the color
    /// shown at the bottom of the board.
    pub me: Color,
    pub flipped: bool,
    pub cursor: Square,
    pub selected: Option<Square>,
    /// The file letter typed so far of a square being typed in, as `e` then `4`.
    typed_file: Option<u8>,
    pub prompt: Option<Prompt>,
    pub message: String,
    thinking: Option<Thinking>,
    draw_offer_sent: bool,
    rematch_wanted: bool,
    rematch_offered: bool,
    pub geometry: Option<Geometry>,
    button_down: bool,
}

impl App {
    pub fn new(truecolor: bool, settings: Settings) -> App {
        App {
            screen: Screen::Menu,
            quit: false,
            truecolor,
            theme: settings.theme,
            pieces: settings.pieces,
            settings_path: None,
            menu_item: 0,
            level: settings.level,
            side: Side::White,
            port: net::DEFAULT_PORT,
            address: String::new(),
            menu_error: String::new(),
            buttons: Vec::new(),
            pending: None,
            hosting: false,
            link: None,
            game: Game::new(),
            opponent: Opponent::Local,
            me: Color::White,
            flipped: false,
            cursor: sq(4, 1),
            selected: None,
            typed_file: None,
            prompt: None,
            message: String::new(),
            thinking: None,
            draw_offer_sent: false,
            rematch_wanted: false,
            rematch_offered: false,
            geometry: None,
            button_down: false,
        }
    }

    // ---- how it looks ----

    pub fn theme(&self) -> &'static Theme {
        &THEMES[self.theme % THEMES.len()]
    }

    fn change_theme(&mut self, forward: bool) {
        self.theme = (self.theme + if forward { 1 } else { THEMES.len() - 1 }) % THEMES.len();
        self.remember();
    }

    fn change_pieces(&mut self, forward: bool) {
        let i = Pieces::ALL.iter().position(|&p| p == self.pieces).unwrap_or(0);
        self.pieces = Pieces::ALL[(i + if forward { 1 } else { Pieces::ALL.len() - 1 }) % Pieces::ALL.len()];
        self.remember();
    }

    fn remember(&self) {
        if let Some(path) = &self.settings_path {
            Settings { theme: self.theme, pieces: self.pieces, level: self.level }.save(path);
        }
    }

    // ---- starting and leaving games ----

    fn start(&mut self, opponent: Opponent, me: Color) {
        self.thinking = None;
        self.game = Game::new();
        self.opponent = opponent;
        self.me = me;
        self.flipped = false;
        self.cursor = if me == Color::White { sq(4, 1) } else { sq(4, 6) };
        self.selected = None;
        self.typed_file = None;
        self.prompt = None;
        self.message.clear();
        self.draw_offer_sent = false;
        self.rematch_wanted = false;
        self.rematch_offered = false;
        self.screen = Screen::Game;
        self.after_move();
    }

    pub fn start_computer(&mut self) {
        self.start(Opponent::Computer(self.level), self.side.color());
    }

    pub fn start_local(&mut self) {
        self.start(Opponent::Local, Color::White);
    }

    pub fn start_host(&mut self) {
        match net::host(self.port) {
            Ok(pending) => {
                self.me = self.side.color();
                self.pending = Some(pending);
                self.hosting = true;
                let port = if self.port == net::DEFAULT_PORT { String::new() } else { format!(":{}", self.port) };
                let address = net::local_ip().map_or("<this computer's address>".to_string(), |ip| ip.to_string());
                self.screen = Screen::Connecting(vec![
                    format!("Waiting for the other player. You play {}.", self.me.name()),
                    String::new(),
                    "On the other computer, run:".into(),
                    format!("funchess join {address}{port}"),
                    String::new(),
                    "Both computers must be on the same network, or the".into(),
                    format!("other must be able to reach this one on port {}.", self.port),
                ]);
            }
            Err(e) => self.back_to_menu(format!("Cannot host on port {}: {e}", self.port)),
        }
    }

    pub fn start_join(&mut self) {
        let address = self.address.trim().to_string();
        if address.is_empty() {
            return;
        }
        self.pending = Some(net::join(&address));
        self.hosting = false;
        self.screen = Screen::Connecting(vec![format!("Connecting to {address}...")]);
    }

    fn back_to_menu(&mut self, error: String) {
        self.thinking = None;
        self.pending = None;
        self.link = None;
        self.prompt = None;
        self.menu_error = error;
        self.screen = Screen::Menu;
    }

    // ---- whose move it is ----

    pub fn over(&self) -> bool {
        self.game.outcome.is_some()
    }

    /// Whether the player at this keyboard may move a piece now.
    pub fn my_turn(&self) -> bool {
        !self.over() && (self.opponent == Opponent::Local || self.game.board.turn == self.me)
    }

    /// The color at the bottom of the board.
    pub fn bottom(&self) -> Color {
        if self.flipped { self.me.other() } else { self.me }
    }

    pub fn is_thinking(&self) -> bool {
        self.thinking.is_some()
    }

    pub fn targets(&self) -> Vec<Square> {
        let Some(from) = self.selected else { return Vec::new() };
        self.game.board.legal_moves().into_iter().filter(|m| m.from == from).map(|m| m.to).collect()
    }

    // ---- moving ----

    /// What happens when a square is chosen, by click, by Enter or by typing its name.
    fn choose(&mut self, square: Square) {
        self.cursor = square;
        self.message.clear();
        if !self.my_turn() {
            return;
        }
        let board = &self.game.board;
        if let Some(from) = self.selected {
            let moves: Vec<Move> = board.legal_moves().into_iter().filter(|m| m.from == from && m.to == square).collect();
            match moves.as_slice() {
                [] => {}
                [only] => return self.commit(*only),
                _ => {
                    self.prompt = Some(Prompt::Promotion { from, to: square });
                    return;
                }
            }
        }
        let own = board.at(square).is_some_and(|p| p.color == board.turn);
        self.selected = (own && self.selected != Some(square)).then_some(square);
        if own && self.selected.is_some() && self.targets().is_empty() {
            self.message = "That piece has no legal move".into();
        }
    }

    fn commit(&mut self, m: Move) {
        if !self.game.play(m) {
            return;
        }
        if let Some(link) = &mut self.link {
            link.send(Message::Move(m));
        }
        self.after_move();
    }

    /// Housekeeping after any move, whoever made it.
    fn after_move(&mut self) {
        self.selected = None;
        self.typed_file = None;
        self.draw_offer_sent = false;
        if matches!(self.prompt, Some(Prompt::Promotion { .. } | Prompt::DrawOffered)) {
            self.prompt = None;
        }
        self.thinking = None;
        if let Opponent::Computer(level) = self.opponent
            && !self.over()
            && self.game.board.turn != self.me
        {
            let (board, keys, stop) = (self.game.board, self.game.keys(), Arc::new(AtomicBool::new(false)));
            let (tx, rx) = mpsc::channel();
            let stopped = stop.clone();
            thread::spawn(move || {
                let _ = tx.send(engine::best_move(&board, &keys, level, &stopped, seed()));
            });
            self.thinking = Some(Thinking { rx, stop, since: Instant::now() });
        }
    }

    fn undo(&mut self) {
        if self.opponent == Opponent::Remote {
            self.message = "No take-backs in a network game".into();
            return;
        }
        self.message.clear();
        // A resignation or an agreed draw is itself the last thing that happened.
        if matches!(self.game.outcome, Some(Outcome::Resigned(_) | Outcome::DrawAgreed)) {
            self.game.outcome = None;
            return self.after_move();
        }
        // Against the computer, back to this player's previous turn: their move, and
        // the reply if there was one.
        let keep = match self.opponent {
            Opponent::Computer(_) => self.game.history.iter().rposition(|p| p.before.turn == self.me),
            _ => self.game.history.len().checked_sub(1),
        };
        let Some(keep) = keep else {
            self.message = "Nothing to take back".into();
            return;
        };
        while self.game.history.len() > keep {
            self.game.undo();
        }
        self.after_move();
    }

    fn resign(&mut self) {
        let loser = if self.opponent == Opponent::Local { self.game.board.turn } else { self.me };
        self.end(Outcome::Resigned(loser.other()));
        if let Some(link) = &mut self.link {
            link.send(Message::Resign);
        }
    }

    fn end(&mut self, outcome: Outcome) {
        self.game.outcome = Some(outcome);
        self.thinking = None;
        self.selected = None;
        self.prompt = None;
    }

    fn offer_draw(&mut self) {
        match self.opponent {
            _ if self.over() => {}
            Opponent::Computer(_) => self.message = "The computer plays on".into(),
            Opponent::Local => self.prompt = Some(Prompt::DrawOffered),
            Opponent::Remote if self.draw_offer_sent => self.message = "Draw already offered; make a move".into(),
            Opponent::Remote => {
                if let Some(link) = &mut self.link {
                    link.send(Message::DrawOffer);
                    self.draw_offer_sent = true;
                    self.message = "Draw offered".into();
                }
            }
        }
    }

    fn new_game(&mut self) {
        match self.opponent {
            Opponent::Computer(_) | Opponent::Local => self.start(self.opponent, self.me),
            Opponent::Remote => {
                let Some(link) = &mut self.link else {
                    self.message = "The other player has left".into();
                    return;
                };
                if !self.rematch_wanted {
                    link.send(Message::Rematch);
                    self.rematch_wanted = true;
                }
                self.message = "Rematch requested".into();
                self.maybe_rematch();
            }
        }
    }

    /// Once both players have asked, the next game starts with the colors swapped.
    fn maybe_rematch(&mut self) {
        if self.rematch_wanted && self.rematch_offered {
            self.start(Opponent::Remote, self.me.other());
            self.message = "New game, colors swapped".into();
        }
    }

    // ---- things that happen without a key being pressed ----

    /// Called before every frame. True when something changed.
    pub fn tick(&mut self) -> bool {
        let mut changed = false;
        if let Some(thinking) = &self.thinking
            && thinking.since.elapsed() >= MIN_THINK
            && let Ok(reply) = thinking.rx.try_recv()
        {
            self.thinking = None;
            if let Some(m) = reply {
                self.game.play(m);
                self.after_move();
            }
            changed = true;
        }
        if let Some(result) = self.pending.as_ref().and_then(Pending::poll) {
            self.pending = None;
            match result.and_then(|stream| Link::open(stream, self.hosting.then_some(self.me)).map_err(|e| e.to_string())) {
                Ok(link) => self.link = Some(link),
                Err(e) => self.back_to_menu(format!("Could not connect: {e}")),
            }
            changed = true;
        }
        while let Some(incoming) = self.link.as_ref().and_then(Link::poll) {
            self.receive(incoming);
            changed = true;
        }
        changed
    }

    fn receive(&mut self, incoming: Incoming) {
        if !matches!(self.screen, Screen::Game) {
            // Still at the greeting.
            match incoming {
                Incoming::Hello(host_color) => self.start(Opponent::Remote, host_color.map_or(self.me, Color::other)),
                Incoming::Closed(reason) => self.back_to_menu(format!("Could not connect: {reason}")),
                Incoming::Message(_) => {}
            }
            return;
        }
        match incoming {
            Incoming::Hello(_) => {}
            Incoming::Closed(reason) => {
                self.link = None;
                if !self.over() {
                    self.end(Outcome::Abandoned(self.me));
                }
                self.message = format!("Connection closed: {reason}");
            }
            Incoming::Message(Message::Move(m)) => {
                if !self.over() && self.game.board.turn != self.me && self.game.play(m) {
                    self.after_move();
                    self.message.clear();
                } else if !self.over() {
                    self.link = None;
                    self.end(Outcome::Abandoned(self.me));
                    self.message = "The other player sent an impossible move".into();
                }
            }
            Incoming::Message(Message::Resign) if !self.over() => self.end(Outcome::Resigned(self.me)),
            Incoming::Message(Message::DrawOffer) if !self.over() => self.prompt = Some(Prompt::DrawOffered),
            Incoming::Message(Message::DrawAccept) if !self.over() && self.draw_offer_sent => self.end(Outcome::DrawAgreed),
            Incoming::Message(Message::DrawDecline) if self.draw_offer_sent => self.message = "Draw declined".into(),
            Incoming::Message(Message::Rematch) if self.over() => {
                self.rematch_offered = true;
                self.message = "Rematch offered: Ctrl-N to accept".into();
                self.maybe_rematch();
            }
            Incoming::Message(_) => {}
        }
    }

    // ---- keys ----

    pub fn on_key(&mut self, key: KeyEvent) {
        // Every command is Ctrl with a letter, so that no plain key ever does anything
        // but move the marker, name a square or answer a question.
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            if let KeyCode::Char(c) = key.code {
                self.command(c.to_ascii_lowercase());
            }
            return;
        }
        match self.screen {
            Screen::Menu => self.menu_key(key.code),
            Screen::Join => match key.code {
                KeyCode::Esc => self.back_to_menu(String::new()),
                KeyCode::Enter => self.start_join(),
                KeyCode::Backspace => {
                    self.address.pop();
                }
                KeyCode::Char(c) if c.is_ascii_graphic() && self.address.len() < 64 => self.address.push(c),
                _ => {}
            },
            Screen::Connecting(_) => {
                if key.code == KeyCode::Esc {
                    self.back_to_menu(String::new());
                }
            }
            Screen::Game => self.game_key(key.code),
        }
    }

    fn command(&mut self, letter: char) {
        let in_game = matches!(self.screen, Screen::Game);
        match letter {
            'c' => self.quit = true,
            'q' if in_game => self.leave(),
            'q' if matches!(self.screen, Screen::Menu) => self.quit = true,
            'q' => self.back_to_menu(String::new()),
            't' => self.change_theme(true),
            'p' => self.change_pieces(true),
            // The rest act on a game, and wait while a question is open.
            _ if !in_game || self.prompt.is_some() => {}
            'u' => self.undo(),
            'r' | 'd' if self.over() => self.message = "The game is over".into(),
            'r' => self.prompt = Some(Prompt::Resign),
            'd' => self.offer_draw(),
            'f' => self.flipped = !self.flipped,
            'n' => {
                if self.over() || self.game.history.is_empty() {
                    self.new_game();
                } else if self.opponent == Opponent::Remote {
                    self.message = "Finish or resign this game first".into();
                } else {
                    self.prompt = Some(Prompt::NewGame);
                }
            }
            _ => {}
        }
    }

    /// Back to the menu, asking first if that abandons a game.
    fn leave(&mut self) {
        if self.over() || self.game.history.is_empty() {
            self.back_to_menu(String::new());
        } else {
            self.prompt = Some(Prompt::Leave);
        }
    }

    fn menu_key(&mut self, code: KeyCode) {
        let item = MenuItem::ALL[self.menu_item];
        match code {
            KeyCode::Up | KeyCode::Char('k') => self.menu_item = (self.menu_item + MenuItem::ALL.len() - 1) % MenuItem::ALL.len(),
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => self.menu_item = (self.menu_item + 1) % MenuItem::ALL.len(),
            KeyCode::Left => self.menu_change(item, false),
            KeyCode::Right => self.menu_change(item, true),
            KeyCode::Enter | KeyCode::Char(' ') => self.menu_activate(item),
            KeyCode::Esc => self.quit = true,
            KeyCode::Char(c @ '1'..='4') => {
                self.menu_item = c as usize - '1' as usize;
                self.menu_activate(MenuItem::ALL[self.menu_item]);
            }
            _ => {}
        }
    }

    fn menu_change(&mut self, item: MenuItem, forward: bool) {
        fn cycle<T: Copy + PartialEq>(all: &[T], now: T, forward: bool) -> T {
            let i = all.iter().position(|&x| x == now).unwrap_or(0);
            all[(i + if forward { 1 } else { all.len() - 1 }) % all.len()]
        }
        match item {
            MenuItem::Level => {
                self.level = cycle(&Level::ALL, self.level, forward);
                self.remember();
            }
            MenuItem::Theme => self.change_theme(forward),
            MenuItem::Pieces => self.change_pieces(forward),
            MenuItem::Side => self.side = cycle(&[Side::White, Side::Black, Side::Random], self.side, forward),
            _ => {}
        }
    }

    fn menu_activate(&mut self, item: MenuItem) {
        self.menu_error.clear();
        match item {
            MenuItem::Computer => self.start_computer(),
            MenuItem::Local => self.start_local(),
            MenuItem::Host => self.start_host(),
            MenuItem::Join => self.screen = Screen::Join,
            MenuItem::Level | MenuItem::Side | MenuItem::Theme | MenuItem::Pieces => self.menu_change(item, true),
            MenuItem::Quit => self.quit = true,
        }
    }

    fn game_key(&mut self, code: KeyCode) {
        if let Some(prompt) = self.prompt {
            return self.prompt_key(prompt, code);
        }
        // Arrow keys follow the board as drawn, whichever way round it is.
        let step = |app: &mut App, df: i8, dr: i8| {
            let sign = if app.bottom() == Color::White { 1 } else { -1 };
            let f = (file_of(app.cursor) as i8 + df * sign).clamp(0, 7);
            let r = (rank_of(app.cursor) as i8 + dr * sign).clamp(0, 7);
            app.cursor = sq(f as u8, r as u8);
            app.typed_file = None;
        };
        match code {
            KeyCode::Left => step(self, -1, 0),
            KeyCode::Right => step(self, 1, 0),
            KeyCode::Up => step(self, 0, 1),
            KeyCode::Down => step(self, 0, -1),
            KeyCode::Enter | KeyCode::Char(' ') => self.choose(self.cursor),
            KeyCode::Char(c @ 'a'..='h') => self.typed_file = Some(c as u8 - b'a'),
            KeyCode::Char(c @ '1'..='8') => {
                if let Some(file) = self.typed_file.take() {
                    self.choose(sq(file, c as u8 - b'1'));
                }
            }
            KeyCode::Esc if self.selected.is_some() || self.typed_file.is_some() => {
                self.selected = None;
                self.typed_file = None;
            }
            KeyCode::Esc => self.leave(),
            KeyCode::Tab => self.flipped = !self.flipped,
            KeyCode::Char('?') => self.prompt = Some(Prompt::Help),
            _ => {}
        }
    }

    fn prompt_key(&mut self, prompt: Prompt, code: KeyCode) {
        let yes = matches!(code, KeyCode::Char('y' | 'Y') | KeyCode::Enter);
        let no = matches!(code, KeyCode::Char('n' | 'N') | KeyCode::Esc);
        match prompt {
            Prompt::Help => self.prompt = None,
            Prompt::Promotion { from, to } => {
                let kind = match code {
                    KeyCode::Char('q') | KeyCode::Enter => Kind::Queen,
                    KeyCode::Char('r') => Kind::Rook,
                    KeyCode::Char('b') => Kind::Bishop,
                    KeyCode::Char('n') => Kind::Knight,
                    KeyCode::Esc => {
                        self.prompt = None;
                        return;
                    }
                    _ => return,
                };
                self.prompt = None;
                self.commit(Move { from, to, promo: Some(kind) });
            }
            Prompt::DrawOffered if yes || no => {
                self.prompt = None;
                if let Some(link) = &mut self.link {
                    link.send(if yes { Message::DrawAccept } else { Message::DrawDecline });
                }
                if yes {
                    self.end(Outcome::DrawAgreed);
                } else {
                    self.message = "Draw declined".into();
                }
            }
            Prompt::Resign if yes => self.resign(),
            Prompt::Leave if yes => self.back_to_menu(String::new()),
            Prompt::NewGame if yes => self.new_game(),
            _ if no => self.prompt = None,
            _ => {}
        }
    }

    // ---- mouse ----

    pub fn on_mouse(&mut self, mouse: MouseEvent) {
        // A click counts when the button goes down. Should a terminal only ever report
        // it coming up, that counts instead.
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => self.button_down = true,
            MouseEventKind::Up(MouseButton::Left) if !std::mem::take(&mut self.button_down) => {}
            MouseEventKind::Up(MouseButton::Left) => return,
            _ => return,
        }
        let at = Position::new(mouse.column, mouse.row);
        match self.buttons.iter().rev().find(|(rect, _)| rect.contains(at)).map(|&(_, click)| click) {
            Some(Click::Key(code, ctrl)) => self.on_key(KeyEvent::new(code, if ctrl { KeyModifiers::CONTROL } else { KeyModifiers::NONE })),
            Some(Click::Menu(i, forward)) => {
                self.menu_item = i;
                match MenuItem::ALL[i] {
                    item @ (MenuItem::Level | MenuItem::Side | MenuItem::Theme | MenuItem::Pieces) => self.menu_change(item, forward),
                    item => self.menu_activate(item),
                }
            }
            None if matches!(self.screen, Screen::Game) && self.prompt.is_none() => {
                if let Some(square) = self.square_at(mouse.column, mouse.row) {
                    self.choose(square);
                }
            }
            None => {}
        }
    }

    fn square_at(&self, x: u16, y: u16) -> Option<Square> {
        let g = self.geometry?;
        let col = x.checked_sub(g.x)? / g.cell_w;
        let row = y.checked_sub(g.y)? / g.cell_h;
        if col >= 8 || row >= 8 {
            return None;
        }
        let (col, row) = (col as u8, row as u8);
        Some(if self.bottom() == Color::White { sq(col, 7 - row) } else { sq(7 - col, row) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(app: &mut App, code: KeyCode) {
        app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    /// Types the text, with `^` before a letter meaning Ctrl with that letter.
    fn type_keys(app: &mut App, text: &str) {
        let mut modifiers = KeyModifiers::NONE;
        for c in text.chars() {
            if c == '^' {
                modifiers = KeyModifiers::CONTROL;
                continue;
            }
            app.on_key(KeyEvent::new(KeyCode::Char(c), modifiers));
            modifiers = KeyModifiers::NONE;
        }
    }

    fn local() -> App {
        let mut app = App::new(true, Settings::default());
        app.start_local();
        app
    }

    fn sans(app: &App) -> Vec<&str> {
        app.game.history.iter().map(|p| p.san.as_str()).collect()
    }

    /// Lets the computer finish its move.
    fn wait_for_reply(app: &mut App) {
        let started = Instant::now();
        while app.is_thinking() {
            app.tick();
            assert!(started.elapsed() < Duration::from_secs(20), "the computer never moved");
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn squares_can_be_typed_or_reached_with_the_arrows() {
        let mut app = local();
        type_keys(&mut app, "e2e4");
        assert_eq!(sans(&app), ["e4"]);
        // The cursor is on e4; Black's e-pawn is three up and stays put, so walk to e7.
        for _ in 0..3 {
            key(&mut app, KeyCode::Up);
        }
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.selected, Some(sq(4, 6)));
        assert_eq!(app.targets().len(), 2);
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Char(' '));
        assert_eq!(sans(&app), ["e4", "e5"]);
    }

    #[test]
    fn clicking_follows_the_board_when_it_is_flipped() {
        let mut app = local();
        app.geometry = Some(Geometry { x: 10, y: 5, cell_w: 8, cell_h: 4 });
        let click = |app: &mut App, col: u16, row: u16| {
            app.on_mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 10 + col * 8 + 3,
                row: 5 + row * 4 + 1,
                modifiers: KeyModifiers::NONE,
            })
        };
        click(&mut app, 4, 6);
        click(&mut app, 4, 4);
        assert_eq!(sans(&app), ["e4"]);
        key(&mut app, KeyCode::Tab);
        // Black now sits at the bottom, with the h-file on the left.
        click(&mut app, 3, 6);
        click(&mut app, 3, 4);
        assert_eq!(sans(&app), ["e4", "e5"]);
        // A click off the board does nothing.
        app.on_mouse(MouseEvent { kind: MouseEventKind::Down(MouseButton::Left), column: 2, row: 2, modifiers: KeyModifiers::NONE });
        assert_eq!(app.selected, None);
    }

    #[test]
    fn the_look_can_be_changed_anywhere_and_is_remembered() {
        let path = std::env::temp_dir().join(format!("funchess-app-test-{}/settings", std::process::id()));
        let mut app = App::new(true, Settings::default());
        app.settings_path = Some(path.clone());
        // In the menu, then in a game.
        type_keys(&mut app, "^t");
        app.start_local();
        type_keys(&mut app, "^t^p^p");
        assert_eq!((app.theme().name, app.pieces), (THEMES[2].name, Pieces::Letters));
        assert_eq!(Settings::load(&path), Settings { theme: 2, pieces: Pieces::Letters, level: Level::Easy });
        // Going round the end comes back to the start.
        for _ in 0..THEMES.len() - 2 {
            type_keys(&mut app, "^t");
        }
        assert_eq!(app.theme, 0);
        // Neither key is taken for part of a move.
        assert!(app.game.history.is_empty() && app.selected.is_none());
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    /// Letters that are not part of a square's name do nothing at all in a game, and
    /// the letters that are commands with Ctrl are just letters without it.
    #[test]
    fn plain_letters_are_never_commands() {
        let mut app = local();
        type_keys(&mut app, "e2e4");
        let (theme, pieces) = (app.theme, app.pieces);
        type_keys(&mut app, "qurntpxijklmosvwyz=QURN");
        assert!(matches!(app.screen, Screen::Game) && app.prompt.is_none() && !app.quit && !app.flipped);
        assert_eq!((app.theme, app.pieces, app.game.history.len()), (theme, pieces, 1));
        // In the menu too.
        type_keys(&mut app, "^qy");
        assert!(matches!(app.screen, Screen::Menu));
        type_keys(&mut app, "qtp");
        assert!(!app.quit && (app.theme, app.pieces) == (theme, pieces));
        type_keys(&mut app, "^q");
        assert!(app.quit);
    }

    #[test]
    fn commands_wait_while_a_question_is_open() {
        let mut app = local();
        type_keys(&mut app, "e2e4^r^u^n^f");
        assert_eq!(app.prompt, Some(Prompt::Resign));
        assert!(app.game.history.len() == 1 && !app.flipped);
        type_keys(&mut app, "n^f");
        assert!(app.prompt.is_none() && app.flipped);
    }

    fn click_at(app: &mut App, column: u16, row: u16) {
        app.on_mouse(MouseEvent { kind: MouseEventKind::Down(MouseButton::Left), column, row, modifiers: KeyModifiers::NONE });
    }

    #[test]
    fn buttons_act_like_their_keys_and_the_top_one_wins() {
        let mut app = local();
        type_keys(&mut app, "e2e4");
        app.buttons = vec![
            (Rect::new(0, 0, 10, 1), Click::Key(KeyCode::Char('u'), true)),
            (Rect::new(20, 0, 10, 1), Click::Key(KeyCode::Char('r'), true)),
            (Rect::new(20, 0, 2, 1), Click::Key(KeyCode::Char('f'), true)),
        ];
        click_at(&mut app, 10, 0);
        click_at(&mut app, 5, 1);
        assert_eq!(app.game.history.len(), 1);
        click_at(&mut app, 9, 0);
        assert!(app.game.history.is_empty());
        click_at(&mut app, 21, 0);
        assert!(app.flipped && app.prompt.is_none());
        click_at(&mut app, 22, 0);
        assert_eq!(app.prompt, Some(Prompt::Resign));
    }

    #[test]
    fn a_click_reported_only_on_release_still_counts() {
        let mut app = local();
        app.buttons = vec![(Rect::new(0, 0, 5, 1), Click::Key(KeyCode::Char('f'), true))];
        let event = |kind| MouseEvent { kind, column: 1, row: 0, modifiers: KeyModifiers::NONE };
        app.on_mouse(event(MouseEventKind::Up(MouseButton::Left)));
        assert!(app.flipped);
        // The usual press and release is one click, not two.
        app.on_mouse(event(MouseEventKind::Down(MouseButton::Left)));
        app.on_mouse(event(MouseEventKind::Up(MouseButton::Left)));
        assert!(!app.flipped);
        app.on_mouse(event(MouseEventKind::Moved));
        app.on_mouse(event(MouseEventKind::Drag(MouseButton::Left)));
        assert!(!app.flipped);
    }

    #[test]
    fn promotion_asks_which_piece() {
        let mut app = local();
        app.game.board = crate::chess::Board::from_fen("7k/P7/8/8/8/8/8/K7 w - - 0 1").unwrap();
        type_keys(&mut app, "a7a8");
        assert_eq!(app.prompt, Some(Prompt::Promotion { from: sq(0, 6), to: sq(0, 7) }));
        type_keys(&mut app, "n");
        assert_eq!(sans(&app), ["a8=N"]);
        assert_eq!(app.prompt, None);
    }

    #[test]
    fn local_games_can_be_resigned_drawn_and_taken_back() {
        let mut app = local();
        type_keys(&mut app, "e2e4^u");
        assert!(app.game.history.is_empty());
        type_keys(&mut app, "e2e4^dy");
        assert_eq!(app.game.outcome, Some(Outcome::DrawAgreed));
        type_keys(&mut app, "^n");
        assert!(app.game.history.is_empty() && !app.over());
        type_keys(&mut app, "e2e4^ry");
        assert_eq!(app.game.outcome, Some(Outcome::Resigned(Color::White)));
        // Taking back a resignation reopens the game without undoing a move.
        type_keys(&mut app, "^r^d");
        assert_eq!((app.prompt, app.message.as_str()), (None, "The game is over"));
        type_keys(&mut app, "^u");
        assert_eq!((app.game.outcome, app.game.history.len()), (None, 1));
        type_keys(&mut app, "^u^u");
        assert_eq!((app.game.history.len(), app.message.as_str()), (0, "Nothing to take back"));
    }

    #[test]
    fn the_computer_answers_and_undo_takes_back_both_moves() {
        let mut app = App::new(true, Settings::default());
        app.level = Level::Beginner;
        app.start_computer();
        type_keys(&mut app, "e2e4");
        assert!(app.is_thinking());
        // Not this player's turn: typing a move does nothing.
        type_keys(&mut app, "d2d4");
        wait_for_reply(&mut app);
        assert_eq!(app.game.history.len(), 2);
        type_keys(&mut app, "^u");
        assert!(app.game.history.is_empty() && !app.is_thinking());
    }

    #[test]
    fn playing_black_lets_the_computer_open() {
        let mut app = App::new(true, Settings::default());
        app.level = Level::Beginner;
        app.side = Side::Black;
        app.start_computer();
        assert_eq!(app.bottom(), Color::Black);
        wait_for_reply(&mut app);
        assert_eq!(app.game.history.len(), 1);
        // Nothing of Black's to take back yet, so the opening move stays.
        type_keys(&mut app, "^u");
        assert_eq!(app.game.history.len(), 1);
    }

    /// Two whole programs talking over a real connection on this machine.
    #[test]
    fn two_apps_play_over_the_network() {
        let (mut host, mut guest) = (App::new(true, Settings::default()), App::new(true, Settings::default()));
        host.port = 46470;
        host.side = Side::Black;
        host.start_host();
        guest.address = "127.0.0.1:46470".into();
        guest.start_join();
        let settle = |a: &mut App, b: &mut App, done: &dyn Fn(&App, &App) -> bool| {
            let started = Instant::now();
            while !done(a, b) {
                a.tick();
                b.tick();
                assert!(started.elapsed() < Duration::from_secs(10), "timed out");
                thread::sleep(Duration::from_millis(5));
            }
        };
        settle(&mut host, &mut guest, &|a, b| matches!(a.screen, Screen::Game) && matches!(b.screen, Screen::Game));
        assert_eq!((host.me, guest.me), (Color::Black, Color::White));

        // The host is Black and may not move first.
        type_keys(&mut host, "e7e5");
        type_keys(&mut guest, "e2e4");
        settle(&mut host, &mut guest, &|a, _| a.game.history.len() == 1);
        type_keys(&mut host, "e7e5");
        settle(&mut host, &mut guest, &|_, b| b.game.history.len() == 2);
        assert_eq!(sans(&guest), ["e4", "e5"]);

        type_keys(&mut guest, "^d");
        settle(&mut host, &mut guest, &|a, _| a.prompt == Some(Prompt::DrawOffered));
        type_keys(&mut host, "n");
        settle(&mut host, &mut guest, &|_, b| b.message == "Draw declined");

        type_keys(&mut guest, "^ry");
        settle(&mut host, &mut guest, &|a, _| a.over());
        assert_eq!(host.game.outcome, Some(Outcome::Resigned(Color::Black)));

        type_keys(&mut host, "^n");
        type_keys(&mut guest, "^n");
        settle(&mut host, &mut guest, &|a, b| !a.over() && !b.over());
        assert_eq!((host.me, guest.me), (Color::White, Color::Black));

        drop(guest);
        settle(&mut host, &mut App::new(true, Settings::default()), &|a, _| a.over());
        assert_eq!(host.game.outcome, Some(Outcome::Abandoned(Color::White)));
    }
}
