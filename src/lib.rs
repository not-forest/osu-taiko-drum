//! Library space for Taiko Drum Firmware.
#![no_std]
#![no_main]

use stm32f1::stm32f103 as pac;

/// Contains configured logger for the application.
mod logger;
/// Piezoelectric sensors driver with a set of analysis functions.
mod piezo;

#[rtic::app(
    device = stm32f1::stm32f103,
    dispatchers = [CAN_RX1],
    peripherals = true,
)]
mod app {
    use rtic_monotonics::systick::prelude::*;
    use rtic_sync::make_channel; 
    use super::piezo::{PiezoSample, PIEZO_SENSOR_QUEUE_CAPACITY, PiezoSensorHandler, Receiver};

    /* Firmware clocks. */
    systick_monotonic!(Systick);

    #[shared]
    struct Shared {}
    
    #[local]
    struct Local {
        piezo_handler: PiezoSensorHandler,
    }

    /// Initialization function for drum functionality.
    ///
    /// # Init
    ///
    /// During the initialization phase, application does the following:
    /// - Initializes the logger for debug and release builds;
    /// - Configures monotonic timers;
    /// - Prepares ADC1 & ADC2 for reading input from four piezoelectric sensors;
    #[init]
    fn Init(ctx: Init::Context) -> (Shared, Local) {
        let (core, mut dev) = (ctx.core, ctx.device);
        let (s, r) = make_channel!(PiezoSample, PIEZO_SENSOR_QUEUE_CAPACITY);

        /* Logging initialization. */
        if let Err(log_set_err) = super::logger::init() {
            unimplemented!()
        }  
        log::info!("Booting taiko firmware version: [{}]", super::TAIKO_HID_FIRMWARE_VERSION);

        /* Monotonics. */
        log::debug!("Enabling Systick monotonic...");
        Systick::start(core.SYST, super::ARM_SYSTICK_HZ);
        log::info!("Internal clocks enabled");

        /* Tasks */ 
        UsbHidSender::spawn(r).expect("First HID task initialization.");

        unsafe {    /* Interrupts unmasking. */
            use super::pac::Interrupt;
            cortex_m::peripheral::NVIC::unmask(Interrupt::ADC1_2);
        }

        (
            Shared {}, 
            Local {
                piezo_handler: PiezoSensorHandler::new(
                    (dev.ADC1, dev.ADC2), &mut dev.GPIOA, &mut dev.RCC, dev.TIM4, s.clone()
                ),
            },
        )    
    }

    /// Handles the USB HID connection with the host machine.
    ///
    ///
    #[task()]
    async fn UsbHidSender(ctx: UsbHidSender::Context, mut r: Receiver) {
        log::info!("UsbHidSender task spawned. Awaiting on upcoming data.");

        /* Handling samples obtained from the piezoelectric sensor */
        while let Ok(sample) = r.recv().await {
            log::info!("Obtained sample value: {:#?}", sample);
        }
    }

    /// Piezoelectric sensor handling hardware task.
    ///
    /// # Binds
    ///
    /// This handler function is binded to ADC1_2 interrupt vector. 
    ///
    /// The underlying sensor handling structure is queuing next injected sample from the ADC pin
    /// to the [`super::app::UsbHidSender`] task.
    #[task(
        binds = ADC1_2,
        local = [piezo_handler],
    )]
    fn SensorHandling(ctx: SensorHandling::Context) {
        log::debug!("Updating sensors data.");
        ctx.local.piezo_handler.send();
    }

    // Panic handler.
    //
    // Performs a full system reset after a several second timeout.
    // TODO! Perform a better panic restart procedure.
    panic_custom::define_panic!(|info| {
        log::error!("System panic occured: {}", info);

        loop {}
        cortex_m::peripheral::SCB::sys_reset(); 
    });
}

/// Current firmware version triple is aligned with crate version.
const TAIKO_HID_FIRMWARE_VERSION: &'static str = env!("CARGO_PKG_VERSION");
const ARM_SYSTICK_HZ: u32 = 12_000_000;
