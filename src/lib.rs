//! Library space for Taiko Drum Firmware.
#![no_std]
#![no_main]

use stm32f1::stm32f103 as pac;

/// Contains configured logger for the application.
mod logger;

#[rtic::app(
    device = stm32f1::stm32f103,
    dispatchers = [ADC1_2],
    peripherals = true,
)]
mod app {
    use rtic_monotonics::systick::prelude::*;

    /* Firmware clocks. */
    systick_monotonic!(Systick);

    #[shared]
    struct Shared {

    }
    
    #[local]
    struct Local {

    }

    /// Initialization function for drum functionality.
    ///
    /// # Init
    ///
    /// During the initialization phase, application does the following:
    /// - Initializes the logger for debug and release builds;
    #[init]
    fn Init(cx: Init::Context) -> (Shared, Local) {
        let (core, device) = (cx.core, cx.device);
        if let Err(log_set_err) = super::logger::init() {
            unimplemented!()
        }  

        log::info!("Booting taiko firmware version: [{}]", super::TAIKO_HID_FIRMWARE_VERSION);

        log::debug!("Enabling Systick monotonic...");
        Systick::start(core.SYST, super::ARM_SYSTICK_HZ);

        log::info!("Internal clocks enabled");

        (
            Shared {}, 
            Local {},
        )    
    }

    // Panic handler.
    //
    // Performs a full system reset.
    panic_custom::define_panic!(|info| {
        log::error!("System panic occured: {}", info);

/*         cortex_m::peripheral::SCB::sys_reset();  */
    });
}

/// Current firmware version triple is aligned with crate version.
pub const TAIKO_HID_FIRMWARE_VERSION: &'static str = env!("CARGO_PKG_VERSION");
pub const ARM_SYSTICK_HZ: u32 = 12_000_000;
