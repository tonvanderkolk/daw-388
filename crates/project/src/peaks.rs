//! A waveform overview: the lowest and highest sample per block, so a region draws quickly at
//! any zoom.

/// Samples per block.
pub const BLOCK: usize = 256;

/// The overview of one source.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Peaks {
    /// The source's sample rate.
    pub rate: u32,
    /// Lowest and highest sample of each block.
    pub blocks: Vec<(f32, f32)>,
}

impl Peaks {
    /// The overview of `samples` at `rate`.
    pub fn of(samples: &[f32], rate: u32) -> Self {
        let blocks = samples.chunks(BLOCK).map(|c| c.iter().fold((0.0f32, 0.0f32), |(lo, hi), s| (lo.min(*s), hi.max(*s)))).collect();
        Self { rate, blocks }
    }

    /// Lowest and highest sample between `from` and `to` seconds in the source; at least one
    /// block, so a very close zoom still shows something.
    pub fn range(&self, from: f64, to: f64) -> (f32, f32) {
        let per_second = f64::from(self.rate) / BLOCK as f64;
        let first = (from.max(0.0) * per_second).floor() as usize;
        if first >= self.blocks.len() {
            return (0.0, 0.0);
        }
        let last = ((to * per_second).ceil() as usize).max(first + 1).min(self.blocks.len());
        self.blocks[first..last].iter().fold((0.0, 0.0), |(lo, hi), (l, h)| (lo.min(*l), hi.max(*h)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_hold_the_lowest_and_highest_sample() {
        assert_eq!(Peaks::of(&[0.1; 600], 48_000).blocks.len(), 3);
        let sine: Vec<f32> = (0..1024).map(|i| 0.5 * (i as f32 * std::f32::consts::TAU / 64.0).sin()).collect();
        for (lo, hi) in Peaks::of(&sine, 48_000).blocks {
            assert!((lo + 0.5).abs() < 0.01 && (hi - 0.5).abs() < 0.01, "{lo} {hi}");
        }
    }

    #[test]
    fn a_range_covers_its_blocks_and_at_least_one() {
        let samples: Vec<f32> = (0..1024).map(|i| if i < 512 { 0.25 } else { 1.0 }).collect();
        let peaks = Peaks::of(&samples, 1024);
        assert_eq!(peaks.range(0.0, 0.49), (0.0, 0.25));
        assert_eq!(peaks.range(0.0, 1.0), (0.0, 1.0));
        assert_eq!(peaks.range(2.0, 3.0), (0.0, 0.0), "past the end");
        assert_eq!(peaks.range(0.6, 0.6), (0.0, 1.0), "one block");
    }
}
