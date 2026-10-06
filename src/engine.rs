//! The computer opponent: a plain alpha-beta search over material and piece placement.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::chess::{Board, Color, Kind, Move, file_of, rank_of};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Beginner,
    Easy,
    Medium,
    Hard,
}

impl Level {
    pub const ALL: [Level; 4] = [Level::Beginner, Level::Easy, Level::Medium, Level::Hard];

    pub fn name(self) -> &'static str {
        match self {
            Level::Beginner => "Beginner",
            Level::Easy => "Easy",
            Level::Medium => "Medium",
            Level::Hard => "Hard",
        }
    }

    pub fn parse(text: &str) -> Option<Level> {
        Level::ALL.into_iter().find(|l| l.name().eq_ignore_ascii_case(text))
    }

    /// How many moves ahead it looks, how long it may think, and how much luck is
    /// mixed into its judgement (in hundredths of a pawn). The weaker levels look at
    /// every move with the same care and then misjudge them at random; the stronger
    /// ones look as deep as the time allows.
    fn limits(self) -> (u32, Duration, i32) {
        match self {
            Level::Beginner => (1, Duration::from_secs(1), 250),
            Level::Easy => (2, Duration::from_secs(1), 80),
            Level::Medium => (4, Duration::from_millis(1500), 0),
            Level::Hard => (32, Duration::from_millis(3000), 0),
        }
    }
}

const MATE: i32 = 100_000;
const INFINITY: i32 = 1_000_000;
/// Checks extend the search, so a line of checks needs a hard stop.
const MAX_PLY: usize = 48;

fn value(kind: Kind) -> i32 {
    match kind {
        Kind::Pawn => 100,
        Kind::Knight => 320,
        Kind::Bishop => 330,
        Kind::Rook => 500,
        Kind::Queen => 900,
        Kind::King => 0,
    }
}

// Where each piece likes to stand, seen from White's side with rank 8 on the first
// line (Tomasz Michniewski's "simplified evaluation function" tables).
#[rustfmt::skip]
const PAWN: [i32; 64] = [
     0,  0,  0,  0,  0,  0,  0,  0,
    50, 50, 50, 50, 50, 50, 50, 50,
    10, 10, 20, 30, 30, 20, 10, 10,
     5,  5, 10, 25, 25, 10,  5,  5,
     0,  0,  0, 20, 20,  0,  0,  0,
     5, -5,-10,  0,  0,-10, -5,  5,
     5, 10, 10,-20,-20, 10, 10,  5,
     0,  0,  0,  0,  0,  0,  0,  0,
];
#[rustfmt::skip]
const KNIGHT: [i32; 64] = [
    -50,-40,-30,-30,-30,-30,-40,-50,
    -40,-20,  0,  0,  0,  0,-20,-40,
    -30,  0, 10, 15, 15, 10,  0,-30,
    -30,  5, 15, 20, 20, 15,  5,-30,
    -30,  0, 15, 20, 20, 15,  0,-30,
    -30,  5, 10, 15, 15, 10,  5,-30,
    -40,-20,  0,  5,  5,  0,-20,-40,
    -50,-40,-30,-30,-30,-30,-40,-50,
];
#[rustfmt::skip]
const BISHOP: [i32; 64] = [
    -20,-10,-10,-10,-10,-10,-10,-20,
    -10,  0,  0,  0,  0,  0,  0,-10,
    -10,  0,  5, 10, 10,  5,  0,-10,
    -10,  5,  5, 10, 10,  5,  5,-10,
    -10,  0, 10, 10, 10, 10,  0,-10,
    -10, 10, 10, 10, 10, 10, 10,-10,
    -10,  5,  0,  0,  0,  0,  5,-10,
    -20,-10,-10,-10,-10,-10,-10,-20,
];
#[rustfmt::skip]
const ROOK: [i32; 64] = [
     0,  0,  0,  0,  0,  0,  0,  0,
     5, 10, 10, 10, 10, 10, 10,  5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
     0,  0,  0,  5,  5,  0,  0,  0,
];
#[rustfmt::skip]
const QUEEN: [i32; 64] = [
    -20,-10,-10, -5, -5,-10,-10,-20,
    -10,  0,  0,  0,  0,  0,  0,-10,
    -10,  0,  5,  5,  5,  5,  0,-10,
     -5,  0,  5,  5,  5,  5,  0, -5,
      0,  0,  5,  5,  5,  5,  0, -5,
    -10,  5,  5,  5,  5,  5,  0,-10,
    -10,  0,  5,  0,  0,  0,  0,-10,
    -20,-10,-10, -5, -5,-10,-10,-20,
];
#[rustfmt::skip]
const KING_MIDDLE: [i32; 64] = [
    -30,-40,-40,-50,-50,-40,-40,-30,
    -30,-40,-40,-50,-50,-40,-40,-30,
    -30,-40,-40,-50,-50,-40,-40,-30,
    -30,-40,-40,-50,-50,-40,-40,-30,
    -20,-30,-30,-40,-40,-30,-30,-20,
    -10,-20,-20,-20,-20,-20,-20,-10,
     20, 20,  0,  0,  0,  0, 20, 20,
     20, 30, 10,  0,  0, 10, 30, 20,
];
#[rustfmt::skip]
const KING_END: [i32; 64] = [
    -50,-40,-30,-20,-20,-30,-40,-50,
    -30,-20,-10,  0,  0,-10,-20,-30,
    -30,-10, 20, 30, 30, 20,-10,-30,
    -30,-10, 30, 40, 40, 30,-10,-30,
    -30,-10, 30, 40, 40, 30,-10,-30,
    -30,-10, 20, 30, 30, 20,-10,-30,
    -30,-30,  0,  0,  0,  0,-30,-30,
    -50,-30,-30,-30,-30,-30,-30,-50,
];

/// How good the position is for the side to move, in hundredths of a pawn.
fn evaluate(b: &Board) -> i32 {
    let heavy: i32 = b.squares.iter().flatten().filter(|p| p.kind != Kind::Pawn).map(|p| value(p.kind)).sum();
    // Once the queens and most pieces are gone the king belongs in the middle.
    let endgame = heavy <= 2600;
    let mut score = 0;
    for (s, piece) in b.squares.iter().enumerate() {
        let Some(p) = piece else { continue };
        let (file, rank) = (file_of(s as u8) as usize, rank_of(s as u8) as usize);
        let i = if p.color == Color::White { (7 - rank) * 8 + file } else { rank * 8 + file };
        let table = match p.kind {
            Kind::Pawn => &PAWN,
            Kind::Knight => &KNIGHT,
            Kind::Bishop => &BISHOP,
            Kind::Rook => &ROOK,
            Kind::Queen => &QUEEN,
            Kind::King if endgame => &KING_END,
            Kind::King => &KING_MIDDLE,
        };
        let worth = value(p.kind) + table[i];
        score += if p.color == b.turn { worth } else { -worth };
    }
    score
}

/// Captures of valuable pieces by cheap ones first, then promotions, then the rest.
fn order(b: &Board, moves: &mut [Move], first: Option<Move>) {
    moves.sort_by_cached_key(|&m| {
        if Some(m) == first {
            return -INFINITY;
        }
        let mut score = 0;
        if b.is_capture(m) {
            let victim = b.at(m.to).map_or(100, |p| value(p.kind));
            score += 10_000 + victim * 10 - value(b.at(m.from).expect("a move starts from a piece").kind);
        }
        if let Some(k) = m.promo {
            score += 9_000 + value(k);
        }
        -score
    });
}

struct Search<'a> {
    stop: &'a AtomicBool,
    deadline: Instant,
    nodes: u64,
    aborted: bool,
    /// Keys of the positions from the start of the game down the line being looked at.
    path: Vec<u64>,
}

impl Search<'_> {
    fn out_of_time(&mut self) -> bool {
        self.nodes += 1;
        if self.nodes.is_multiple_of(2048) && (self.stop.load(Ordering::Relaxed) || Instant::now() >= self.deadline) {
            self.aborted = true;
        }
        self.aborted
    }

    /// Whether the position at the end of `path` has been seen before. Counting one
    /// repeat as a draw is enough to stop the search shuffling to and fro.
    fn repeated(&self, b: &Board) -> bool {
        let Some((&key, earlier)) = self.path.split_last() else { return false };
        earlier.iter().rev().take(b.halfmove as usize).any(|&k| k == key)
    }

    fn negamax(&mut self, b: &Board, depth: u32, mut alpha: i32, beta: i32, ply: usize) -> i32 {
        if self.out_of_time() {
            return 0;
        }
        if ply > 0 && (b.halfmove >= 100 || self.repeated(b) || b.insufficient_material()) {
            return 0;
        }
        let in_check = b.in_check(b.turn);
        if ply >= MAX_PLY {
            return evaluate(b);
        }
        let depth = if in_check { depth + 1 } else { depth };
        if depth == 0 {
            return self.quiesce(b, alpha, beta, ply);
        }
        let mut moves = Vec::with_capacity(48);
        b.pseudo_moves(&mut moves);
        order(b, &mut moves, None);
        let mut any = false;
        for m in moves {
            let next = b.play(m);
            if next.in_check(b.turn) {
                continue;
            }
            any = true;
            self.path.push(next.key());
            let score = -self.negamax(&next, depth - 1, -beta, -alpha, ply + 1);
            self.path.pop();
            if self.aborted {
                return 0;
            }
            if score >= beta {
                return score;
            }
            alpha = alpha.max(score);
        }
        match (any, in_check) {
            (true, _) => alpha,
            // Sooner mates score higher, so a won game is finished and not dragged out.
            (false, true) => -MATE + ply as i32,
            (false, false) => 0,
        }
    }

    /// Plays out the captures at the end of a line, so that a position is not judged
    /// in the middle of an exchange.
    fn quiesce(&mut self, b: &Board, mut alpha: i32, beta: i32, ply: usize) -> i32 {
        if self.out_of_time() {
            return 0;
        }
        let stand = evaluate(b);
        if stand >= beta || ply >= MAX_PLY {
            return stand;
        }
        alpha = alpha.max(stand);
        let mut moves = Vec::with_capacity(48);
        b.pseudo_moves(&mut moves);
        moves.retain(|&m| b.is_capture(m) || m.promo == Some(Kind::Queen));
        order(b, &mut moves, None);
        for m in moves {
            let next = b.play(m);
            if next.in_check(b.turn) {
                continue;
            }
            let score = -self.quiesce(&next, -beta, -alpha, ply + 1);
            if score >= beta {
                return score;
            }
            alpha = alpha.max(score);
        }
        alpha
    }

    /// The value of every legal move at one depth. With `exact`, each is searched in
    /// full; otherwise only the best is exact and the rest are upper bounds, which is
    /// much faster and all that picking the best move needs.
    fn root(&mut self, b: &Board, moves: &[Move], depth: u32, exact: bool) -> Option<Vec<(Move, i32)>> {
        let mut scored = Vec::with_capacity(moves.len());
        let mut alpha = -INFINITY;
        for &m in moves {
            let next = b.play(m);
            self.path.push(next.key());
            let score = -self.negamax(&next, depth - 1, -INFINITY, if exact { INFINITY } else { -alpha }, 1);
            self.path.pop();
            if self.aborted {
                return None;
            }
            alpha = alpha.max(score);
            scored.push((m, score));
        }
        Some(scored)
    }
}

/// A small random number generator (xorshift), so that no dependency is needed for it.
fn random(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

/// The move the computer plays. `keys` are the keys of the positions of the game so
/// far, the current one last. Setting `stop` makes it return early with whatever it
/// has. `None` only when there is no legal move.
pub fn best_move(b: &Board, keys: &[u64], level: Level, stop: &AtomicBool, mut seed: u64) -> Option<Move> {
    let (max_depth, time, luck) = level.limits();
    let mut moves = b.legal_moves();
    seed |= 1;
    // A different order each game, so equal moves are not always resolved the same way.
    for i in (1..moves.len()).rev() {
        moves.swap(i, (random(&mut seed) % (i as u64 + 1)) as usize);
    }
    order(b, &mut moves, None);
    let mut best = *moves.first()?;
    if moves.len() == 1 {
        return Some(best);
    }
    let mut search = Search { stop, deadline: Instant::now() + time, nodes: 0, aborted: false, path: keys.to_vec() };
    for depth in 1..=max_depth {
        let exact = luck > 0 && depth == max_depth;
        let Some(mut scored) = search.root(b, &moves, depth, exact) else { break };
        if exact {
            for (_, score) in &mut scored {
                // A mate it has seen is never left to luck.
                if score.abs() < MATE - 1000 {
                    *score += (random(&mut seed) % (luck as u64 + 1)) as i32;
                }
            }
        }
        // Stable, so that among equal scores the move searched first (the one whose
        // score is exact) stays in front.
        scored.sort_by_key(|&(_, score)| -score);
        best = scored[0].0;
        moves = scored.iter().map(|&(m, _)| m).collect();
        if scored[0].1.abs() >= MATE - 1000 && depth >= 2 {
            break;
        }
    }
    Some(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn think(fen: &str, level: Level) -> String {
        let b = Board::from_fen(fen).unwrap();
        best_move(&b, &[b.key()], level, &AtomicBool::new(false), 7).unwrap().uci()
    }

    #[test]
    fn finds_mate_in_one_at_every_level() {
        for level in Level::ALL {
            assert_eq!(think("6k1/5ppp/8/8/8/8/8/R3K3 w - - 0 1", level), "a1a8", "{level:?}");
        }
    }

    #[test]
    fn finds_mate_in_two() {
        // 1. Qxh7+ Kf8 2. Qh8#
        assert_eq!(think("r4rk1/5ppp/8/6N1/8/8/7Q/4K3 w - - 0 1", Level::Medium), "h2h7");
    }

    #[test]
    fn takes_a_hanging_queen() {
        for level in [Level::Easy, Level::Medium, Level::Hard] {
            assert_eq!(think("4k3/8/8/3q4/8/8/8/3RK3 w - - 0 1", level), "d1d5", "{level:?}");
        }
    }

    #[test]
    fn avoids_stalemate_when_winning() {
        assert_ne!(think("7k/5Q2/8/8/8/8/8/K7 w - - 0 1", Level::Medium), "f7g6");
    }

    /// Whole games between the weaker levels: every move it picks must be legal, and
    /// nothing may panic along the way.
    #[test]
    fn plays_whole_games_against_itself() {
        for seed in 1..=3 {
            let mut game = crate::chess::Game::new();
            while game.outcome.is_none() && game.history.len() < 300 {
                let level = if game.board.turn == Color::White { Level::Beginner } else { Level::Easy };
                let m = best_move(&game.board, &game.keys(), level, &AtomicBool::new(false), seed + game.history.len() as u64).unwrap();
                assert!(game.play(m), "{} is not legal", m.uci());
            }
        }
    }

    #[test]
    fn stops_when_told_to() {
        let stop = AtomicBool::new(true);
        let b = Board::start();
        let started = Instant::now();
        assert!(best_move(&b, &[b.key()], Level::Hard, &stop, 1).is_some());
        assert!(started.elapsed() < Duration::from_millis(500));
    }
}
