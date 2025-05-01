//! Runtime programmer for configuration and firmware.

use usbd_hid::UsbError;
use usbd_serial::embedded_io::{ReadReady, Write};
use usbd_serial::SerialPort;

use crate::cfg;

use super::pac::FLASH;
use super::cfg::DrumConfig;
use super::usb::{UsbBus, UsbAllocator};

const COMM_IF_NAME: &'static str = "Taiko Drum CDC Control";
const DATA_IF_NAME: &'static str = "Taiko Drum CDC Data";
const BUFF_LEN: usize = 16;
const ACK: u8 = 0x06;

/// Local serializer implementation used to communicate with taiko drum utility.
trait ProgrammerSerializer: Sized {
    type Error: Sized;
    /// Serializes a structure in a proper format for utility read.
    fn serialize(&self, buff: &mut [u8; BUFF_LEN]);
    /// Deserializes upcoming stream of bytes from the utility into a structure of corresponding type.
    fn deserialize(buff: &[u8]) -> Result<Self, Self::Error>;
}

#[repr(u8)]
enum Command {
    /// Unknown state.
    Unknown = 0x00,
    /// Read current configuration.
    Read    = 0x01,
    /// Write new configuration.
    Write   = 0x02,

    /// Reset the firmware.
    Reset   = 0xff,
}

impl TryFrom<u8> for Command {
    type Error = u8;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        use Command::*;
        Ok(match value {
            0x00 => Unknown,
            0x01 => Read,
            0x02 => Write,

            0xff => Reset,
            _ => return Err(value)
        })
    }
}

/// Runtime Programmer.
///
/// Utilizes the serial port in order to perform basic tasks obtained from the host machine via
/// application specific utility. Below is the list of currently available features of this
/// programmer:
/// - Configuration Management (reading the configuration from flash and saving new one.);
/// - Reset the firmware;
pub(crate) struct Programmer<'a> {
    /// Serial port interface for straight communication between host and firmware.
    pub(crate) serial: SerialPort<'a, UsbBus>,
    /// Holds current drum configuration.
    pub(crate) cfg: DrumConfig,
    /// Flash is only controller by [`UsbConfigManager`] task to save new configurations on runtime.
    pub(crate) flash: super::pac::FLASH,
}

impl<'a> Programmer<'a> {
    /// Initializes new instance of [`Programmer`]
    pub(crate) fn new(alloc: &'a Option<UsbAllocator>, cfg: DrumConfig, flash: FLASH) -> Self {
        let serial = SerialPort::new_with_interface_names(
            alloc.as_ref().expect("Won't panic if this function is only called once."),
            Some(COMM_IF_NAME),
            Some(DATA_IF_NAME),
        );
        Self { serial, cfg, flash }
    }
}

impl Programmer<'_> {
    pub(crate) fn info(&self) {
        let lc = self.serial.line_coding();
        log::info!("Runtime programmer configured with: {:?}, {:?}, {}", 
            lc.data_rate(), lc.data_bits(), lc.stop_bits() as u8
        )
    }

    /// Command parsing and execution function.
    pub(crate) fn program(&mut self) {
        let mut buff = [0u8; BUFF_LEN];

        // Perform a non-blocking read.
        if let Ok(true) = self.serial.read_ready() {
            match self.serial.read(&mut buff) {
                Ok(rsize) => if rsize > 0 {
                    // Performing only properly parsed CMDs.
                    match buff[0].try_into() {
                        Ok(cmd) => match cmd {
                            Command::Reset => super::app::FirmwareReset::spawn().expect("Reset function cannot be called more than once."),
                            Command::Read => {
                                self.ack();
                                self.cfg.serialize(&mut buff);
                                // Sending current configuration back.
                                match self.serial.write(&buff) {
                                    Ok(wsize) => log::info!("Current configuration was send [{}] bytes", wsize),
                                    Err(err) => todo!(),
                                }
                            }
                            Command::Write => {
                                self.ack();
                                match DrumConfig::deserialize(&buff[1..]) {
                                    Ok(new_cfg) => self.cfg = new_cfg,
                                    Err(byte) => if byte != 0 { 
                                        log::error!("Unexpected byte value obtained: {}", byte) 
                                    },
                                }
                            }
                            _ => (),
                        }
                        Err(err) => log::warn!("Unknown command byte received: {:#x}, ignoring...", err),
                    }
                },
                Err(usb_err) => match usb_err {
                    UsbError::WouldBlock | UsbError::Unsupported => (),
                    _ => panic!("{:?}", usb_err),
                }
            }
        }
    }

    /// Sends an acknowledge signal with a small delay.
    fn ack(&mut self) {
        if let Err(err) = self.serial.write(&[ACK]) {
            todo!()
        }
        cortex_m::asm::delay(720);
    }
}

const LEFTKAT: u8 = 0x10;
const LEFTDON: u8 = 0x11;
const RIGHTDON: u8 = 0x12;
const RIGHTKAT: u8 = 0x13; 

impl ProgrammerSerializer for DrumConfig {
    type Error = u8;
    fn serialize(&self, buff: &mut [u8; BUFF_LEN]) {

        let data = [
            LEFTKAT, self.left_kat as u8,
            LEFTDON, self.left_don as u8,
            RIGHTDON, self.right_don as u8,
            RIGHTKAT, self.right_kat as u8,
        ];

        buff[..data.len()].copy_from_slice(&data);
    }

    fn deserialize(buff: &[u8]) -> Result<Self, Self::Error> {
        let mut iter = buff.into_iter();
        let mut s = Self::default();

        while let Some(&b) = iter.next() {
            match b {
                cmd @ (LEFTKAT | LEFTDON | RIGHTDON | RIGHTKAT) => 
                    if let Some(&key) = iter.next() {
                        match cmd {
                            LEFTKAT => s.left_kat = key.into(),
                            LEFTDON => s.left_don = key.into(),
                            RIGHTDON => s.right_don = key.into(),
                            RIGHTKAT => s.right_kat = key.into(),
                            _ => unreachable!(),
                        }
                    } else {
                        log::error!("Desserialization error: Unexpected end of stream withi the configuration command.");
                        return Err(0);
                    },
                _ => {
                    log::error!("Deserialization error: Unable to properly parse upcoming configuration byte-stream from the utility software.");
                    return Err(b);
                }
            }
        }

        Ok(s)
    }
}
