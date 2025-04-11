//! Defines a piezoelectric sensor driver to detect precise hits for Taiko Drum.

use crate::app;

use super::pac::{RCC, ADC1, ADC2, GPIOA, TIM4};
use rtic_sync::channel::TrySendError;

/* Constant sampler configuration values. TODO! swap to configurable values saved in flash */
const INTERRUPT_SAMPLER_TIMER_CC: u16 = 1000;
/* 12-bit ADC will obtain this value when the voltage will spike to >=0,3V */
const WATCHDOG_THRESHOLD_HALT_MODE_VALUE: u16 = 500;

/* Sensor position to channel mapping, */
const LEFT_EDGE_PIEZO: u8 = 3;
const LEFT_CENTER_PIEZO: u8 = 4;
const RIGHT_CENTER_PIEZO: u8 = 5;
const RIGHT_EDGE_PIEZO: u8 = 6;

/// Communication queue capacity.
pub(crate) const PIEZO_SENSOR_QUEUE_CAPACITY: usize = 32;
/// Type alias for 32-bit analog value from ADC.
///
/// Sensor handler samples central and edge sensors simultaneously in one such value.
#[repr(C, packed)]
#[derive(Debug)]
pub(crate) struct PiezoSample {
    le: u16, lc: u16,
    rc: u16, re: u16,
}

/// Defines sampling mode for [`PiezoSensorHandler`].
///
/// Different modes are used to improve power efficiency and utilize different peripherals for
/// their needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PiezoSensorSampleMode {
    /// Halt Sample Mode.
    ///
    /// Default sensor sampling behavior. Performs no sensor sampling at all, until ADC's internal
    /// analog watchdog finds a first sensor peak. Then this mode is quickly set to
    /// [`PiezoSensorSampleMode::TIMER`] and watchdog will be turned off.
    HALT,
    /// Timer Sample Mode.
    ///
    /// Default sampling mode to analyze peaks from any of four drum's sensor during singular taps
    /// and bursts. When timer counts to the provided compare value, two ADCs will sample upcoming
    /// data simultaneously on injected channels.
    ///
    /// Sensor handler will be set to [`PiezoSensorSampleMode::HALT`] mode, when no peaks are seen
    /// on all four sensors (communication queue will be sending zeroed data). It will then halt
    /// the timer completely until it listens to the first peak.
    TIMER(u16),
}

type Sender = rtic_sync::channel::Sender<'static, PiezoSample, PIEZO_SENSOR_QUEUE_CAPACITY>;
pub(crate) type Receiver = rtic_sync::channel::Receiver<'static, PiezoSample, PIEZO_SENSOR_QUEUE_CAPACITY>;

/// Handler structure which collects new injected ADC samples on each interrupt.
///
/// This structure is local to [`super::pac::Interrupt::ADC1_2`] interrupt handler hardware task and used to sample and
/// transfer data to the [`super::app::UsbHidSender`] task. Structure handles both ADC's and four
/// analog channels from GPIOA.
///
/// Handler configures two ADCs (ADC1, ADC2) to work in dual injected simultaneous mode.
pub(crate) struct PiezoSensorHandler {
    /// Holds ownership for both ADCs, since they are always used by this structure during interrupts.
    adcs: (ADC1, ADC2),
    /// Timer that causes injected ADC channels to perform the conversion.
    tim: TIM4,
    /// Local queue sender for communicating with HID task.
    sender: Sender,
    /// Currently used sample mode.
    mode: PiezoSensorSampleMode,
}

impl PiezoSensorHandler {
    /// Initializes required peripherals and returns a singular instance of [`PiezoSensorHandler`]
    ///
    /// # Port Mapping
    ///
    /// Port mapping is performed according to the PCB schematic connections for Taiko Drum PCB board.
    /// ADCs are configured to work in dual mode with injected channels, with timer 3 being an
    /// external interrupt for both of them. Two ADCs sample center and edge hits of the drum simultaneously.
    pub(crate) fn new(
        adcs: (ADC1, ADC2), 
        gpios: &mut GPIOA,
        rcc: &mut RCC, 
        tim: TIM4,
        sender: Sender, 
    ) -> Self {
        log::debug!("Configuring piezoelectric sensor handler.");
        /* Enabling clocking for ADC1, ADC2 from APB2 high frequency domain. */
        rcc.cfgr.modify(|_, w| 
            w
             .ppre1().div16()       // Clock prescaler for low-freq area.
             .ppre2().div1()        // Fully sampled from prescaled AHB (12 Mhz)
             .adcpre().div2()       // Least div rate for ADC sampling.
        );
        rcc.apb1enr.modify(|_, w|   // Enables clock for TIM4.
            w
             .tim4en().set_bit()
        );
        rcc.apb2enr.modify(|_, w|   // Enables clock for both ADCs.
            w
             .adc1en().set_bit()
             .adc2en().set_bit()
        );

        Self::__sensor_gpios_conf(gpios);   // GPIO configuration. 

        /* Enabling both ADC's */
        adcs.0.cr2.modify(|_, w|
            w
             .jextsel().tim4trgo()  /* In dual mode only master shall be triggered by external event. */
             .jexttrig().set_bit()
             .adon().set_bit()  
        );
        adcs.1.cr2.modify(|_, w|
            w
             .jextsel().jswstart() /* Software interrupts must be enabled for slave ADC to prevent spurious interrupt. */
             .jexttrig().set_bit()
             .adon().set_bit()
        );

        /* 
         * ADC calibration procedure.
         *
         * This will also halt the CPU in the loop until ADC will be properly started after waiting
         * for t_STAB, which is not well defined.
         * */
        adcs.0.cr2.modify(|_, w| w.cal().set_bit());
        while adcs.0.cr2.read().cal().bit_is_set() {}
        adcs.1.cr2.modify(|_, w| w.cal().set_bit());
        while adcs.1.cr2.read().cal().bit_is_set() {}

        // ADC1, ADC2 dual mode synchronized configuration with iterrupts enabled from ADC1.
        adcs.0.cr1.modify(|_, w|
            w
             .jeocie().set_bit()    /* Performing interrupt on ADC1 for injected channels only.         */
             .awdsgl().clear_bit()  /* Watchdog listens on all channels. */
            .scan().set_bit()      /* Scan mode will store multiple channels in JDR1, JDR2 */
        );
        adcs.1.cr1.modify(|_, w|
            w
             .awdsgl().clear_bit()
             .scan().set_bit()
        );
        
        /* 
         * Processing two injected conversions on each ADC 
         *
         * Center hit sensors and edge hit sensors are being sampled simultaneously. Each ADC
         * handles one edge and one center piezoelectric sensor in the following order:
         * ADC1: LEFT_EDGE -> LEFT_CENTER -> JEOC 
         * ADC2: RIGHT_EDGE -> RIGHT_CENTER -> JEOC 
         * */
        adcs.0.jsqr.modify(|_, w|
            w.jl().variant(1)
             .jsq3().variant(LEFT_EDGE_PIEZO)
             .jsq4().variant(LEFT_CENTER_PIEZO)
        );

        adcs.1.jsqr.modify(|_, w|
            w.jl().variant(1)
             .jsq3().variant(RIGHT_EDGE_PIEZO)
             .jsq4().variant(RIGHT_CENTER_PIEZO)
        );
        
        // Configure watchdog thresholds
        adcs.0.htr.modify(|_, w| w.ht().bits(WATCHDOG_THRESHOLD_HALT_MODE_VALUE));

        adcs.0.cr1.modify(|_, w|
            w 
             .dualmod().injected()  /* Setting this bit at the end of ADC configuration provides better synchronization between two ADCs. */
        );
        // Enabling ADCs
        adcs.0.cr2.modify(|_, w| w.adon().set_bit());
        adcs.1.cr2.modify(|_, w| w.adon().set_bit());

        tim.psc.write(|w| w.psc().bits(0));                    /* Prescaler value for timer.            */
        tim.ccmr1_output().modify(|_, w| w.oc1m().frozen());   /* Don't generate PWM signal on channel  */
        tim.cr1.modify(|_, w| w.opm().clear_bit());            /* Continuous mode.                      */
        tim.cr2.modify(|_, w| w.mms().update());               /* Generate TRGO when hitting CC         */

        log::info!("ADC sampling subsystem is initialized. Waiting for global interrupt unmask.");

        let mut s = Self { adcs, sender, tim, mode: PiezoSensorSampleMode::HALT };
        s.__set_pssm_halt();
        s.set_interrupt_mode(PiezoSensorSampleMode::TIMER(INTERRUPT_SAMPLER_TIMER_CC));
        s
    }

    /// Internal function for switching between sampling modes.
    ///
    /// Used to not consume power when no peaks are detected during a long period of time. (TODO!)
    fn set_interrupt_mode(&mut self, mode: PiezoSensorSampleMode) {
        if self.mode == mode { return }

        match mode {
            PiezoSensorSampleMode::HALT => self.__set_pssm_halt(),
            PiezoSensorSampleMode::TIMER(cc) => self.__set_pssm_timer(cc),
        }
        self.mode = mode;
    }

    /// Sends next sample over communication queue.
    pub(crate) fn send(&mut self) {
        if self.adcs.0.sr.read().jeoc().bit_is_clear() {
            log::warn!("Unable to read from ADC's that haven't ended their conversion");
            return
        }

        if let Err(err) = self.sender.try_send(self.read()) {
            match err {
                /* 
                 * This shall not happen at all in this application, since that means loosing
                 * connection with the host machine. 
                 * */
                TrySendError::NoReceiver(_) => {
                    log::warn!("Tried to send without a receiver. Loosing data.");
                },
                /*  
                 * This means that [`super::app::UsbHidSender`] task is starving. Might cause huge
                 * input lag spike
                 * */
                TrySendError::Full(_) => {
                    log::warn!("FIFO queue is full. Loosing data.");
                    crate::int_disable!(ADC1_2);    // Stopping the transmition for some time.
                }
            }
        }
    }

    /// Reads ADC conversion result from all sensors.
    fn read(&self) -> PiezoSample {
        PiezoSample {
            le: self.adcs.0.jdr1().read().jdata().bits(),
            lc: self.adcs.0.jdr2().read().jdata().bits(),
            re: self.adcs.1.jdr1().read().jdata().bits(),
            rc: self.adcs.1.jdr2().read().jdata().bits(),
        }
    }

    fn __set_pssm_halt(&mut self) {
        log::info!("PSSM: Entering HALT mode.");

        // Stops the timer if running.
        self.tim.cr1.modify(|r, w| 
            if r.cen().bit_is_set() { w.cen().clear_bit() } else { w }
        );

        // Enable analog watchdog and disable JEOC interrupts.
        self.adcs.0.cr1.modify(|_, w|
            w
             .jeocie().clear_bit()
             .jawden().set_bit()
             .awdie().set_bit()
        );
    }

    fn __set_pssm_timer(&mut self, cc: u16) {
        log::info!("PSSM: Entering TIMER mode with CC={}.", cc);

        // Disable watchdog, enable JEOC interrupt
        self.adcs.0.cr1.modify(|_, w| {
            w
             .jawden().clear_bit()
             .awdie().clear_bit()
             .jeocie().set_bit()
        });

        /* CC setup */
        self.tim.ccr1().write(|w| w.ccr().bits(cc));
        self.tim.cr1.modify(|r, w| 
            if r.cen().bit_is_clear() { w.cen().set_bit() } else { w }
        );
    }

    fn __sensor_gpios_conf(gpios: &mut GPIOA) {
        // Gpio pins configuration.
        gpios.crl.modify(|_, w|         /* Configuring required pins as ADC analog input            */
            w                           /* `push_pull()` method is equal to set analog input mode   */
             .mode3().input() 
             .cnf3().push_pull()
             .mode4().input()
             .cnf4().push_pull()
             .mode5().input() 
             .cnf5().push_pull()
             .mode6().input()
             .cnf6().push_pull()
        );

        gpios.lckr.modify(|_, w|       /* Locking gpio configuration for used pins. This allows to      */ 
            w                          /* remove the ownership of [`GPIOA`] for [`PiezoSensorHandler`]  */
             .lck3().set_bit()
             .lck4().set_bit()
             .lck5().set_bit()
             .lck6().set_bit()
             .lckk().set_bit()
        );
    }
}
