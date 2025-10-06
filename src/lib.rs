//! Library space for Taiko Drum Firmware.
#![no_std]
#![no_main]
#![feature(slice_from_ptr_range)]

use stm32f1::stm32f103 as pac;

/// Contains configured logger for application.
mod logger;
/// Piezoelectric sensors driver.
mod piezo;
/// Parses samples to detect proper hits.
mod parser;
/// USB device handling.
mod usb;
/// HID class implementations for drum controller.
mod hid;
/// Firmware configuration (Non-volatile).
mod cfg;
/// Runtime programmer.
mod prog;
/// Cross-correlation signal processing.
mod cross_correlation;

#[rtic::app(
    device = stm32f1::stm32f103,
    dispatchers = [SDIO, RTC],
    peripherals = true,
)]
mod app {
    use usb_device::UsbError;
    use rtic_monotonics::systick::prelude::*;
    use rtic_sync::make_channel;

    use crate::hid::DrumHitStrokeHidReport;

    use super::cfg::DrumConfig;
    use super::piezo::{PiezoSample, PIEZO_SENSOR_QUEUE_CAPACITY, PiezoSensorHandler, Receiver};
    use super::usb::{UsbTaikoDrum, UsbAllocator, UsbBus};
    use super::parser::Parser as P;
    use super::prog::Programmer;

    /* Firmware clocks. */
    systick_monotonic!(Systick);

    #[shared]
    struct Shared {
        reset_pend: bool,
        /// Shared pins of GPIOA port.
        gpioa: super::pac::GPIOA,
        /// USB device wrapper is used across interrupt handlers and tasks to communicate withhost.
        usb_dev: UsbTaikoDrum<'static>,
    }
    
    #[local]
    struct Local {
        /// Local to ADC1_2 interrupt handler, which reads the state of current hits periodically.
        piezo_handler: PiezoSensorHandler,
        /// Sensor samples parser.
        parser: P,
    }

    /// Performs a software system reset.
    #[task(local = [timeout: u32 = 10], shared = [reset_pend])]
    async fn FirmwareReset(mut ctx: FirmwareReset::Context) {
        ctx.shared.reset_pend.lock(|pend| *pend = true);

        let timeout = *ctx.local.timeout;
        log::info!("A system reset was called. Restarting in {} seconds...", timeout);
        Systick::delay(timeout.secs()).await;
        rtic::export::SCB::sys_reset();
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
    #[init(
        local = [usb_alloc: Option<UsbAllocator> = None]
    )]
    fn Init(ctx: Init::Context) -> (Shared, Local) {
        let (core, mut dev, alloc) = (ctx.core, ctx.device, ctx.local.usb_alloc);
        let (s, r) = make_channel!(PiezoSample, PIEZO_SENSOR_QUEUE_CAPACITY);

        /* Logging initialization. */
        if let Err(log_set_err) = super::logger::init() {
            unimplemented!()
        }  
        log::info!("Booting taiko firmware version: [{}]", super::version::TAIKO_HID_FIRMWARE_VERSION);


        /* Setting SYSCLK source to PLL (72 MHz on this line.) */
        let (rcc, flash) = (&mut dev.RCC, &mut dev.FLASH);

        // Enabling internal high speed clock
        rcc.cr.modify(|_, w| w.hseon().set_bit());
        while rcc.cr.read().hserdy().bit_is_clear() {}

        rcc.cfgr.modify(|_, w|
            w   /* Multiplying HSE to reach a maximal value of 72 MHz */
             .pllsrc().set_bit()
             .pllxtpre().clear_bit()
             .pllmul().mul9()
        );

        // Enabling PLL.
        rcc.cr.modify(|_, w| w.pllon().set_bit());
        while rcc.cr.read().pllrdy().bit_is_clear() {}

        flash.acr.modify(|_, w| w.latency().ws2());

        // Architecture specific USB bus allocator.
        alloc.replace(UsbBus::new(super::usb::UsbControllerSTM32F103));

        // Sys clock switch.
        rcc.cfgr.modify(|_, w| w.sw().pll());
        while !rcc.cfgr.read().sws().is_pll() {}

        /* Monotonics. */
        log::debug!("Enabling Systick monotonic...");
        Systick::start(core.SYST, ARM_SYSTICK_HZ);
        log::info!("Internal clocks enabled");

        // Runtime firmware and configuration programmer.
        let programmer = Programmer::new(
            alloc,
            //DrumConfig::new(&mut dev.FLASH),
            DrumConfig::default(),
            dev.FLASH,
        );

        let usb_dev = UsbTaikoDrum::new(alloc, programmer, dev.USB, &mut dev.GPIOA, &mut dev.RCC);
        let piezo_handler = PiezoSensorHandler::new(
            (dev.ADC1, dev.ADC2), &mut dev.GPIOA, &mut dev.RCC, dev.TIM4, s.clone()
        );
        let cfg = &usb_dev.programmer.cfg;

        /* Tasks */ 
        Parser::spawn(r).expect("First parser initialization.");

        (
            Shared { usb_dev, gpioa: dev.GPIOA, reset_pend: false }, 
            Local { piezo_handler, parser: P::default() },
        )    
    }

    /// Parses upcoming samples to detect proper hits and ignore spurious ones.
    ///
    /// Obtained samples are being parsed to detect a proper drum hit and it's location. Based on
    /// the current hits, HID reports are being sent to the host machine, simulating a keyboard
    /// device that presses the corresponding keystrokes.
    #[task(local = [parser], shared = [usb_dev])]
    async fn Parser(mut ctx: Parser::Context, mut r: Receiver) {
        let parser = ctx.local.parser;
        log::info!("Parser task spawned. Waiting for samples.");

        /* Handling samples obtained from the piezoelectric sensor */
        while let Ok(sample) = r.recv().await {
            ctx.shared.usb_dev.lock(|dev| {
                parser.parse(&dev.programmer.cfg, sample).map(|report|
                    UsbHidSender::spawn(report).expect("Higher priority task spawn condition.")
                );
            });

            super::int_enable!(ADC1_2); // TODO! do not enable on each loop.
            Systick::delay(500.nanos()).await;
        }
    }

    /// Sends USB HID reports to the host machine.
    #[task(priority = 1, shared = [usb_dev])]
    async fn UsbHidSender(mut ctx: UsbHidSender::Context, report: DrumHitStrokeHidReport) {
        ctx.shared.usb_dev.lock(|dev| {
           
            dev.poll();
            match dev.hid_keyboard.push_input(&report) {
                Ok(report_length) => {
                    log::debug!("Bytes send: {}", report_length);
                },
                Err(usb_err) => match usb_err {
                    // Checking if device is properly initialized at that point.
                    UsbError::WouldBlock => dev.init_poll(),
                    UsbError::Unsupported => (),
                    _ => panic!("{:?}", usb_err),
                }
            }
        });
    }

    /// Piezoelectric sensor handling hardware task.
    ///
    /// # Binds
    ///
    /// This handler function is binded to ADC1_2 interrupt vector. 
    ///
    /// The underlying sensor handling structure is queuing next injected sample from the ADC pin
    /// to the [`super::app::UsbHidSender`] task.
    #[task(binds = ADC1_2, priority = 2, local = [piezo_handler])]
    fn SensorHandling(ctx: SensorHandling::Context) {
        ctx.local.piezo_handler.send();
    }

    /// USB TX Polling.
    #[task(binds = USB_HP_CAN_TX, priority = 2, shared = [usb_dev])]
    fn UsbPollTx(mut ctx: UsbPollTx::Context) {
        log::debug!("USB_EVENT_Tx");
        ctx.shared.usb_dev.lock(|dev| {
            crate::app::__usb_poll(dev);
        });
    }

    /// USB RX Polling.
    #[task(binds = USB_LP_CAN_RX0, priority = 2, shared = [usb_dev])]
    fn UsbPollRx(mut ctx: UsbPollRx::Context) {
        log::debug!("USB_EVENT_Rx");
        ctx.shared.usb_dev.lock(|dev| {
            dev.init_poll();   /* Low priority interrupts include enumeration requests and error handling. */
            crate::app::__usb_poll(dev);
        });
    }

    fn __usb_poll(dev: &mut UsbTaikoDrum) {
        dev.poll();
        dev.programmer.program();
    }

    // Panic handler.
    //
    // Performs a full system reset after a several second timeout.
    // TODO! Perform a better panic restart procedure.
    panic_custom::define_panic!(|info| {
        log::error!("System panic occured: {}", info);
    });

    const ARM_SYSTICK_HZ: u32 = 72_000_000;
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
