use core::fmt::Write;

use alloc::format;
use embedded_graphics::prelude::{RgbColor, WebColors};

use crate::{
    framebuffer::{Color, FRAMEBUFFER, render_text_buf},
    task::context,
};

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
        let target = record.target().split("::").last().unwrap_or("");
        crate::arch::serial::write_fmt(format_args!(
            "{}[{}]{} [{}.{:09}] {} [{}] {}\n",
            color,
            level_str,
            reset,
            uptime_secs,
            uptime_subsec_nanos,
            pid,
            target,
            record.args(),
        ));

        if let Some(fb) = FRAMEBUFFER.get() {
            let mut fb = fb.lock();
            fb.set_text_fgcolor_default();
            fb.write_fmt(format_args!("[")).ok();
            let color = match level {
                log::Level::Error => Color::RED,
                log::Level::Warn => Color::YELLOW,
                log::Level::Info => Color::GREEN,
                log::Level::Debug => Color::BLUE,
                log::Level::Trace => Color::CSS_LIGHT_GRAY,
            };
            fb.set_text_fgcolor(color);
            fb.write_fmt(format_args!("{level_str}")).ok();
            fb.set_text_fgcolor_default();
            fb.write_fmt(format_args!(
                "] [{}.{:09}] {} [{}] {}\n",
                uptime_secs,
                uptime_subsec_nanos,
                pid,
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
    log::set_max_level(log::LevelFilter::Debug);
    log::info!("Logger initialized");
}
