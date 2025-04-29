pub use log::*;

pub struct Logger;

impl Log for Logger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        // Enable all log levels
        true
    }

    fn flush(&self) {}

    fn log(&self, record: &Record) {
        let level = record.level();
        let level_str = match level {
            log::Level::Error => "ERR",
            log::Level::Warn => "WRN",
            log::Level::Info => "INF",
            log::Level::Debug => "DBG",
            log::Level::Trace => "TRC",
        };
        let color = match level {
            log::Level::Error => "\x1b[31m", // Red
            log::Level::Warn => "\x1b[33m",  // Yellow
            log::Level::Info => "\x1b[32m",  // Green
            log::Level::Debug => "\x1b[34m", // Blue
            log::Level::Trace => "\x1b[37m", // White
        };
        let reset = "\x1b[0m"; // Reset color
        crate::serial::write_fmt(format_args!(
            "{}[{}]{} {}: {}\n",
            color,
            level_str,
            reset,
            record.target(),
            record.args(),
        ));
    }
}

pub fn init() {
    log::set_logger(&Logger).expect("Failed to set logger");
    log::set_max_level(LevelFilter::Trace);
}
