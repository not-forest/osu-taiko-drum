//! Main application entry point.

#![no_main]
#![no_std]

panic_custom::define_panic!(|info| {
    log::error!("System panic occured: {}", info);
});

#[rtic::app(
    device = stm32f1::stm32f103,
    dispatchers = [ADC1_2],
    peripherals = true,
)]
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
    fn Init(cx: Init::Context) -> (Shared, Local) {
        let (core, device) = (cx.core, cx.device);
        
        if let Err(log_set_err) = TaikoHID::log::init() {
            unimplemented!()
        } 

        log::info!("Booting taiko firmware version: [{}]", TaikoHID::TAIKO_HID_FIRMWARE_VERSION);

        (
            Shared {}, 
            Local {},
        )    
    }
}
