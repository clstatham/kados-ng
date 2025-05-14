use core::ops::Add;

use alloc::boxed::Box;
use embedded_graphics::{
    mono_font::{MonoFont, MonoTextStyle, ascii},
    prelude::*,
    text::Text,
};
use spin::Once;

pub use embedded_graphics::pixelcolor::Rgb888 as Color;

use crate::{
    arch::clean_data_cache,
    mem::units::VirtAddr,
    sync::{IrqMutex, IrqMutexGuard},
};

const FONT: MonoFont = ascii::FONT_10X20;

pub const TEXT_BUFFER_WIDTH: usize = 80;
pub const TEXT_BUFFER_HEIGHT: usize = 25;

#[derive(Clone, Copy)]
pub struct FbChar {
    char: u8,
    fg: Color,
}

impl FbChar {
    pub const DEFAULT: Self = Self {
        char: b' ',
        fg: Color::BLACK,
    };

    pub fn new(char: u8, fg: Color) -> Self {
        Self { char, fg }
    }

    pub fn to_text(&self, top_left: Point, x: usize, y: usize) -> Text<MonoTextStyle<Color>> {
        Text::new(
            core::str::from_utf8(core::slice::from_ref(&self.char)).unwrap_or(" "),
            top_left
                + Point::new(
                    FONT.character_size.width as i32 * (x as i32 + 1),
                    FONT.character_size.height as i32 * (y as i32 + 1),
                ),
            MonoTextStyle::new(&FONT, self.fg),
        )
    }
}

pub struct FrameBuffer {
    start_addr: VirtAddr,
    size_bytes: usize,
    width: usize,
    height: usize,
    bpp: usize,
    text_buf: Box<[[Option<FbChar>; TEXT_BUFFER_WIDTH]; TEXT_BUFFER_HEIGHT]>,
    text_cursor_x: usize,
    text_cursor_y: usize,
    text_fgcolor: Color,
}

impl FrameBuffer {
    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn bpp(&self) -> usize {
        self.bpp
    }

    pub fn size_pixels(&self) -> usize {
        self.width * self.height
    }

    pub fn size_bytes(&self) -> usize {
        self.size_bytes
    }

    pub fn set_text_fgcolor(&mut self, color: Color) {
        self.text_fgcolor = color;
    }

    pub fn set_text_fgcolor_default(&mut self) {
        self.text_fgcolor = Color::WHITE;
    }

    pub fn render_text_buf(&mut self) {
        for line in 0..TEXT_BUFFER_HEIGHT {
            for col in 0..TEXT_BUFFER_WIDTH {
                if let Some(ch) = self.text_buf[line][col] {
                    ch.to_text(self.bounding_box().top_left, col, line)
                        .draw(self)
                        .unwrap();
                }
            }
        }
    }

    pub fn clear_pixels(&mut self) {
        self.clear(Color::BLACK).unwrap();
    }

    pub fn frame_mut(&mut self) -> &mut [u32] {
        unsafe {
            core::slice::from_raw_parts_mut(
                self.start_addr.as_raw_ptr_mut(),
                self.size_bytes() / size_of::<u32>(),
            )
        }
    }

    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            0x8 => self.backspace(),
            b'\n' => self.new_line(),
            b'\r' => self.text_cursor_x = 0,
            byte => {
                if self.text_cursor_x >= TEXT_BUFFER_WIDTH - 1 {
                    self.new_line();
                }

                let row = self.text_cursor_y;
                let col = self.text_cursor_x;

                self.text_buf[row][col] = Some(FbChar {
                    char: byte,
                    fg: self.text_fgcolor,
                });
                self.move_right();
            }
        }
        self.cursor_color_hook();
    }

    pub fn set_pixel_raw(&mut self, x: usize, y: usize, color: u32) {
        let width = self.width;
        unsafe {
            let ptr = self.frame_mut().as_mut_ptr().add(x + y * width);
            ptr.write_volatile(color);

            clean_data_cache(ptr.cast(), size_of::<u32>());
        }
    }

    fn cursor_color_hook(&mut self) {}

    pub fn backspace(&mut self) {
        let row = self.text_cursor_y;
        let col = self.text_cursor_x.saturating_sub(1);
        self.text_buf[row][col] = None;
        self.text_cursor_x = col;
        self.cursor_color_hook();
    }

    pub fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            self.write_byte(byte)
        }
    }

    pub fn new_line(&mut self) {
        if self.text_cursor_y >= TEXT_BUFFER_HEIGHT - 1 {
            for row in 1..TEXT_BUFFER_HEIGHT {
                for col in 0..TEXT_BUFFER_WIDTH {
                    let character = self.text_buf[row][col];
                    self.text_buf[row - 1][col] = character;
                }
            }
            self.text_cursor_y = TEXT_BUFFER_HEIGHT - 1;
            self.clear_row(self.text_cursor_y);
            self.text_cursor_x = 0;
        } else {
            self.text_cursor_y += 1;
            self.text_cursor_x = 0;
        }
        self.cursor_color_hook();
    }

    pub fn clear_row(&mut self, row: usize) {
        for col in 0..TEXT_BUFFER_WIDTH {
            self.text_buf[row][col] = None;
        }
        self.cursor_color_hook();
    }
    pub fn clear_until_end(&mut self) {
        for col in self.text_cursor_x..TEXT_BUFFER_WIDTH {
            self.text_buf[self.text_cursor_y][col] = None;
        }
        for row in self.text_cursor_y + 1..TEXT_BUFFER_HEIGHT {
            self.clear_row(row);
        }
        self.cursor_color_hook();
    }
    pub fn clear_until_beginning(&mut self) {
        for col in 0..self.text_cursor_x {
            self.text_buf[self.text_cursor_y][col] = None;
        }
        for row in 0..self.text_cursor_y - 1 {
            self.clear_row(row);
        }
        self.cursor_color_hook();
    }
    pub fn clear_until_eol(&mut self) {
        for col in self.text_cursor_x..TEXT_BUFFER_WIDTH {
            self.text_buf[self.text_cursor_y][col] = None;
        }
        self.cursor_color_hook();
    }
    pub fn clear_from_bol(&mut self) {
        for col in 0..self.text_cursor_x {
            self.text_buf[self.text_cursor_y][col] = None;
        }
        self.cursor_color_hook();
    }
    pub fn clear_line(&mut self) {
        self.clear_row(self.text_cursor_y);
    }
    pub fn clear_text(&mut self) {
        for row in 0..TEXT_BUFFER_HEIGHT {
            self.clear_row(row)
        }
        self.cursor_color_hook();
    }
    pub fn move_up(&mut self) {
        let new_y = self.text_cursor_y.saturating_sub(1);
        self.text_cursor_y = new_y;
        self.cursor_color_hook();
    }
    pub fn move_down(&mut self) {
        let new_y = self.text_cursor_y.add(1).min(TEXT_BUFFER_HEIGHT - 1);
        self.text_cursor_y = new_y;
        self.cursor_color_hook();
    }
    pub fn move_left(&mut self) {
        self.text_cursor_x = self.text_cursor_x.saturating_sub(1);
        self.cursor_color_hook();
    }
    pub fn move_right(&mut self) {
        self.text_cursor_x = self.text_cursor_x.add(1).min(TEXT_BUFFER_WIDTH - 1);
        self.cursor_color_hook();
    }
}

impl core::fmt::Write for FrameBuffer {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

impl DrawTarget for FrameBuffer {
    type Color = Color;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics::Pixel<Self::Color>>,
    {
        for Pixel(coord, color) in pixels.into_iter() {
            let (x, y) = coord.into();
            if (0..self.width as i32).contains(&x) && (0..self.height as i32).contains(&y) {
                self.set_pixel_raw(x as usize, y as usize, color.into_storage());
            }
        }

        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.frame_mut().fill(color.into_storage());
        let ptr = self.start_addr.as_raw_ptr_mut();
        unsafe { clean_data_cache(ptr, self.size_bytes) };
        Ok(())
    }
}

impl OriginDimensions for FrameBuffer {
    fn size(&self) -> embedded_graphics::prelude::Size {
        Size::new(self.width as u32, self.height as u32)
    }
}

pub static FRAMEBUFFER: Once<IrqMutex<FrameBuffer>> = Once::new();

pub fn fb<'a>() -> IrqMutexGuard<'a, FrameBuffer> {
    FRAMEBUFFER.get().unwrap().lock()
}

pub fn render_text_buf() {
    fb().clear_pixels();
    fb().render_text_buf();
}

#[macro_export]
macro_rules! fb_print {
    ($($arg:tt)*) => ({
        $crate::framebuffer::write_fmt(format_args!($($arg)*));
    });
}

#[macro_export]
macro_rules! fb_println {
    () => ($crate::fb_print!("\n"));
    ($($arg:tt)*) => ($crate::fb_print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn write_fmt(args: core::fmt::Arguments) {
    use core::fmt::Write;
    if let Some(fb) = FRAMEBUFFER.get() {
        fb.lock().write_fmt(args).unwrap();
        render_text_buf();
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FramebufferInfo {
    pub base: VirtAddr,
    pub size_bytes: usize,
    pub width: usize,
    pub height: usize,
    pub bpp: usize,
}

pub static FRAMEBUFFER_INFO: Once<FramebufferInfo> = Once::new();

pub fn init() {
    let Some(FramebufferInfo {
        base,
        size_bytes,
        width,
        height,
        bpp,
    }) = FRAMEBUFFER_INFO.get().copied()
    else {
        return;
    };

    let framebuf = FrameBuffer {
        start_addr: base,
        size_bytes,
        width,
        height,
        bpp,
        text_buf: Box::new([[None; TEXT_BUFFER_WIDTH]; TEXT_BUFFER_HEIGHT]),
        text_cursor_x: 0,
        text_cursor_y: 0,
        text_fgcolor: Color::WHITE,
    };

    log::debug!(
        "fb: 0x{:016x} .. 0x{:016x}",
        framebuf.start_addr,
        framebuf.start_addr.add_bytes(framebuf.size_bytes())
    );

    FRAMEBUFFER.call_once(|| IrqMutex::new(framebuf));

    fb().set_text_fgcolor_default();
    fb().clear_pixels();
    fb().clear_text();

    log::info!("Framebuffer resolution: {width}x{height}");
}
