//! Library space for Taiko Drum Firmware.
#![no_std]
#![no_main]

use stm32f1::stm32f103 as pac;

/// Contains configured logger for the application.
mod logger;
/// Piezoelectric sensors driver with a set of analysis functions.
mod piezo;
/// HID clas implementation for drum controller.
mod hid;

#[rtic::app(
    device = stm32f1::stm32f103,
    dispatchers = [CAN_RX1],
    peripherals = true,
)]
mod app {
    use super::piezo::{PiezoSample, PIEZO_SENSOR_QUEUE_CAPACITY, PiezoSensorHandler, Receiver};
    use super::hid::{UsbTaikoDrum, UsbBus};

    use usb_device::bus::UsbBusAllocator; 

    use rtic_monotonics::systick::prelude::*;
    use rtic_sync::make_channel;

    /* Firmware clocks. */
    systick_monotonic!(Systick);

    #[shared]
    struct Shared {}
    
    #[local]
    struct Local {
        piezo_handler: PiezoSensorHandler,
        usb_bus_alloc: UsbBusAllocator<UsbBus>,
    }

    /// Initialization function for drum functionality.
    ///
    /// # Init
    ///
    /// During the initialization phase, application does the following:
    /// - Initializes the logger for debug and release builds;
    /// - Configures monotonic timers;
    /// - Prepares ADC1 & ADC2 for reading input from four piezoelectric sensors in injected
    /// simultaneous mode;
    /// - Prepares communication channel between [`app::SensorHandling`] and [`app::UsbHidSender`] tasks.
    #[init]
    fn Init(ctx: Init::Context) -> (Shared, Local) {
        let (core, mut dev) = (ctx.core, ctx.device);
        let (s, r) = make_channel!(PiezoSample, PIEZO_SENSOR_QUEUE_CAPACITY);

        /* Logging initialization. */
        if let Err(log_set_err) = super::logger::init() {
            unimplemented!()
        }  
        log::info!("Booting taiko firmware version: [{}]", super::version::TAIKO_HID_FIRMWARE_VERSION);

        /* Monotonics. */
        log::debug!("Enabling Systick monotonic...");
        Systick::start(core.SYST, ARM_SYSTICK_HZ);
        log::info!("Internal clocks enabled");

        /* Tasks */ 
        UsbHidSender::spawn(r).expect("First HID task initialization.");

        (
            Shared {}, 
            Local {
                piezo_handler: PiezoSensorHandler::new(
                    (dev.ADC1, dev.ADC2), &mut dev.GPIOA, &mut dev.RCC, dev.TIM4, s.clone()
                ),
                usb_bus_alloc: UsbTaikoDrum::bus(dev.USB),
            },
        )    
    }

    /// Handles the USB HID connection with the host machine.
    ///
    ///
    #[task(priority = 2, local = [usb_bus_alloc])]
    async fn UsbHidSender(ctx: UsbHidSender::Context, mut r: Receiver) {
        let mut usb_dev = UsbTaikoDrum::new(ctx.local.usb_bus_alloc);
        log::info!("UsbHidSender task spawned. Awaiting on upcoming data.");

        /* Handling samples obtained from the piezoelectric sensor */
        while let Ok(sample) = r.recv().await {
/*             log::info!("Obtained sample value: {:#?}", sample); */

            usb_dev.poll(|_| {
                log::info!("Debug POLL");
            });

            super::int_enable!(ADC1_2); // TODO! do not enable on each loop.
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
    #[task(binds = ADC1_2, priority = 1, local = [piezo_handler])]
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

        // Halting on debug builds.
        #[cfg(not(debug_assertions))]
        cortex_m::peripheral::SCB::sys_reset(); 
    });

    const ARM_SYSTICK_HZ: u32 = 12_000_000;
}

#[macro_export]
macro_rules! int_enable {
    ($name:ident) => {
        unsafe {
        cortex_m::peripheral::NVIC::unmask(super::pac::Interrupt::$name); 
        }
    };
}

#[macro_export]
macro_rules! int_disable {
    ($name:ident) => {
        cortex_m::peripheral::NVIC::mask(super::pac::Interrupt::$name); 
    };
}

/// Module containing all information about current firmware version.
mod version {
    /// Current firmware version triple is aligned with crate version.
    pub(crate) const TAIKO_HID_FIRMWARE_VERSION: &'static str = env!("CARGO_PKG_VERSION");
    /// Current firmware version triple in BCD format for USB HID. 
    pub(crate) const TAIKO_HID_FIRMWARE_VERSION_BCD: u16 = __version_to_bcd(TAIKO_HID_FIRMWARE_VERSION);

    /// Converts current version number to BCD at compile time.
    const fn __version_to_bcd(version: &str) -> u16 {
        let mut major = 0;
        let mut minor = 0;
        let mut idx = 0;

        // Major
        while idx < version.len() {
            let byte = version.as_bytes()[idx];
            if byte == b'.' || byte == b'\0' {
                break;
            }
            major = major * 10 + (byte - b'0') as u16;
            idx += 1;
        }

        idx += 1;

        // Minor
        while idx < version.len() {
            let byte = version.as_bytes()[idx];
            if byte == b'.' || byte == b'\0' {
                break;
            }
            minor = minor * 10 + (byte - b'0') as u16;
            idx += 1;
        }

        (major << 8) | minor
    }
}
