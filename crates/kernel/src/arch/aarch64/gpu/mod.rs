use core::arch::asm;

use bitflags::bitflags;
use derive_more::{Deref, DerefMut, TryFrom};
use fdt::Fdt;
use thiserror::Error;

use crate::{
    arch::ArchTrait,
    dtb::{Phandle, get_mmio_addr},
    framebuffer::FramebufferInfo,
    mem::{
        paging::{
            allocator::KernelFrameAllocator,
            table::{PageFlags, PageTable, TableKind},
        },
        units::{FrameCount, PhysAddr},
    },
    syscall::errno::Errno,
};

use super::{AArch64, mmio::Mmio};
use props::*;

pub mod props;

// from config.txt
pub const FRAMEBUFFER_WIDTH: usize = 640;
pub const FRAMEBUFFER_HEIGHT: usize = 480;

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
    pub fn encode(buffer: *mut MailboxBuffer, channel: MailboxChannel) -> Self {
        let addr: u32 = (buffer as usize - crate::HHDM_PHYSICAL_OFFSET)
            .try_into()
            .unwrap_or_else(|_| panic!("{} >= u32::MAX", buffer as usize));
        assert_eq!(addr & 0b1111, 0, "buffer is not aligned to 16 bytes");
        Self(addr | (channel as u32))
    }

    pub fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    pub fn decode(self) -> *const MailboxBuffer {
        (self.payload() as usize + crate::HHDM_PHYSICAL_OFFSET) as *const MailboxBuffer
    }

    pub fn channel(&self) -> MailboxChannel {
        MailboxChannel::try_from((self.0 & 0b1111) as u32).unwrap()
    }

    pub fn payload(&self) -> u32 {
        self.0 & !0b1111
    }

    pub fn raw(&self) -> u32 {
        self.0
    }
}

pub trait MailboxProperty: Sized {
    const TAG: u32;
    type Response: Sized;
    fn encode_request(self, request: MailboxRequest) -> MailboxRequest;
    fn decode_response(response: &[u32]) -> Option<Self::Response>;
}

pub const MAX_PROPS: usize = AArch64::PAGE_SIZE / size_of::<u32>();

#[derive(Deref, DerefMut)]
#[repr(C, align(16))]
pub struct MailboxBuffer {
    pub props: [u32; MAX_PROPS],
}

impl MailboxBuffer {
    pub const SIZE_IDX: usize = 0;
    pub const CODE_IDX: usize = 1;

    pub fn buffer_size(&self) -> u32 {
        self.props[Self::SIZE_IDX] >> 2
    }

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
        let frame = unsafe {
            KernelFrameAllocator.allocate(FrameCount::from_bytes(size_of::<MailboxBuffer>()))
        }
        .unwrap();
        let mut mapper = PageTable::current(TableKind::Kernel);
        mapper
            .kernel_map_range(
                frame.as_identity_virt(),
                frame,
                size_of::<MailboxBuffer>(),
                PageFlags::new_device(),
            )
            .unwrap()
            .flush();

        MailboxRequest {
            buf: frame.as_hhdm_virt().as_raw_ptr_mut(),
            index: 2,
        }
    }

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

    pub fn finish(self) -> *mut MailboxBuffer {
        unsafe {
            (&mut *self.buf)[MailboxBuffer::SIZE_IDX] = (self.index + 1) << 2; // add 1 for the zero-tag at the end
            (&mut *self.buf)[MailboxBuffer::CODE_IDX] = 0; // request
            self.buf
        }
    }
}

pub struct MailboxResponse {
    buf: *const MailboxBuffer,
}

impl MailboxResponse {
    pub fn decode<T: MailboxProperty>(&mut self) -> Option<T::Response> {
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
}

impl Drop for MailboxResponse {
    fn drop(&mut self) {
        let addr = PhysAddr::new_canonical(self.buf as usize - crate::HHDM_PHYSICAL_OFFSET);
        KernelFrameAllocator
            .free(addr, FrameCount::from_bytes(size_of::<MailboxBuffer>()))
            .unwrap();
    }
}

#[derive(Debug)]
pub struct Mailbox {
    pub phandle: Phandle,
    pub base: Mmio<u32>,
}

impl Mailbox {
    const READ: usize = 0x00;
    const STATUS: usize = 0x18;
    const WRITE: usize = 0x20;

    pub fn parse(fdt: &Fdt) -> Result<Self, Errno> {
        let mbox = fdt.find_compatible(&["brcm,bcm2835-mbox"]).unwrap();
        let phandle = mbox.property("phandle").unwrap().as_usize().unwrap() as u32;
        let mut regions = mbox.reg().unwrap();
        let region = regions.next().unwrap();
        assert!(regions.next().is_none());
        let mmio_addr = get_mmio_addr(fdt, &region).unwrap();

        Ok(Self {
            phandle: Phandle(phandle),
            base: Mmio::new(mmio_addr.as_hhdm_virt()),
        })
    }

    pub fn status(&self) -> MailboxStatus {
        MailboxStatus::from_bits_truncate(unsafe { self.base.read(Self::STATUS) })
    }

    pub unsafe fn call(
        &mut self,
        request: MailboxRequest,
        channel: MailboxChannel,
    ) -> Result<MailboxResponse, MailboxError> {
        let buf = request.finish();
        let message = MailboxMessage::encode(buf, channel);

        // send it along
        while self.status().contains(MailboxStatus::MAILBOX_FULL) {
            core::hint::spin_loop();
        }
        unsafe { self.base.write(Self::WRITE, message.raw()) };

        // wait for response
        let resp = loop {
            while self.status().contains(MailboxStatus::MAILBOX_EMPTY) {
                core::hint::spin_loop();
            }
            let resp = unsafe { self.base.read(Self::READ) };
            let resp = MailboxMessage::from_raw(resp);
            if resp.channel() == channel && resp.payload() == message.payload() {
                break resp;
            }
        };

        unsafe { asm!("dsb sy; isb") }

        let buf = resp.decode();
        assert!(!buf.is_null());
        let code = unsafe { (*buf).request_code() };
        let response = MailboxResponse { buf };

        if code & 0x80000000 == 0x80000000 {
            Ok(response)
        } else {
            Err(MailboxError)
        }
    }
}

pub fn init(fdt: &Fdt) {
    let mut mbox = Mailbox::parse(fdt).unwrap();
    log::debug!("mailbox @ {}", mbox.base.addr);

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
        .encode(SetDepth { bpp: 32 })
        .encode(AllocateBuffer {
            align: AArch64::PAGE_SIZE as u32,
        })
        .encode(GetPitch {})
        .encode(GetPhysicalSize {})
        .encode(GetDepth {});

    let mut response = unsafe { mbox.call(request, MailboxChannel::TagsArmToVc).unwrap() };
    let rev = response.decode::<GetFirmwareRevision>().unwrap();
    log::debug!("firmware revision: {:#x}", rev.revision);
    let buffer = response.decode::<AllocateBuffer>().unwrap();
    log::debug!(
        "buffer: 0x{:016x} .. 0x{:016x}",
        buffer.base_addr,
        buffer.base_addr + buffer.size
    );
    let phys_size = response.decode::<GetPhysicalSize>().unwrap();
    log::debug!("physical size = {}x{}", phys_size.width, phys_size.height);
    let pitch = response.decode::<GetPitch>().unwrap();
    log::debug!("pitch = {}", pitch.pitch);
    let depth = response.decode::<GetDepth>().unwrap();
    log::debug!("depth = {}", depth.depth);

    // map it
    let mut mapper = PageTable::current(TableKind::Kernel);
    let frame = PhysAddr::new_canonical(buffer.base_addr as usize);
    let page = frame.as_hhdm_virt();
    mapper
        .kernel_map_range(page, frame, buffer.size as usize, PageFlags::new_device())
        .unwrap()
        .flush();

    crate::framebuffer::init(FramebufferInfo {
        base: page.value(),
        size_bytes: buffer.size as usize,
        width: phys_size.width as usize,
        height: phys_size.height as usize,
        bpp: depth.depth as usize,
    });
}
