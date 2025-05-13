use core::time::Duration;

use fdt::Fdt;

use crate::{
    arch::{AArch64, driver::DriverTrait, time::spin_for},
    dtb::{Irq, IrqHandlerTrait, get_interrupt, get_mmio_addr, irq_chip, register_irq},
    mem::units::VirtAddr,
    syscall::errno::Errno,
};

use super::{dma_alloc, mmio::Mmio};

pub fn init(fdt: &Fdt) {
    let node = fdt.find_compatible(&["brcm,bcm2708-usb"]).unwrap();
    let mut regions = node.reg().unwrap();
    let region = regions.next().unwrap();
    let mmio_addr = get_mmio_addr(fdt, &region).unwrap();
    let base = mmio_addr.as_hhdm_virt();

    let irq_data = get_interrupt(fdt, &node, 0).unwrap();
    let irq = unsafe { irq_chip().chip.translate_irq(irq_data).unwrap() };

    log::debug!("USB controller @ {} IRQ = {}", base, irq);
    let mut usb = UsbController::new(base);

    unsafe { usb.init(fdt).unwrap() };

    unsafe {
        register_irq(irq, usb);
    }
}

#[derive(Debug, Default)]
pub struct UsbController {
    base: Mmio<u32>,
}

impl UsbController {
    /* AHB configuration */
    const GAHBCFG: usize = 0x008;
    /* USB PHY & mode config */
    const GUSBCFG: usize = 0x00C;
    /* Core soft‑reset */
    const GRSTCTL: usize = 0x010;
    /* Interrupt status & mask */
    const GINTSTS: usize = 0x014;
    const GINTMSK: usize = 0x018;

    const GRXFSIZ: usize = 0x024;
    const GNPTXFSIZ: usize = 0x028;

    // Host‑mode registers
    /* Host configuration (frame clock select) */
    const HCFG: usize = 0x400;
    /* Frame interval (SOF) */
    const HFIR: usize = 0x404;
    /* Host port control */
    const HPRT0: usize = 0x440;

    // Host‑Channel 0 (control transfers on EP 0)
    /* Channel characteristics */
    const HCCHAR0: usize = 0x500;
    /* DMA address for SETUP packet */
    const HCDMA0: usize = 0x508;
    /* Transfer size & packet count */
    const HCTSIZ0: usize = 0x510;
    /* Channel‑0 interrupt mask */
    const HCINTMSK0: usize = 0x518;

    pub fn new(base: VirtAddr) -> Self {
        Self {
            base: Mmio::new(base),
        }
    }

    unsafe fn dwc2_irq_setup(&mut self) {
        unsafe {
            // 1) Globally enable DWC2 interrupts
            // write32(GAHBCFG, (1 << 0)     // GINTMSKEN
            //                | (1 << 4)     // NPTXFELVL
            //                | (1 << 5));   // PTXFELVL
            self.base
                .write(Self::GAHBCFG, (1 << 0) | (1 << 4) | (1 << 5));

            // 2) Unmask specific events: RXFLVL, PTXFEMP, NPTXFEMP, PRTINT, HCINT
            // write32(GINTMSK,
            //     (1 << 4)  // RXFLVL
            //   | (1 << 5)  // PTXFEMP
            //   | (1 << 7)  // NPTXFEMP
            //   | (1 << 24) // PRTINT (port change)
            //   | (1 << 25) // HCINT  (channel done)
            // );
            self.base.write(
                Self::GINTMSK,
                (1 << 4)  // RXFLVL
            | (1 << 5)  // PTXFEMP
            | (1 << 7)  // NPTXFEMP
            | (1 << 24) // PRTINT (port change)
            | (1 << 25), // HCINT  (channel done)
            );

            // 3) Clear any old pending interrupts
            // let pending = read32(GINTSTS);
            let pending = self.base.read(Self::GINTSTS);
            // write32(GINTSTS, pending);
            self.base.write(Self::GINTSTS, pending);
        }
    }

    unsafe fn dwc2_core_init(&mut self) {
        unsafe {
            // 1) Wait for AHB master idle (GRSTCTL.AHBIDL = bit 31)
            // while read32(GRSTCTL) & (1 << 31) == 0 { }
            self.base.spin_while_lo(Self::GRSTCTL, 1 << 31);

            // 2) Issue core soft‑reset (GRSTCTL.CSRST = bit 0)
            // write32(GRSTCTL, 1 << 0);
            self.base.write(Self::GRSTCTL, 1 << 0);
            // while read32(GRSTCTL) & (1 << 0) != 0 { }
            self.base.spin_while_hi(Self::GRSTCTL, 1 << 0);
            // delay_us(10);
            spin_for(Duration::from_micros(10));

            // 3) Select internal Full‑Speed PHY (GUSBCFG.PHYSEL = bit 6)
            // let mut usbcfg = read32(GUSBCFG);
            let mut usbcfg = self.base.read(Self::GUSBCFG);
            usbcfg |= 1 << 6;
            // write32(GUSBCFG, usbcfg);
            self.base.write(Self::GUSBCFG, usbcfg);
            // delay_us(100);
            spin_for(Duration::from_micros(100));

            // 4) Unmask the AHB master & global interrupts
            //    GAHBCFG: GINTMSKEN=bit 0, NPTXFELVL=bit 4, PTXFELVL=bit 5
            // write32(GAHBCFG, (1 << 0) | (1 << 4) | (1 << 5));
            self.base
                .write(Self::GAHBCFG, (1 << 0) | (1 << 4) | (1 << 5));

            // 5) Unmask port‑ and channel‑level IRQs in GINTMSK
            //    RXFLVL=4, PTXFEMP=5, NPTXFEMP=7, PRTINT=24, HCINT=25
            // write32(
            //     GINTMSK,
            //     (1 << 4) | (1 << 5) | (1 << 7) | (1 << 24) | (1 << 25),
            // );
            self.base.write(
                Self::GINTMSK,
                (1 << 4) | (1 << 5) | (1 << 7) | (1 << 24) | (1 << 25),
            );
        }
    }

    unsafe fn dwc2_set_host_mode(&mut self) {
        unsafe {
            self.base.write(Self::HCFG, 0b00);
            let v = self.base.read(Self::HCFG) & 0b11;
            log::debug!("DWC2 now in host mode, FSLSPclkSel = {:#b}", v);
        }
    }

    unsafe fn dwc2_host_port_init(&mut self) {
        unsafe {
            // 1) Program frame clock for Full‑Speed (HFIR = 48 MHz × 1 ms)
            // write32(HFIR, 48_000);
            self.base.write(Self::HFIR, 48_000);

            // 2) Power on port (HPRT0.PPWR = bit 12)
            // let mut hprt = read32(HPRT0);
            let hprt = self.base.read(Self::HPRT0);
            // write32(HPRT0, hprt | (1 << 12));
            self.base.write(Self::HPRT0, hprt | (1 << 12));
            // delay_ms(50);
            spin_for(Duration::from_millis(50));

            // 3) Port reset (HPRT0.PRST = bit 8)
            // write32(HPRT0, hprt | (1 << 8));
            self.base.write(Self::HPRT0, hprt | (1 << 8));
            // delay_ms(60);
            spin_for(Duration::from_millis(60));
            // write32(HPRT0, hprt & !(1 << 8));
            self.base.write(Self::HPRT0, hprt & !(1 << 8));
            // delay_ms(10);
            spin_for(Duration::from_millis(10));

            // 4) Allocate 1 KiB Rx FIFO & 512 B non‑periodic Tx FIFO
            //    GRXFSIZ @ 0x24, GNPTXFSIZ @ 0x28
            // write32(0x24, 256);
            self.base.write(Self::GRXFSIZ, 256);
            // write32(0x28, (128 << 16) | 128);
            self.base.write(Self::GNPTXFSIZ, (128 << 16) | 128);
        }
    }

    unsafe fn chan0_ctrl_setup(&mut self) {
        unsafe {
            static SETUP_DEV_DESC: [u8; 8] = [
                0x80, // bmRequestType: IN, Standard, Device
                0x06, // bRequest: GET_DESCRIPTOR
                0x01, 0x00, // wValue: DescriptorType=DEVICE (1) << 8
                0x00, 0x00, // wIndex: 0
                0x12, 0x00, // wLength: 18 bytes
            ];
            // 1) DMA buffer for SETUP
            // let buf = dma_alloc(8) as u32;
            // core::ptr::copy_nonoverlapping(SETUP_DEV_DESC.as_ptr(), buf as *mut u8, 8);
            let buf = dma_alloc(8);
            core::ptr::copy_nonoverlapping(SETUP_DEV_DESC.as_ptr(), buf, 8);

            // 2) HCCHAR0: DevAddr=0, Ep=0, Type=Control(0), MPS=8
            let hcchar = (0 << 0) | (0 << 11) | (0 << 18) | (8 << 0);
            // write32(HCCHAR0, hcchar);
            self.base.write(Self::HCCHAR0, hcchar);

            // 3) Point DMA → SETUP buffer
            // write32(HCDMA0, buf);
            self.base.write(Self::HCDMA0, buf as u32);

            // 4) HCTSIZ0: XferSize=8, PktCnt=1, SETUP=bit 29
            let hctsiz = (8 << 0) | (1 << 19) | (1 << 29);
            // write32(HCTSIZ0, hctsiz);
            self.base.write(Self::HCTSIZ0, hctsiz);

            // 5) Unmask channel 0 interrupt (Xfer complete = bit 0)
            // write32(HCINTMSK0, 1 << 0);
            self.base.write(Self::HCINTMSK0, 1 << 0);

            // 6) Start the transfer (CHENA = bit 31)
            // write32(HCCHAR0, hcchar | (1 << 31));
            self.base.write(Self::HCCHAR0, hcchar | (1 << 31));
        }
    }
}
impl DriverTrait for UsbController {
    type Arch = AArch64;
    const CONST_DEFAULT: Self = Self {
        base: Mmio::new(VirtAddr::NULL),
    };

    unsafe fn init(&mut self, _fdt: &fdt::Fdt) -> Result<(), Errno> {
        Ok(())
    }
}

impl IrqHandlerTrait for UsbController {
    fn post_register_hook(&mut self, _irq: Irq) {
        unsafe {
            self.dwc2_irq_setup();
            self.dwc2_core_init();
            self.dwc2_set_host_mode();
            self.dwc2_host_port_init();
            self.chan0_ctrl_setup();
        }
    }

    fn handle_irq(&mut self, irq: Irq) {
        log::debug!("IRQ {irq}");

        if unsafe { self.base.read(Self::GINTSTS) & (1 << 24) == 0 } {
            return;
        }

        // let hprt = unsafe { read32(HPRT0) };
        let hprt = unsafe { self.base.read(Self::HPRT0) };
        // Bits we will write back to clear: CONNDET=bit 2, ENACHG=17, OVRCUR=18
        let clr = hprt & !((1 << 2) | (1 << 17) | (1 << 18));

        // 1) Device Connected?
        if hprt & (1 << 2) != 0 {
            unsafe {
                // write32(HPRT0, clr | (1 << 2));
                self.base.write(Self::HPRT0, clr | (1 << 2)); // clear CONNDET
                // Start enumeration
                self.dwc2_core_init();
                self.dwc2_set_host_mode();
                self.dwc2_host_port_init();
                self.chan0_ctrl_setup();
            }

            // Clear the global interrupt bit by writing 1
            unsafe {
                // write32(GINTSTS, 1 << 24);
                self.base.write(Self::GINTSTS, 1 << 24);
            }

            return; // skip other events until SETUP completes
        }

        // 2) Port‑Enable Change?
        if hprt & (1 << 17) != 0 {
            unsafe {
                // write32(HPRT0, clr | (1 << 17));
                self.base.write(Self::HPRT0, clr | (1 << 17)); // clear ENACHG
            }
            // Port is now enabled → proceed to DATA‑IN stage, Set Address, etc.
            // ...
        }

        // 3) Over‑current?
        if hprt & (1 << 18) != 0 {
            unsafe {
                // write32(HPRT0, clr | (1 << 18));
                self.base.write(Self::HPRT0, clr | (1 << 18)); // clear OVRCUR
            }
            panic!("USB port over-current!");
        }

        // Clear the global interrupt bit by writing 1
        unsafe { self.base.write(Self::GINTSTS, 1 << 24) };
    }
}
