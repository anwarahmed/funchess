//! Sound effects. Every sound is made here out of a few notes, written once as a WAV
//! file, and played by handing that file to a program the system already has
//! (`pw-play`, `paplay` or `aplay` on Linux, `afplay` on macOS). Nothing is linked
//! against an audio library, and where no such program exists the game is silent.

use std::f32::consts::TAU;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

/// Samples a second, of one channel of 16 bits.
const RATE: u32 = 22_050;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sound {
    /// A new game.
    Start,
    Move,
    Capture,
    Castle,
    Check,
    Promote,
    Hint,
    /// The game ended, and whoever is at this keyboard won it, lost it, or neither.
    Win,
    Lose,
    Draw,
}

#[derive(Clone, Copy)]
enum Wave {
    /// A pure tone: a bell.
    Sine,
    /// A brighter one.
    Triangle,
    /// A knock: a tone that drops as it dies away, which is what wood does.
    Knock,
    /// A hiss.
    Noise,
}

/// One note: what it sounds like, its pitch in Hz, when it starts and how long it
/// lasts in seconds, and how loud it is, from 0 to 1.
type Note = (Wave, f32, f32, f32, f32);

// The notes of the tunes, in Hz.
const C4: f32 = 261.6;
const E4: f32 = 329.6;
const G4: f32 = 392.0;
const C5: f32 = 523.3;
const E5: f32 = 659.3;
const G5: f32 = 784.0;
const C6: f32 = 1046.5;
const E6: f32 = 1318.5;
const G6: f32 = 1568.0;

impl Sound {
    #[cfg(test)]
    pub const ALL: [Sound; 10] =
        [Sound::Start, Sound::Move, Sound::Capture, Sound::Castle, Sound::Check, Sound::Promote, Sound::Hint, Sound::Win, Sound::Lose, Sound::Draw];

    fn name(self) -> &'static str {
        match self {
            Sound::Start => "start",
            Sound::Move => "move",
            Sound::Capture => "capture",
            Sound::Castle => "castle",
            Sound::Check => "check",
            Sound::Promote => "promote",
            Sound::Hint => "hint",
            Sound::Win => "win",
            Sound::Lose => "lose",
            Sound::Draw => "draw",
        }
    }

    fn notes(self) -> &'static [Note] {
        use Wave::{Knock, Noise, Sine, Triangle};
        match self {
            Sound::Start => &[(Triangle, C5, 0.0, 0.12, 0.3), (Triangle, G5, 0.1, 0.22, 0.3)],
            // A piece put down on wood.
            Sound::Move => &[(Knock, 330.0, 0.0, 0.09, 0.6), (Noise, 0.0, 0.0, 0.02, 0.2)],
            // The same, harder, with the other piece knocked away.
            Sound::Capture => &[(Noise, 0.0, 0.0, 0.07, 0.45), (Knock, 200.0, 0.0, 0.16, 0.7), (Knock, 420.0, 0.05, 0.08, 0.35)],
            Sound::Castle => &[(Knock, 330.0, 0.0, 0.09, 0.6), (Knock, 290.0, 0.11, 0.09, 0.6)],
            Sound::Check => &[(Triangle, G5, 0.0, 0.1, 0.35), (Triangle, C6, 0.09, 0.18, 0.35)],
            Sound::Promote => &[(Sine, C5, 0.0, 0.12, 0.3), (Sine, E5, 0.07, 0.12, 0.3), (Sine, G5, 0.14, 0.12, 0.3), (Sine, C6, 0.21, 0.3, 0.35)],
            Sound::Hint => &[(Sine, E6, 0.0, 0.3, 0.25), (Sine, G6, 0.0, 0.2, 0.1)],
            Sound::Win => &[
                (Triangle, C5, 0.0, 0.14, 0.3),
                (Triangle, E5, 0.12, 0.14, 0.3),
                (Triangle, G5, 0.24, 0.14, 0.3),
                (Triangle, C6, 0.36, 0.6, 0.3),
                (Sine, E6, 0.36, 0.6, 0.15),
                (Sine, G6, 0.36, 0.6, 0.12),
            ],
            // Down instead of up, and softly: it is no fun to be laughed at.
            Sound::Lose => &[(Sine, G4, 0.0, 0.22, 0.25), (Sine, E4, 0.18, 0.22, 0.25), (Sine, C4, 0.36, 0.45, 0.25)],
            Sound::Draw => &[(Sine, E5, 0.0, 0.16, 0.25), (Sine, E5, 0.2, 0.3, 0.25)],
        }
    }

    /// The sound itself.
    fn samples(self) -> Vec<i16> {
        let notes = self.notes();
        let length = notes.iter().map(|&(_, _, start, lasts, _)| start + lasts).fold(0.0, f32::max);
        let mut mix = vec![0.0f32; (length * RATE as f32) as usize];
        let mut seed = 0x2545_F491u32;
        for &(wave, pitch, start, lasts, loud) in notes {
            let first = (start * RATE as f32) as usize;
            for i in 0..(lasts * RATE as f32) as usize {
                let t = i as f32 / RATE as f32;
                let turn = (t * pitch).fract();
                let value = match wave {
                    Wave::Sine => (turn * TAU).sin(),
                    Wave::Triangle => 4.0 * (turn - 0.5).abs() - 1.0,
                    Wave::Knock => (TAU * pitch * (t - 2.0 * t * t)).sin(),
                    Wave::Noise => {
                        seed ^= seed << 13;
                        seed ^= seed >> 17;
                        seed ^= seed << 5;
                        seed as f32 / u32::MAX as f32 * 2.0 - 1.0
                    }
                };
                // In quickly, so that it does not click, and then dying away.
                let shape = (t / 0.004).min(1.0) * (-5.0 * t / lasts).exp() * (1.0 - t / lasts);
                if let Some(sample) = mix.get_mut(first + i) {
                    *sample += value * shape * loud;
                }
            }
        }
        mix.into_iter().map(|v| (v.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).collect()
    }

    /// The sound as a WAV file.
    fn wav(self) -> Vec<u8> {
        let samples = self.samples();
        let bytes = samples.len() as u32 * 2;
        let mut out = Vec::with_capacity(44 + bytes as usize);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + bytes).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        // 16 bytes of format: plain samples, one channel, the rate, bytes a second,
        // bytes a sample and bits a sample.
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&RATE.to_le_bytes());
        out.extend_from_slice(&(RATE * 2).to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&bytes.to_le_bytes());
        out.extend(samples.into_iter().flat_map(i16::to_le_bytes));
        out
    }
}

/// What plays the sounds, or does not.
pub struct Speaker {
    /// The program that plays a WAV file given as its last argument, and where the
    /// files are kept. Nothing when the game is silent.
    player: Option<(PathBuf, &'static [&'static str], PathBuf)>,
    written: Vec<Sound>,
    /// The players still playing, so that none is left behind as a zombie.
    playing: Vec<Child>,
    /// Everything asked for, in order.
    #[cfg(test)]
    pub heard: Vec<Sound>,
}

impl Speaker {
    pub fn silent() -> Speaker {
        Speaker {
            player: None,
            written: Vec::new(),
            playing: Vec::new(),
            #[cfg(test)]
            heard: Vec::new(),
        }
    }

    /// Plays through `program`, keeping the sound files in `dir`.
    pub fn through(program: PathBuf, arguments: &'static [&'static str], dir: PathBuf) -> Speaker {
        Speaker { player: Some((program, arguments, dir)), ..Speaker::silent() }
    }

    /// Plays through whichever of the system's own players is installed, looked for
    /// along `PATH`. Silent when there is none.
    pub fn find(dir: PathBuf) -> Speaker {
        let players: &[(&str, &[&str])] = if cfg!(target_os = "macos") { &[("afplay", &[])] } else { &[("pw-play", &[]), ("paplay", &[]), ("aplay", &["-q"])] };
        let path = std::env::var_os("PATH").unwrap_or_default();
        for &(name, arguments) in players {
            if let Some(program) = std::env::split_paths(&path).map(|dir| dir.join(name)).find(|file| file.is_file()) {
                return Speaker::through(program, arguments, dir);
            }
        }
        Speaker::silent()
    }

    /// Whether there is anything to play a sound with.
    pub fn is_on(&self) -> bool {
        self.player.is_some()
    }

    /// Starts the sound and returns at once. A sound that cannot be played is not
    /// worth interrupting a game for.
    pub fn play(&mut self, sound: Sound) {
        #[cfg(test)]
        self.heard.push(sound);
        let Some((program, arguments, dir)) = &self.player else { return };
        self.playing.retain_mut(|child| !matches!(child.try_wait(), Ok(Some(_)) | Err(_)));
        let file = dir.join(format!("{}.wav", sound.name()));
        if !self.written.contains(&sound) {
            if std::fs::create_dir_all(dir).and_then(|()| std::fs::write(&file, sound.wav())).is_err() {
                return;
            }
            self.written.push(sound);
        }
        // It must not write on the screen the game is drawn on.
        let child = Command::new(program).args(*arguments).arg(&file).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
        self.playing.extend(child);
    }

    /// Where a sound's file is kept, once it has been played.
    #[cfg(test)]
    fn file(&self, sound: Sound) -> Option<PathBuf> {
        self.player.as_ref().map(|(_, _, dir)| dir.join(format!("{}.wav", sound.name())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sound_is_short_heard_and_never_too_loud() {
        for sound in Sound::ALL {
            let samples = sound.samples();
            let seconds = samples.len() as f32 / RATE as f32;
            assert!((0.05..1.2).contains(&seconds), "{sound:?} lasts {seconds}");
            let loudest = samples.iter().map(|s| s.unsigned_abs()).max().unwrap();
            assert!((3000..30000).contains(&loudest), "{sound:?} peaks at {loudest}");
            // It starts and ends in silence, so nothing clicks.
            assert!(samples[0].abs() < 600 && samples[samples.len() - 1].abs() < 600, "{sound:?}");

            let wav = sound.wav();
            assert_eq!((&wav[..4], &wav[8..16], &wav[36..40]), (&b"RIFF"[..], &b"WAVEfmt "[..], &b"data"[..]));
            assert_eq!(u32::from_le_bytes(wav[4..8].try_into().unwrap()) as usize, wav.len() - 8);
            assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()) as usize, samples.len() * 2);
        }
        let names: std::collections::HashSet<_> = Sound::ALL.into_iter().map(Sound::name).collect();
        assert_eq!(names.len(), Sound::ALL.len());
    }

    /// A stand-in for the system's player, which notes what it was asked to play.
    #[test]
    fn a_sound_is_a_file_handed_to_the_systems_player() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("funchess-sound-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (player, log) = (dir.join("player"), dir.join("log"));
        std::fs::write(&player, format!("#!/bin/sh\necho \"$@\" >> '{}'\n", log.display())).unwrap();
        std::fs::set_permissions(&player, std::fs::Permissions::from_mode(0o755)).unwrap();

        let mut speaker = Speaker::through(player, &["-q"], dir.join("sounds"));
        assert!(speaker.is_on());
        speaker.play(Sound::Move);
        speaker.play(Sound::Win);
        speaker.play(Sound::Move);
        for child in &mut speaker.playing {
            child.wait().unwrap();
        }
        let (moved, won) = (speaker.file(Sound::Move).unwrap(), speaker.file(Sound::Win).unwrap());
        let mut asked: Vec<String> = std::fs::read_to_string(&log).unwrap().lines().map(String::from).collect();
        asked.sort();
        assert_eq!(asked, [format!("-q {}", moved.display()), format!("-q {}", moved.display()), format!("-q {}", won.display())]);
        assert_eq!(std::fs::read(&moved).unwrap(), Sound::Move.wav());
        assert_eq!(speaker.heard, [Sound::Move, Sound::Win, Sound::Move]);

        // With no player there is nothing to hear and nothing is written.
        let mut speaker = Speaker::silent();
        speaker.play(Sound::Check);
        assert!(!speaker.is_on() && speaker.playing.is_empty() && speaker.written.is_empty());
        // A player that is not there is no reason to stop.
        let mut speaker = Speaker::through(dir.join("missing"), &[], dir.join("sounds"));
        speaker.play(Sound::Check);
        assert!(speaker.playing.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
