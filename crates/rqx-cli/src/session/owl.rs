//! What the owl is doing, and which frame of the sprite shows it.
//!
//! While a job runs it reads: the pupils drop and sweep along a line, jump back for the
//! next, and then the owl looks up and around. While the header is live at the prompt,
//! the owl in it wakes up, reacts to the verdict, rests on the verdict's face, and makes
//! one small move every few seconds; a keystroke stops it. The header that goes into
//! scrollback carries the still frame. After a later scan the busy owl plays the same
//! reaction before its rows go away.
//! With motion off it is the still frame always. The plumage follows the verdict, which
//! is the caller's to say: this only picks the frame.
//!
//! Every sequence is a table of `(frame, ticks)` rows, a tick being one `motion::FRAME`.

use std::ops::RangeInclusive;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ratatui::text::Line;

use crate::brand::{self, Ink, Palette};

use super::motion::{FRAME, Motion};
use super::sprite::Sprite;

/// One row of a sequence: a frame of the sprite and how many ticks it holds.
type Step = (&'static str, u32);

/// A row naming this frame shows whichever face the owl rests on.
const REST: &str = "rest";

/// The header owl waking up: tufts tucked and eyes shut, the lids lift, it blinks once
/// and flicks its right tuft twice.
const ENTRANCE: [Step; 9] = [
    ("sleep", 10),
    ("drowsy", 6),
    ("half", 4),
    ("idle", 6),
    ("blink", 3),
    ("idle", 5),
    ("twitch-right", 3),
    ("idle", 2),
    ("twitch-right", 3),
];

/// One of the small moves the resting owl makes, and how often it is picked out of the
/// weights' total of 100.
struct Move {
    weight: u32,
    steps: &'static [Step],
}

const MOVES: [Move; 9] = [
    Move {
        weight: 40,
        steps: &[("blink", 4)],
    },
    Move {
        weight: 10,
        steps: &[("blink", 3), (REST, 4), ("blink", 3)],
    },
    Move {
        weight: 10,
        steps: &[("half", 4), ("blink", 10), ("half", 4)],
    },
    Move {
        weight: 8,
        steps: &[("glance-left", 20)],
    },
    Move {
        weight: 8,
        steps: &[("glance-right", 20)],
    },
    Move {
        weight: 8,
        steps: &[("look-down", 30)],
    },
    Move {
        weight: 4,
        steps: &[("twitch-left", 3), (REST, 3), ("twitch-left", 3)],
    },
    Move {
        weight: 4,
        steps: &[("twitch-right", 3), (REST, 3), ("twitch-right", 3)],
    },
    Move {
        weight: 8,
        steps: &[("breathe", 45)],
    },
];

/// Ticks of rest between two moves, about 4 to 10 seconds.
const GAP: RangeInclusive<u32> = 121..=303;

/// How a scan came out, in the bands the plumage colour uses.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Good,
    Fair,
    Poor,
}

impl Verdict {
    /// Read off `brand::mascot_ink`, so the face and the plumage cannot disagree.
    pub const fn of(health: Option<u32>) -> Self {
        match brand::mascot_ink(health) {
            Ink::Warn => Self::Fair,
            Ink::Danger => Self::Poor,
            _ => Self::Good,
        }
    }

    /// What the owl does the moment the scan lands, 24 ticks for every verdict.
    const fn reaction(self) -> &'static [Step] {
        match self {
            Self::Good => &[("blink", 3), ("happy", 21)],
            Self::Fair => &[("look-down", 8), ("half", 10), ("blink", 3), ("idle", 3)],
            Self::Poor => &[("alarm", 5), ("worried", 7), ("blink", 3), ("worried", 9)],
        }
    }

    /// How long the reaction plays, which is how long the busy owl stays on screen for
    /// it after a later scan.
    pub fn reaction_time(self) -> Duration {
        FRAME * length(self.reaction())
    }

    /// The face the owl rests on until the next verdict.
    const fn rest(self) -> &'static str {
        match self {
            Self::Good => "content",
            Self::Fair => "idle",
            Self::Poor => "worried",
        }
    }
}

/// The move a roll of `0..100` lands on, walking the weights in order.
fn move_for(mut roll: u32) -> &'static Move {
    MOVES
        .iter()
        .find(|candidate| {
            let hit = roll < candidate.weight;
            roll = roll.saturating_sub(candidate.weight);
            hit
        })
        .unwrap_or(&MOVES[0])
}

/// Where the idle moves' randomness comes from: SplitMix64, which is small and, seeded,
/// repeats exactly, so a test can pin which move comes and when.
pub struct Dice(u64);

impl Dice {
    pub const fn seeded(seed: u64) -> Self {
        Self(seed)
    }

    pub fn from_clock() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        Self::seeded(nanos as u64)
    }

    /// A number in `0..bound`.
    fn below(&mut self, bound: u32) -> u32 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut mixed = self.0;
        mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        mixed ^= mixed >> 31;
        (mixed % u64::from(bound.max(1))) as u32
    }
}

/// The live owl's state at the prompt, in ticks of the session clock: the sequence it is
/// playing, the face it rests on between them, and when the next idle move is due.
pub struct Stage {
    rest: &'static str,
    act: Vec<Step>,
    act_from: u32,
    quiet_until: u32,
    dice: Dice,
}

impl Stage {
    pub fn new(dice: Dice) -> Self {
        let mut stage = Self {
            rest: "idle",
            act: Vec::new(),
            act_from: 0,
            quiet_until: 0,
            dice,
        };
        stage.quiet_until = stage.gap();
        stage
    }

    /// Wakes the owl up, as the header first shows it.
    pub fn enter(&mut self, now: u32) {
        self.play(&ENTRANCE, now);
    }

    /// Plays the verdict's reaction once whatever is playing has ended, and makes the
    /// verdict's face the one the owl rests on from then on.
    pub fn react(&mut self, verdict: Verdict, now: u32) {
        self.advance(now);
        self.rest = verdict.rest();
        if self.act.is_empty() {
            self.play(verdict.reaction(), now);
        } else {
            self.act.extend_from_slice(verdict.reaction());
        }
    }

    /// Any key: whatever is playing snaps to the resting face, and the next move waits a
    /// whole gap from now, so nothing moves while someone types.
    pub fn typed(&mut self, now: u32) {
        self.act.clear();
        self.quiet_until = now.saturating_add(self.gap());
    }

    /// Brings the state up to `now`: a sequence that is over ends and a new gap starts,
    /// and a gap that is over starts the next move where the gap ended.
    pub fn advance(&mut self, now: u32) {
        loop {
            if self.act.is_empty() {
                if now < self.quiet_until {
                    return;
                }
                let steps = move_for(self.dice.below(100)).steps;
                self.play(steps, self.quiet_until);
            } else {
                let end = self.act_from.saturating_add(length(&self.act));
                if now < end {
                    return;
                }
                self.act.clear();
                self.quiet_until = end.saturating_add(self.gap());
            }
        }
    }

    /// The frame to show at `now`, once `advance` has brought the state there.
    pub fn face(&self, now: u32) -> &'static str {
        match frame_in(&self.act, now.saturating_sub(self.act_from)) {
            None | Some(REST) => self.rest,
            Some(frame) => frame,
        }
    }

    /// The tick at which the frame can next change, so the session sleeps until then.
    pub fn next_change(&self, now: u32) -> u32 {
        if self.act.is_empty() {
            return self.quiet_until;
        }
        let into = now.saturating_sub(self.act_from);
        let mut end = 0;
        for &(_, ticks) in &self.act {
            end += ticks;
            if into < end {
                break;
            }
        }
        self.act_from.saturating_add(end)
    }

    fn play(&mut self, steps: &[Step], from: u32) {
        self.act = steps.to_vec();
        self.act_from = from;
    }

    fn gap(&mut self) -> u32 {
        GAP.start() + self.dice.below(GAP.end() - GAP.start() + 1)
    }
}

/// Two lines read left to right, then a look up and around.
const READ: [Step; 11] = [
    ("look-down-left", 9),
    ("look-down", 5),
    ("look-down-right", 11),
    ("look-down-left", 9),
    ("look-down", 5),
    ("look-down-right", 11),
    ("idle", 10),
    ("glance-left", 8),
    ("idle", 3),
    ("glance-right", 8),
    ("idle", 5),
];
/// `READ` with a blink inside the look up, played every third cycle.
const READ_BLINK: [Step; 13] = [
    ("look-down-left", 9),
    ("look-down", 5),
    ("look-down-right", 11),
    ("look-down-left", 9),
    ("look-down", 5),
    ("look-down-right", 11),
    ("idle", 4),
    ("blink", 3),
    ("idle", 3),
    ("glance-left", 8),
    ("idle", 3),
    ("glance-right", 8),
    ("idle", 5),
];
/// Cycles before the reading loop repeats exactly: two plain ones and one that blinks.
const READ_CYCLES: u32 = 3;

/// The frame `tick` ticks into `steps`, or `None` once they are over.
fn frame_in(steps: &[Step], tick: u32) -> Option<&'static str> {
    let mut end = 0;
    steps.iter().find_map(|&(frame, ticks)| {
        end += ticks;
        (tick < end).then_some(frame)
    })
}

fn length(steps: &[Step]) -> u32 {
    steps.iter().map(|&(_, ticks)| ticks).sum()
}

/// Whole ticks in `elapsed`.
fn ticks(elapsed: Duration) -> u32 {
    u32::try_from(elapsed.as_millis() / FRAME.as_millis()).unwrap_or(u32::MAX)
}

pub struct Owl {
    sprite: Sprite,
    stage: Stage,
}

impl Owl {
    pub fn new(sprite: Sprite) -> Self {
        Self {
            sprite,
            stage: Stage::new(Dice::from_clock()),
        }
    }

    /// Wakes up the owl the header shows live. `motion` is the session's clock here and
    /// in every method below.
    pub fn enter(&mut self, motion: &Motion) {
        self.stage.enter(ticks(motion.elapsed()));
    }

    pub fn react(&mut self, verdict: Verdict, motion: &Motion) {
        self.stage.react(verdict, ticks(motion.elapsed()));
    }

    pub fn typed(&mut self, motion: &Motion) {
        self.stage.typed(ticks(motion.elapsed()));
    }

    /// The owl in the live header, at the frame it has reached.
    pub fn live(&mut self, motion: &Motion, palette: Palette, signal: Ink) -> Vec<Line<'static>> {
        if motion.reduced() {
            return self.still(palette, signal);
        }
        let now = ticks(motion.elapsed());
        self.stage.advance(now);
        self.posed(self.stage.face(now), palette, signal)
    }

    /// How long until the live owl's frame can change. `None` with motion off, when it
    /// never does.
    pub fn next_change(&self, motion: &Motion) -> Option<Duration> {
        if motion.reduced() {
            return None;
        }
        let elapsed = motion.elapsed();
        let due = FRAME * self.stage.next_change(ticks(elapsed));
        Some(due.saturating_sub(elapsed))
    }

    /// The reading frame `elapsed` after the job started. A pure function of time, so
    /// the loop needs no state and always starts at the beginning of a line.
    fn frame_at(elapsed: Duration) -> &'static str {
        let tick = ticks(elapsed);
        let cycle = length(&READ);
        let table: &[Step] = if (tick / cycle) % READ_CYCLES == READ_CYCLES - 1 {
            &READ_BLINK
        } else {
            &READ
        };
        frame_in(table, tick % cycle).unwrap_or("idle")
    }

    /// The owl as it looks while a job runs, reading. `motion` is the job's own clock,
    /// so the loop starts when the job does.
    pub fn glancing(&self, motion: &Motion, palette: Palette, signal: Ink) -> Vec<Line<'static>> {
        let name = if motion.reduced() {
            "idle"
        } else {
            Self::frame_at(motion.elapsed())
        };
        self.posed(name, palette, signal)
    }

    /// The busy owl reacting to a verdict `elapsed` after the reaction began, on its last
    /// frame once the reaction is over. It is the reaction the header owl plays.
    pub fn reacting(
        &self,
        verdict: Verdict,
        elapsed: Duration,
        palette: Palette,
        signal: Ink,
    ) -> Vec<Line<'static>> {
        let steps = verdict.reaction();
        let name = frame_in(steps, ticks(elapsed))
            .or_else(|| steps.last().map(|&(frame, _)| frame))
            .unwrap_or("idle");
        self.posed(name, palette, signal)
    }

    /// The still owl a header carries, which cannot move because scrollback cannot.
    pub fn still(&self, palette: Palette, signal: Ink) -> Vec<Line<'static>> {
        self.posed("idle", palette, signal)
    }

    fn posed(&self, name: &str, palette: Palette, signal: Ink) -> Vec<Line<'static>> {
        self.sprite
            .render_frame(palette, self.sprite.frame(name), signal, usize::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brand::ColorDepth;

    fn owl() -> Owl {
        Owl::new(Sprite::parse(include_str!(
            "../../assets/mascot-compact.txt"
        )))
    }

    fn reading(tick: u32) -> &'static str {
        Owl::frame_at(FRAME * tick)
    }

    #[test]
    fn a_reading_cycle_is_eighty_four_ticks_with_or_without_the_blink() {
        assert_eq!(length(&READ), 84);
        assert_eq!(length(&READ_BLINK), 84);
    }

    #[test]
    fn a_working_owl_reads_two_lines_then_looks_up_and_around() {
        assert_eq!(reading(0), "look-down-left", "the loop starts on a line");
        assert_eq!(reading(9), "look-down");
        assert_eq!(reading(14), "look-down-right");
        assert_eq!(
            reading(25),
            "look-down-left",
            "the jump back to the next line"
        );
        assert_eq!(reading(50), "idle", "the look up");
        assert_eq!(reading(60), "glance-left");
        assert_eq!(reading(71), "glance-right");
        assert_eq!(reading(83), "idle");
        assert_eq!(reading(84), "look-down-left", "the second cycle");
    }

    #[test]
    fn every_third_cycle_blinks_during_the_look_up() {
        let blinks = |cycle: u32| (0..84).any(|tick| reading(cycle * 84 + tick) == "blink");
        assert_eq!(
            [0, 1, 2, 3, 4, 5].map(blinks),
            [false, false, true, false, false, true]
        );
        assert_eq!(reading(2 * 84 + 54), "blink");
        assert_eq!(reading(2 * 84 + 57), "idle");
        assert_eq!(
            reading(3 * 84),
            "look-down-left",
            "the loop repeats after three cycles"
        );
    }

    /// The frames a stage shows from `from` to `to`, advanced one tick at a time as the
    /// session's clock would.
    fn played(stage: &mut Stage, from: u32, to: u32) -> Vec<&'static str> {
        (from..to)
            .map(|now| {
                stage.advance(now);
                stage.face(now)
            })
            .collect()
    }

    #[test]
    fn the_entrance_is_forty_two_ticks_and_wakes_the_owl_up() {
        assert_eq!(length(&ENTRANCE), 42);
        let mut stage = Stage::new(Dice::seeded(1));
        stage.enter(100);
        let frames = played(&mut stage, 100, 142);
        assert_eq!(frames[0], "sleep");
        assert_eq!(frames[10], "drowsy");
        assert_eq!(frames[16], "half");
        assert_eq!(frames[20], "idle");
        assert_eq!(frames[26], "blink");
        assert_eq!(frames[34], "twitch-right");
        assert_eq!(frames[41], "twitch-right");
        stage.advance(142);
        assert_eq!(stage.face(142), "idle", "it ends on the resting face");
    }

    #[test]
    fn after_the_entrance_the_owl_rests_for_a_whole_gap() {
        let mut stage = Stage::new(Dice::seeded(2));
        stage.enter(0);
        stage.advance(42);
        let due = stage.next_change(42);
        assert!(GAP.contains(&(due - 42)), "{due}");
        assert!(
            played(&mut stage, 42, due)
                .iter()
                .all(|face| *face == "idle")
        );
        stage.advance(due);
        assert_ne!(
            stage.next_change(due),
            due,
            "a move starts when the gap ends"
        );
    }

    #[test]
    fn every_gap_is_four_to_ten_seconds() {
        let mut stage = Stage::new(Dice::seeded(3));
        let gaps: Vec<u32> = (0..2000).map(|_| stage.gap()).collect();
        assert!(gaps.iter().all(|gap| GAP.contains(gap)));
        assert!(gaps.iter().any(|gap| *gap < 130) && gaps.iter().any(|gap| *gap > 295));
        assert_eq!(FRAME * *GAP.start(), Duration::from_millis(3993));
        assert_eq!(FRAME * *GAP.end(), Duration::from_millis(9999));
    }

    #[test]
    fn the_moves_are_picked_by_weight() {
        assert_eq!(MOVES.iter().map(|each| each.weight).sum::<u32>(), 100);
        let first = |roll| move_for(roll).steps[0].0;
        assert_eq!(first(0), "blink");
        assert_eq!(move_for(39).steps.len(), 1, "a single blink");
        assert_eq!(move_for(40).steps.len(), 3, "the double blink");
        assert_eq!(first(50), "half", "the slow blink");
        assert_eq!(first(60), "glance-left");
        assert_eq!(first(68), "glance-right");
        assert_eq!(first(76), "look-down");
        assert_eq!(first(84), "twitch-left");
        assert_eq!(first(88), "twitch-right");
        assert_eq!(first(92), "breathe");
        assert_eq!(first(99), "breathe");

        let mut dice = Dice::seeded(4);
        let blinks = (0..10_000)
            .filter(|_| move_for(dice.below(100)).steps == MOVES[0].steps)
            .count();
        assert!((3700..4300).contains(&blinks), "{blinks}");
    }

    #[test]
    fn a_seeded_stage_plays_the_same_moves_every_time() {
        let run = || played(&mut Stage::new(Dice::seeded(5)), 0, 3000);
        let frames = run();
        assert_eq!(frames, run());
        assert!(frames.iter().any(|face| *face != "idle"), "nothing moved");
    }

    #[test]
    fn a_move_starts_and_ends_on_the_resting_face() {
        let mut stage = Stage::new(Dice::seeded(6));
        let frames = played(&mut stage, 0, 5000);
        assert_eq!(frames[0], "idle");
        let moves = frames
            .windows(2)
            .filter(|pair| pair[0] == "idle" && pair[1] != "idle")
            .count();
        assert!(moves >= 5000 / 303, "{moves} moves in 5000 ticks");
    }

    #[test]
    fn a_keystroke_stops_a_move_and_restarts_the_wait() {
        let mut stage = Stage::new(Dice::seeded(7));
        let start = stage.next_change(0);
        stage.advance(start);
        assert!(!stage.act.is_empty(), "a move is playing");
        stage.typed(start + 1);
        stage.advance(start + 1);
        assert_eq!(stage.face(start + 1), "idle");
        let next = stage.next_change(start + 1);
        assert!(next - (start + 1) >= *GAP.start(), "the wait restarted");

        let mut stage = Stage::new(Dice::seeded(8));
        stage.enter(0);
        stage.typed(5);
        stage.advance(5);
        assert_eq!(stage.face(5), "idle", "a keystroke skips the entrance");
    }

    #[test]
    fn typing_keeps_the_owl_still() {
        let mut stage = Stage::new(Dice::seeded(9));
        let faces: Vec<&str> = (0..3000)
            .map(|now| {
                if now % 30 == 0 {
                    stage.typed(now);
                }
                stage.advance(now);
                stage.face(now)
            })
            .collect();
        assert!(faces.iter().all(|face| *face == "idle"));
    }

    #[test]
    fn the_verdict_follows_the_plumage_bands() {
        assert_eq!(Verdict::of(None), Verdict::Good, "no score yet");
        assert_eq!(Verdict::of(Some(100)), Verdict::Good);
        assert_eq!(Verdict::of(Some(80)), Verdict::Good);
        assert_eq!(Verdict::of(Some(79)), Verdict::Fair);
        assert_eq!(Verdict::of(Some(50)), Verdict::Fair);
        assert_eq!(Verdict::of(Some(49)), Verdict::Poor);
        assert_eq!(Verdict::of(Some(0)), Verdict::Poor);
        for health in 0..=100 {
            let plumage = brand::mascot_ink(Some(health));
            let wanted = match Verdict::of(Some(health)) {
                Verdict::Good => Ink::Accent,
                Verdict::Fair => Ink::Warn,
                Verdict::Poor => Ink::Danger,
            };
            assert_eq!(plumage, wanted, "health {health}");
        }
    }

    #[test]
    fn every_reaction_is_twenty_four_ticks_and_ends_on_the_verdicts_face() {
        for (verdict, first, rest) in [
            (Verdict::Good, "blink", "content"),
            (Verdict::Fair, "look-down", "idle"),
            (Verdict::Poor, "alarm", "worried"),
        ] {
            assert_eq!(length(verdict.reaction()), 24, "{verdict:?}");
            let mut stage = Stage::new(Dice::seeded(10));
            stage.react(verdict, 0);
            let frames = played(&mut stage, 0, 25);
            assert_eq!(frames[0], first, "{verdict:?}");
            assert_eq!(frames[24], rest, "{verdict:?}");
        }
    }

    #[test]
    fn the_busy_owls_reaction_lasts_as_long_as_the_header_owls() {
        for verdict in [Verdict::Good, Verdict::Fair, Verdict::Poor] {
            assert_eq!(verdict.reaction_time(), FRAME * 24, "{verdict:?}");
        }
    }

    #[test]
    fn the_busy_owl_reacts_with_the_verdicts_frames_and_holds_the_last_one() {
        let owl = owl();
        let palette = Palette::with_depth(ColorDepth::Monochrome);
        let at = |verdict, tick: u32| owl.reacting(verdict, FRAME * tick, palette, Ink::Accent);
        let posed = |name| owl.posed(name, palette, Ink::Accent);
        assert_eq!(at(Verdict::Good, 0), posed("blink"));
        assert_eq!(at(Verdict::Good, 3), posed("happy"));
        assert_eq!(at(Verdict::Fair, 8), posed("half"));
        assert_eq!(at(Verdict::Poor, 4), posed("alarm"));
        assert_eq!(at(Verdict::Poor, 5), posed("worried"));
        assert_eq!(at(Verdict::Poor, 500), posed("worried"), "held once over");
    }

    #[test]
    fn each_verdict_reacts_with_its_own_faces() {
        let at = |verdict, tick| {
            let mut stage = Stage::new(Dice::seeded(11));
            stage.react(verdict, 0);
            stage.advance(tick);
            stage.face(tick)
        };
        assert_eq!(at(Verdict::Good, 3), "happy");
        assert_eq!(at(Verdict::Good, 23), "happy");
        assert_eq!(at(Verdict::Fair, 8), "half");
        assert_eq!(at(Verdict::Fair, 18), "blink");
        assert_eq!(at(Verdict::Fair, 21), "idle");
        assert_eq!(at(Verdict::Poor, 4), "alarm");
        assert_eq!(at(Verdict::Poor, 5), "worried");
        assert_eq!(at(Verdict::Poor, 12), "blink");
        assert_eq!(at(Verdict::Poor, 15), "worried");
    }

    #[test]
    fn a_reaction_after_the_entrance_waits_for_it_to_end() {
        let mut stage = Stage::new(Dice::seeded(12));
        stage.enter(0);
        stage.react(Verdict::Poor, 0);
        let frames = played(&mut stage, 0, 70);
        assert_eq!(frames[0], "sleep");
        assert_eq!(frames[41], "twitch-right", "the entrance plays whole");
        assert_eq!(frames[42], "alarm", "then the reaction");
        assert_eq!(frames[66], "worried", "then the resting face");
    }

    #[test]
    fn a_keystroke_skips_the_reaction_to_the_verdicts_face() {
        let mut stage = Stage::new(Dice::seeded(13));
        stage.react(Verdict::Good, 0);
        stage.typed(2);
        stage.advance(2);
        assert_eq!(stage.face(2), "content");
    }

    #[test]
    fn an_idle_move_returns_to_the_verdicts_face() {
        let mut stage = Stage::new(Dice::seeded(14));
        stage.react(Verdict::Poor, 0);
        let frames = played(&mut stage, 0, 4000);
        assert!(frames[24..].contains(&"worried"));
        assert!(
            !frames[24..].contains(&"idle"),
            "a resting row in a move shows the verdict's face"
        );
    }

    #[test]
    fn with_motion_off_the_live_owl_is_the_still_frame_and_never_wakes_the_loop() {
        let palette = Palette::with_depth(ColorDepth::Truecolor);
        let mut owl = owl();
        let motion = Motion::new(true);
        owl.enter(&motion);
        owl.react(Verdict::Poor, &motion);
        assert_eq!(
            owl.live(&motion, palette, Ink::Warn),
            owl.still(palette, Ink::Warn)
        );
        assert_eq!(owl.next_change(&motion), None);
    }

    #[test]
    fn with_motion_off_the_owl_is_the_still_frame() {
        let palette = Palette::with_depth(ColorDepth::Truecolor);
        let owl = owl();
        assert_eq!(
            owl.glancing(&Motion::new(true), palette, Ink::Good),
            owl.still(palette, Ink::Good)
        );
    }

    #[test]
    fn the_plumage_is_whatever_colour_the_caller_names() {
        let palette = Palette::with_depth(ColorDepth::Truecolor);
        let owl = owl();
        assert_ne!(
            owl.still(palette, Ink::Good),
            owl.still(palette, Ink::Danger)
        );
    }

    /// Every frame the session plays, in the order the asset holds them. `idle` is first
    /// because an unknown name falls back to frame 0.
    const FRAMES: [&str; 17] = [
        "idle",
        "blink",
        "glance-left",
        "glance-right",
        "sleep",
        "drowsy",
        "half",
        "look-down",
        "look-down-left",
        "look-down-right",
        "twitch-left",
        "twitch-right",
        "breathe",
        "happy",
        "content",
        "worried",
        "alarm",
    ];

    #[test]
    fn the_compact_owl_is_eight_by_six_pixels_in_every_frame() {
        let sprite = Sprite::parse(include_str!("../../assets/mascot-compact.txt"));
        assert_eq!(sprite.width(), 8);
        assert_eq!(sprite.cell_height(), 3);
        assert_eq!(sprite.frames(), FRAMES.len());
        for (at, name) in FRAMES.iter().enumerate() {
            assert_eq!(sprite.frame(name), at, "{name} is not where it should be");
            let pixels = sprite.pixels(at);
            assert_eq!(pixels.len(), 6, "{name} is not six pixels tall");
            assert!(
                pixels.iter().all(|row| row.len() == 8),
                "{name} is not eight pixels wide"
            );
        }
    }
}
