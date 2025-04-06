//! Main application entry point.

#![no_main]
#![no_std]

use panic_rtt_target as _;

#[rtic::app(device = stm32f1::stm32f103, peripherals = true)]
mod app {
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
    fn init(cx: init::Context) -> (Shared, Local, init::Monotonics) {
        let (core, device) = (cx.core, cx.device);
        
        if let Err(log_set_err) = TaikoHID::log::init() {
            unimplemented!()
        } 
        log::info!("Init");

        (
            Shared {}, 
            Local {}, 
            init::Monotonics()
        )    
    }

    #[idle]
    fn periodic_task(cx: periodic_task::Context) -> ! {
        log::info!("Running periodic task!");

        loop {}
    }
}
