# funchess

Chess for the terminal (TUI): against a built-in computer opponent, two players at one
keyboard, or two computers over a direct TCP connection. Keyboard and mouse both do
everything. Rust + ratatui, targets macOS and Linux. `README.md` is for players; this
file is for whoever changes the code.

## Commands

```sh
cargo run --release                          # play (from a checkout it never updates itself)
cargo test                                   # unit tests: rules (perft), engine, network, app, drawing, settings, update
cargo clippy --all-targets -- -D warnings    # CI fails on any warning
cargo fmt                                    # rustfmt.toml: max_width 160
cargo build --release && tests/e2e.sh        # the built program in tmux, install.sh, the updater
FUNCHESS_LOG=/tmp/events.log cargo run --release   # record every key and mouse event received
```

The dev profile is built with `opt-level = 2`: unoptimized, the computer opponent is
too slow to play and the perft test takes minutes.

`install.sh` (POSIX sh, macOS + Linux) downloads the latest release binary into
`~/.local/bin` (`FUNCHESS_BIN_DIR` overrides) and verifies its checksum. `--source`
builds instead (the checkout it is in, else a fresh clone), and it falls back to that
when no binary exists for the platform. `--link` symlinks to the checkout's build,
`--uninstall` removes it. It never installs Rust and never edits shell profiles.
`FUNCHESS_RELEASE_URL` points it, and the self-updater, at another download base (a
`file://` directory holding `VERSION`, `SHA256SUMS` and a binary; `tests/e2e.sh` makes
such directories).

## Workflow

- **`main` only accepts pull requests** (GitHub ruleset "Main"): no direct pushes, no
  force-pushes, no deletion, no bypass for anyone. A PR needs these checks to pass,
  matched by job name: `test (ubuntu-latest)`, `test (macos-latest)`, `msrv`,
  `release checklist`. Renaming a CI job means updating the ruleset or PRs wait forever.
  PRs are squash-merged.
- **Local layout.** The user keeps this repo as a bare clone with one worktree per
  branch: `~/Developer/GitHub/anwarahmed/funchess/main` plus a sibling directory per
  feature branch (`git worktree add -b <branch> <branch> origin/main` from the bare
  repo). Remove the worktree and branch after the PR merges, then fast-forward `main`.
- **Releasing** is merging a version bump; a merge without one publishes nothing.
  **Follow [RELEASING.md](RELEASING.md) every time, every step.** The release PR's
  description must carry its checklist with every line ticked (`gh pr create --body`
  does not add it for you), or the `release checklist` check fails. Tick a line only
  after doing what it says. Then do its "After merging" steps and report each one.
- **Sibling repo:** https://github.com/anwarahmed/homebrew-tap holds the generated
  Homebrew formula (`Formula/funchess.rb`, written by its
  `scripts/formulae/funchess.sh`). It takes direct pushes, because its bot commits
  formulae to `main`.
- **Sister projects:** wordl and typeshelf (same owner) share this release workflow,
  `install.sh` and `src/update.rs` design. A fix to any of those in one project belongs
  in the others in the same sitting.

## Architecture

Single binary crate, no async. Threads are used in exactly two places: the computer
opponent's search, and the network reader/acceptor; both report over an `mpsc` channel
that `App::tick` polls every frame. One file per concern in `src/`:

| File        | Role |
|-------------|------|
| `main.rs`   | CLI, terminal setup/teardown, the event loop (50 ms poll) |
| `chess.rs`  | The rules: `Board` (mailbox, `Copy`), move generation, SAN, `Game` with history and outcomes. Knows nothing else |
| `engine.rs` | The computer: alpha-beta with quiescence over material + piece-square tables; `Level` |
| `net.rs`    | Two computers: `host`/`join` give a `Pending`, which becomes a `Link`; one text line per message |
| `app.rs`    | `App` state; every key, click, engine reply and network message, and what each does |
| `ui.rs`     | All drawing, the layout, and the list of clickable rectangles (`App::buttons`) |
| `theme.rs`  | Color themes, piece styles, and `Settings` (the `key=value` file that remembers them) |
| `update.rs` | Self-update, and `state_dir()` |

### Patterns to keep

- **`chess.rs` is the only judge of legality.** The UI, the engine and the network all
  go through `Board::legal_moves` / `Game::play`. A move from the network is played
  with `Game::play`, which refuses an illegal one; then the game is stopped as
  abandoned. Neither side trusts the other.
- **A move is `from`, `to`, `promo` and nothing else.** Castling is the king moving two
  files, en passant a pawn capturing onto the empty en passant square. `Board::play`
  works both out. `Board::ep` is only set when an enemy pawn stands beside the pawn
  that advanced, so that `Board::key` is right for the repetition rule.
- **The perft test is the safety net** for move generation. Any change to `chess.rs`
  must keep `perft_counts_match_the_published_ones` passing; the counts are the
  published ones and are never to be edited to fit.
- **Every command is Ctrl + a letter** (`App::command`). Plain letters only name
  squares (`a`-`h`, `1`-`8`) or answer a question box. The user asked for this after
  `q` (menu) clashed with typing; do not add plain-letter commands. The main menu is
  the exception, also at the user's request: nothing is typed there, so `T`, `P` and
  `Q` work alone (`App::menu_key`) and are shown without the `^`. Ctrl still works.
  Ctrl-B is avoided because it is tmux's prefix, and Ctrl-H/I/M are Backspace/Tab/Enter
  to a terminal.
- **Everything clickable is in `App::buttons`**, rebuilt by `ui::draw` every frame as
  `(Rect, Click)`. A `Click::Key` button does exactly what its key does, by calling
  `on_key`, so mouse and keyboard cannot drift apart. Board squares are the one
  exception (`App::geometry` and `square_at`). While a question box is open only its
  own buttons are registered. Anything new on screen that acts must have both a key
  and a button; the user asked for the interface to be fully usable with either.
- **A command that can do nothing says why** in `App::message` ("Nothing to take
  back", "The game is over"). A silent no-op looks like a broken click.
- **The layout is computed from the window size on every frame** (`ui::game`): the
  board is as tall as the window allows (one row kept for the file letters), the
  panel with players, status and commands is to its right, and the moves go to its
  left when there is room, else into the right panel. The user asked for the board
  to use the height and for the sides to hold everything else.
- **The menu always shows its nine choices** (`ui::menu`). A short window gives up, in
  order, the empty rows, the title, the keys line and the error line; a narrow one
  gets a shorter keys line. It fits whole at 39 x 12 and keeps every choice at 9 rows,
  the shortest window a game fits in. The user asked for it to fit at 39 x 12.
- **Pixel art** is drawn with `▀`, foreground the upper pixel and background the
  lower. A square of height `h` rows is `2h` columns wide, so `2h` pixels square.
  The art exists at 8, 10 and 12 pixels (`SMALL`, `MEDIUM`, `LARGE`); the largest
  size or whole multiple that fits is centered. Rims and shading are worked out from
  the shape (`Cell::pixels`), not drawn into the art. Below 4 rows a square holds a
  chess symbol or a letter.
- **Colors are RGB in `theme.rs`** and go through `ui::paint`, which sends the nearest
  of 256 colors when the terminal does not announce truecolor (`COLORTERM`).
- **Display uses text characters only.** No terminal image protocols: the user's
  terminal does not show them.
- **The network protocol** is one line of text per message after a greeting line
  (`funchess 1 white|black|join`). An unknown line is skipped; a first line that is
  not the greeting ends the connection with a reason. Raise the number in `HELLO`
  whenever the messages change incompatibly.
- **Settings** are one file, `settings`, in the state directory; `App::remember`
  rewrites it whenever the theme, the piece style or the level changes. Tests set
  `settings_path` to a temporary file or leave it `None`; nothing in the tests may
  touch the user's real file.

## Decisions and why

- **Built-in engine, no Stockfish.** The user chose a self-contained game over a
  stronger opponent that needs a separate install. It is a plain alpha-beta without a
  transposition table: "hard" searches as deep as three seconds allow. The two weak
  levels search one or two plies exactly and then add random error to each move's
  score, which makes natural-looking mistakes without missing a mate in one.
- **Direct TCP, no relay server.** The user chose this over hosting a server. It
  works on one network, a VPN, or a forwarded port (6464 by default), and not between
  two home networks otherwise. The README says so plainly.
- **No take-backs in a network game**, and the computer never accepts a draw. Both are
  simplifications, not principles.
- **With two players at one keyboard the board faces whoever is to move**
  (`App::bottom`), at the user's request. It is worked out from the position, so a
  take-back turns it back, and Ctrl-F shows the other side until it is pressed again.
  Against the computer or over the network the board stays where the player sits.
- **Undo after a resignation or agreed draw** reopens the game without removing a
  move; against the computer, undo goes back to the player's previous turn (their
  move and the reply).
- **A click counts on press; a release with no press before it counts too**, in case
  a terminal only reports the release. The user reported clicks not registering in
  foot under tmux; it could not be reproduced (not even through a real tmux client
  with their configuration), so this and `FUNCHESS_LOG` were added. If it comes back,
  ask for a `FUNCHESS_LOG` recording before guessing.
- **Mouse capture uses crossterm's `EnableMouseCapture`** in `main.rs`, with a panic
  hook that turns it off, since ratatui's own hook does not know about it.
- **Self-update** follows wordl's design exactly: a `VERSION` file from the latest
  release (no GitHub API, so no rate limit), a `share/funchess/managed-by` marker by
  which Homebrew and pacman switch it off, and a check at most once a day
  (`last-update-check` in the state directory). The release asset names
  (`funchess-<target>`, `SHA256SUMS`, `VERSION`, `PKGBUILD`) are a contract with
  `install.sh`, the updater, the tap's formula generator and the AUR package.

## Verifying changes

- `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`.
- `cargo build --release && tests/e2e.sh`: the real program in tmux on a private
  socket (keyboard, mouse, the computer, two copies over a real connection), then
  `install.sh` and the updater against made-up releases served from `file://`.
- Look at it. To see the pixel art without a screen, capture a run with
  `tmux -L <private socket> capture-pane -p -e` and turn the half blocks into an
  image.
- **Never run `tmux kill-server` on the default socket, and never start test sessions
  there.** The user works inside tmux; doing so once closed their terminal. Use
  `tmux -L <name>` for everything, as `tests/e2e.sh` does.
- A real tmux client can be driven through a pseudo-terminal (`pty.fork`, then write
  SGR mouse sequences to it) when a bug only shows with tmux's own mouse handling.
  Remember the status line: with `status-position top` the pane starts on row 2.

## Known gaps and ideas

- Run on a real Mac by the user, in Ghostty (October 2026): it works well there. No
  other macOS terminal has been tried.
- Never run between two physically separate computers; CI runs the network tests on
  one machine.
- The AUR package `funchess-bin` is rendered for each release but not pushed: the user
  has no AUR account.
- No README screenshot yet.
- No clocks, no saved or resumable games, no PGN export, no way to set up a position.
- The engine has no opening book, transposition table or draw-offer judgement.
- The 8-pixel art is rough; the rook and queen are less distinct than at larger sizes.
- Squares grow in whole rows, so some window heights leave rows unused above and
  below the board.
