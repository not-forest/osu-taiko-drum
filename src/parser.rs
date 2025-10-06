//! Logical structure that performs complete analysis on the upcoming samples from the
//! piezoelectric sensors and pushes further information about true and spurious hits.

use crate::{
    cfg::{DrumConfig, HitMapping}, 
    hid::DrumHitStrokeHidReport, 
    piezo::PiezoSample,
    cross_correlation::xcorr,
};
use heapless::Vec;

const MID_RANGE: i16 = 4096 / 2;
const WINDOW_SIZE: usize = 256;

#[derive(Debug)]
pub struct Parser { 
    /// Sliding windows of samples. It's length is based on the fact that each piezo signal will
    /// likely last for around 1-2ms and 20 kHz sampling rate of ADC. Each sensor has it's own window.
    windows: [SampleWindow<i16, WINDOW_SIZE>; 4],
    /// Four booleans representing the current state of four hit spots.
    states: [bool; 4],
}

impl Default for Parser {
    fn default() -> Self {
        Self {
            states: [false; 4],
            windows: core::array::from_fn(|_| SampleWindow::new(0i16)),
        }
    }
}

impl Parser {
    /// Parses upcoming samples and returns a boolean according to the current change of state.
    pub(crate) fn parse(
        &mut self, 
        cfg: &DrumConfig, 
        sample: PiezoSample
    ) -> Option<DrumHitStrokeHidReport> {
        let (sharp, sens) = (cfg.parse_cfg.sharpness, cfg.parse_cfg.sensitivity);
        let (mut state_change, mut second_stage) = (false, false);

        self.windows.iter_mut()
            .zip(sample.0)
            .zip(&mut self.states)
            .map(|((a, b), c)| (a, b, c))
            .for_each(|(w, s, b)| {
                w.store(s as i16 - MID_RANGE);
                if w.index_fifo == 0 {
                    // If deviation is too large, calculating performing second stage signal processing.
                    if check_deviation(w.threshold(), w.min(), w.max(), sharp, sens) {
                        if *b != true {
                            *b = true;
                            second_stage = true;
                            state_change = true;
                        }
                    } else {
                        *b = false;
                        state_change = true;
                    }
                }
            });

        // Pairwise cross-correlation. Delayed hit is more likely to be sensor cross-talk
        if second_stage {
            for i in 0..4 {
                if !self.states[i] { continue }
                let reference = &self.windows[i];
                for j in (i + 1)..4 {
                    if !self.states[j] { continue }
                    let occurance = &self.windows[j];
                    let delay = xcorr(
                        &occurance.fifo, 
                        occurance.threshold(), 
                        &reference.fifo, 
                        reference.threshold()
                    );

                    log::info!("piezo{} ~ piezo{} = {}", i, j, delay);

                    match delay {
                        ..0 => self.states[i] = false,
                        0.. => self.states[j] = false,
                    }
                }
            }
        }

        if state_change {
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

/// Time window that holds N samples with helper methods.
///
/// Accumulates oncoming samples from one piezo sensor with additional sorting for obtaining the
/// median value in the whole window. This median value is used as an adaptive noise threshold,
/// which detects when piezoelectric sensor is being hit (or spurious hit).
#[derive(Debug)]
struct SampleWindow<T: Ord + Copy + core::fmt::Debug, const N: usize> {
    /// This window is guaranteed to be always sorted.
    sorted: Vec<T, N>,
    /// FIFO buffer of N last samples.
    fifo: [T; N],
    index_fifo: usize,
}

impl<T: Ord + Copy + core::fmt::Debug, const N: usize> SampleWindow<T, N> {
    /// Creates a new instance of [`SampleWindow`] with initial sorted window filled with copied argument value.
    fn new(filler: T) -> Self {
        debug_assert!(N.is_power_of_two(), "Current implementation only works for power of two N.");
        Self {
            sorted: Vec::from_array([filler; N]),
            fifo: [filler; N],
            index_fifo: 0,
        }
    }

    /// Stores new value into a both fifo array sorted vector.
    fn store(&mut self, new: T) {
        let old = self.fifo[self.index_fifo];

        // Removes old element from the array.
        if let Ok(i) = self.sorted.binary_search(&old) {
            self.sorted.remove(i); 
        } else {
            panic!("Implementation error. Both fifo array and sorted vector must be synchronized.");
        }

        // Inserts new one. Both operation shall proceed to not overflow the vector.
        let (Ok(i) | Err(i)) = self.sorted.binary_search(&new);
        self.sorted.insert(i, new)
            .expect("Implementation error. Vector shall always have place for one more element at that point.");

        self.fifo[self.index_fifo] = new;
        self.index_fifo = (self.index_fifo + 1) & (N - 1);  // This is only fine if N is a power of two. 

        assert!(self.sorted.is_sorted(), "Implementation error. Unsorted sorted vector.");
    }

    /// Returns the minimal value in the whole window.
    fn min(&self) -> T {
        *self.sorted.first()
            .expect("Shall never be empty at the point where deviation is calculated.")
    }

    /// Returns the maximal value in the whole window.
    fn max(&self) -> T {
        *self.sorted.last()
            .expect("Shall never be empty at the point where deviation is calculated.")
    }

    /// Adaptive threshold is being calculated as a median value of N samples.
    fn threshold(&self) -> T {
        *self.sorted.get(N / 2)
            .expect("Shall never be empty at the point where deviation is calculated.")
    }
}

fn relative_deviation(median: i16, value: i16, scale: u16) -> f32 {
    ( (value - median).abs() as f32 ) / scale as f32
}

fn check_deviation(median: i16, min_val: i16, max_val: i16, scale: u16, percent: u8) -> bool {
    let max_dev = relative_deviation(median, max_val, scale);
    let min_dev = relative_deviation(median, min_val, scale);

    let pc = percent as f32 / 100f32;
    max_dev > pc || min_dev > pc
}
