use fixed_fft::{fft_radix2_q15, Direction};
use num_complex::Complex;

/// FFT-based Cross Correlation implementation
///
/// This function calculates the cross-correlation by using frequency domain of both signals. This
/// way cross-correlation is obtained by multiplying one signal with conjugate of another in
/// frequency domain.
///
/// Similar signals will cause cross-correlation output to provide bigger numeric values, where the
/// biggest one shall correspond to the time delay between one signal and another.
pub fn xcorr(
    signal: &[i16; 512],
    signal_median: i16,
    reference: &[i16; 512],
    reference_median: i16,
) -> isize {
    const N: usize = 512;
    let mut buf_signal = [Complex { re: 0, im: 0 }; N];
    let mut buf_reference = [Complex { re: 0, im: 0 }; N];

    for i in 0..N {
        buf_signal[i].re = signal[i] - signal_median;
        buf_signal[i].im = 0;
        buf_reference[i].re = reference[i] - reference_median;
        buf_reference[i].im = 0;
    }

    fft_radix2_q15(&mut buf_signal, Direction::ForwardScaled).unwrap();
    fft_radix2_q15(&mut buf_reference, Direction::ForwardScaled).unwrap();

    // z1 = z1 x z2*
    buf_signal.iter_mut()
        .zip(buf_reference)
        .for_each(|(z1, z2)| *z1 = *z1 * z2.conj());

    fft_radix2_q15(&mut buf_signal, Direction::Inverse).unwrap();
    buf_signal.rotate_right(N / 2);

    let (max_idx, _) = buf_signal.iter()    // Maximum shall correspond to the delay value.
        .enumerate()
        .max_by_key(|(_, z)| z.re)
        .unwrap();

    max_idx as isize - (N / 2) as isize
}
