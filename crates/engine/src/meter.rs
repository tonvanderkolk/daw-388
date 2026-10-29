//! Energy meters: the audio thread adds up squares, the UI reads the mean and starts over.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// The energy of a signal since the UI last read it: the sum of squares (as `f64` bits) and the
/// number of samples. Sum and count are two atomics, so a read can split one block between two
/// readings; the meter's own slowness hides that.
#[derive(Debug, Default)]
pub(crate) struct Energy {
    sum: AtomicU64,
    count: AtomicU32,
}

impl Energy {
    /// Add `sum` (a sum of squares) over `count` samples.
    pub(crate) fn add(&self, sum: f64, count: u32) {
        let mut current = self.sum.load(Ordering::Relaxed);
        loop {
            let next = (f64::from_bits(current) + sum).to_bits();
            match self.sum.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
        self.count.fetch_add(count, Ordering::Relaxed);
    }

    /// The mean square since the last call (0 without samples), and start over.
    pub(crate) fn take(&self) -> f32 {
        let count = self.count.swap(0, Ordering::Relaxed);
        let sum = f64::from_bits(self.sum.swap(0, Ordering::Relaxed));
        if count == 0 { 0.0 } else { (sum / f64::from(count)) as f32 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_the_mean_square_and_starts_over() {
        let energy = Energy::default();
        assert_eq!(energy.take(), 0.0, "no samples");
        energy.add(2.0, 4);
        energy.add(1.0, 4);
        assert!((energy.take() - 0.375).abs() < 1e-6);
        assert_eq!(energy.take(), 0.0, "read once");
    }
}
