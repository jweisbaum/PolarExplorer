//! The time axis every archive reader shares.

/// A regular time axis, in UTC epoch seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeAxis {
    /// Time of index 0.
    pub first: i64,
    /// Seconds between steps.
    pub step: i64,
    /// Number of steps.
    pub len: u64,
}

impl TimeAxis {
    /// The time of step `i`.
    pub fn at(&self, i: u64) -> i64 {
        self.first + self.step * i as i64
    }

    /// The time of the last step.
    pub fn last(&self) -> i64 {
        self.at(self.len.saturating_sub(1))
    }

    /// The steps either side of `t` and the weight of the later one, or
    /// `None` if `t` is outside the axis. On a step exactly, both are it.
    pub fn bracket(&self, t: i64) -> Option<(u64, u64, f64)> {
        if self.len == 0 || t < self.first || t > self.last() {
            return None;
        }
        let offset = t - self.first;
        let i = (offset / self.step) as u64;
        let rem = offset % self.step;
        if rem == 0 {
            return Some((i, i, 0.0));
        }
        Some((i, i + 1, rem as f64 / self.step as f64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_between_steps_is_bracketed_with_its_weight() {
        let axis = TimeAxis {
            first: 0,
            step: 3600,
            len: 48,
        };
        assert_eq!(axis.bracket(0), Some((0, 0, 0.0)));
        assert_eq!(axis.bracket(7200), Some((2, 2, 0.0)));
        assert_eq!(axis.bracket(7200 + 900), Some((2, 3, 0.25)));
        assert_eq!(axis.bracket(-1), None);
        assert_eq!(axis.bracket(axis.last()), Some((47, 47, 0.0)));
        assert_eq!(axis.bracket(axis.last() + 1), None);
    }
}
