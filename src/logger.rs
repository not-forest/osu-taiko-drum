//! Custom semihosting logger.

use rtt_target::rprintln;
use log::{Log, Level, SetLoggerError};

/// Semihosting debug logger for taiko drum board.
struct TaikoLogger;

const APP_LOGGER: TaikoLogger = TaikoLogger; 

impl TaikoLogger {
    /// Initializes global [`TaikoLogger`] structure for the application.
    ///
    /// # Debug
    ///
    /// While in debug build, uses Trace logging level.
    fn init() -> Result<(), SetLoggerError> {
        log::set_logger(&APP_LOGGER)
            .map(|_l| {
                #[cfg(debug_assertions)] {
                    rtt_target::debug_rtt_init_print!();
                    log::set_max_level(log::LevelFilter::Trace);
                } 
                #[cfg(not(debug_assertions))] {
                    rtt_target::rtt_init_print!();
                    log::set_max_level(log::LevelFilter::Info);
                } 
            })
    }
}

impl Log for TaikoLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        #[cfg(debug_assertions)] {
            metadata.level() <= Level::Trace
        } 
        #[cfg(not(debug_assertions))] {
            metadata.level() <= Level::Info
        }
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            rprintln!("{{{}}}, [{}], {}", 
                record.target(), 
                record.level(), 
                record.args()
            ); 
        }
    }

    fn flush(&self) {}
}

/// Initializes global [`TaikoLogger`] structure for the application.
///
/// # Debug
///
/// While in debug build, uses Trace logging level.
pub fn init() -> Result<(), SetLoggerError> {
    TaikoLogger::init()
}
