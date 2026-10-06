# funchess

Chess for the terminal, on Linux and macOS.

- **Against the computer**, at four levels (beginner, easy, medium, hard), as White or Black.
- **Two players at one keyboard.**
- **Two players on two computers**, over a direct connection.

All the rules are in: castling, en passant, promotion, check, checkmate, stalemate,
and draws by repetition, the fifty-move rule and too few pieces.

## Build and run

Needs Rust 1.88 or newer.

    cargo build --release
    ./target/release/funchess

## Playing

    funchess                    choose from the menu
    funchess computer -l hard   play the computer (add --black to play Black)
    funchess local              two players at this keyboard
    funchess host               wait for a player on another computer
    funchess join 192.168.1.20  join the game hosted there

Move a piece by picking it and then picking where it goes: click with the mouse,
or move the marker with the arrow keys and press Enter, or type the two squares
(`e2` `e4`).

Commands are Ctrl with a letter, so no plain letter ever does anything but name a
square or answer a question.

| Key | |
|---|---|
| `Ctrl-U` | take back a move (not in a network game) |
| `Ctrl-R` | resign |
| `Ctrl-D` | offer a draw |
| `Ctrl-N` | new game; in a network game, a rematch with colors swapped |
| `Ctrl-F` or `Tab` | flip the board |
| `Ctrl-T` | next color theme |
| `Ctrl-P` | next way of drawing the pieces |
| `Ctrl-Q` or `Esc` | back to the menu |
| `Ctrl-C` | quit at once |
| `?` | help |

Everything can also be clicked: the lines of the main menu (a setting steps
forward, or back from its `<`), the commands beside the board, and
the answers in a question box.

## Looks

Six color themes: forest, wood, ocean, slate, plum and ruby. Five ways of drawing
the pieces:

| Style | |
|---|---|
| `shaded` | pixel art lit from the top left (the default) |
| `solid` | flat pixel art |
| `outlined` | flat pixel art with a strong rim |
| `symbols` | the font's own chess symbols |
| `letters` | K Q R B N P, for fonts without chess symbols |

Change either with `Ctrl-T` and `Ctrl-P`, in the menu (which shows a sample) or during a
game, or start with `--theme ocean --pieces outlined`. The choice is remembered in
`~/.local/state/funchess/settings`, along with the computer's level.

The board is as tall as the window allows. The players, the state of the game and
the commands are to its right, and the moves to its left when the window is wide
enough (otherwise they share the right side). Pixel art needs a window of at least
98 x 33 characters; in smaller ones (down to 58 x 9) the three pixel-art styles are
shown as chess symbols.

## Playing across two computers

One player runs `funchess host`, which shows the command for the other to type,
for example `funchess join 192.168.1.20`. The host picks the color (`--black`,
`--random`); the other player gets the one left.

The two programs talk directly to each other on TCP port 6464 (`funchess host 7000`
picks another). That works when both computers are on the same network, or on a
shared VPN such as Tailscale, or when the host's router forwards the port. There
is no server in between, so two home networks cannot reach each other without one
of those. The host's firewall must allow the port.

Each side checks the other's moves against the rules, and nothing but moves and
resign/draw/rematch messages is exchanged.

## Tests

    cargo test
