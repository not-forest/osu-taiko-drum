//! Module to hold all configurations related to the taiko drum.

use super::pac::FLASH;
use usbd_hid::descriptor::KeyboardUsage;
use core::mem;
use core::ptr;

/* 
 *  Holds start and end addresses of the last kilobyte of flash, used to store drum's configuration.
 * */
unsafe extern "C" {
    static __cfg_start: u8;
    static __cfg_end: u8;
}

/// Drum configuration.
///
/// This structure represents a raw set of bytes stored in the flash memory.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct DrumConfig {
    pub left_kat: KeyboardUsage,
    pub left_don: KeyboardUsage,
    pub right_don: KeyboardUsage,
    pub right_kat: KeyboardUsage,
}

const CFG_START: *const u8 = unsafe { &__cfg_start as *const u8 };
const CFG_END: *const u8 = unsafe { &__cfg_end as *const u8 };
/// Size of configuration structure.
const CFG_SIZE: usize = mem::size_of::<DrumConfig>();
/// Ensures at runtime that the structure does not require additional padding.
const _: () = assert!(CFG_SIZE.is_power_of_two());

impl DrumConfig {
    // Represents the current structure as an array of words.
    #[inline(always)]
    fn to_bytes(&self) -> &[u16; CFG_SIZE / 2] {
        unsafe { &*(self as *const Self as *const [u16; CFG_SIZE / 2]) }
    }

    // Checking all bytes within the flash page that store our data.
    #[inline(always)]
    fn __is_erased() -> bool {
        unsafe {
            core::slice::from_ptr_range(CFG_START..CFG_END)
                .iter()
                .all(|&b| b == 0xFF)
        }
    }

    // All write flash operations must be done while the flash is not busy.
    #[inline(always)]
    fn __bsy<F>(flash: &mut FLASH, f: F) where 
        F: FnOnce(&mut FLASH)
    {
        while flash.sr.read().bsy().bit_is_set() {}
        f(flash);
        while flash.sr.read().bsy().bit_is_set() {}
    }

    // If flash is locked on reboot, it shall be unlocked via two-key sequence.
    #[inline(always)]
    fn __unlock_flash(flash: &mut FLASH) { 
        const KEY1: u32 = 0x45670123;
        const KEY2: u32 = 0xcdef89ab;

        if flash.cr.read().lock().bit_is_set() {
            log::info!("Flash is locked. Unlocking...");
            flash.keyr.write(|w| w.key().variant(KEY1));
            flash.keyr.write(|w| w.key().variant(KEY2));
        }
    }

    /// Generates a new configuration based on contents written to flash memory containing the
    /// configuration. Otherwise the default value will be used.
    #[inline(never)]
    #[unsafe(link_section = ".data")]
    pub(crate) fn new(flash: &mut FLASH) -> Self {
        // Unlocking the flash for this function.
        Self::__unlock_flash(flash);

        if Self::__is_erased() {
            log::warn!("Configuration is erased from flash. Using default values.");
            Self::default()
        } else {
            log::info!("Reading previous configuration from flash.");
            unsafe {
                // Expecting the structure to be written at the very start of the last page.
                let ptr = CFG_START as *const Self;

                ptr.as_ref()
                    .expect("Flash memory should contain valid config data.")
                    .clone()
            }
        }
    }

    /// Saves the current configuration to the flash memory region.
    #[inline(never)]
    #[unsafe(link_section = ".data")]
    pub(crate) fn save(&mut self, flash: &mut FLASH) {
        log::info!("Writing new configuration to memory.");

        // Unlocking the flash for this function.
        Self::__unlock_flash(flash);

        Self::__bsy(flash, |f| {
            f.cr.modify(|_, w| w.per().set_bit());
            f.ar.write(|w| w.far().variant(CFG_START as u32));   /* Erasing the page within the provided address. */
            f.cr.modify(|_, w| w.strt().set_bit());
        });

        if Self::__is_erased() {
            self.to_bytes()
                .into_iter()
                .enumerate()
                .for_each(|(i, &word)| unsafe {
                    Self::__unlock_flash(flash);
                    let ptr = (CFG_START as *mut u16).add(i);

                    flash.cr.modify(|_, w| w.per().clear_bit());

                    log::info!("Writing: {:X} -> {:x}", ptr as u32, word);
                    Self::__bsy(flash, |f| {
                        f.cr.modify(|_, w| w.pg().set_bit());
                        ptr::write_volatile(ptr, word);
                    });

                    assert!(ptr::read_volatile(ptr) == word);
                });
        } else {
            log::error!("Unable to erase flash memory page.");
        }
    }
}

impl Default for DrumConfig {
    fn default() -> Self {
        Self {
            left_kat: KeyboardUsage::KeyboardZz,
            left_don: KeyboardUsage::KeyboardXx,
            right_don: KeyboardUsage::KeyboardCc,
            right_kat: KeyboardUsage::KeyboardVv,
        }
    }
}
