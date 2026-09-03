use std::time::{Duration, Instant};

use super::sprite::Sprite;

macro_rules! frames {
    ($(($file:literal, $ms:literal)),+ $(,)?) => {
        vec![$((Sprite::parse(include_str!(concat!("../../assets/rex/", $file, ".txt"))),
                Duration::from_millis($ms))),+]
    };
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Mood {
    Idle,
    Walking,
    Chomping,
    Roaring,
    Sleeping,
}

impl Mood {
    const fn is_one_shot(self) -> bool {
        matches!(self, Self::Chomping | Self::Roaring)
    }
}

/// Frames carry their own duration because a natural blink is a long open eye and
/// a short shut one; a uniform frame rate makes the mascot look like a metronome.
struct Reel {
    frames: Vec<(Sprite, Duration)>,
    total: Duration,
}

impl Reel {
    fn new(frames: Vec<(Sprite, Duration)>) -> Self {
        let total = frames.iter().map(|(_, duration)| *duration).sum();
        Self { frames, total }
    }

    fn index_at(&self, elapsed: Duration) -> usize {
        let mut cursor = if self.total.is_zero() {
            Duration::ZERO
        } else {
            Duration::from_nanos((elapsed.as_nanos() % self.total.as_nanos()) as u64)
        };
        for (index, (_, duration)) in self.frames.iter().enumerate() {
            if cursor < *duration {
                return index;
            }
            cursor -= *duration;
        }
        self.frames.len() - 1
    }

    fn remaining_in_frame(&self, elapsed: Duration) -> Duration {
        let mut cursor = if self.total.is_zero() {
            Duration::ZERO
        } else {
            Duration::from_nanos((elapsed.as_nanos() % self.total.as_nanos()) as u64)
        };
        for (_, duration) in &self.frames {
            if cursor < *duration {
                return *duration - cursor;
            }
            cursor -= *duration;
        }
        Duration::from_millis(50)
    }
}

pub struct Mascot {
    idle: Reel,
    walking: Reel,
    chomping: Reel,
    roaring: Reel,
    sleeping: Reel,
    mood: Mood,
    resting_mood: Mood,
    since: Instant,
    shown_frame: usize,
}

impl Mascot {
    pub fn new() -> Self {
        Self {
            idle: Reel::new(frames![
                ("idle_0", 2600),
                ("idle_1", 130),
                ("idle_0", 200),
                ("idle_1", 130),
            ]),
            walking: Reel::new(frames![
                ("walk_0", 110),
                ("walk_1", 110),
                ("walk_2", 110),
                ("walk_3", 110),
            ]),
            chomping: Reel::new(frames![("chomp_0", 70), ("chomp_1", 130), ("chomp_0", 70)]),
            roaring: Reel::new(frames![
                ("roar_0", 140),
                ("roar_1", 420),
                ("roar_0", 140),
                ("idle_0", 120),
            ]),
            sleeping: Reel::new(frames![("sleep_0", 1600), ("sleep_1", 1600)]),
            mood: Mood::Idle,
            resting_mood: Mood::Idle,
            since: Instant::now(),
            shown_frame: usize::MAX,
        }
    }

    fn reel(&self) -> &Reel {
        match self.mood {
            Mood::Idle => &self.idle,
            Mood::Walking => &self.walking,
            Mood::Chomping => &self.chomping,
            Mood::Roaring => &self.roaring,
            Mood::Sleeping => &self.sleeping,
        }
    }

    pub const fn mood(&self) -> Mood {
        self.mood
    }

    pub fn set_mood(&mut self, mood: Mood) {
        if self.mood == mood {
            return;
        }
        if !mood.is_one_shot() {
            self.resting_mood = mood;
        }
        self.mood = mood;
        self.since = Instant::now();
        self.shown_frame = usize::MAX;
    }

    pub fn width(&self) -> u16 {
        self.reel().frames[0].0.width()
    }

    pub fn sprite(&self) -> &Sprite {
        let reel = self.reel();
        &reel.frames[reel.index_at(self.since.elapsed())].0
    }

    /// The caller redraws on this, so it must stay false while the visible frame is
    /// unchanged or the screen repaints on every poll for nothing.
    pub fn tick(&mut self) -> bool {
        let elapsed = self.since.elapsed();
        if self.mood.is_one_shot() && elapsed >= self.reel().total {
            self.mood = self.resting_mood;
            self.since = Instant::now();
            self.shown_frame = usize::MAX;
            return true;
        }
        let index = self.reel().index_at(elapsed);
        if index == self.shown_frame {
            return false;
        }
        self.shown_frame = index;
        true
    }

    /// The event loop sleeps exactly until the next frame is due, so a mascot
    /// holding a 2.6-second open-eyed frame costs one wake-up, not sixty.
    pub fn time_to_next_frame(&self) -> Duration {
        self.reel().remaining_in_frame(self.since.elapsed())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_advance_in_order_across_the_cycle() {
        let reel = Reel::new(vec![
            (Sprite::parse("T"), Duration::from_millis(100)),
            (Sprite::parse("B"), Duration::from_millis(100)),
        ]);
        assert_eq!(reel.index_at(Duration::from_millis(0)), 0);
        assert_eq!(reel.index_at(Duration::from_millis(99)), 0);
        assert_eq!(reel.index_at(Duration::from_millis(100)), 1);
        assert_eq!(reel.index_at(Duration::from_millis(199)), 1);
        assert_eq!(reel.index_at(Duration::from_millis(200)), 0, "the cycle repeats");
    }

    #[test]
    fn the_sleep_until_next_frame_never_overshoots_it() {
        let reel = Reel::new(vec![
            (Sprite::parse("T"), Duration::from_millis(100)),
            (Sprite::parse("B"), Duration::from_millis(300)),
        ]);
        assert_eq!(reel.remaining_in_frame(Duration::from_millis(0)), Duration::from_millis(100));
        assert_eq!(reel.remaining_in_frame(Duration::from_millis(60)), Duration::from_millis(40));
        assert_eq!(reel.remaining_in_frame(Duration::from_millis(100)), Duration::from_millis(300));
    }

    #[test]
    fn a_one_shot_mood_returns_to_what_it_interrupted() {
        let mut mascot = Mascot::new();
        mascot.set_mood(Mood::Walking);
        mascot.set_mood(Mood::Chomping);
        assert_eq!(mascot.mood(), Mood::Chomping);

        mascot.since = Instant::now() - Duration::from_secs(5);
        assert!(mascot.tick(), "finishing a one-shot is a visible change");
        assert_eq!(mascot.mood(), Mood::Walking, "chomping should hand back to walking");
    }

    #[test]
    fn every_reel_has_frames_and_a_nonzero_cycle() {
        let mascot = Mascot::new();
        for reel in
            [&mascot.idle, &mascot.walking, &mascot.chomping, &mascot.roaring, &mascot.sleeping]
        {
            assert!(!reel.frames.is_empty());
            assert!(!reel.total.is_zero());
        }
    }

    #[test]
    fn all_frames_of_a_reel_share_one_size() {
        let mascot = Mascot::new();
        for reel in
            [&mascot.idle, &mascot.walking, &mascot.chomping, &mascot.roaring, &mascot.sleeping]
        {
            let (width, height) = (reel.frames[0].0.width(), reel.frames[0].0.cell_height());
            for (sprite, _) in &reel.frames {
                assert_eq!((sprite.width(), sprite.cell_height()), (width, height));
            }
        }
    }
}
