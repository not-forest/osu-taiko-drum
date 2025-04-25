//! Logical structure that performs complete analysis on the upcoming samples from the
//! piezoelectric sensors and pushes further information about true and spurious hits.

use crate::{hid::DrumHitStrokeHidReport, piezo::PiezoSample};
use usbd_hid::descriptor::KeyboardUsage;

/* TODO! Swap all static configuration to some user-friendly configurable variables. */
const MAX_WINDOW_SIZE: usize = 8;

const LEFT_KAT_MAPPED_KEY: KeyboardUsage = KeyboardUsage::KeyboardZz;
const LEFT_DON_MAPPED_KEY: KeyboardUsage = KeyboardUsage::KeyboardXx;
const RIGHT_DON_MAPPED_KEY: KeyboardUsage = KeyboardUsage::KeyboardCc;
const RIGHT_KAT_MAPPED_KEY: KeyboardUsage = KeyboardUsage::KeyboardVv;

/// Piezoelectric sample parser.
///
/// Samples are expected to come in at 20 kHz, which allows to cover several millisecond
/// windows, based on [`WINDOW_WRAP_SIZE`].
#[derive(Debug)]
pub struct Parser {
    /// Holds values of [`MAX_WINDOW_SIZE`] last samples.
    window: [PiezoSample; MAX_WINDOW_SIZE],
    /// Dynamic treshhold value for each window slice.
    thresh: u16,
    /// Sliding window index.
    idx: usize,
    /// Current state of four sensor hits based on that order: LK, LD, RD, RK
    hits: [bool; 4],
    /// Set to true from the parse method, when current state has changed.
    changed: bool,
}

impl Default for Parser {
    fn default() -> Self {
        Self {
            window: [PiezoSample::default(); MAX_WINDOW_SIZE],
            idx: 0,
            thresh: 0,
            hits: Default::default(),
            changed: false,
        }
    }
}

impl Parser {
    /// Pushes one new sample to the parser.
    pub(crate) fn parse(&mut self, sample: PiezoSample) -> Option<DrumHitStrokeHidReport> { 
        self.window[self.idx] = sample;
        self.idx += 1;

        if self.idx == MAX_WINDOW_SIZE {
            self.detect();
            self.idx = 0;
            self.current()
        } else { None } 
    }

    /// Performs a full analysis on the current window.
    ///
    /// # Note
    ///
    /// Shall only be executes when a full window of samples is ready for flushing.
    fn detect(&mut self) {
        let mut new_hits = [false; 4];

        const THRESHOLD_DX: u16 = 500;
        const THRESHOLD_ENERGY: u32 = 2_000_000;

        // Closure to process each channel
        let process_channel = |extract: fn(PiezoSample) -> u16| -> bool {
            let samples: [u16; MAX_WINDOW_SIZE] = self.window.map(extract);

            // Compute max derivative
            let mut max_dx = 0u16;
            for i in 1..MAX_WINDOW_SIZE {
                let dx = samples[i].saturating_sub(samples[i - 1]);
                if dx > max_dx {
                    max_dx = dx;
                }
            }

            // Mean energy
            let energy: u32 = samples.iter()
                .map(|&x| (x as u32).pow(2))
                .sum::<u32>() / (MAX_WINDOW_SIZE as u32);

            // Return hit decision
            max_dx < THRESHOLD_DX && energy > THRESHOLD_ENERGY
        };

        // Processing all 4 channels
        new_hits[0] = process_channel(|s| s.le);
        new_hits[1] = process_channel(|s| s.lc);
        new_hits[2] = process_channel(|s| s.rc);
        new_hits[3] = process_channel(|s| s.re);

        self.changed = self.hits != new_hits;
        self.hits = new_hits;
    }

    /// Reads currently detected hits on all four sensors.
    pub(crate) fn current(&self) -> Option<DrumHitStrokeHidReport> {
        if !self.changed {
            return None;
        }

        Some(
            DrumHitStrokeHidReport::new(
                cortex_m::interrupt::free(|_| 
                    [
                        (self.hits[0], LEFT_KAT_MAPPED_KEY),
                        (self.hits[1], LEFT_DON_MAPPED_KEY),
                        (self.hits[2], RIGHT_DON_MAPPED_KEY),
                        (self.hits[3], RIGHT_KAT_MAPPED_KEY),
                    ]
                        .into_iter()
                        .filter_map(|(active, key)| if active { Some(key) } else { None })
                )
            )
        )
    }
}

/// First derivative approximation.
fn dx(ni: u16, nim1: u16) -> i32 {
    ni as i32 - nim1 as i32
}
