//! Logical structure that performs complete analysis on the upcoming samples from the
//! piezoelectric sensors and pushes further information about true and spurious hits.

use core::u32;

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
    energies: [u32; 4],
    /// History of last energy values in previous [`SHARPNESS`] windows with an index value. 
    histograms: [EnergyHistogram<16>; 4],
    /// Four booleans representing the current state of four hit spots.
    states: [bool; 4],

    /// Becomes true when the state of at least one sensor is changed.
    state_change: bool,
}

impl Default for Parser {
    fn default() -> Self {
        Self {
            window_cnt: 0,
            state_change: false,
            energies: [0u32; 4],
            states: [false; 4],
            // MAX values are used to not spam keystrokes during startup.
            histograms: [EnergyHistogram::new(u32::MAX); 4],
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
        let (sha, sens) = (cfg.parse_cfg.sharpness, cfg.parse_cfg.sensitivity);

        self.window_cnt += 1;

        // Energy buffering.
        self.energies.iter_mut().zip(sample.0)
            .for_each(|(e, s)|
                *e = e.saturating_add(
                    (s as i32).saturating_sub(MID_RANGE as i32)
                        .pow(2) as u32
                )
            );

        if self.window_cnt == sha {
            // Deducing which sensors was hit based on buffered energy on each individual channel.
            log::info!("SEPARATOR::::::::::");
            self.states.iter_mut()
                .zip(self.energies)
                .zip(&mut self.histograms)
                .map(|((a, b), c)| (a, b, c))
                .for_each(|(s, e, h)| {
                    let thresh = h.threshold() + sens;
                    let new_state = e > thresh;
                    log::info!("ENERGY: {} AND THRESH: {}", e, thresh);

                    if *s != new_state {
                        *s = new_state;
                        self.state_change = true;
                    }
                    
                    h.store(e)

                });

            self.window_cnt = 0;
            self.energies = [0u32; 4]; 
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
                self.states.into_iter().zip([
                    hit_mapping.left_kat,
                    hit_mapping.left_don,
                    hit_mapping.right_don,
                    hit_mapping.right_kat,
                ])
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
        if let Ok(i) = self.sorted.binary_search(&old) {
            if i < N - 1 {
                self.sorted[i..].rotate_left(1);
            }
        } else {
            // This should never happen if both `fifo` and `sorted` are kept in sync.
            debug_assert!(false, "Old value not found in sorted array");
        }

        match self.sorted.binary_search(&new) {
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
        self.index_fifo = (self.index_fifo + 1) & (N - 1);

        debug_assert!(self.sorted.is_sorted(), "Sorted array must stay sorted during the whole life of a program.");
    }

    /// Threshold is equal to median of last N values.
    fn threshold(&self) -> u32 {
        self.sorted[N / 2]
    }
}
