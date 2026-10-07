//! The rules of chess: the board, legal moves, move notation, and how a game ends.
//! Nothing here knows about the screen, the computer opponent or the network.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Color {
    White,
    Black,
}

impl Color {
    pub fn other(self) -> Color {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Color::White => "White",
            Color::Black => "Black",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

impl Kind {
    pub const ALL: [Kind; 6] = [Kind::Pawn, Kind::Knight, Kind::Bishop, Kind::Rook, Kind::Queen, Kind::King];

    /// The letter used in move notation and FEN (uppercase).
    pub fn letter(self) -> char {
        match self {
            Kind::Pawn => 'P',
            Kind::Knight => 'N',
            Kind::Bishop => 'B',
            Kind::Rook => 'R',
            Kind::Queen => 'Q',
            Kind::King => 'K',
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Pawn => "pawn",
            Kind::Knight => "knight",
            Kind::Bishop => "bishop",
            Kind::Rook => "rook",
            Kind::Queen => "queen",
            Kind::King => "king",
        }
    }

    fn from_letter(c: char) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.letter() == c.to_ascii_uppercase())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Piece {
    pub color: Color,
    pub kind: Kind,
}

/// 0 is a1, 7 is h1, 63 is h8.
pub type Square = u8;

pub fn sq(file: u8, rank: u8) -> Square {
    rank * 8 + file
}

pub fn file_of(s: Square) -> u8 {
    s % 8
}

pub fn rank_of(s: Square) -> u8 {
    s / 8
}

pub fn square_name(s: Square) -> String {
    format!("{}{}", (b'a' + file_of(s)) as char, rank_of(s) + 1)
}

pub fn parse_square(text: &str) -> Option<Square> {
    let &[f, r] = text.as_bytes() else { return None };
    (matches!(f, b'a'..=b'h') && matches!(r, b'1'..=b'8')).then(|| sq(f - b'a', r - b'1'))
}

/// Castling is the king moving two files; en passant is a pawn capturing onto the
/// empty en passant square. Neither needs a flag of its own.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Move {
    pub from: Square,
    pub to: Square,
    pub promo: Option<Kind>,
}

impl Move {
    /// Coordinate notation, as sent over the network: `e2e4`, `e7e8q`.
    pub fn uci(self) -> String {
        let mut s = square_name(self.from) + &square_name(self.to);
        if let Some(k) = self.promo {
            s.push(k.letter().to_ascii_lowercase());
        }
        s
    }

    pub fn parse_uci(text: &str) -> Option<Move> {
        if !text.is_ascii() || !(4..=5).contains(&text.len()) {
            return None;
        }
        let promo = match text[4..].chars().next() {
            None => None,
            Some(c) => Some(Kind::from_letter(c).filter(|k| !matches!(k, Kind::Pawn | Kind::King))?),
        };
        Some(Move { from: parse_square(&text[..2])?, to: parse_square(&text[2..4])?, promo })
    }
}

const KNIGHT_STEPS: [(i8, i8); 8] = [(1, 2), (2, 1), (2, -1), (1, -2), (-1, -2), (-2, -1), (-2, 1), (-1, 2)];
const KING_STEPS: [(i8, i8); 8] = [(1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1), (0, -1), (1, -1)];
const DIAGONALS: [(i8, i8); 4] = [(1, 1), (-1, 1), (-1, -1), (1, -1)];
const STRAIGHTS: [(i8, i8); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

fn offset(s: Square, df: i8, dr: i8) -> Option<Square> {
    let f = file_of(s) as i8 + df;
    let r = rank_of(s) as i8 + dr;
    ((0..8).contains(&f) && (0..8).contains(&r)).then(|| (r * 8 + f) as Square)
}

/// Castling rights are indexed white king side, white queen side, black king side,
/// black queen side; each goes with the rook's home square.
const ROOK_HOMES: [Square; 4] = [7, 0, 63, 56];

/// One random number per piece on a square, plus a few for the rest of the position,
/// so that a position can be summed up in one number (see `Board::key`).
const fn key_table() -> [u64; 12 * 64 + 16] {
    let mut table = [0u64; 12 * 64 + 16];
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut i = 0;
    while i < table.len() {
        // splitmix64
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        table[i] = z ^ (z >> 31);
        i += 1;
    }
    table
}
static KEYS: [u64; 12 * 64 + 16] = key_table();

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Board {
    pub squares: [Option<Piece>; 64],
    pub turn: Color,
    pub castling: [bool; 4],
    /// The square a pawn that just advanced two passed over, when an enemy pawn
    /// stands next to it and so might capture there.
    pub ep: Option<Square>,
    /// Half-moves since a capture or a pawn move, for the fifty-move rule.
    pub halfmove: u16,
    pub fullmove: u16,
}

impl Board {
    pub fn start() -> Board {
        Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").expect("the starting position is valid")
    }

    pub fn from_fen(fen: &str) -> Option<Board> {
        let mut parts = fen.split_whitespace();
        let mut squares = [None; 64];
        let mut ranks = 0;
        for (i, row) in parts.next()?.split('/').enumerate() {
            let rank = 7u8.checked_sub(i as u8)?;
            let mut file = 0u8;
            for c in row.chars() {
                if let Some(n) = c.to_digit(10) {
                    file += n as u8;
                } else {
                    let color = if c.is_ascii_uppercase() { Color::White } else { Color::Black };
                    *squares.get_mut(sq(file, rank) as usize).filter(|_| file < 8)? = Some(Piece { color, kind: Kind::from_letter(c)? });
                    file += 1;
                }
            }
            if file != 8 {
                return None;
            }
            ranks += 1;
        }
        if ranks != 8 {
            return None;
        }
        let turn = match parts.next()? {
            "w" => Color::White,
            "b" => Color::Black,
            _ => return None,
        };
        let rights = parts.next().unwrap_or("-");
        let castling = ['K', 'Q', 'k', 'q'].map(|c| rights.contains(c));
        let ep = parts.next().and_then(parse_square);
        let halfmove = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
        let fullmove = parts.next().and_then(|n| n.parse().ok()).unwrap_or(1);
        let board = Board { squares, turn, castling, ep, halfmove, fullmove };
        // Exactly one king each, or nothing else here can be relied on.
        let kings = |color| squares.iter().filter(|&&p| p == Some(Piece { color, kind: Kind::King })).count();
        (kings(Color::White) == 1 && kings(Color::Black) == 1).then_some(board)
    }

    pub fn at(&self, s: Square) -> Option<Piece> {
        self.squares[s as usize]
    }

    pub fn king_square(&self, color: Color) -> Square {
        let king = Some(Piece { color, kind: Kind::King });
        self.squares.iter().position(|&p| p == king).expect("each side always has a king") as Square
    }

    /// Whether any piece of color `by` attacks the square.
    pub fn attacked(&self, target: Square, by: Color) -> bool {
        let is = |s: Square, kind: Kind| self.at(s) == Some(Piece { color: by, kind });
        // A pawn attacks diagonally forward, so look one rank back from the target.
        let back = if by == Color::White { -1 } else { 1 };
        if [-1, 1].into_iter().filter_map(|df| offset(target, df, back)).any(|s| is(s, Kind::Pawn)) {
            return true;
        }
        if KNIGHT_STEPS.into_iter().filter_map(|(df, dr)| offset(target, df, dr)).any(|s| is(s, Kind::Knight)) {
            return true;
        }
        if KING_STEPS.into_iter().filter_map(|(df, dr)| offset(target, df, dr)).any(|s| is(s, Kind::King)) {
            return true;
        }
        for (steps, slider) in [(DIAGONALS, Kind::Bishop), (STRAIGHTS, Kind::Rook)] {
            for (df, dr) in steps {
                let mut s = target;
                while let Some(next) = offset(s, df, dr) {
                    s = next;
                    if let Some(p) = self.at(s) {
                        if p.color == by && (p.kind == slider || p.kind == Kind::Queen) {
                            return true;
                        }
                        break;
                    }
                }
            }
        }
        false
    }

    pub fn in_check(&self, color: Color) -> bool {
        self.attacked(self.king_square(color), color.other())
    }

    /// Every move the side to move could make if leaving its own king in check were
    /// allowed. Cheaper than `legal_moves`; the computer opponent filters as it goes.
    pub fn pseudo_moves(&self, out: &mut Vec<Move>) {
        let us = self.turn;
        for from in 0..64u8 {
            let Some(piece) = self.at(from) else { continue };
            if piece.color != us {
                continue;
            }
            match piece.kind {
                Kind::Pawn => self.pawn_moves(from, out),
                Kind::Knight => self.step_moves(from, &KNIGHT_STEPS, out),
                Kind::King => {
                    self.step_moves(from, &KING_STEPS, out);
                    self.castling_moves(from, out);
                }
                Kind::Bishop => self.slide_moves(from, &DIAGONALS, out),
                Kind::Rook => self.slide_moves(from, &STRAIGHTS, out),
                Kind::Queen => {
                    self.slide_moves(from, &DIAGONALS, out);
                    self.slide_moves(from, &STRAIGHTS, out);
                }
            }
        }
    }

    fn pawn_moves(&self, from: Square, out: &mut Vec<Move>) {
        let (forward, home, last) = if self.turn == Color::White { (1, 1, 7) } else { (-1, 6, 0) };
        let mut push = |to: Square| {
            if rank_of(to) == last {
                out.extend([Kind::Queen, Kind::Rook, Kind::Bishop, Kind::Knight].map(|k| Move { from, to, promo: Some(k) }));
            } else {
                out.push(Move { from, to, promo: None });
            }
        };
        if let Some(one) = offset(from, 0, forward)
            && self.at(one).is_none()
        {
            push(one);
            if rank_of(from) == home
                && let Some(two) = offset(one, 0, forward)
                && self.at(two).is_none()
            {
                push(two);
            }
        }
        for df in [-1, 1] {
            if let Some(to) = offset(from, df, forward)
                && (self.at(to).is_some_and(|p| p.color != self.turn) || self.ep == Some(to))
            {
                push(to);
            }
        }
    }

    fn step_moves(&self, from: Square, steps: &[(i8, i8)], out: &mut Vec<Move>) {
        for &(df, dr) in steps {
            if let Some(to) = offset(from, df, dr)
                && self.at(to).is_none_or(|p| p.color != self.turn)
            {
                out.push(Move { from, to, promo: None });
            }
        }
    }

    fn slide_moves(&self, from: Square, steps: &[(i8, i8)], out: &mut Vec<Move>) {
        for &(df, dr) in steps {
            let mut s = from;
            while let Some(to) = offset(s, df, dr) {
                s = to;
                match self.at(to) {
                    None => out.push(Move { from, to, promo: None }),
                    Some(p) => {
                        if p.color != self.turn {
                            out.push(Move { from, to, promo: None });
                        }
                        break;
                    }
                }
            }
        }
    }

    fn castling_moves(&self, from: Square, out: &mut Vec<Move>) {
        let (us, them) = (self.turn, self.turn.other());
        let (rank, rights) = if us == Color::White { (0, 0) } else { (7, 2) };
        if from != sq(4, rank) || !(self.castling[rights] || self.castling[rights + 1]) || self.attacked(from, them) {
            return;
        }
        let rook = Some(Piece { color: us, kind: Kind::Rook });
        let empty = |files: &[u8]| files.iter().all(|&f| self.at(sq(f, rank)).is_none());
        // The king may not pass over an attacked square; where it lands is checked
        // along with every other move.
        if self.castling[rights] && self.at(sq(7, rank)) == rook && empty(&[5, 6]) && !self.attacked(sq(5, rank), them) {
            out.push(Move { from, to: sq(6, rank), promo: None });
        }
        if self.castling[rights + 1] && self.at(sq(0, rank)) == rook && empty(&[1, 2, 3]) && !self.attacked(sq(3, rank), them) {
            out.push(Move { from, to: sq(2, rank), promo: None });
        }
    }

    pub fn legal_moves(&self) -> Vec<Move> {
        let mut moves = Vec::with_capacity(48);
        self.pseudo_moves(&mut moves);
        moves.retain(|&m| !self.play(m).in_check(self.turn));
        moves
    }

    pub fn is_capture(&self, m: Move) -> bool {
        self.at(m.to).is_some() || self.is_en_passant(m)
    }

    fn is_en_passant(&self, m: Move) -> bool {
        self.ep == Some(m.to) && self.at(m.from).is_some_and(|p| p.kind == Kind::Pawn)
    }

    /// The position after a move. The move must be one of `pseudo_moves`.
    pub fn play(&self, m: Move) -> Board {
        let mut b = *self;
        let piece = self.at(m.from).expect("a move starts from a piece");
        let capture = self.is_capture(m);
        if self.is_en_passant(m) {
            b.squares[sq(file_of(m.to), rank_of(m.from)) as usize] = None;
        }
        b.squares[m.from as usize] = None;
        b.squares[m.to as usize] = Some(Piece { color: piece.color, kind: m.promo.unwrap_or(piece.kind) });

        if piece.kind == Kind::King {
            let rank = rank_of(m.from);
            if file_of(m.from) == 4 && file_of(m.to) == 6 {
                b.squares[sq(5, rank) as usize] = b.squares[sq(7, rank) as usize].take();
            } else if file_of(m.from) == 4 && file_of(m.to) == 2 {
                b.squares[sq(3, rank) as usize] = b.squares[sq(0, rank) as usize].take();
            }
            let rights = if piece.color == Color::White { 0 } else { 2 };
            b.castling[rights] = false;
            b.castling[rights + 1] = false;
        }
        // A rook that moves, or is captured at home, takes its castling right with it.
        for (i, home) in ROOK_HOMES.into_iter().enumerate() {
            if m.from == home || m.to == home {
                b.castling[i] = false;
            }
        }

        b.ep = None;
        if piece.kind == Kind::Pawn && rank_of(m.from).abs_diff(rank_of(m.to)) == 2 {
            let enemy_pawn = Some(Piece { color: piece.color.other(), kind: Kind::Pawn });
            if [-1, 1].into_iter().filter_map(|df| offset(m.to, df, 0)).any(|s| self.at(s) == enemy_pawn) {
                b.ep = Some((m.from + m.to) / 2);
            }
        }

        b.halfmove = if capture || piece.kind == Kind::Pawn { 0 } else { self.halfmove + 1 };
        if self.turn == Color::Black {
            b.fullmove += 1;
        }
        b.turn = self.turn.other();
        b
    }

    /// A number that is the same for two positions exactly when they count as the
    /// same position for the repetition rule (barring a one-in-2^64 coincidence).
    pub fn key(&self) -> u64 {
        let mut key = 0;
        for (s, piece) in self.squares.iter().enumerate() {
            if let Some(p) = piece {
                key ^= KEYS[(p.color as usize * 6 + p.kind as usize) * 64 + s];
            }
        }
        let extra = &KEYS[12 * 64..];
        if self.turn == Color::Black {
            key ^= extra[0];
        }
        for (i, &right) in self.castling.iter().enumerate() {
            if right {
                key ^= extra[1 + i];
            }
        }
        if let Some(ep) = self.ep {
            key ^= extra[5 + file_of(ep) as usize];
        }
        key
    }

    /// Standard algebraic notation for a legal move: `Nf3`, `exd5`, `O-O`, `e8=Q+`.
    pub fn san(&self, m: Move) -> String {
        let piece = self.at(m.from).expect("a move starts from a piece");
        let mut s = String::new();
        if piece.kind == Kind::King && file_of(m.from).abs_diff(file_of(m.to)) == 2 {
            s.push_str(if file_of(m.to) == 6 { "O-O" } else { "O-O-O" });
        } else {
            let capture = self.is_capture(m);
            if piece.kind == Kind::Pawn {
                if capture {
                    s.push((b'a' + file_of(m.from)) as char);
                }
            } else {
                s.push(piece.kind.letter());
                // Say which piece only when another of the same kind could go there too.
                let rivals: Vec<Square> =
                    self.legal_moves().into_iter().filter(|o| o.to == m.to && o.from != m.from && self.at(o.from) == Some(piece)).map(|o| o.from).collect();
                if !rivals.is_empty() {
                    let name = square_name(m.from);
                    if rivals.iter().all(|&r| file_of(r) != file_of(m.from)) {
                        s.push_str(&name[..1]);
                    } else if rivals.iter().all(|&r| rank_of(r) != rank_of(m.from)) {
                        s.push_str(&name[1..]);
                    } else {
                        s.push_str(&name);
                    }
                }
            }
            if capture {
                s.push('x');
            }
            s.push_str(&square_name(m.to));
            if let Some(k) = m.promo {
                s.push('=');
                s.push(k.letter());
            }
        }
        let after = self.play(m);
        if after.in_check(after.turn) {
            s.push(if after.legal_moves().is_empty() { '#' } else { '+' });
        }
        s
    }

    /// Neither side could ever give checkmate: bare kings, a single knight or bishop,
    /// or only bishops that all stand on the same color of square.
    pub fn insufficient_material(&self) -> bool {
        let mut knights = 0;
        let mut bishops = [0, 0];
        for (s, piece) in self.squares.iter().enumerate() {
            match piece.map(|p| p.kind) {
                Some(Kind::Pawn | Kind::Rook | Kind::Queen) => return false,
                Some(Kind::Knight) => knights += 1,
                Some(Kind::Bishop) => bishops[(s / 8 + s % 8) % 2] += 1,
                _ => {}
            }
        }
        (knights <= 1 && bishops == [0, 0]) || (knights == 0 && (bishops[0] == 0 || bishops[1] == 0))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Checkmate(Color),
    Resigned(Color),
    /// The winner's opponent left a network game.
    Abandoned(Color),
    Stalemate,
    Repetition,
    FiftyMoves,
    InsufficientMaterial,
    DrawAgreed,
}

impl Outcome {
    /// Who won, when somebody did.
    pub fn winner(self) -> Option<Color> {
        match self {
            Outcome::Checkmate(c) | Outcome::Resigned(c) | Outcome::Abandoned(c) => Some(c),
            _ => None,
        }
    }

    pub fn describe(self) -> String {
        match self {
            Outcome::Checkmate(c) => format!("Checkmate. {} wins", c.name()),
            Outcome::Resigned(c) => format!("{} resigned. {} wins", c.other().name(), c.name()),
            Outcome::Abandoned(c) => format!("{} left. {} wins", c.other().name(), c.name()),
            Outcome::Stalemate => "Draw by stalemate".into(),
            Outcome::Repetition => "Draw by repetition".into(),
            Outcome::FiftyMoves => "Draw by the fifty-move rule".into(),
            Outcome::InsufficientMaterial => "Draw: too few pieces to mate".into(),
            Outcome::DrawAgreed => "Draw agreed".into(),
        }
    }
}

/// One move that was played, with the position it was played from.
pub struct Played {
    pub before: Board,
    pub mv: Move,
    pub san: String,
}

/// A game in progress: the current position and everything that led to it.
pub struct Game {
    pub board: Board,
    pub history: Vec<Played>,
    pub outcome: Option<Outcome>,
}

impl Game {
    pub fn new() -> Game {
        Game { board: Board::start(), history: Vec::new(), outcome: None }
    }

    /// Plays the move if it is legal in the current position and the game is not over.
    pub fn play(&mut self, m: Move) -> bool {
        if self.outcome.is_some() || !self.board.legal_moves().contains(&m) {
            return false;
        }
        self.history.push(Played { before: self.board, mv: m, san: self.board.san(m) });
        self.board = self.board.play(m);
        self.outcome = self.judge();
        true
    }

    fn judge(&self) -> Option<Outcome> {
        let b = &self.board;
        if b.legal_moves().is_empty() {
            return Some(if b.in_check(b.turn) { Outcome::Checkmate(b.turn.other()) } else { Outcome::Stalemate });
        }
        if b.insufficient_material() {
            return Some(Outcome::InsufficientMaterial);
        }
        if b.halfmove >= 100 {
            return Some(Outcome::FiftyMoves);
        }
        let key = b.key();
        (self.history.iter().filter(|p| p.before.key() == key).count() >= 2).then_some(Outcome::Repetition)
    }

    /// Takes back the last move, also reopening a game that it had ended.
    pub fn undo(&mut self) -> bool {
        let Some(last) = self.history.pop() else { return false };
        self.board = last.before;
        self.outcome = None;
        true
    }

    pub fn last_move(&self) -> Option<Move> {
        self.history.last().map(|p| p.mv)
    }

    /// The keys of every position so far, the current one last.
    pub fn keys(&self) -> Vec<u64> {
        self.history.iter().map(|p| p.before.key()).chain([self.board.key()]).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn perft(b: &Board, depth: u32) -> u64 {
        if depth == 0 {
            return 1;
        }
        b.legal_moves().into_iter().map(|m| perft(&b.play(m), depth - 1)).sum()
    }

    fn fen(text: &str) -> Board {
        Board::from_fen(text).unwrap()
    }

    /// Move counts from the well-known perft positions: these catch any slip in
    /// castling, en passant, promotion or pins.
    #[test]
    fn perft_counts_match_the_published_ones() {
        assert_eq!(perft(&Board::start(), 4), 197_281);
        assert_eq!(perft(&fen("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"), 3), 97_862);
        assert_eq!(perft(&fen("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1"), 4), 43_238);
        assert_eq!(perft(&fen("r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1"), 3), 9_467);
        assert_eq!(perft(&fen("rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8"), 3), 62_379);
        assert_eq!(perft(&fen("r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10"), 3), 89_890);
    }

    fn play_all(game: &mut Game, moves: &str) {
        for text in moves.split_whitespace() {
            assert!(game.play(Move::parse_uci(text).unwrap()), "{text} should be legal");
        }
    }

    fn sans(game: &Game) -> Vec<&str> {
        game.history.iter().map(|p| p.san.as_str()).collect()
    }

    #[test]
    fn fools_mate_is_checkmate() {
        let mut game = Game::new();
        play_all(&mut game, "f2f3 e7e5 g2g4 d8h4");
        assert_eq!(game.outcome, Some(Outcome::Checkmate(Color::Black)));
        assert_eq!(sans(&game), ["f3", "e5", "g4", "Qh4#"]);
        assert!(!game.play(Move::parse_uci("a2a3").unwrap()));
        assert!(game.undo());
        assert_eq!(game.outcome, None);
    }

    #[test]
    fn notation_covers_castling_captures_and_promotion() {
        let mut game = Game::new();
        play_all(&mut game, "e2e4 d7d5 e4d5 g8f6 g1f3 f6d5 f1c4 e7e6 e1g1");
        assert_eq!(sans(&game), ["e4", "d5", "exd5", "Nf6", "Nf3", "Nxd5", "Bc4", "e6", "O-O"]);

        let b = fen("4k3/P7/8/8/8/8/8/R3K2R w KQ - 0 1");
        assert_eq!(b.san(Move::parse_uci("a7a8q").unwrap()), "a8=Q+");
        assert_eq!(b.san(Move::parse_uci("e1c1").unwrap()), "O-O-O");
        let b = fen("4k3/8/8/8/8/8/8/R4RK1 w - - 0 1");
        assert_eq!(b.san(Move::parse_uci("a1d1").unwrap()), "Rad1");
        let b = fen("4k3/8/8/8/R7/8/8/R3K3 w - - 0 1");
        assert_eq!(b.san(Move::parse_uci("a1a3").unwrap()), "R1a3");
    }

    #[test]
    fn en_passant_removes_the_passed_pawn() {
        let mut game = Game::new();
        play_all(&mut game, "e2e4 a7a6 e4e5 d7d5 e5d6");
        assert_eq!(game.history.last().unwrap().san, "exd6");
        assert_eq!(game.board.at(parse_square("d5").unwrap()), None);
    }

    #[test]
    fn draws_are_recognized() {
        let mut game = Game::new();
        play_all(&mut game, "g1f3 g8f6 f3g1 f6g8 g1f3 g8f6 f3g1 f6g8");
        assert_eq!(game.outcome, Some(Outcome::Repetition));

        let mut game = Game::new();
        game.board = fen("7k/5Q2/8/8/8/8/8/K7 w - - 0 1");
        play_all(&mut game, "f7g6");
        assert_eq!(game.outcome, Some(Outcome::Stalemate));

        assert!(fen("4k3/8/8/8/8/8/8/3BK3 w - - 0 1").insufficient_material());
        assert!(fen("4kb2/8/8/8/8/8/8/2B1K3 w - - 0 1").insufficient_material());
        assert!(!fen("4kb2/8/8/8/8/8/8/3BK3 w - - 0 1").insufficient_material());
        assert!(!fen("4k3/8/8/8/8/8/8/3RK3 w - - 0 1").insufficient_material());
    }

    #[test]
    fn coordinate_notation_round_trips() {
        for text in ["e2e4", "a7a8q", "h2h1n"] {
            assert_eq!(Move::parse_uci(text).unwrap().uci(), text);
        }
        for text in ["", "e2", "e2e9", "e7e8k", "e7e8p", "é2e4", "e2e4qq"] {
            assert_eq!(Move::parse_uci(text), None, "{text}");
        }
    }
}
