use std::time::{Duration, Instant};

/// Frames per second while something is moving. The loop blocks on input when
/// nothing is, so this is a ceiling on a busy screen rather than a steady wakeup.
pub const FRAME: Duration = Duration::from_millis(33);
/// What the mascot alone needs. A blink is the only thing moving on most screens, but it
/// has to be shorter than a blink looks to be, so the wakeups have to outpace it.
pub const IDLE: Duration = Duration::from_millis(200);

const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// A clock, not an engine. Screens ask it how far through something they are and
/// draw accordingly; nothing here owns state about what is being animated.
#[derive(Copy, Clone)]
pub struct Motion {
    started: Instant,
    reduced: bool,
}

impl Motion {
    pub fn new(reduced: bool) -> Self {
        Self {
            started: Instant::now(),
            reduced,
        }
    }

    pub const fn reduced(&self) -> bool {
        self.reduced
    }

    /// Cubic ease-out over `over`, clamped to `0..=1`. With motion off everything is
    /// already finished, so a screen written against this draws its end state.
    pub fn eased(&self, since: Instant, over: Duration) -> f32 {
        if self.reduced || over.is_zero() {
            return 1.0;
        }
        let progress = (since.elapsed().as_secs_f32() / over.as_secs_f32()).clamp(0.0, 1.0);
        1.0 - (1.0 - progress).powi(3)
    }

    pub fn finished(&self, since: Instant, over: Duration) -> bool {
        self.reduced || since.elapsed() >= over
    }

    /// A 0..1 sawtooth, for anything that repeats rather than arrives.
    pub fn phase(&self, period: Duration) -> f32 {
        if self.reduced || period.is_zero() {
            return 0.0;
        }
        let elapsed = self.started.elapsed().as_secs_f32();
        (elapsed % period.as_secs_f32()) / period.as_secs_f32()
    }

    pub fn spinner(&self) -> char {
        if self.reduced {
            return '·';
        }
        let step = (self.started.elapsed().as_millis() / 80) as usize;
        SPINNER[step % SPINNER.len()]
    }

    /// Counts from zero to `value`, so a score is watched arriving rather than
    /// appearing — which is what makes the band it lands in register.
    pub fn count_to(&self, value: u32, since: Instant, over: Duration) -> u32 {
        (f64::from(value) * f64::from(self.eased(since, over))).round() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduced_motion_starts_at_the_end_state() {
        let motion = Motion::new(true);
        let now = Instant::now();
        assert!((motion.eased(now, Duration::from_secs(10)) - 1.0).abs() < f32::EPSILON);
        assert_eq!(motion.count_to(73, now, Duration::from_secs(10)), 73);
        assert!(motion.finished(now, Duration::from_secs(10)));
    }

    #[test]
    fn easing_is_clamped_at_both_ends() {
        let motion = Motion::new(false);
        let past = Instant::now() - Duration::from_secs(5);
        assert!((motion.eased(past, Duration::from_millis(100)) - 1.0).abs() < f32::EPSILON);
        assert!(motion.eased(Instant::now(), Duration::from_secs(60)) < 0.01);
    }

    #[test]
    fn a_zero_duration_never_divides_by_zero() {
        let motion = Motion::new(false);
        assert!((motion.eased(Instant::now(), Duration::ZERO) - 1.0).abs() < f32::EPSILON);
        assert!((motion.phase(Duration::ZERO) - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn the_spinner_stops_turning_when_motion_is_off() {
        assert_eq!(Motion::new(true).spinner(), '·');
        assert!(SPINNER.contains(&Motion::new(false).spinner()));
    }
}
