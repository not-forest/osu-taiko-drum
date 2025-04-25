//! Module that defines HID reports, required for sending drum hits.

pub(crate) use usbd_hid::descriptor::{generator_prelude::*, *};

pub(crate) const USB_HID_CLASS_POLLING_MS: u8 = 60;

/// Drum Stroke HID Class Report.
///
/// Acts as a keyboard device that sends corresponding keycodes mapped to the corresponding hitstrokes
/// obtained from the four drum sensors. Allows to play from the Taiko Drum just like from regular
/// keyboard.
#[gen_hid_descriptor(
    (collection = APPLICATION, usage_page = GENERIC_DESKTOP, usage = KEYBOARD) = {
        (usage_page = KEYBOARD, usage_min = 0xE0, usage_max = 0xE7) = {
            #[packed_bits 8] #[item_settings data,variable,absolute] _modifier=input;
        };
        (usage_min = 0x00, usage_max = 0xFF) = {
            #[item_settings constant,variable,absolute] _reserved=input;
        };
        (usage_page = LEDS, usage_min = 0x01, usage_max = 0x05) = {
            #[packed_bits 5] #[item_settings data,variable,absolute] _leds=output;
        };
        (usage_page = KEYBOARD, usage_min = 0x00, usage_max = 0xDD) = {
            #[item_settings data,array,absolute] keycode=input;
        };
    }
)]
#[allow(dead_code)]
#[derive(Default)]
pub(crate) struct DrumHitStrokeHidReport {
    _modifier: u8,
    _reserved: u8,
    _leds: u8,
    keycode: [u8; 6],
}

impl DrumHitStrokeHidReport {
    /// Generates new keystroke HID report from the provided pressed keys.
    ///
    /// # Iterator
    ///
    /// Input iterator must be an iterator with maximum capacity of 6 elements. More elements will
    /// be ignored.
    pub(crate) fn new<I>(keys: I) -> Self where
        I: IntoIterator<Item = KeyboardUsage>,
    {
        let mut iter = keys.into_iter().take(6).map(|k| k as u8);
        Self {
            keycode: core::array::from_fn(|_| iter.next().unwrap_or(0)),
            ..Default::default()
        }
    }

    /// Constructs an empty HID report.
    ///
    /// Can be used to fully reset the state of HID device (release all keys).
    pub(crate) fn empty() -> Self {
        Self { ..Default::default() }
    }
}
