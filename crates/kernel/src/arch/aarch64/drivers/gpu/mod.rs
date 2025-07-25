use core::{arch::asm, fmt::Debug};

use bitflags::bitflags;
use derive_more::{Deref, DerefMut, TryFrom};
use fdt::Fdt;
use thiserror::Error;

use crate::{
    arch::{Architecture, clean_data_cache, invalidate_data_cache},
    fdt::{Phandle, get_mmio_addr},
    framebuffer::FramebufferInfo,
    mem::{
        paging::table::{PageFlags, PageTable, TableKind},
        units::{PhysAddr, VirtAddr},
    },
    syscall::errno::Errno,
    util::{DebugCheckedPanic, DebugPanic},
};

use crate::arch::Arch;
use props::{
    AllocateBuffer, GetDepth, GetFirmwareRevision, GetPhysicalSize, GetPitch, SetDepth,
    SetPhysicalSize, SetPixelOrder, SetVirtualSize,
};

use super::{dma_alloc, dma_free};

pub mod props;

// from config.txt
pub const FRAMEBUFFER_WIDTH: usize = 1280;
pub const FRAMEBUFFER_HEIGHT: usize = 720;

bitflags! {
    pub struct MailboxStatus: u32 {
        const MAILBOX_EMPTY = 1 << 30;
        const MAILBOX_FULL = 1 << 31;
    }
}

#[derive(TryFrom, PartialEq, Clone, Copy, Debug)]
#[try_from(repr)]
#[repr(u32)]
pub enum MailboxChannel {
    PowerManagement = 0,
    FrameBuffer,
    VirtualUart,
    Vchiq,
    Leds,
    Buttons,
    Touchscreen,
    Unused,
    TagsArmToVc,
    TagsVcToArm,
}

#[derive(Debug, Error)]
#[error("Mailbox status not OK")]
pub struct MailboxError;

#[repr(transparent)]
pub struct MailboxMessage(u32);

impl MailboxMessage {
    pub fn encode(
        buffer: *mut MailboxBuffer,
        channel: MailboxChannel,
    ) -> Result<Self, MailboxError> {
        let addr = u32::try_from(buffer as usize - crate::HHDM_PHYSICAL_OFFSET)
            .map_err(|_| MailboxError)
            .debug_expect("Mailbox buffer address is not a valid HHDM physical address")?;
        debug_assert_eq!(addr & 0b1111, 0, "buffer is not aligned to 16 bytes");
        Ok(Self(addr | (channel as u32)))
    }

    #[must_use]
    pub fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    #[must_use]
    pub fn decode(self) -> *mut MailboxBuffer {
        ((self.payload() as usize) + crate::HHDM_PHYSICAL_OFFSET) as *mut MailboxBuffer
    }

    #[must_use]
    pub fn channel(&self) -> MailboxChannel {
        MailboxChannel::try_from((self.0 & 0b1111) as u32)
            .debug_checked_expect("Invalid mailbox channel")
    }

    #[must_use]
    pub fn payload(&self) -> u32 {
        self.0 & !0b1111
    }

    #[must_use]
    pub fn raw(&self) -> u32 {
        self.0
    }
}

impl Debug for MailboxMessage {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "MailboxMessage {{ payload: 0x{:016x}, channel: {:?} }}",
            self.payload(),
            self.channel()
        )
    }
}

pub trait MailboxProperty: Sized {
    const TAG: u32;
    type Response: Sized;
    fn encode_request(self, request: MailboxRequest) -> MailboxRequest;
    fn decode_response(response: &[u32]) -> Option<Self::Response>;
}

pub const MAX_PROPS: usize = Arch::PAGE_SIZE / size_of::<u32>();

#[derive(Deref, DerefMut)]
#[repr(C, align(16))]
pub struct MailboxBuffer {
    pub props: [u32; MAX_PROPS],
}

impl MailboxBuffer {
    pub const SIZE_IDX: usize = 0;
    pub const CODE_IDX: usize = 1;

    #[must_use]
    pub fn buffer_size(&self) -> u32 {
        self.props[Self::SIZE_IDX] >> 2
    }

    #[must_use]
    pub fn request_code(&self) -> u32 {
        self.props[Self::CODE_IDX]
    }
}

#[must_use = "call `finish()` to finalize the request"]
pub struct MailboxRequest {
    buf: *mut MailboxBuffer,
    index: u32,
}

impl MailboxRequest {
    pub fn new() -> MailboxRequest {
        let buf = dma_alloc::<MailboxBuffer>();

        MailboxRequest { buf, index: 2 }
    }

    #[allow(clippy::cast_possible_truncation)]
    pub fn encode<T: MailboxProperty>(self, prop: T) -> Self {
        let mut this = self;
        this = this.int(T::TAG);
        let max_size = usize::max(size_of::<T>(), size_of::<T::Response>());

        this = this.int(max_size as u32);
        this = this.int(0); // request
        let start = this.index as usize;
        this = prop.encode_request(this);
        while (this.index as usize) < start + (max_size >> 2) {
            this = this.int(0); // add placeholders
        }
        this
    }

    pub fn int(mut self, prop: u32) -> Self {
        (unsafe { &mut *self.buf })[self.index as usize] = prop;
        self.index += 1;
        self
    }

    pub fn skip(mut self, n: u32) -> Self {
        self.index += n;
        self
    }

    #[must_use = "this will leak memory if the buffer is not consumed"]
    pub fn finish(self) -> *mut MailboxBuffer {
        unsafe {
            (&mut *self.buf)[MailboxBuffer::SIZE_IDX] = (self.index + 1) << 2; // add 1 for the zero-tag at the end
            (&mut *self.buf)[MailboxBuffer::CODE_IDX] = 0; // request
            let this = self.int(0); // end tag
            this.buf
        }
    }
}

pub struct MailboxResponse {
    buf: *mut MailboxBuffer,
}

impl MailboxResponse {
    #[must_use]
    pub fn decode<T: MailboxProperty>(&self) -> Option<T::Response> {
        let buf = unsafe { &*self.buf };
        let size = buf.buffer_size() as usize;
        let mut i = 2;
        while i < size {
            let prop_size = (buf[i + 1] >> 2) as usize;

            if buf[i] == T::TAG {
                return T::decode_response(&buf[i + 3..i + 3 + prop_size]);
            }

            i += prop_size + 3;
        }

        None
    }

    pub fn recycle(self) -> MailboxRequest {
        let buf = self.buf;
        unsafe {
            (*buf).fill(0);
        }
        MailboxRequest { buf, index: 2 }
    }
}

impl Drop for MailboxResponse {
    fn drop(&mut self) {
        dma_free(self.buf);
    }
}

#[derive(Debug)]
pub struct Mailbox {
    pub phandle: Phandle,
    pub base: VirtAddr,
}

impl Mailbox {
    const READ: usize = 0x00;
    const STATUS: usize = 0x18;
    const WRITE: usize = 0x20;

    /// Parses the mailbox from the FDT.
    pub fn parse(fdt: &Fdt) -> Result<Self, Errno> {
        let Some(mbox) = fdt.find_compatible(&["brcm,bcm2835-mbox"]) else {
            return Err(Errno::EINVAL);
        };

        let Some(phandle) = mbox.property("phandle") else {
            return Err(Errno::EINVAL);
        };

        let Some(phandle) = phandle.as_usize() else {
            return Err(Errno::EINVAL);
        };

        let Ok(phandle) = u32::try_from(phandle) else {
            return Err(Errno::EINVAL);
        };

        let Some(region) = mbox.reg().and_then(|mut r| r.next()) else {
            return Err(Errno::EINVAL);
        };

        let Some(mmio_addr) = get_mmio_addr(fdt, &region) else {
            return Err(Errno::EINVAL);
        };

        Ok(Self {
            phandle: Phandle::from(phandle),
            base: mmio_addr.as_hhdm_virt(),
        })
    }

    /// Returns the status of the mailbox.
    ///
    /// # Panics
    ///
    /// This function will panic if the read operation fails.
    #[must_use]
    pub fn status(&self) -> MailboxStatus {
        MailboxStatus::from_bits_truncate(unsafe {
            self.base.add_bytes(Self::STATUS).read_volatile().unwrap()
        })
    }

    /// Calls the mailbox with a request and channel, returning the response.
    ///
    /// # Safety
    ///
    /// This function is unsafe because it directly interacts with hardware and assumes that the mailbox is correctly configured.
    ///
    /// # Returns
    ///
    /// Returns `Ok(MailboxResponse)` if the call was successful, or `Err(MailboxError)` if there was an error.
    ///
    /// # Panics
    ///
    /// This function will panic if the mailbox is full or if MMIO operations fail.
    pub unsafe fn call(
        &mut self,
        request: MailboxRequest,
        channel: MailboxChannel,
    ) -> Result<MailboxResponse, MailboxError> {
        let buf = request.finish();
        let Ok(message) = MailboxMessage::encode(buf, channel) else {
            // don't leak memory
            dma_free(buf);
            return Err(MailboxError);
        };

        unsafe {
            asm!("dsb ishst");
            clean_data_cache(buf.cast(), (*buf).buffer_size() as usize * size_of::<u32>());
            asm!("dsb ish; isb");
        }

        // send it along
        while self.status().contains(MailboxStatus::MAILBOX_FULL) {
            core::hint::spin_loop();
        }
        unsafe {
            self.base
                .add_bytes(Self::WRITE)
                .write_volatile(message.raw())
                .unwrap();
        };

        // wait for response
        let resp = loop {
            while self.status().contains(MailboxStatus::MAILBOX_EMPTY) {
                core::hint::spin_loop();
            }
            let resp = unsafe { self.base.add_bytes(Self::READ).read_volatile().unwrap() };
            let resp = MailboxMessage::from_raw(resp);
            if resp.channel() == message.channel() && resp.payload() == message.payload() {
                break resp;
            }
        };

        let buf = resp.decode();

        unsafe {
            asm!("dsb ish; isb");
            invalidate_data_cache(buf.cast(), (*buf).buffer_size() as usize * size_of::<u32>());
        }

        let code = unsafe { (*buf).request_code() };
        let response = MailboxResponse { buf };

        if code & 0x8000_0000 == 0x8000_0000 {
            Ok(response)
        } else {
            Err(MailboxError)
        }
    }
}

/// Initializes the GPU framebuffer.
///
/// # Panics
///
/// This function will panic if the mailbox call fails or if the framebuffer cannot be initialized.
pub fn init(fdt: &Fdt) {
    let mut mbox = Mailbox::parse(fdt).unwrap();
    log::debug!("mailbox @ {}", mbox.base);

    let request = MailboxRequest::new()
        .encode(GetFirmwareRevision {})
        .encode(SetPhysicalSize {
            width: FRAMEBUFFER_WIDTH as u32,
            height: FRAMEBUFFER_HEIGHT as u32,
        })
        .encode(SetVirtualSize {
            width: FRAMEBUFFER_WIDTH as u32,
            height: FRAMEBUFFER_HEIGHT as u32,
        })
        .encode(SetPixelOrder { order: 0x0 }) // BGR
        .encode(SetDepth { bpp: 32 })
        .encode(AllocateBuffer { align: 0 })
        .encode(GetPitch {})
        .encode(GetPhysicalSize {})
        .encode(GetDepth {});

    let response = unsafe { mbox.call(request, MailboxChannel::TagsArmToVc).unwrap() };
    let rev = response.decode::<GetFirmwareRevision>().unwrap();
    log::debug!("firmware revision: {:#x}", rev.revision);
    let buffer = response.decode::<AllocateBuffer>().unwrap();
    let base_addr = buffer.bus_addr & 0x3FFF_FFFF;
    log::debug!(
        "buffer: 0x{:016x} .. 0x{:016x}",
        base_addr,
        base_addr + buffer.size
    );
    let phys_size = response.decode::<GetPhysicalSize>().unwrap();
    log::debug!("physical size = {}x{}", phys_size.width, phys_size.height);
    let pitch = response.decode::<GetPitch>().unwrap();
    log::debug!("pitch = {}", pitch.pitch);
    let depth = response.decode::<GetDepth>().unwrap();
    log::debug!("depth = {}", depth.depth);

    // map the framebuffer
    let mut mapper = PageTable::current(TableKind::Kernel);
    let frame = PhysAddr::new_canonical(base_addr as usize);
    let page = frame.as_hhdm_virt();
    let flush = mapper
        .kernel_map_range(
            page,
            frame,
            buffer.size as usize,
            PageFlags::new().writable(),
        )
        .unwrap();
    flush.flush();

    crate::framebuffer::FRAMEBUFFER_INFO.call_once(|| FramebufferInfo {
        base: page,
        size_bytes: buffer.size as usize,
        width: phys_size.width as usize,
        height: phys_size.height as usize,
        bpp: depth.depth as usize,
    });
}
