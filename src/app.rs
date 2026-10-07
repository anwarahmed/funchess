//! What the program is doing and how it reacts to keys, clicks, the computer
//! opponent finishing its thinking, and messages from the other computer.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use crate::chess::{Color, Game, Kind, Move, Outcome, Square, file_of, rank_of, sq, square_name};
use crate::engine::{self, Level};
use crate::fx::{self, MoveFx};
use crate::net::{self, Incoming, Link, Message, Pending};
use crate::sound::{Sound, Speaker};
use crate::theme::{Pieces, Settings, THEMES, Theme};

/// The computer's move is held back this long, so that it does not land the instant
/// the player lets go of their own piece.
const MIN_THINK: Duration = Duration::from_millis(450);
/// A hint comes from the computer playing at this level, whatever level the game is at:
/// strong enough to be worth following, and quick.
const HINT_LEVEL: Level = Level::Medium;
/// A win in this many moves or fewer earns the third star.
pub const QUICK_WIN: usize = 40;

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
    /// The game has just ended: the result, and what can be done next. Not a question,
    /// so a command closes it and acts.
    GameOver,
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
    /// The update switch, kept only so that saving the look does not lose it.
    auto_update: bool,
    /// Whether pieces are shown moving. Off, everything is where it belongs at once.
    pub animations: bool,
    /// The time everything that moves on screen goes by. It follows the real time
    /// (`advance`), and a test moves it by hand.
    pub clock: Duration,
    advanced: Instant,
    /// The last move, while it is being shown.
    pub fx: Option<MoveFx>,
    /// When the confetti for a win began, or will.
    pub party: Option<Duration>,
    /// Whether sounds are played. What plays them, if anything can, is `speaker`.
    pub sound: bool,
    pub speaker: Speaker,
    /// Sounds that wait for what they belong to: a piece landing, the result appearing.
    cues: Vec<(Duration, Sound)>,

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
    /// The move suggested to the player, its two squares marked until a move is made.
    pub hint: Option<Move>,
    hinting: Option<Thinking>,
    /// How often the player asked for a hint or took a move back in this game, which
    /// costs the second star.
    helped: u32,
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
            auto_update: settings.update,
            animations: settings.animations,
            clock: Duration::ZERO,
            advanced: Instant::now(),
            fx: None,
            party: None,
            sound: settings.sound,
            speaker: Speaker::silent(),
            cues: Vec::new(),
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
            hint: None,
            hinting: None,
            helped: 0,
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
            Settings { theme: self.theme, pieces: self.pieces, level: self.level, update: self.auto_update, animations: self.animations, sound: self.sound }
                .save(path);
        }
    }

    // ---- starting and leaving games ----

    fn start(&mut self, opponent: Opponent, me: Color) {
        self.thinking = None;
        self.fx = None;
        self.party = None;
        self.cues.clear();
        self.play(Sound::Start);
        self.helped = 0;
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
                let mut lines = vec![
                    format!("Waiting for the other player. You play {}.", self.me.name()),
                    String::new(),
                    "On the other computer, run:".into(),
                    format!("funchess join {address}{port}"),
                    String::new(),
                    "Both computers must be on the same network, and this".into(),
                    format!("one's firewall must let connections in on port {}.", self.port),
                ];
                // A firewall that drops the attempt is the usual reason nobody arrives.
                if let Some(command) = net::firewall_command(self.port) {
                    lines.extend([String::new(), "This computer's firewall is blocking that port.".into(), "To let the other player in, run:".into(), command]);
                }
                self.screen = Screen::Connecting(lines);
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
        self.hinting = None;
        self.fx = None;
        self.party = None;
        self.cues.clear();
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

    /// The color at the bottom of the board. Two players at one keyboard take turns
    /// sitting there, so the board turns to face whoever is to move, once the move
    /// before has been shown.
    pub fn bottom(&self) -> Color {
        let turn = self.game.board.turn;
        let near = match self.opponent {
            Opponent::Local if self.fx.is_some() => turn.other(),
            Opponent::Local => turn,
            _ => self.me,
        };
        if self.flipped { near.other() } else { near }
    }

    /// Whether the other computer is still there, in a network game.
    pub fn connected(&self) -> bool {
        self.link.is_some()
    }

    pub fn is_thinking(&self) -> bool {
        self.thinking.is_some()
    }

    /// Whether something on screen is moving quickly enough to want drawing often.
    pub fn animating(&self) -> bool {
        self.fx.is_some() || self.party.is_some()
    }

    /// The stars for beating the computer: for the win, for doing it without a hint
    /// or a take-back, and for doing it in `QUICK_WIN` moves or fewer.
    pub fn stars(&self) -> Option<[bool; 3]> {
        let won = matches!(self.opponent, Opponent::Computer(_)) && self.game.outcome.and_then(Outcome::winner) == Some(self.me);
        won.then(|| [true, self.helped == 0, self.game.history.len().div_ceil(2) <= QUICK_WIN])
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
        self.moved();
    }

    /// After a move was played, by anybody: it is shown, and then the housekeeping.
    fn moved(&mut self) {
        self.fx = match self.game.history.last() {
            Some(last) if self.animations => {
                let mate = matches!(self.game.outcome, Some(Outcome::Checkmate(_)));
                MoveFx::new(self.clock, &last.before, last.mv, &self.game.board, mate)
            }
            _ => None,
        };
        // One sound for the move, the most telling one, when the piece lands.
        if let Some(last) = self.game.history.last() {
            let (before, m) = (&last.before, last.mv);
            let castled = before.at(m.from).is_some_and(|p| p.kind == Kind::King) && file_of(m.from).abs_diff(file_of(m.to)) == 2;
            let sound = if m.promo.is_some() {
                Sound::Promote
            } else if self.game.board.in_check(self.game.board.turn) && !self.over() {
                Sound::Check
            } else if before.is_capture(m) {
                Sound::Capture
            } else if castled {
                Sound::Castle
            } else {
                Sound::Move
            };
            self.cues.push((self.fx.as_ref().map_or(self.clock, MoveFx::lands), sound));
        }
        self.after_move();
    }

    fn play(&mut self, sound: Sound) {
        if self.sound {
            self.speaker.play(sound);
        }
    }

    /// Switches sound on or off, for this game and the next ones, and says which. On,
    /// a move is heard, so that it can be told at once whether anything will be.
    fn toggle_sound(&mut self) {
        self.sound = !self.sound;
        self.remember();
        self.play(Sound::Move);
        self.message = match (self.sound, self.speaker.is_on()) {
            (false, _) => "Sound off",
            (true, true) => "Sound on",
            (true, false) => "Sound on, but nothing here can play it",
        }
        .into();
    }

    /// Plays the sounds whose time has come.
    fn play_cues(&mut self, now: Duration) {
        let due: Vec<Sound> = self.cues.iter().filter(|&&(when, _)| when <= now).map(|&(_, sound)| sound).collect();
        self.cues.retain(|&(when, _)| when > now);
        for sound in due {
            self.play(sound);
        }
    }

    /// Stops showing the last move: everything is where it ends up. True when there
    /// was something still moving.
    fn settle(&mut self) -> bool {
        self.party = self.party.map(|start| start.min(self.clock));
        // What was waiting for it to finish is heard now.
        self.play_cues(Duration::MAX);
        self.fx.take().is_some()
    }

    /// The end of the game is heard, and there is confetti when it was won by somebody
    /// at this keyboard.
    fn celebrate(&mut self) {
        let winner = self.game.outcome.and_then(Outcome::winner);
        let here = winner.is_some_and(|c| self.opponent == Opponent::Local || c == self.me);
        // Both wait for the last move to be shown.
        let when = self.fx.as_ref().map_or(self.clock, MoveFx::end);
        self.party = (here && self.animations).then_some(when);
        self.cues.push((
            when,
            if here {
                Sound::Win
            } else if winner.is_some() {
                Sound::Lose
            } else {
                Sound::Draw
            },
        ));
    }

    /// Housekeeping after any move, whoever made it.
    fn after_move(&mut self) {
        self.selected = None;
        self.typed_file = None;
        self.draw_offer_sent = false;
        if matches!(self.prompt, Some(Prompt::Promotion { .. } | Prompt::DrawOffered)) {
            self.prompt = None;
        }
        self.hint = None;
        self.hinting = None;
        self.party = None;
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
        if self.over() {
            self.prompt = Some(Prompt::GameOver);
            self.celebrate();
        }
    }

    /// Asks the computer what it would play here, for the player to see.
    fn ask_hint(&mut self) {
        if self.opponent == Opponent::Remote {
            self.message = "No hints in a network game".into();
        } else if !self.my_turn() {
            self.message = "Wait for your turn".into();
        } else if let Some(m) = self.hint {
            self.show_hint(m);
        } else if self.hinting.is_none() {
            let (board, keys, stop) = (self.game.board, self.game.keys(), Arc::new(AtomicBool::new(false)));
            let (tx, rx) = mpsc::channel();
            let stopped = stop.clone();
            thread::spawn(move || {
                let _ = tx.send(engine::best_move(&board, &keys, HINT_LEVEL, &stopped, seed()));
            });
            self.hinting = Some(Thinking { rx, stop, since: Instant::now() });
            self.helped += 1;
            self.message = "Thinking of a hint...".into();
        }
    }

    fn show_hint(&mut self, m: Move) {
        if self.hint.is_none() {
            self.play(Sound::Hint);
        }
        self.hint = Some(m);
        let kind = self.game.board.at(m.from).map_or("piece", |p| p.kind.name());
        self.message = format!("Try the {kind} from {} to {}", square_name(m.from), square_name(m.to));
    }

    fn undo(&mut self) {
        if self.opponent == Opponent::Remote {
            self.message = "No take-backs in a network game".into();
            return;
        }
        self.message.clear();
        self.fx = None;
        self.cues.clear();
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
        self.helped += 1;
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
        self.hint = None;
        self.hinting = None;
        self.fx = None;
        self.selected = None;
        // The result box repeats the message, so one left from earlier must not linger.
        self.message.clear();
        self.prompt = Some(Prompt::GameOver);
        self.celebrate();
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

    /// Brings the clock up to the real time.
    fn advance(&mut self) {
        let now = Instant::now();
        self.clock += now - self.advanced;
        self.advanced = now;
    }

    /// Called before every frame. True when something changed.
    pub fn tick(&mut self) -> bool {
        self.advance();
        // Whatever moves, pulses or counts dots needs drawing again, and stops by itself.
        let mut changed = self.animating() || self.hint.is_some() || self.thinking.is_some() || self.hinting.is_some();
        if self.fx.as_ref().is_some_and(|fx| self.clock >= fx.end()) {
            self.fx = None;
        }
        if self.party.is_some_and(|start| self.clock >= start + fx::CONFETTI) {
            self.party = None;
        }
        self.play_cues(self.clock);
        if let Some(thinking) = &self.thinking
            && thinking.since.elapsed() >= MIN_THINK
            && let Ok(reply) = thinking.rx.try_recv()
        {
            self.thinking = None;
            if let Some(m) = reply
                && self.game.play(m)
            {
                self.moved();
            }
            changed = true;
        }
        if let Some(reply) = self.hinting.as_ref().and_then(|hinting| hinting.rx.try_recv().ok()) {
            self.hinting = None;
            match reply {
                Some(m) => self.show_hint(m),
                None => self.message.clear(),
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
                    self.moved();
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
        self.advance();
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        // A key ends whatever was still moving and then does what it always does. The
        // result box is the exception: it only appears once the last move has been
        // shown, and a key pressed before that must not answer a box nobody has seen.
        if self.settle() && self.prompt == Some(Prompt::GameOver) && !ctrl {
            return;
        }
        // Every command is Ctrl with a letter, so that no plain key ever does anything
        // but move the marker, name a square or answer a question. Only the menu, where
        // nothing is typed, also takes its commands as plain letters.
        if ctrl {
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
            // The rest act on a game, and wait while a question is open. The box that
            // announces the result is not a question: it closes and the command acts.
            _ if !in_game || self.prompt.is_some_and(|p| p != Prompt::GameOver) => {}
            _ if self.prompt.take().is_some() => self.command(letter),
            'u' => self.undo(),
            'r' | 'd' | 'g' if self.over() => self.message = "The game is over".into(),
            'g' => self.ask_hint(),
            'r' => self.prompt = Some(Prompt::Resign),
            'd' => self.offer_draw(),
            'f' => self.flipped = !self.flipped,
            's' => self.toggle_sound(),
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
            KeyCode::Esc | KeyCode::Char('q' | 'Q') => self.quit = true,
            // Nothing is typed in the menu, so its commands need no Ctrl here.
            KeyCode::Char('t' | 'T') => self.change_theme(true),
            KeyCode::Char('p' | 'P') => self.change_pieces(true),
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
            // Only the keys it names close it, so that a key meant for the board,
            // pressed as the game ended, does not make the result vanish unread.
            Prompt::GameOver if yes || no || code == KeyCode::Char(' ') => self.prompt = None,
            Prompt::GameOver => {}
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
        self.advance();
        // As with a key, a click ends whatever was still moving, and then counts. A
        // click on a square does not when that makes the board turn round to the other
        // player: the square under it is no longer the one that was aimed at.
        let (facing, moving) = (self.bottom(), self.settle());
        let turned = moving && facing != self.bottom();
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
            None if matches!(self.screen, Screen::Game) && self.prompt.is_none() && !turned => {
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

    /// A two-player game in which nothing takes time to be shown.
    fn local() -> App {
        let mut app = App::new(true, Settings { animations: false, ..Settings::default() });
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
        // The cursor is on e4 and the board now faces Black, whose e-pawn is three
        // rows nearer: walk down to e7.
        for _ in 0..3 {
            key(&mut app, KeyCode::Down);
        }
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.selected, Some(sq(4, 6)));
        assert_eq!(app.targets().len(), 2);
        key(&mut app, KeyCode::Up);
        key(&mut app, KeyCode::Up);
        key(&mut app, KeyCode::Char(' '));
        assert_eq!(sans(&app), ["e4", "e5"]);
    }

    /// Two players at one keyboard: the board turns to face whoever is to move.
    #[test]
    fn the_board_faces_the_player_to_move_in_a_local_game() {
        let mut app = local();
        assert_eq!(app.bottom(), Color::White);
        type_keys(&mut app, "e2e4");
        assert_eq!(app.bottom(), Color::Black);
        type_keys(&mut app, "e7e5");
        assert_eq!(app.bottom(), Color::White);
        // Taking a move back turns it back, and flipping by hand shows the other side.
        type_keys(&mut app, "^u");
        assert_eq!(app.bottom(), Color::Black);
        type_keys(&mut app, "^f");
        assert_eq!(app.bottom(), Color::White);
        type_keys(&mut app, "e7e5");
        assert_eq!(app.bottom(), Color::Black);
        // Against the computer the board stays where the player sits.
        let mut app = App::new(true, Settings::default());
        app.level = Level::Beginner;
        app.start_computer();
        type_keys(&mut app, "e2e4");
        assert_eq!(app.bottom(), Color::White);
        wait_for_reply(&mut app);
        assert_eq!(app.bottom(), Color::White);
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
        // Black now sits at the bottom, with the h-file on the left.
        click(&mut app, 3, 6);
        click(&mut app, 3, 4);
        assert_eq!(sans(&app), ["e4", "e5"]);
        // Flipped by hand, White to move is at the top: d2 is in the second row.
        key(&mut app, KeyCode::Tab);
        click(&mut app, 4, 1);
        click(&mut app, 4, 3);
        assert_eq!(sans(&app), ["e4", "e5", "d4"]);
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
        assert_eq!(Settings::load(&path), Settings { theme: 2, pieces: Pieces::Letters, level: Level::Easy, update: true, animations: true, sound: true });
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
        // Nor while an address is being typed.
        type_keys(&mut app, "^qy");
        app.screen = Screen::Join;
        app.address.clear();
        type_keys(&mut app, "qtp");
        assert!(!app.quit && (app.theme, app.pieces) == (theme, pieces) && app.address == "qtp");
    }

    /// Nothing is typed in the menu, so there the commands work without Ctrl as well.
    #[test]
    fn the_menu_takes_its_commands_as_plain_letters() {
        let mut app = App::new(true, Settings::default());
        let (theme, pieces) = (app.theme, app.pieces);
        type_keys(&mut app, "tpT");
        assert!(app.theme == (theme + 2) % THEMES.len() && app.pieces != pieces && !app.quit);
        type_keys(&mut app, "^t");
        assert_eq!(app.theme, (theme + 3) % THEMES.len());
        type_keys(&mut app, "q");
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
        // A king and a knight cannot mate, so that was the last move.
        assert_eq!((app.prompt, app.game.outcome), (Some(Prompt::GameOver), Some(Outcome::InsufficientMaterial)));
    }

    /// The end of a game is announced in a box, which the commands work through.
    #[test]
    fn the_end_of_a_game_is_announced() {
        let mut app = local();
        type_keys(&mut app, "f2f3e7e5g2g4d8h4");
        assert_eq!((app.game.outcome, app.prompt), (Some(Outcome::Checkmate(Color::Black)), Some(Prompt::GameOver)));
        // Keys meant for the board do not close it; Enter does.
        type_keys(&mut app, "e2x?");
        key(&mut app, KeyCode::Up);
        assert_eq!(app.prompt, Some(Prompt::GameOver));
        key(&mut app, KeyCode::Enter);
        assert!(app.prompt.is_none() && app.over());
        // A command closes it and acts: here the mate is taken back, and given again.
        type_keys(&mut app, "^ud8h4");
        assert_eq!(app.prompt, Some(Prompt::GameOver));
        type_keys(&mut app, "^u");
        assert!(app.prompt.is_none() && !app.over() && app.game.history.len() == 3);
        type_keys(&mut app, "d8h4^n");
        assert!(app.prompt.is_none() && !app.over() && app.game.history.is_empty());
        // Resigning and agreeing a draw announce it too, and Ctrl-Q leaves without asking.
        type_keys(&mut app, "^u^ue2e4^ry");
        assert_eq!((app.prompt, app.message.as_str()), (Some(Prompt::GameOver), ""));
        type_keys(&mut app, "^q");
        assert!(matches!(app.screen, Screen::Menu) && app.prompt.is_none());
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

    /// Lets everything that is moving finish.
    fn let_it_finish(app: &mut App) {
        app.clock += Duration::from_secs(10);
        app.tick();
    }

    #[test]
    fn a_move_is_shown_and_a_key_or_a_click_cuts_it_short() {
        let mut app = App::new(true, Settings::default());
        app.start_local();
        app.geometry = Some(Geometry { x: 10, y: 5, cell_w: 8, cell_h: 4 });
        let click = |app: &mut App, col: u16, row: u16| click_at(app, 10 + col * 8 + 3, 5 + row * 4 + 1);
        type_keys(&mut app, "e2e4");
        // Played at once, and shown for a while: the board waits before it turns.
        assert!(app.fx.is_some() && app.animating() && sans(&app) == ["e4"]);
        assert_eq!(app.bottom(), Color::White);
        assert!(app.tick(), "there is something to draw");
        let_it_finish(&mut app);
        assert!(app.fx.is_none() && !app.animating() && app.bottom() == Color::Black);
        assert!(!app.tick());

        // A key ends it and then acts: the marker moves on the board as it now faces.
        type_keys(&mut app, "e7e5");
        assert!(app.fx.is_some() && app.bottom() == Color::Black);
        key(&mut app, KeyCode::Up);
        assert!(app.fx.is_none() && app.bottom() == Color::White);
        assert_eq!(app.cursor, sq(4, 5));

        // A click that lands while the board is about to turn is dropped, since the
        // square under it is about to be another one. The next one counts.
        type_keys(&mut app, "g1f3");
        click(&mut app, 4, 6);
        assert!(app.fx.is_none() && app.selected.is_none());
        click(&mut app, 4, 6);
        assert_eq!(app.selected, Some(sq(3, 6)));
        // A button is where it was, so a click on one always counts.
        type_keys(&mut app, "d7d5");
        app.buttons = vec![(Rect::new(0, 0, 5, 1), Click::Key(KeyCode::Char('f'), true))];
        click_at(&mut app, 1, 0);
        assert!(app.fx.is_none() && app.flipped);
        app.buttons.clear();

        // Against the computer the board stays, so a click during its move counts.
        let mut app = App::new(true, Settings::default());
        app.level = Level::Beginner;
        app.start_computer();
        app.geometry = Some(Geometry { x: 10, y: 5, cell_w: 8, cell_h: 4 });
        type_keys(&mut app, "e2e4");
        wait_for_reply(&mut app);
        assert!(app.fx.is_some());
        click(&mut app, 3, 6);
        assert_eq!((app.fx.is_none(), app.selected), (true, Some(sq(3, 1))));

        // Switched off, nothing is ever on its way.
        let mut app = local();
        type_keys(&mut app, "e2e4");
        assert!(app.fx.is_none() && !app.animating() && !app.tick());
        type_keys(&mut app, "f7f6d2d4g7g5d1h5");
        assert!(app.over() && app.party.is_none());
    }

    #[test]
    fn the_result_is_not_answered_before_it_is_seen() {
        let mut app = App::new(true, Settings::default());
        app.start_local();
        type_keys(&mut app, "f2f3e7e5g2g4d8h4");
        assert!(app.over() && app.fx.is_some() && app.prompt == Some(Prompt::GameOver));
        // Enter while the queen is still on her way shows the box instead of closing it.
        key(&mut app, KeyCode::Enter);
        assert!(app.fx.is_none() && app.prompt == Some(Prompt::GameOver));
        // Somebody at this keyboard won, so there is confetti, for a while.
        assert!(app.party.is_some() && app.animating());
        let_it_finish(&mut app);
        assert!(app.party.is_none() && !app.animating());
        key(&mut app, KeyCode::Enter);
        assert!(app.prompt.is_none());
        // A command is never dropped: here the mate is taken back while it is shown.
        type_keys(&mut app, "^ud8h4^u");
        assert!(!app.over() && app.fx.is_none() && app.party.is_none() && app.prompt.is_none());
        // No confetti for losing to the computer, nor for a draw.
        let mut app = App::new(true, Settings::default());
        app.start_computer();
        type_keys(&mut app, "^ry");
        assert!(app.over() && app.party.is_none());
        let mut app = App::new(true, Settings::default());
        app.start_local();
        type_keys(&mut app, "e2e4^dy");
        assert!(app.over() && app.party.is_none());
    }

    #[test]
    fn a_hint_is_a_legal_move_and_costs_a_star() {
        let wait_for_hint = |app: &mut App| {
            let started = Instant::now();
            while app.hint.is_none() {
                app.tick();
                assert!(started.elapsed() < Duration::from_secs(20), "no hint came");
                thread::sleep(Duration::from_millis(10));
            }
        };
        let mut app = local();
        type_keys(&mut app, "^g");
        assert_eq!(app.message, "Thinking of a hint...");
        wait_for_hint(&mut app);
        let hint = app.hint.unwrap();
        assert!(app.game.board.legal_moves().contains(&hint));
        assert!(app.message.starts_with("Try the ") && app.message.contains(&square_name(hint.to)), "{}", app.message);
        // Picking a piece clears the words and keeps the squares; asking again says it again.
        type_keys(&mut app, "a2^g");
        assert!(app.message.starts_with("Try the ") && app.hint == Some(hint));
        // A move ends it.
        type_keys(&mut app, "a3");
        assert!(app.hint.is_none() && sans(&app) == ["a3"]);
        // There is nothing to suggest once the game is over, or to the player waiting,
        // and no help in a network game.
        type_keys(&mut app, "^ry^g");
        assert_eq!(app.message, "The game is over");
        let mut app = local();
        app.opponent = Opponent::Remote;
        type_keys(&mut app, "^g");
        assert_eq!(app.message, "No hints in a network game");
        app.opponent = Opponent::Computer(Level::Beginner);
        app.me = Color::Black;
        type_keys(&mut app, "^g");
        assert_eq!(app.message, "Wait for your turn");

        // Mate in one against the computer: three stars, or two after asking how.
        let mate_in_one = |ask: bool| {
            let mut app = App::new(true, Settings { animations: false, ..Settings::default() });
            app.start_computer();
            app.game.board = crate::chess::Board::from_fen("6k1/5ppp/8/8/8/8/8/R3K3 w - - 0 1").unwrap();
            assert_eq!(app.stars(), None);
            if ask {
                type_keys(&mut app, "^g");
                wait_for_hint(&mut app);
                assert_eq!(app.hint.map(Move::uci).as_deref(), Some("a1a8"));
            }
            type_keys(&mut app, "a1a8");
            assert_eq!(app.game.outcome, Some(Outcome::Checkmate(Color::White)));
            app
        };
        assert_eq!(mate_in_one(false).stars(), Some([true, true, true]));
        assert_eq!(mate_in_one(true).stars(), Some([true, false, true]));
        // Winning because the computer has no say is not possible; losing earns none.
        let mut app = App::new(true, Settings::default());
        app.start_computer();
        type_keys(&mut app, "^ry");
        assert_eq!(app.stars(), None);
        assert_eq!(local().stars(), None);
    }

    #[test]
    fn what_happens_is_heard_when_it_is_seen() {
        // Nothing moving: every sound comes at once.
        let mut app = local();
        assert_eq!(app.speaker.heard, [Sound::Start]);
        type_keys(&mut app, "e2e4d7d5e4d5");
        app.tick();
        assert_eq!(app.speaker.heard[1..], [Sound::Move, Sound::Move, Sound::Capture]);
        // Taking it back is silent.
        type_keys(&mut app, "^u");
        app.tick();
        assert_eq!(app.speaker.heard.len(), 4);

        let heard_after = |fen: &str, typed: &str| {
            let mut app = local();
            app.game.board = crate::chess::Board::from_fen(fen).unwrap();
            type_keys(&mut app, typed);
            app.tick();
            app.speaker.heard[1..].to_vec()
        };
        assert_eq!(heard_after("r3k3/8/8/8/8/8/8/R3K2R w KQq - 0 1", "e1g1"), [Sound::Castle]);
        assert_eq!(heard_after("4k3/8/8/3p4/4P3/8/8/4K3 w - - 0 1", "e4d5"), [Sound::Capture]);
        assert_eq!(heard_after("r3k3/8/8/8/8/8/8/R3K2R w KQq - 0 1", "h1h8"), [Sound::Check]);
        assert_eq!(heard_after("7k/P7/8/8/8/8/8/K7 w - - 0 1", "a7a8q"), [Sound::Promote]);
        // The end of a game: won at this keyboard, drawn, and lost to the computer.
        assert_eq!(heard_after("6k1/5ppp/8/8/8/8/8/R3K3 w - - 0 1", "a1a8"), [Sound::Move, Sound::Win]);
        assert_eq!(heard_after("7k/P7/8/8/8/8/8/K7 w - - 0 1", "a7a8n"), [Sound::Promote, Sound::Draw]);
        assert_eq!(heard_after("8/8/8/8/8/8/8/K6k w - - 0 1", "^ry"), [Sound::Win]);
        let mut app = App::new(true, Settings::default());
        app.start_computer();
        type_keys(&mut app, "^ry");
        app.tick();
        assert_eq!(app.speaker.heard, [Sound::Start, Sound::Lose]);

        // With pieces moving, the sound waits for the piece to land and the result
        // for the king to fall.
        let mut app = App::new(true, Settings::default());
        app.start_local();
        type_keys(&mut app, "f2f3e7e5g2g4");
        assert_eq!(app.speaker.heard, [Sound::Start, Sound::Move, Sound::Move], "each key ended the move before it");
        app.tick();
        assert_eq!(app.speaker.heard.len(), 3, "the pawn is still on its way");
        app.clock += Duration::from_millis(400);
        app.tick();
        assert_eq!(app.speaker.heard.len(), 4);
        type_keys(&mut app, "d8h4");
        app.clock += Duration::from_millis(400);
        app.tick();
        assert_eq!(app.speaker.heard[4..], [Sound::Move]);
        let_it_finish(&mut app);
        assert_eq!(app.speaker.heard[4..], [Sound::Move, Sound::Win]);
        // Cut short by a key, both are heard at once and never twice.
        type_keys(&mut app, "^ud8h4x");
        let_it_finish(&mut app);
        assert_eq!(app.speaker.heard[6..], [Sound::Move, Sound::Win]);
    }

    #[test]
    fn a_hint_is_heard_once() {
        let mut app = local();
        type_keys(&mut app, "^g");
        let started = Instant::now();
        while app.hint.is_none() {
            app.tick();
            assert!(started.elapsed() < Duration::from_secs(20), "no hint came");
            thread::sleep(Duration::from_millis(10));
        }
        type_keys(&mut app, "^g^g");
        assert_eq!(app.speaker.heard, [Sound::Start, Sound::Hint]);
    }

    #[test]
    fn sound_is_switched_off_and_on_in_the_game_and_remembered() {
        let path = std::env::temp_dir().join(format!("funchess-sound-key-test-{}/settings", std::process::id()));
        let mut app = local();
        app.settings_path = Some(path.clone());
        type_keys(&mut app, "e2e4^s");
        assert_eq!((app.sound, app.message.as_str()), (false, "Sound off"));
        assert!(!Settings::load(&path).sound);
        // Nothing is asked of the speaker while it is off, not even for the end of a game.
        type_keys(&mut app, "e7e5^g^ry");
        app.tick();
        assert_eq!(app.speaker.heard, [Sound::Start, Sound::Move]);
        // Switched on again, a move is heard at once. Here nothing can play it, and it says so.
        type_keys(&mut app, "^s");
        assert_eq!((app.sound, app.message.as_str()), (true, "Sound on, but nothing here can play it"));
        assert!(Settings::load(&path).sound);
        assert_eq!(app.speaker.heard, [Sound::Start, Sound::Move, Sound::Move]);
        // With something to play it (a program that does nothing), it only says "on".
        app.speaker = Speaker::through("/bin/true".into(), &[], path.parent().unwrap().join("sounds"));
        type_keys(&mut app, "^s^s");
        assert_eq!(app.message, "Sound on");
        // It is a command of the game: in the menu, and while a question is open, it waits.
        type_keys(&mut app, "^n^r^s");
        assert!(app.sound && app.prompt == Some(Prompt::Resign));
        type_keys(&mut app, "n^q^s");
        assert!(app.sound && matches!(app.screen, Screen::Menu));
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
