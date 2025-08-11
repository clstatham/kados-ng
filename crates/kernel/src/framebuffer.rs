use core::ops::Add;

use alloc::boxed::Box;
use embedded_graphics::{
    Pixel,
    mono_font::{MonoFont, MonoTextStyle, ascii},
    prelude::{Size, *},
    text::Text,
};
use spin::Once;

use embedded_graphics::pixelcolor::Rgb888;

use crate::{
    arch::clean_data_cache, mem::units::VirtAddr, sync::IrqMutex, util::DebugCheckedPanic,
};

/// Represents a pixel color in the framebuffer.
pub type Color = Rgb888;

const FONT: MonoFont = ascii::FONT_10X20;

/// The width of the framebuffer's text buffer.
pub const TEXT_BUFFER_WIDTH: usize = 80;
/// The height of the framebuffer's text buffer.
pub const TEXT_BUFFER_HEIGHT: usize = 25;

/// A character in the framebuffer's text buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FbChar {
    char: u8,
    fg: Color,
}

impl FbChar {
    /// A default character for the framebuffer (a space with black foreground).
    pub const DEFAULT: Self = Self {
        char: b' ',
        fg: Color::BLACK,
    };

    /// Creates a new [`FbChar`] with the given character and foreground color.
    #[must_use]
    pub fn new(char: u8, fg: Color) -> Self {
        Self { char, fg }
    }

    /// Converts the [`FbChar`] to a [`Text`] object for rendering.
    #[must_use]
    pub fn as_text(
        &'_ self,
        top_left: Point,
        x: usize,
        y: usize,
    ) -> Text<'_, MonoTextStyle<'_, Color>> {
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

/// Represents a framebuffer for rendering graphics and text.
#[derive(Debug)]
pub struct FrameBuffer {
    start_addr: VirtAddr,
    size_bytes: usize,
    width: usize,
    height: usize,
    bpp: usize,
    back_buffer: Box<[u32]>,
    text_buf: Box<[[Option<FbChar>; TEXT_BUFFER_WIDTH]]>, // TEXT_BUFFER_WIDTH x TEXT_BUFFER_HEIGHT
    text_cursor_x: usize,
    text_cursor_y: usize,
    text_fgcolor: Color,
}

impl FrameBuffer {
    /// Returns the width of the framebuffer in pixels.
    #[must_use]
    pub fn width(&self) -> usize {
        self.width
    }

    /// Returns the height of the framebuffer in pixels.
    #[must_use]
    pub fn height(&self) -> usize {
        self.height
    }

    /// Returns the number of bits per pixel.
    #[must_use]
    pub fn bpp(&self) -> usize {
        self.bpp
    }

    /// Returns the area of the framebuffer in pixels.
    #[must_use]
    pub fn size_pixels(&self) -> usize {
        self.width * self.height
    }

    /// Returns the size of the framebuffer in bytes.
    #[must_use]
    pub fn size_bytes(&self) -> usize {
        self.size_bytes
    }

    /// Sets the foreground color for text rendering.
    pub fn set_text_fgcolor(&mut self, color: Color) {
        self.text_fgcolor = color;
    }

    /// Sets the foreground color for text rendering to the default color (white).
    pub fn set_text_fgcolor_default(&mut self) {
        self.text_fgcolor = Color::WHITE;
    }

    /// Renders the text buffer to the framebuffer.
    pub fn render_text_buf(&mut self) {
        for line in 0..TEXT_BUFFER_HEIGHT {
            for col in 0..TEXT_BUFFER_WIDTH {
                if let Some(ch) = self.text_buf[line][col] {
                    let text = ch.as_text(self.bounding_box().top_left, col, line);
                    text.draw(self).ok();
                }
            }
        }
    }

    /// Clears the framebuffer by filling it with black pixels.
    pub fn clear_pixels(&mut self) {
        self.clear(Color::BLACK).debug_checked_unwrap(); // should never fail
    }

    /// Returns a mutable slice of the framebuffer's pixel data.
    pub fn frame_mut(&mut self) -> &mut [u32] {
        unsafe {
            core::slice::from_raw_parts_mut(
                self.start_addr.as_raw_ptr_mut(),
                self.size_bytes() / size_of::<u32>(),
            )
        }
    }

    /// Writes a single byte to the framebuffer's text buffer at the current cursor position.
    /// The cursor position is updated accordingly, wrapping to the next line if necessary.
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

    /// Sets a pixel at the given coordinates to the specified color.
    pub fn set_pixel(&mut self, x: usize, y: usize, color: Color) {
        if x >= self.width || y >= self.height {
            return;
        }

        self.back_buffer[x + y * self.width] = color.into_storage();
    }

    /// Sets a pixel at the given coordinates to the specified raw color value.
    ///
    /// This writes to raw memory, whereas [`set_pixel`](FrameBuffer::set_pixel)
    /// actually writes to the framebuffer's back buffer.
    pub fn set_pixel_raw(&mut self, x: usize, y: usize, color: u32) {
        let offset = x + y * self.width;
        if offset > self.size_bytes / size_of::<u32>() {
            return;
        }
        unsafe {
            let ptr = self.frame_mut().as_mut_ptr().add(offset);
            ptr.write(color);
            clean_data_cache(ptr.cast(), size_of::<u32>());
        }
    }

    /// Copies the back buffer to the framebuffer, making the changes visible.
    pub fn present(&mut self) {
        unsafe {
            core::ptr::copy_nonoverlapping(
                self.back_buffer.as_ptr(),
                self.frame_mut().as_mut_ptr(),
                self.size_bytes() / size_of::<u32>(),
            );
            clean_data_cache(self.frame_mut().as_mut_ptr().cast(), self.size_bytes());
        }
    }

    #[allow(clippy::unused_self)]
    fn cursor_color_hook(&mut self) {}

    /// Backspaces the last character in the text buffer.
    ///
    /// Note that this does not wrap backwards to the previous line.
    pub fn backspace(&mut self) {
        let row = self.text_cursor_y;
        let col = self.text_cursor_x.saturating_sub(1);
        self.text_buf[row][col] = None;
        self.text_cursor_x = col;
        self.cursor_color_hook();
    }

    /// Writes a string to the framebuffer's text buffer at the current cursor position.
    pub fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            self.write_byte(byte);
        }
    }

    /// Advances the cursor to the next line in the text buffer.
    /// If the cursor is already at the last line, it scrolls the text buffer up.
    /// The cursor is reset to the beginning of the new line.
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

    /// Clears the specified row in the text buffer.
    pub fn clear_row(&mut self, row: usize) {
        for col in 0..TEXT_BUFFER_WIDTH {
            self.text_buf[row][col] = None;
        }
        self.cursor_color_hook();
    }

    /// Clears the text buffer from the current cursor position to the end of the text buffer.
    pub fn clear_until_end(&mut self) {
        for col in self.text_cursor_x..TEXT_BUFFER_WIDTH {
            self.text_buf[self.text_cursor_y][col] = None;
        }
        for row in self.text_cursor_y + 1..TEXT_BUFFER_HEIGHT {
            self.clear_row(row);
        }
        self.cursor_color_hook();
    }

    /// Clears the text buffer from the beginning of the text buffer to the current cursor position.
    pub fn clear_until_beginning(&mut self) {
        for col in 0..self.text_cursor_x {
            self.text_buf[self.text_cursor_y][col] = None;
        }
        for row in 0..self.text_cursor_y - 1 {
            self.clear_row(row);
        }
        self.cursor_color_hook();
    }

    /// Clears the text buffer from the current cursor position to the end of the line.
    pub fn clear_until_eol(&mut self) {
        for col in self.text_cursor_x..TEXT_BUFFER_WIDTH {
            self.text_buf[self.text_cursor_y][col] = None;
        }
        self.cursor_color_hook();
    }

    /// Clears the text buffer from the beginning of the line to the current cursor position.
    pub fn clear_from_bol(&mut self) {
        for col in 0..self.text_cursor_x {
            self.text_buf[self.text_cursor_y][col] = None;
        }
        self.cursor_color_hook();
    }

    /// Clears the current line in the text buffer.
    pub fn clear_line(&mut self) {
        self.clear_row(self.text_cursor_y);
    }

    /// Clears the entire text buffer.
    pub fn clear_text(&mut self) {
        for row in 0..TEXT_BUFFER_HEIGHT {
            self.clear_row(row);
        }
        self.cursor_color_hook();
    }

    /// Moves the text cursor up by one line, if possible.
    pub fn move_up(&mut self) {
        let new_y = self.text_cursor_y.saturating_sub(1);
        self.text_cursor_y = new_y;
        self.cursor_color_hook();
    }

    /// Moves the text cursor down by one line, if possible.
    pub fn move_down(&mut self) {
        let new_y = self.text_cursor_y.add(1).min(TEXT_BUFFER_HEIGHT - 1);
        self.text_cursor_y = new_y;
        self.cursor_color_hook();
    }

    /// Moves the text cursor to the left by one character, if possible.
    pub fn move_left(&mut self) {
        self.text_cursor_x = self.text_cursor_x.saturating_sub(1);
        self.cursor_color_hook();
    }

    /// Moves the text cursor to the right by one character, if possible.
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
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(coord, color) in pixels {
            let (x, y) = coord.into();
            if (0..self.width as i32).contains(&x) && (0..self.height as i32).contains(&y) {
                self.set_pixel(x as usize, y as usize, color);
            }
        }

        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        let color = color.into_storage();
        self.back_buffer.fill(color);

        Ok(())
    }
}

impl OriginDimensions for FrameBuffer {
    fn size(&self) -> Size {
        Size::new(self.width as u32, self.height as u32)
    }
}

/// A global framebuffer instance, protected by an IRQ mutex.
pub static FRAMEBUFFER: Once<IrqMutex<FrameBuffer>> = Once::new();

/// Runs the provided function with the framebuffer locked.
pub fn with_fb<R>(f: impl FnOnce(&mut FrameBuffer) -> R) -> Option<R> {
    let fb = FRAMEBUFFER.get()?;
    let result = {
        let mut fb = fb.try_lock().ok()?;
        f(&mut fb)
    };
    Some(result)
}

/// Prints a formatted string to the framebuffer's text buffer.
#[macro_export]
macro_rules! fb_print {
    ($($arg:tt)*) => ({
        $crate::framebuffer::write_fmt(format_args!($($arg)*));
    });
}

/// Prints a formatted string to the framebuffer's text buffer, followed by a newline.
#[macro_export]
macro_rules! fb_println {
    () => ($crate::fb_print!("\n"));
    ($($arg:tt)*) => ($crate::fb_print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn write_fmt(args: core::fmt::Arguments) {
    use core::fmt::Write;
    with_fb(|fb| {
        fb.write_fmt(args).ok();
        fb.clear_pixels();
        fb.render_text_buf();
        fb.present();
    });
}

/// Information about the framebuffer.
#[derive(Debug, Clone, Copy)]
pub struct FramebufferInfo {
    pub start_addr: VirtAddr,
    pub size_bytes: usize,
    pub width: usize,
    pub height: usize,
    pub bpp: usize,
}

/// A static reference to the framebuffer information, set by the kernel during device initialization.
pub static FRAMEBUFFER_INFO: Once<FramebufferInfo> = Once::new();

/// Initializes the global [`FRAMEBUFFER`] from the predefined [`FRAMEBUFFER_INFO`].
pub fn init() {
    let Some(FramebufferInfo {
        start_addr,
        size_bytes,
        width,
        height,
        bpp,
    }) = FRAMEBUFFER_INFO.get().copied()
    else {
        return;
    };

    let mut framebuf = FrameBuffer {
        start_addr,
        size_bytes,
        width,
        height,
        bpp,
        back_buffer: alloc::vec![0; size_bytes / size_of::<u32>()].into_boxed_slice(),
        text_buf: alloc::vec![[None; TEXT_BUFFER_WIDTH]; TEXT_BUFFER_HEIGHT].into_boxed_slice(),
        text_cursor_x: 0,
        text_cursor_y: 0,
        text_fgcolor: Color::WHITE,
    };

    log::debug!(
        "fb: 0x{:016x} .. 0x{:016x}",
        framebuf.start_addr,
        framebuf.start_addr.add_bytes(framebuf.size_bytes())
    );

    framebuf.clear_pixels();
    framebuf.clear_text();
    framebuf.set_text_fgcolor_default();
    framebuf.render_text_buf();
    framebuf.present();

    FRAMEBUFFER.call_once(|| IrqMutex::new(framebuf));

    log::info!("Framebuffer resolution: {width}x{height}");
}
