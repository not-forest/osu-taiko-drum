//! Module that defines HID part of the firmware as well as defining all required trait implementations.

use core::marker::PhantomData;
use usb_device::{bus::UsbBusAllocator, device::{UsbDevice, UsbDeviceBuilder, UsbVidPid}};
use usbd_hid::{descriptor::{generator_prelude::*, *}, hid_class::HIDClass};
use super::pac::{RCC, USB};

/* Constant USB definitions. See: https://github.com/obdev/v-usb/blob/master/usbdrv/USB-IDs-for-free.txt */
const USB_VID: u16 = 0x16c0;
const USB_PID: u16 = 0x27db;
const MANUFACTURER: &'static str = "Serhii Shkliaiev [not-forest]";
const PRODUCT: &'static str = "Taiko Drum Controller";
const USB_HID_CLASS_POLLING_FREQ: usize = 100;

/// Usb VID-PID Pair
const TAIKO_DRUM_VIDPID: UsbVidPid  = UsbVidPid(USB_VID, USB_PID);

pub(crate) type UsbBus = stm32_usbd::UsbBus<UsbControllerSTM32F103>;

/// Main USB communication wrapper structure for Taiko Drum.
///
/// # Note
///
/// This wrapper is specific to STM32F103xx family of microcontrollers.
pub(crate) struct UsbTaikoDrum<'a> {
    dev: UsbDevice<'a, UsbBus>,
    hid: HIDClass<'a, UsbBus>,
    _phantom: PhantomData<USB>,
}

impl<'a> UsbTaikoDrum<'a> {
    /// Initializes a new instance of [`UsbTaikoDrum`].
    pub(crate) fn new(alloc: &'a UsbBusAllocator<UsbBus>) -> Self {
        let hid = HIDClass::new(&alloc, DrumHitStrokeHidReport::desc(), ((1 / USB_HID_CLASS_POLLING_FREQ) * 1000) as u8);
        let dev = UsbDeviceBuilder::new(&alloc, TAIKO_DRUM_VIDPID)
            .device_class(0x07)
            .device_release(crate::version::TAIKO_HID_FIRMWARE_VERSION_BCD)
            .build();

        Self { dev, hid, _phantom: PhantomData }
    }

    /// Allows to perform a HID communication over USB.
    pub(crate) fn poll<F>(&mut self, f: F) where
        F: FnOnce(&mut HIDClass<UsbBus>)
    {
        self.dev.poll(&mut [&mut self.hid]).then(|| f(&mut self.hid));
    } 

    /// Initializes a new bus allocator from the underlying usb controller.
    ///
    /// The obtained bus must be a static variable for the whole application, since all USB-related
    /// functionality requires it.
    pub(crate) fn bus(usb: USB) -> UsbBusAllocator<UsbBus> {
        drop(usb);
        UsbBus::new(UsbControllerSTM32F103)
    }
}

/// Drum Stroke HID Class Report.
///
/// Acts as a keyboard device that sends corresponding keycodes mapped to the corresponding hitstroke
/// obtained from the four drum sensors.
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

/// Marker microcontroller-dependent structure.
pub(crate) struct UsbControllerSTM32F103;

unsafe impl Sync for UsbControllerSTM32F103 {}

unsafe impl stm32_usbd::UsbPeripheral for UsbControllerSTM32F103 {
    const REGISTERS: *const () = USB::ptr() as *const ();
    const DP_PULL_UP_FEATURE: bool = false;
    const EP_MEMORY: *const () = 0x4000_6000 as _;
    const EP_MEMORY_ACCESS_2X16: bool = false;
    const EP_MEMORY_SIZE: usize = 512;

    fn enable() {
        let rcc = unsafe { &*RCC::ptr() };

        cortex_m::interrupt::free(|_| {
            // Enables USB peripheral
            rcc.apb1enr.modify(|_, w| w.usben().set_bit());

            // Resets USB peripheral
            rcc.apb1rstr.modify(|_, w| w.usbrst().set_bit());
            rcc.apb1rstr.modify(|_, w| w.usbrst().clear_bit());
        });
    }

    fn startup_delay() {
        // There is a chip specific startup delay. For STM32F103xx it's 1µs and this should wait for
        // at least that long.
        cortex_m::asm::delay(72);
    }
}
