mod app;
mod chess;
mod engine;
mod fx;
mod net;
mod sound;
mod theme;
mod ui;
mod update;

use std::io::{self, IsTerminal, Write, stdout};
use std::process::ExitCode;
use std::time::Duration;

use ratatui::crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind};
use ratatui::crossterm::execute;

use app::{App, Side};
use engine::Level;
use theme::{Pieces, Settings};

const USAGE: &str = "\
funchess - chess for the terminal

Usage:
  funchess                    choose from the menu
  funchess computer           play the computer
  funchess local              two players at this keyboard
  funchess host [PORT]        wait for a player on another computer
  funchess join ADDRESS       join a game hosted at ADDRESS (name or IP, optionally :PORT)
  funchess update             check for a newer release now and install it
  funchess update off | on    stop, or resume, checking when the game starts
  funchess --help | --version

Options:
  -l, --level LEVEL           beginner, easy (default), medium or hard
  -w, --white                 play White (default)
  -b, --black                 play Black
  -r, --random                play either color, picked at random
  -t, --theme NAME            forest, wood, ocean, slate, plum, ruby, candy, sunset,
                              neon, tropical, citrus or aurora
  -p, --pieces STYLE          solid, outlined, shaded (pixel art, in a large enough
                              window), symbols, or letters if your font lacks
                              chess symbols
  -a, --animations on|off     whether pieces slide, burst and fall (on by default)
  -s, --sound on|off          whether moves, captures and wins are heard (on by default)

The theme, the piece style, the level, the animations and the sound are remembered
for next time, in $XDG_STATE_HOME/funchess (~/.local/state/funchess).
The color applies to `computer` and `host`; the joining player gets the other one.
Network games use a direct connection on port 6464 unless another is given: both
computers must be on the same network, or the host must be reachable on that port.

In the game: click a piece and then a square, or move the marker with the arrow
keys and press Enter, or type the two squares (e2 e4).
Commands are Ctrl with a letter (shown as ^):
  ^G hint     ^U undo      ^R resign          ^D offer a draw
  ^N new game ^F flip the board               ^T theme       ^P pieces
  ^S sound on or off       ^A animations on or off
  ^Q menu     ^C quit      ?  help
In the menu T, P and Q work without Ctrl.

Environment:
  FUNCHESS_NO_UPDATE          set to skip the update check for one run
  FUNCHESS_NO_SOUND           set to play no sound for one run
  FUNCHESS_LOG                a file to record every key and mouse event in
";

fn fail(msg: &str) -> ExitCode {
    eprintln!("funchess: {msg}");
    ExitCode::FAILURE
}

fn main() -> ExitCode {
    let truecolor = matches!(std::env::var("COLORTERM").as_deref(), Ok("truecolor" | "24bit"));
    let settings_path = Settings::path();
    let mut settings = Settings::load(&settings_path);
    let mut app = App::new(truecolor, settings);
    app.settings_path = Some(settings_path.clone());
    let mut command = None;
    let mut operand: Option<String> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "-V" | "-v" | "--version" => {
                println!("funchess {} ({})", update::VERSION, if update::COMMIT.is_empty() { "unknown commit" } else { update::COMMIT });
                return ExitCode::SUCCESS;
            }
            "update" if command.is_none() => {
                return match args.next().as_deref() {
                    None => update::command().map_or_else(|e| fail(&e), |()| ExitCode::SUCCESS),
                    Some(switch @ ("on" | "off")) => {
                        settings.update = switch == "on";
                        settings.save(&settings_path);
                        println!("The update check at startup is {switch}.");
                        ExitCode::SUCCESS
                    }
                    Some(_) => fail("'update' takes on, off or nothing"),
                };
            }
            "-l" | "--level" => match args.next().as_deref().and_then(Level::parse) {
                Some(level) => app.level = level,
                None => return fail("--level needs one of: beginner, easy, medium, hard"),
            },
            "-w" | "--white" => app.side = Side::White,
            "-b" | "--black" => app.side = Side::Black,
            "-r" | "--random" => app.side = Side::Random,
            "-t" | "--theme" => match args.next().as_deref().and_then(theme::find_theme) {
                Some(theme) => app.theme = theme,
                None => return fail("--theme needs one of: forest, wood, ocean, slate, plum, ruby, candy, sunset, neon, tropical, citrus, aurora"),
            },
            "-p" | "--pieces" => match args.next().as_deref().and_then(Pieces::parse) {
                Some(pieces) => app.pieces = pieces,
                None => return fail("--pieces needs one of: solid, outlined, shaded, symbols, letters"),
            },
            "-a" | "--animations" => match args.next().as_deref() {
                Some(switch @ ("on" | "off")) => {
                    app.animations = switch == "on";
                    settings.animations = app.animations;
                    settings.save(&settings_path);
                }
                _ => return fail("--animations needs on or off"),
            },
            "-s" | "--sound" => match args.next().as_deref() {
                Some(switch @ ("on" | "off")) => {
                    app.sound = switch == "on";
                    settings.sound = app.sound;
                    settings.save(&settings_path);
                }
                _ => return fail("--sound needs on or off"),
            },
            "--ascii" => app.pieces = Pieces::Letters,
            "computer" | "local" | "host" | "join" if command.is_none() => command = Some(arg),
            _ if !arg.starts_with('-') && matches!(command.as_deref(), Some("host" | "join")) && operand.is_none() => operand = Some(arg),
            _ => return fail(&format!("unknown argument '{arg}'")),
        }
    }
    if command.as_deref() == Some("host")
        && let Some(port) = &operand
    {
        match port.parse() {
            Ok(port) if port > 0 => app.port = port,
            _ => return fail("the port must be a number from 1 to 65535"),
        }
    }
    if command.as_deref() == Some("join") && operand.is_none() {
        return fail("join needs the host's address, as in 'funchess join 192.168.1.20'");
    }
    if !io::stdin().is_terminal() || !stdout().is_terminal() {
        return fail("this is an interactive game and needs a terminal");
    }
    update::before_start(settings.update);
    // Found even while sound is off, so that Ctrl-S has something to switch on.
    if std::env::var_os("FUNCHESS_NO_SOUND").is_none() {
        app.speaker = sound::Speaker::find(update::state_dir().join("sounds"));
    }

    // Installed before ratatui's hook, which restores the terminal and then calls this one.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // ratatui's hook does not know the mouse was captured.
        let _ = execute!(stdout(), DisableMouseCapture);
        default_hook(info)
    }));

    match (command.as_deref(), operand) {
        (None, _) => {}
        (Some("computer"), _) => app.start_computer(),
        (Some("local"), _) => app.start_local(),
        (Some("host"), _) => app.start_host(),
        (Some("join"), Some(address)) => {
            app.address = address;
            app.start_join();
        }
        (Some(_), _) => unreachable!("the commands are checked above"),
    }
    match run(&mut app) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(&e.to_string()),
    }
}

/// How long the loop waits for a key before checking on the computer opponent and
/// the network again.
const POLL: Duration = Duration::from_millis(50);
/// The same while a piece is moving, so that it moves smoothly.
const FRAME: Duration = Duration::from_millis(16);

fn run(app: &mut App) -> io::Result<()> {
    // Raw mode, the alternate screen, and a panic hook that undoes both.
    let mut terminal = ratatui::init();
    // For clicking pieces. While captured, selecting text needs shift (option on macOS).
    execute!(stdout(), EnableMouseCapture)?;
    // FUNCHESS_LOG=file records every key and mouse event as the program receives it,
    // for working out why a terminal's input is not doing what it should.
    let mut log = std::env::var_os("FUNCHESS_LOG").and_then(|path| std::fs::File::create(path).ok());
    let mut redraw = true;
    let result = loop {
        redraw |= app.tick();
        if redraw && let Err(e) = terminal.draw(|f| ui::draw(f, app)) {
            break Err(e);
        }
        redraw = false;
        if app.quit {
            break Ok(());
        }
        match event::poll(if app.animating() { FRAME } else { POLL }) {
            Ok(false) => {}
            Ok(true) => match event::read().inspect(|event| {
                if let Some(log) = &mut log {
                    let _ = writeln!(log, "{event:?}");
                }
            }) {
                Ok(Event::Key(key)) if key.kind != KeyEventKind::Release => {
                    app.on_key(key);
                    redraw = true;
                }
                Ok(Event::Mouse(mouse)) => {
                    app.on_mouse(mouse);
                    redraw = true;
                }
                Ok(_) => redraw = true,
                Err(e) => break Err(e),
            },
            Err(e) => break Err(e),
        }
    };
    let _ = execute!(stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}
