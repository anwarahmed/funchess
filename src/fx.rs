//! What moves on the board: a piece on its way to its square and whatever follows when
//! it lands (a captured piece bursting, a king shaking in check or falling when mated,
//! sparks around a promoted pawn), and the confetti for a win. Only times and places
//! are worked out here, from the clock in `App`; `ui.rs` draws them.

use std::f32::consts::{PI, TAU};
use std::time::Duration;

use crate::chess::{Board, Kind, Move, Piece, Square, file_of, rank_of, sq};

const BURST: Duration = Duration::from_millis(450);
const SHAKE: Duration = Duration::from_millis(500);
const SPARKLE: Duration = Duration::from_millis(650);
/// A mated king shakes this long, and then takes this long to fall.
const FALL_AFTER: Duration = Duration::from_millis(300);
const FALL: Duration = Duration::from_millis(450);
/// How long confetti falls after a win.
pub const CONFETTI: Duration = Duration::from_millis(6500);
/// How many sparks circle a promoted pawn.
pub const SPARKS: u32 = 10;

/// A number from 0 to 1 that looks random and is always the same for the same `n`, so
/// that every frame puts a particle where the frame before would expect it.
pub fn noise(n: u32) -> f32 {
    let mut x = n.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297A_2D39);
    x ^= x >> 15;
    (x >> 8) as f32 / (1 << 24) as f32
}

/// How far through `length` the time `since` is, from 0 to 1; nothing once it is past.
fn progress(since: Duration, length: Duration) -> Option<f32> {
    (since < length).then(|| since.as_secs_f32() / length.as_secs_f32())
}

/// The last move, being shown. Places are in files and ranks, with fractions.
pub struct MoveFx {
    start: Duration,
    /// The piece as it set off: a pawn that promotes is still a pawn on the way.
    pub piece: Piece,
    pub from: Square,
    pub to: Square,
    /// The rook's squares when the move castles.
    pub rook: Option<(Square, Square)>,
    /// What was captured and where it stood, which en passant makes another square.
    pub captured: Option<(Square, Piece)>,
    pub promoted: bool,
    /// The square of the king the move put in check.
    pub checked: Option<Square>,
    pub mate: bool,
}

impl MoveFx {
    /// The move `m` from `before` to `after`, starting at `start` on the clock.
    pub fn new(start: Duration, before: &Board, m: Move, after: &Board, mate: bool) -> Option<MoveFx> {
        let piece = before.at(m.from)?;
        let rank = rank_of(m.from);
        let rook = (piece.kind == Kind::King && file_of(m.from).abs_diff(file_of(m.to)) == 2)
            .then(|| if file_of(m.to) > file_of(m.from) { (sq(7, rank), sq(5, rank)) } else { (sq(0, rank), sq(3, rank)) });
        // A pawn changing file onto an empty square took the pawn beside it.
        let passed = sq(file_of(m.to), rank);
        let captured = match before.at(m.to) {
            Some(taken) => Some((m.to, taken)),
            None if piece.kind == Kind::Pawn && file_of(m.from) != file_of(m.to) => before.at(passed).map(|taken| (passed, taken)),
            None => None,
        };
        let checked = after.in_check(after.turn).then(|| after.king_square(after.turn));
        Some(MoveFx { start, piece, from: m.from, to: m.to, rook, captured, promoted: m.promo.is_some(), checked, mate })
    }

    /// The further the piece goes the longer it takes, but not in proportion.
    fn slide(&self) -> Duration {
        let far = file_of(self.from).abs_diff(file_of(self.to)).max(rank_of(self.from).abs_diff(rank_of(self.to)));
        Duration::from_millis(130 + 40 * far as u64)
    }

    fn length(&self) -> Duration {
        let after = [(self.captured.is_some(), BURST), (self.promoted, SPARKLE), (self.checked.is_some() && !self.mate, SHAKE), (self.mate, FALL_AFTER + FALL)];
        self.slide() + after.into_iter().filter(|&(applies, _)| applies).map(|(_, length)| length).max().unwrap_or_default()
    }

    /// When all of it is over.
    pub fn end(&self) -> Duration {
        self.start + self.length()
    }

    /// When the piece arrives.
    pub fn lands(&self) -> Duration {
        self.start + self.slide()
    }

    /// How long ago the piece arrived; nothing while it is on its way.
    fn landed(&self, clock: Duration) -> Option<Duration> {
        clock.checked_sub(self.start + self.slide())
    }

    pub fn sliding(&self, clock: Duration) -> bool {
        self.landed(clock).is_none()
    }

    /// How far along its way the piece is, from 0 to 1.
    fn along(&self, clock: Duration) -> f32 {
        progress(clock.saturating_sub(self.start), self.slide()).unwrap_or(1.0)
    }

    fn between(from: Square, to: Square, t: f32) -> (f32, f32) {
        // Slow away from the square, quick in the middle, slow onto the other.
        let t = t * t * (3.0 - 2.0 * t);
        let mix = |a: u8, b: u8| a as f32 + (b as f32 - a as f32) * t;
        (mix(file_of(from), file_of(to)), mix(rank_of(from), rank_of(to)))
    }

    /// Where the piece is: file, rank, and how far above the board it has jumped, in
    /// squares. Only the knight jumps.
    pub fn place(&self, clock: Duration) -> (f32, f32, f32) {
        let t = self.along(clock);
        let (file, rank) = MoveFx::between(self.from, self.to, t);
        (file, rank, if self.piece.kind == Kind::Knight { (PI * t).sin() * 0.5 } else { 0.0 })
    }

    /// Where the castling rook is, and the square it is going to.
    pub fn rook_place(&self, clock: Duration) -> Option<(f32, f32)> {
        self.rook.map(|(from, to)| MoveFx::between(from, to, self.along(clock)))
    }

    /// How far the captured piece has burst apart, from 0 to 1.
    pub fn burst(&self, clock: Duration) -> Option<f32> {
        self.captured.and(self.landed(clock)).and_then(|since| progress(since, BURST))
    }

    /// How far the sparks around a promoted pawn have spread, from 0 to 1.
    pub fn sparkle(&self, clock: Duration) -> Option<f32> {
        self.landed(clock).filter(|_| self.promoted).and_then(|since| progress(since, SPARKLE))
    }

    /// How far to one side the king in check is, from -1 to 1, dying away.
    pub fn shake(&self, clock: Duration) -> f32 {
        let length = if self.mate { FALL_AFTER } else { SHAKE };
        match self.landed(clock).filter(|_| self.checked.is_some()).and_then(|since| progress(since, length)) {
            Some(t) => (t * length.as_secs_f32() * 9.0 * TAU).sin() * (1.0 - t * 0.7),
            None => 0.0,
        }
    }

    /// How far the mated king has fallen over, from 0 to 1; nothing until it starts to.
    pub fn fall(&self, clock: Duration) -> Option<f32> {
        let since = self.landed(clock).filter(|_| self.mate)?.checked_sub(FALL_AFTER)?;
        Some(progress(since, FALL).unwrap_or(1.0))
    }
}

/// Where bit `i` of a captured piece has flown to when the burst is `p` of the way
/// through, in squares from where it started; nothing once it has faded.
pub fn fragment(i: u32, p: f32) -> Option<(f32, f32)> {
    if noise(i * 3 + 2) < p * p {
        return None;
    }
    let (angle, speed) = (noise(i * 3) * TAU, 0.3 + 0.9 * noise(i * 3 + 1));
    // Thrown outwards and a little up, then pulled down.
    Some((angle.cos() * speed * p, angle.sin() * speed * p - 0.4 * p + 1.1 * p * p))
}

/// Where spark `k` is when the sparkle is `p` of the way through, in squares from the
/// middle of the square; nothing while it is dark.
pub fn spark(k: u32, p: f32) -> Option<(f32, f32)> {
    if ((p * 9.0) as u32 + k).is_multiple_of(3) {
        return None;
    }
    let (angle, radius) = (k as f32 / SPARKS as f32 * TAU + p * 2.0, 0.2 + 0.5 * p);
    Some((angle.cos() * radius, angle.sin() * radius))
}

/// Where piece `i` of confetti is, `seconds` after it began, over a field `width` by
/// `height`, and which of the colors it has; nothing before it appears or after it
/// has fallen out of the bottom.
pub fn confetti(i: u32, seconds: f32, width: f32, height: f32) -> Option<(f32, f32, u32)> {
    let falling = seconds - noise(i * 4) * 3.5;
    let y = falling * height * (0.4 + 0.4 * noise(i * 4 + 1)) - 1.0;
    if falling < 0.0 || y >= height {
        return None;
    }
    let sway = (falling * 4.0 + noise(i * 4 + 2) * TAU).sin() * width * 0.02;
    Some((noise(i * 4 + 3) * width + sway, y, i))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chess::{Color, parse_square};

    const MS: Duration = Duration::from_millis(1);

    fn fx(fen: &str, uci: &str) -> MoveFx {
        let before = Board::from_fen(fen).unwrap();
        let m = Move::parse_uci(uci).unwrap();
        let after = before.play(m);
        MoveFx::new(Duration::ZERO, &before, m, &after, after.legal_moves().is_empty() && after.in_check(after.turn)).unwrap()
    }

    #[test]
    fn a_piece_slides_from_its_square_to_the_next_and_a_knight_jumps() {
        let slide = fx("4k3/8/8/8/8/8/8/R3K3 w - - 0 1", "a1a8");
        assert_eq!(slide.place(Duration::ZERO), (0.0, 0.0, 0.0));
        let (file, rank, lift) = slide.place(slide.slide() / 2);
        assert!(file == 0.0 && (rank - 3.5).abs() < 0.01 && lift == 0.0, "{file} {rank} {lift}");
        assert_eq!(slide.place(slide.slide()), (0.0, 7.0, 0.0));
        assert!(slide.sliding(slide.slide() - MS) && !slide.sliding(slide.slide()));
        // Check, and nothing else: the king shakes for a while and that is all.
        assert_eq!((slide.checked, slide.mate, slide.captured, slide.rook), (parse_square("e8"), false, None, None));
        assert!(slide.shake(slide.slide() + 20 * MS) != 0.0 && slide.shake(slide.end()) == 0.0);
        assert_eq!(slide.end(), slide.slide() + SHAKE);

        let jump = fx("4k3/8/8/8/8/8/8/1N2K3 w - - 0 1", "b1c3");
        assert!(jump.place(jump.slide() / 2).2 > 0.4 && jump.place(jump.slide()).2.abs() < 0.001);
        assert_eq!(jump.end(), jump.slide(), "a quiet move is over when the piece lands");
        assert!(jump.slide() < slide.slide(), "a long way takes longer");
    }

    #[test]
    fn captures_castling_promotion_and_mate_are_noticed() {
        let pawn = |color| Piece { color, kind: Kind::Pawn };
        let takes = fx("4k3/8/8/3p4/4P3/8/8/4K3 w - - 0 1", "e4d5");
        assert_eq!(takes.captured, Some((parse_square("d5").unwrap(), pawn(Color::Black))));
        assert_eq!((takes.burst(takes.slide() - MS), takes.burst(takes.slide())), (None, Some(0.0)));
        assert_eq!(takes.burst(takes.end()), None);

        let passing = fx("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1", "e5d6");
        assert_eq!(passing.captured, Some((parse_square("d5").unwrap(), pawn(Color::Black))));

        let castles = fx("4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1", "e1g1");
        assert_eq!(castles.rook, Some((parse_square("h1").unwrap(), parse_square("f1").unwrap())));
        assert_eq!(castles.rook_place(castles.slide()), Some((5.0, 0.0)));
        assert_eq!(fx("4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1", "e1c1").rook_place(Duration::ZERO), Some((0.0, 0.0)));

        let queens = fx("7k/P7/8/8/8/8/8/K7 w - - 0 1", "a7a8q");
        assert!(queens.promoted && queens.piece.kind == Kind::Pawn && queens.sparkle(queens.slide() + MS).is_some());
        assert_eq!(queens.sparkle(queens.slide() - MS), None);

        // The mated king shakes, then falls, and stays down.
        let mate = fx("6k1/5ppp/8/8/8/8/8/R3K3 w - - 0 1", "a1a8");
        assert!(mate.mate && mate.fall(mate.slide() + FALL_AFTER - MS).is_none());
        assert_eq!(mate.fall(mate.slide() + FALL_AFTER), Some(0.0));
        assert_eq!((mate.fall(mate.end()), mate.fall(mate.end() * 3)), (Some(1.0), Some(1.0)));
    }

    #[test]
    fn particles_stay_where_they_belong_and_all_go_in_the_end() {
        for i in 0..500 {
            assert!((0.0..1.0).contains(&noise(i)));
            assert_eq!(fragment(i, 0.0), Some((0.0, 0.0)));
            assert_eq!(fragment(i, 1.0), None);
            assert_eq!(confetti(i, CONFETTI.as_secs_f32(), 96.0, 96.0), None);
            for tenth in 0..65 {
                if let Some((x, y, _)) = confetti(i, tenth as f32 / 10.0, 96.0, 96.0) {
                    assert!((-3.0..99.0).contains(&x) && (-1.0..96.0).contains(&y), "{x} {y}");
                }
            }
        }
        assert!((0..500).filter(|&i| confetti(i, 2.0, 96.0, 96.0).is_some()).count() > 100);
        assert!((0..SPARKS).filter(|&k| spark(k, 0.5).is_some()).count() >= SPARKS as usize / 2);
    }
}
