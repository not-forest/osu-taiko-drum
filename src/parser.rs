//! Logical structure that performs complete analysis on the upcoming samples from the
//! piezoelectric sensors and pushes further information about true and spurious hits.

use crate::{
    cfg::{DrumConfig, HitMapping}, 
    hid::DrumHitStrokeHidReport, 
    piezo::PiezoSample,
};

const MID_RANGE: u16 = 4096 / 2;

#[derive(Debug)]
pub struct Parser { 
    /// Window counter. Will reset after reaching [`SHARPNESS`]
    window_cnt: u16,
    /// Buffered energy for each individual channel.
    energy: [u32; 4],

    /// History of last energy values in previous [`SHARPNESS`] windows with an index value. 
    histogram: EnergyHistogram<8>,

    /// Four booleans representing the current state of four hit spots.
    hits: [bool; 4],
    /// Becomes true when the state of at least one sensor is changed.
    state_change: bool,
}

impl Default for Parser {
    fn default() -> Self {
        Self {
            window_cnt: 0,
            energy: [0u32; 4],
            hits: [false; 4],
            state_change: true,
            // MAX values are used to not spam keystrokes during startup.
            histogram: EnergyHistogram::new(u32::MAX),
        }
    }
}

impl Parser {
    /// Parses upcoming samples and returns a boolean according to the curreent change of state.
    pub(crate) fn parse(
        &mut self, 
        cfg: &DrumConfig, 
        sample: PiezoSample
    ) -> Option<DrumHitStrokeHidReport> {
        self.window_cnt += 1;

        // Energy buffering.
        self.energy.iter_mut().zip(sample.0)
            .for_each(|(e, s)|
                *e = e.saturating_add(
                    (s as i32).saturating_sub(MID_RANGE as i32)
                        .pow(2) as u32
                )
            );

        // Deducing which sensors was hit based on buffered energy.
        if self.window_cnt == cfg.parse_cfg.sharpness {
            let thresh = self.histogram.threshold() + cfg.parse_cfg.sensitivity;

            // Storing new energy average.
/*             self.histogram.store(self.energy.iter().max().expect("Would never be None").clone()); */

            /* self.hits.iter_mut().zip(self.energy)
                .for_each(|(b, e)| {
                    log::info!("ENERGY: {} AND THRESH: {}", e, thresh);
                    *b = e > thresh;
                    if *b { self.state_change = true }
                }); */
/*             log::info!("ENERGY: {} AND THRESH: {}", self.energy[0], thresh); */
            let new_state = self.energy[0] > thresh;
            if self.hits[0] != new_state {
                self.hits[0] = new_state;
                self.state_change = true 
            }
         
            self.histogram.store(self.energy[0]);

            self.window_cnt = 0;
            self.energy = [0u32; 4];
            
        }

        if self.state_change {
            self.state_change = false;
            return Some(self.current(cfg.hit_mapping));
        }

        None
    }

    /// Currently pressed keys mapped into a HID report.
    fn current(&self, hit_mapping: HitMapping) -> DrumHitStrokeHidReport {
        DrumHitStrokeHidReport::new(
            cortex_m::interrupt::free(|_| {
                [
                    (self.hits[0], hit_mapping.left_kat),
                    (self.hits[1], hit_mapping.left_don),
                    (self.hits[2], hit_mapping.right_don),
                    (self.hits[3], hit_mapping.right_kat),
                ]
                .into_iter()
                .filter_map(|(hit, key)| if hit { Some(key) } else { None })
            }),
        )
    }
}

/// Sorted window of last [`N`] energy values.
#[derive(Debug, Clone, Copy)]
struct EnergyHistogram<const N: usize> {
    /// This window is guaranteed to be always sorted.
    sorted: [u32; N],
    /// FIFO buffer of N last values.
    fifo: [u32; N],
    index_fifo: usize,
}

impl<const N: usize> EnergyHistogram<N> {
    /// Creates a new instance of [] with initial sorted window filled with copied argument value.
    fn new(filler: u32) -> Self {
        Self {
            sorted: [filler; N],
            fifo: [filler; N],
            index_fifo: 0,
        }
    }

    /// Stores new value into a sorted array, while also remembers it's index position.
    fn store(&mut self, new: u32) {
        let old = self.fifo[self.index_fifo];

        // Remove old value from `sorted` by finding it and shifting the rest.
        if let Ok(i) = self.sorted[..N - 1].binary_search(&old) {
            if i < N - 1 {
                self.sorted[i..].rotate_left(1);
            }
        } else {
            // This should never happen if both `fifo` and `sorted` are kept in sync.
            debug_assert!(false, "Old value not found in sorted array");
        }

        match self.sorted[..N - 1].binary_search(&new) {
            Ok(i) | Err(i) => {
                if i < N - 1 {
                    self.sorted[i..N - 1].rotate_right(1);
                    self.sorted[i] = new;
                } else {
                    self.sorted[N-1] = new;
                }
            }
        }

        self.fifo[self.index_fifo] = new;
        self.index_fifo = (self.index_fifo + 1) % N;

        debug_assert!(self.sorted.is_sorted(), "Sorted array must stay sorted during the whole life of a program.");
    }

    /// Threshold is equal to median of last N values.
    fn threshold(&self) -> u32 {
        self.sorted[N / 2]
    }
}
