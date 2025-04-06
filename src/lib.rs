//! Library space for Taiko Drum Firmware.
#![no_std]
#![no_main]

pub mod log;

pub const TAIKO_HID_FIRMWARE_VERSION: &'static str = env!("CARGO_PKG_VERSION");
