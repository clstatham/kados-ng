use super::gpu::{Mailbox, MailboxChannel, MailboxRequest, props::*};

pub fn init(mbox: &mut Mailbox) {
    let request = MailboxRequest::new().encode(GetPowerState { device_id: 9 });
    let resp = unsafe { mbox.call(request, MailboxChannel::TagsArmToVc).unwrap() };
    let ps9 = resp.decode::<GetPowerState>().unwrap();
    log::debug!(
        "Device 9 exists = {}, power = {}",
        ps9.state & 0x1 != 0,
        ps9.state & 0x2 != 0
    );
    let request = resp.recycle().encode(GetPowerState { device_id: 10 });
    let resp = unsafe { mbox.call(request, MailboxChannel::TagsArmToVc).unwrap() };
    let ps10 = resp.decode::<GetPowerState>().unwrap();
    log::debug!(
        "Device 10 exists = {}, power = {}",
        ps10.state & 0x1 != 0,
        ps10.state & 0x2 != 0
    );
}
