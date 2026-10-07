#!/bin/sh
# End-to-end tests: the built program, run the way a user runs it.
#
#   tests/e2e.sh [path to the funchess binary]     default: target/release/funchess
#
#   1. the command line
#   2. install.sh and the self-updater, against releases made up here and served
#      from file:// (needs curl)
#   3. the game itself in detached tmux sessions (skipped when tmux is missing):
#      by keyboard, by mouse, against the computer, with sound (played by stand-ins
#      that only note what they were given), and two copies playing each other over
#      a real connection on this machine
#
# The rules, the computer's play, the layout and the drawing are covered by
# `cargo test`; this is for what only shows when the real program meets a real
# terminal and a real socket.
set -u

cd "$(dirname "$0")/.." || exit 1
ROOT=$PWD
BIN=${1:-target/release/funchess}
case $BIN in /*) ;; *) BIN=$ROOT/$BIN ;; esac
[ -x "$BIN" ] || { echo "e2e.sh: $BIN is not built (cargo build --release)" >&2; exit 1; }

TMP=$(mktemp -d)
SOCK=funchess-e2e-$$
trap 'tmux -L "$SOCK" kill-server 2>/dev/null; rm -rf "$TMP"' EXIT

fails=0
pass() { printf 'ok    %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1"; fails=$((fails + 1)); }
is() { # name expected actual
    if [ "$2" = "$3" ]; then pass "$1"; else fail "$1: expected '$2', got '$3'"; fi
}
has() { # name text-to-find text
    case $3 in *"$2"*) pass "$1" ;; *) fail "$1: no '$2' in '$3'" ;; esac
}

VERSION=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)

# ---------------------------------------------------------- command line ----

has "--version" "funchess $VERSION (" "$("$BIN" --version)"
has "--help" "funchess update" "$("$BIN" --help)"
has "an unknown option is refused" "unknown argument" "$("$BIN" --nonsense 2>&1)"
has "an unknown theme is refused" "--theme needs one of" "$("$BIN" --theme plaid 2>&1)"
has "an unknown level is refused" "--level needs one of" "$("$BIN" --level grandmaster 2>&1)"
has "join needs an address" "join needs the host's address" "$("$BIN" join 2>&1)"
has "a bad port is refused" "the port must be a number" "$("$BIN" host seventy </dev/null 2>&1)"
has "needs a terminal" "needs a terminal" "$("$BIN" </dev/null 2>&1)"
has "a bad animations switch is refused" "--animations needs on or off" "$("$BIN" --animations sometimes 2>&1)"
XDG_STATE_HOME="$TMP/xdg-anim" "$BIN" --animations off </dev/null >/dev/null 2>&1
is "--animations off is remembered" "animations=0" "$(grep -x 'animations=0' "$TMP/xdg-anim/funchess/settings" 2>/dev/null)"
has "a bad sound switch is refused" "--sound needs on or off" "$("$BIN" --sound loud 2>&1)"
XDG_STATE_HOME="$TMP/xdg-anim" "$BIN" --sound off </dev/null >/dev/null 2>&1
is "--sound off is remembered" "sound=0" "$(grep -x 'sound=0' "$TMP/xdg-anim/funchess/settings" 2>/dev/null)"
has "a checkout never updates itself" "running from a source checkout" "$("$BIN" update 2>&1)"

# ------------------------------------------------- install and self-update ----

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

case "$(uname -s)-$(uname -m)" in
    Linux-x86_64 | Linux-amd64) ASSET=funchess-x86_64-unknown-linux-musl ;;
    Linux-aarch64 | Linux-arm64) ASSET=funchess-aarch64-unknown-linux-musl ;;
    Darwin-arm64) ASSET=funchess-aarch64-apple-darwin ;;
    Darwin-x86_64) ASSET=funchess-x86_64-apple-darwin ;;
    *) ASSET= ;;
esac

# make_release <dir> <version> <file to publish as this platform's binary>
make_release() {
    mkdir -p "$1"
    cp "$3" "$1/$ASSET"
    echo "$2" > "$1/VERSION"
    echo "$(sha256_of "$1/$ASSET")  $ASSET" > "$1/SHA256SUMS"
}

INST=$TMP/inst/bin/funchess
installed() { XDG_STATE_HOME="$TMP/ustate" "$INST" "$@" 2>&1; }
# The copy a user would have: outside any checkout, in a directory they own.
fresh_copy() {
    rm -rf "$TMP/inst"
    mkdir -p "$TMP/inst/bin"
    cp "$BIN" "$INST"
}

if ! command -v curl >/dev/null 2>&1 || [ -z "$ASSET" ]; then
    echo "skip  install and self-update (no curl, or no release binary for this platform)"
else
    # A "newer release" whose binary is a script, so that it is plain which one runs.
    printf '#!/bin/sh\necho "funchess 99.0.0 (fake)"\n' > "$TMP/fake"
    chmod 755 "$TMP/fake"
    make_release "$TMP/rel-now" "$VERSION" "$BIN"
    make_release "$TMP/rel-new" 99.0.0 "$TMP/fake"
    make_release "$TMP/rel-old" 0.0.1 "$TMP/fake"
    make_release "$TMP/rel-bad" 99.0.0 "$TMP/fake"
    echo "0000000000000000000000000000000000000000000000000000000000000000  $ASSET" > "$TMP/rel-bad/SHA256SUMS"

    out=$(FUNCHESS_RELEASE_URL="file://$TMP/rel-now" FUNCHESS_BIN_DIR="$TMP/inst/bin" sh install.sh 2>&1)
    has "install.sh: installs the release" "funchess $VERSION (" "$(installed --version)"
    has "install.sh: says where" "Installed $INST" "$out"
    out=$(FUNCHESS_RELEASE_URL="file://$TMP/rel-bad" FUNCHESS_BIN_DIR="$TMP/inst2/bin" sh install.sh 2>&1)
    has "install.sh: refuses a bad checksum" "checksum mismatch" "$out"
    if [ -e "$TMP/inst2/bin/funchess" ]; then fail "install.sh: installed despite a bad checksum"; else pass "install.sh: a refused download installs nothing"; fi
    FUNCHESS_BIN_DIR="$TMP/inst/bin" sh install.sh --uninstall >/dev/null 2>&1
    if [ -e "$INST" ]; then fail "install.sh: --uninstall left the binary"; else pass "install.sh: --uninstall removes it"; fi

    fresh_copy
    has "update: nothing newer" "funchess $VERSION is up to date (latest release is $VERSION)." "$(FUNCHESS_RELEASE_URL="file://$TMP/rel-now" installed update)"
    has "update: never downgrades" "is up to date (latest release is 0.0.1)." "$(FUNCHESS_RELEASE_URL="file://$TMP/rel-old" installed update)"
    has "update: refuses a bad checksum" "checksum mismatch" "$(FUNCHESS_RELEASE_URL="file://$TMP/rel-bad" installed update)"
    has "update: a refused update changes nothing" "funchess $VERSION (" "$(installed --version)"
    has "update: reports an unreachable server" "could not check for updates" "$(FUNCHESS_RELEASE_URL="file://$TMP/nowhere" installed update)"

    # What a package does when it installs: a marker beside the binary's directory.
    mkdir -p "$TMP/inst/share/funchess"
    echo "Homebrew; use brew upgrade funchess" > "$TMP/inst/share/funchess/managed-by"
    is "update: a package's copy refuses" "funchess: this copy can't update itself: installed with Homebrew; use brew upgrade funchess" "$(FUNCHESS_RELEASE_URL="file://$TMP/rel-new" installed update)"
    has "update: a package's copy is untouched" "funchess $VERSION (" "$(installed --version)"

    # Reached through a link, as Homebrew's bin directory does it: still refused.
    mkdir -p "$TMP/link"
    ln -s "$INST" "$TMP/link/funchess"
    has "update: a package's copy refuses through a link too" "installed with Homebrew" "$(FUNCHESS_RELEASE_URL="file://$TMP/rel-new" XDG_STATE_HOME="$TMP/ustate" "$TMP/link/funchess" update 2>&1)"

    fresh_copy
    has "update: installs a newer release" "Updated to 99.0.0." "$(FUNCHESS_RELEASE_URL="file://$TMP/rel-new" installed update)"
    is "update: the new version is what runs" "funchess 99.0.0 (fake)" "$(installed --version)"

    # Through a link, the real file is replaced and the link is left alone.
    fresh_copy
    FUNCHESS_RELEASE_URL="file://$TMP/rel-new" XDG_STATE_HOME="$TMP/ustate" "$TMP/link/funchess" update >/dev/null 2>&1
    if [ -L "$TMP/link/funchess" ] && [ "$("$INST" --version)" = "funchess 99.0.0 (fake)" ]; then
        pass "update: through a link, the real file is replaced and the link survives"
    else
        fail "update: through a link, the link was replaced or the file was not"
    fi

    fresh_copy
    has "update off" "The update check at startup is off." "$(installed update off)"
    is "update off is remembered" "update=0" "$(grep -x 'update=0' "$TMP/ustate/funchess/settings")"
    has "update on" "The update check at startup is on." "$(installed update on)"
fi

# -------------------------------------------------------------- the game ----

if ! command -v tmux >/dev/null 2>&1; then
    echo "skip  the game in tmux (tmux is not installed)"
else
    # Everything runs on a private tmux server, so no other session is ever touched.
    T() { tmux -L "$SOCK" "$@"; }
    screen() { T capture-pane -p -t "${1:-main}" 2>/dev/null; }
    expect() { # name text [session] -- waits up to 10 seconds for the text to be on screen
        i=0
        while [ "$i" -lt 100 ]; do
            if screen "${3:-main}" | grep -qF -- "$2"; then pass "$1"; return; fi
            sleep 0.1
            i=$((i + 1))
        done
        fail "$1: '$2' never appeared"
        screen "${3:-main}" | sed 's/^/        | /'
    }
    absent() { # name text [session]
        if screen "${3:-main}" | grep -qF -- "$2"; then fail "$1: '$2' is on screen"; else pass "$1"; fi
    }
    keys() { T send-keys -t main "$@"; }
    # click <text> [session]: a left click (SGR mouse press and release) on the first
    # character of the text, wherever it is on screen. Not every awk counts characters
    # (the one on macOS counts bytes, and a chess symbol is three), so the bytes that
    # continue a character are dropped first and everything is then counted as bytes.
    click() {
        at=$(screen "${2:-main}" | LC_ALL=C tr -d '\200-\277' | LC_ALL=C awk -v t="$1" '{ i = index($0, t); if (i) { print i ";" NR; exit } }')
        [ -n "$at" ] || { fail "click: '$1' is not on screen"; return; }
        T send-keys -t "${2:-main}" -l "$(printf '\033[<0;%sM\033[<0;%sm' "$at" "$at")"
    }
    # start <session> <state dir> <command and arguments>: a session running the game
    start() {
        name=$1 state=$2
        shift 2
        T kill-session -t "$name" 2>/dev/null
        T -f /dev/null new-session -d -s "$name" -x 80 -y 24 \
            "env XDG_STATE_HOME='$state' FUNCHESS_NO_UPDATE=1 FUNCHESS_NO_SOUND=1 $*; echo \"EXIT=\$?\"; sleep 20"
    }

    # The menu, a two-player game by keyboard, and the commands.
    start main "$TMP/xdg" "'$BIN'" --pieces symbols
    expect "menu: starts" "Play the computer"
    keys C-t
    expect "menu: Ctrl-T changes the theme" "< wood >"
    is "menu: the theme is remembered" "theme=wood" "$(grep -x 'theme=wood' "$TMP/xdg/funchess/settings" 2>/dev/null)"
    keys -l t
    expect "menu: T alone changes the theme too" "< ocean >"
    keys 2
    expect "game: two players starts" "White to move"
    keys -l e2e4
    expect "game: a typed move is played" "1. e4"
    expect "game: the turn passes" "Black to move"
    # The board has turned to face Black, so Black's pawn is below the marker.
    keys Down Down Down Enter Up Up Enter
    expect "game: the arrow keys and Enter move a piece" "1. e4       e5"
    keys -l qurntp
    sleep 0.2
    expect "game: plain letters are not commands" "1. e4       e5"
    keys C-u
    sleep 0.2
    absent "game: Ctrl-U takes a move back" "1. e4       e5"
    keys '?'
    expect "game: help opens" "Press any key"
    keys Enter
    keys C-r
    expect "game: Ctrl-R asks first" "Black resigns?"
    keys n
    keys C-d
    expect "game: Ctrl-D offers a draw" "Black offers a draw."
    keys y
    expect "game: an accepted draw ends the game" "Draw agreed"
    keys C-n
    expect "game: Ctrl-N starts again" "White to move"
    keys C-g
    expect "game: Ctrl-G suggests a move" "Try the "
    keys C-a
    expect "game: Ctrl-A switches the animations off" "Animations off"
    is "game: animations off is remembered" "animations=0" "$(grep -x 'animations=0' "$TMP/xdg/funchess/settings" 2>/dev/null)"
    click "^A Animations"
    expect "mouse: a click switches the animations on again" "Animations on"

    # The same by mouse: squares, commands and answers.
    keys -l "$(printf '\033[<0;27;16M\033[<0;27;16m')" # e2, then e4
    sleep 0.2
    keys -l "$(printf '\033[<0;27;12M\033[<0;27;12m')"
    expect "mouse: clicking two squares moves a piece" "1. e4"
    click "^U Undo"
    sleep 0.3
    absent "mouse: a clicked command acts" "1. e4"
    click "^U Undo"
    expect "mouse: a command with nothing to do says so" "Nothing to take back"
    click "^R Resign"
    expect "mouse: Resign asks first" "White resigns?"
    click "y  yes"
    expect "mouse: a clicked answer is taken" "White resigned"
    expect "mouse: the result is announced" "Game over"
    click "Ctrl-Q"
    expect "mouse: back at the menu from the result box" "Play the computer"
    click "Pieces"
    expect "mouse: a clicked setting steps forward" "< letters >"
    click "Two players"
    expect "mouse: a clicked menu line starts the game" "White to move"
    expect "letters: pieces are drawn as letters" "r    n    b    q    k    b    n    r"

    T resize-window -t main -x 40 -y 8 2>/dev/null
    expect "game: a tiny window says so" "Please make the window at least"
    T resize-window -t main -x 180 -y 50 2>/dev/null
    expect "game: a big window shows the moves on the left" "two players"
    click "^G Hint"
    expect "mouse: a clicked hint is given" "Try the "
    keys C-q
    expect "game: Ctrl-Q goes to the menu" "Play the computer"
    keys C-q
    expect "game: Ctrl-Q in the menu quits cleanly" "EXIT=0"

    # Against the computer: it answers, and undo takes back both moves.
    start main "$TMP/xdg2" "'$BIN'" computer --level beginner --pieces symbols
    expect "computer: the game starts" "Chick (Beginner)"
    keys -l e2e4
    expect "computer: the move is played" "1. e4"
    expect "computer: it replies" "White to move"
    keys C-u
    sleep 0.3
    absent "computer: undo takes back both moves" "1. e4"
    keys C-c
    expect "computer: Ctrl-C quits at once" "EXIT=0"

    # Playing Black, the computer opens.
    start main "$TMP/xdg2" "'$BIN'" computer --level beginner --black --pieces symbols
    expect "computer: as Black, the computer moves first" "Black to move"
    expect "computer: Black is at the bottom" "h    g    f    e    d    c    b    a"
    keys C-c

    # Sound: the system's player is asked to play a file for a new game and for a move.
    # Stand-ins for every player it might look for, which note what they were given.
    mkdir -p "$TMP/players"
    for player in pw-play paplay aplay afplay; do
        printf '#!/bin/sh\nfor a in "$@"; do echo "$a"; done >> "%s"\n' "$TMP/played" > "$TMP/players/$player"
        chmod 755 "$TMP/players/$player"
    done
    start main "$TMP/xdg5" env -u FUNCHESS_NO_SOUND "PATH='$TMP/players:$PATH'" "'$BIN'" local --pieces symbols
    expect "sound: the game starts" "White to move"
    keys -l e2e4
    expect "sound: the move is played" "1. e4"
    sleep 0.6
    has "sound: a new game is heard" "$TMP/xdg5/funchess/sounds/start.wav" "$(cat "$TMP/played" 2>/dev/null)"
    has "sound: a move is heard" "$TMP/xdg5/funchess/sounds/move.wav" "$(cat "$TMP/played" 2>/dev/null)"
    is "sound: what is played is a WAV file" "RIFF" "$(head -c 4 "$TMP/xdg5/funchess/sounds/move.wav" 2>/dev/null)"
    keys C-s
    expect "sound: Ctrl-S switches it off" "Sound off"
    sleep 0.3
    rm -f "$TMP/played"
    keys -l e7e5
    expect "sound: a move is played with sound off" "1. e4       e5"
    sleep 0.6
    if [ -e "$TMP/played" ]; then fail "sound: something was played after Ctrl-S"; else pass "sound: switched off, nothing is played"; fi
    is "sound: off is remembered" "sound=0" "$(grep -x 'sound=0' "$TMP/xdg5/funchess/sounds/../settings" 2>/dev/null)"
    click "^S Sound"
    expect "sound: a click switches it on again" "Sound on"
    sleep 0.6
    has "sound: switched on, it is heard at once" "move.wav" "$(cat "$TMP/played" 2>/dev/null)"
    keys C-c
    expect "sound: quits cleanly" "EXIT=0"
    rm -f "$TMP/played"
    start main "$TMP/xdg5" env -u FUNCHESS_NO_SOUND "PATH='$TMP/players:$PATH'" "'$BIN'" local --pieces symbols --sound off
    expect "sound off: the game starts" "White to move"
    keys -l e2e4
    expect "sound off: the move is played" "1. e4"
    sleep 0.6
    if [ -e "$TMP/played" ]; then fail "sound off: something was played"; else pass "sound off: nothing is played"; fi
    keys C-c

    # Two copies over a real connection.
    PORT=$((20000 + $$ % 20000))
    start host "$TMP/xdg3" "'$BIN'" host "$PORT" --black --pieces symbols
    expect "network: the host waits and says how to join" "funchess join " host
    start guest "$TMP/xdg4" "'$BIN'" join "127.0.0.1:$PORT" --pieces symbols
    expect "network: the guest gets the other color" "> White  You" guest
    expect "network: the host plays the color it asked for" "Black  You" host
    T send-keys -t host -l e7e5
    sleep 0.3
    absent "network: the host cannot move out of turn" "1." host
    T send-keys -t guest -l e2e4
    expect "network: the guest's move reaches the host" "1. e4" host
    T send-keys -t host -l e7e5
    expect "network: the host's move reaches the guest" "1. e4       e5" guest
    T send-keys -t guest C-u
    expect "network: no take-backs" "No take-backs" guest
    T send-keys -t guest C-d
    expect "network: a draw offer reaches the host" "Your opponent offers a draw." host
    T send-keys -t host n
    expect "network: declining reaches the guest" "Draw declined" guest
    T send-keys -t host C-r
    T send-keys -t host y
    expect "network: a resignation reaches the guest" "Black resigned. White wins" guest
    T send-keys -t host C-n
    expect "network: a rematch offer reaches the guest" "Rematch offered" guest
    T send-keys -t guest C-n
    expect "network: the rematch swaps colors" "> White  You" host
    T send-keys -t guest C-c
    expect "network: the host notices the guest leaving" "Black left. White wins" host
    T kill-session -t host 2>/dev/null
    T kill-session -t guest 2>/dev/null

    # Nobody listening: the menu comes back with the reason.
    start main "$TMP/xdg4" "'$BIN'" join "127.0.0.1:$((PORT + 1))"
    expect "network: joining nobody is reported" "Could not connect"
    keys C-c

    # Starting the installed copy when a newer release exists: it updates and restarts
    # as the new version. Switched off, it starts the game without updating.
    if command -v curl >/dev/null 2>&1 && [ -n "$ASSET" ]; then
        STAMP=$TMP/ustate/funchess/last-update-check
        launch() { # <release directory>
            T kill-session -t main 2>/dev/null
            T -f /dev/null new-session -d -s main -x 80 -y 24 \
                "env XDG_STATE_HOME='$TMP/ustate' FUNCHESS_NO_SOUND=1 FUNCHESS_RELEASE_URL='file://$1' '$INST'; echo \"EXIT=\$?\"; sleep 20"
        }
        fresh_copy
        rm -f "$STAMP"
        installed update off >/dev/null
        launch "$TMP/rel-new"
        expect "update: switched off, the game starts" "Play the computer"
        keys C-q
        expect "update: switched off, it quits cleanly" "EXIT=0"
        has "update: switched off, nothing is updated" "funchess $VERSION (" "$(installed --version)"
        if [ -e "$STAMP" ]; then fail "update: switched off, yet a check was noted"; else pass "update: switched off, nothing is checked"; fi
        installed update on >/dev/null

        # At most one check a day: a start that finds nothing newer notes the time, and
        # the next start does not look again, even with a newer release on offer.
        launch "$TMP/rel-now"
        expect "once a day: the game starts after a check that finds nothing" "Play the computer"
        keys C-q
        expect "once a day: it quits cleanly" "EXIT=0"
        if [ -s "$STAMP" ]; then pass "once a day: the check is noted"; else fail "once a day: no $STAMP"; fi
        launch "$TMP/rel-new"
        expect "once a day: the next start goes straight to the game" "Play the computer"
        keys C-q
        expect "once a day: and quits cleanly" "EXIT=0"
        has "once a day: no second check, so no update" "funchess $VERSION (" "$(installed --version)"
        has "once a day: asking explicitly still checks" "Updating funchess $VERSION -> 99.0.0" "$(FUNCHESS_RELEASE_URL="file://$TMP/rel-bad" installed update)"

        # A day later (the noted time is old), a start checks again and updates.
        fresh_copy
        echo 1000 > "$STAMP"
        launch "$TMP/rel-new"
        expect "update: a day later, starting the game updates it and runs the new version" "funchess 99.0.0 (fake)"
    fi
    T kill-server 2>/dev/null
fi

echo
if [ "$fails" -gt 0 ]; then
    echo "$fails failed"
    exit 1
fi
echo "all passed"
