use core::fmt::Write;

use alloc::format;
use embedded_graphics::prelude::{RgbColor, WebColors};

use crate::{
    arch::serial::lock_uart,
    framebuffer::{Color, with_fb},
    task::context,
    util::DebugCheckedPanic,
};

/// A logger that writes log messages to the serial console and framebuffer.
pub struct Logger;

impl log::Log for Logger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        // Enable all log levels
        true
    }

    fn flush(&self) {}

    fn log(&self, record: &log::Record) {
        let level = record.level();
        let uptime = crate::time::uptime();
        let uptime_secs = uptime.as_secs();
        let uptime_subsec_nanos = uptime.subsec_nanos();
        let pid = match context::current() {
            Some(cx) => match cx.try_read() {
                Some(cx) => &format!("[{}]", cx.pid),
                None => "[-]",
            },
            None => "[-]",
        };

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
        let mut uart = lock_uart();
        let target = record.target().split("::").last().unwrap_or("??");
        let file = record.file().unwrap_or("??");
        let line = record.line().unwrap_or_default();
        uart.write_fmt(format_args!(
            "{}[{}]{} [{}.{:09}] {} [{}:{}] {}\n",
            color,
            level_str,
            reset,
            uptime_secs,
            uptime_subsec_nanos,
            pid,
            if level <= log::Level::Warn {
                file
            } else {
                target
            },
            line,
            record.args(),
        ))
        .ok();
        drop(uart);

        with_fb(|fb| {
            fb.set_text_fgcolor_default();
            let color = match level {
                log::Level::Error => Color::RED,
                log::Level::Warn => Color::YELLOW,
                log::Level::Info => Color::GREEN,
                log::Level::Debug => Color::BLUE,
                log::Level::Trace => Color::CSS_LIGHT_GRAY,
            };
            fb.set_text_fgcolor(color);
            fb.write_fmt(format_args!("[{level_str}]")).ok();
            fb.set_text_fgcolor_default();
            fb.write_fmt(format_args!(
                " [{}.{:09}] {} [{}] {}\n",
                uptime_secs,
                uptime_subsec_nanos,
                pid,
                target,
                record.args()
            ))
            .ok();

            fb.clear_pixels();
            fb.render_text_buf();
            fb.present();
        });
    }
}

/// Initializes the logger by setting it as the global logger and configuring the log level.
pub fn init() {
    log::set_logger(&Logger).debug_checked_expect("Failed to set logger");
    let level_env = match option_env!("KADOS_LOG") {
        Some("trace") => log::LevelFilter::Trace,
        Some("debug") => log::LevelFilter::Debug,
        // Some("info") => log::LevelFilter::Info,
        Some("warn") => log::LevelFilter::Warn,
        Some("error") => log::LevelFilter::Error,
        Some("off") => log::LevelFilter::Off,
        _ => log::LevelFilter::Info,
    };
    log::set_max_level(level_env);
    log::info!("Logger initialized");
}
