use core::fmt::Write;

use embedded_graphics::{
    pixelcolor::Rgb888,
    prelude::{RgbColor, WebColors},
};

use crate::framebuffer::{FRAMEBUFFER, render_text_buf};

pub struct Logger;

impl log::Log for Logger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        // Enable all log levels
        true
    }

    fn flush(&self) {}

    fn log(&self, record: &log::Record) {
        let level = record.level();
        let uptime = crate::arch::time::uptime();
        let uptime_secs = uptime.as_secs();
        let uptime_subsec_nanos = uptime.subsec_nanos();

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
        let target = record.target().split("::").last().unwrap_or("");
        crate::serial::write_fmt(format_args!(
            "{}[{}]{} [{}.{:09}] [{}] {}\n",
            color,
            level_str,
            reset,
            uptime_secs,
            uptime_subsec_nanos,
            target,
            record.args(),
        ));

        if let Some(fb) = FRAMEBUFFER.get() {
            let mut fb = fb.lock();
            fb.set_text_fgcolor_default();
            fb.write_fmt(format_args!("[")).ok();
            let color = match level {
                log::Level::Error => Rgb888::RED,
                log::Level::Warn => Rgb888::YELLOW,
                log::Level::Info => Rgb888::GREEN,
                log::Level::Debug => Rgb888::BLUE,
                log::Level::Trace => Rgb888::CSS_LIGHT_GRAY,
            };
            fb.set_text_fgcolor(color);
            fb.write_fmt(format_args!("{level_str}")).ok();
            fb.set_text_fgcolor_default();
            fb.write_fmt(format_args!(
                "] [{}.{:09}] [{}] {}\n",
                uptime_secs,
                uptime_subsec_nanos,
                target,
                record.args()
            ))
            .ok();
            drop(fb);
            render_text_buf();
        }
    }
}

pub fn init() {
    log::set_logger(&Logger).unwrap();
    log::set_max_level(log::LevelFilter::Trace);
    log::info!("Logger initialized");
}
