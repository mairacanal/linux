// SPDX-License-Identifier: MIT

use core::fmt::Write;
use kernel::c_str;
use kernel::dma_fence::*;
use kernel::prelude::*;

#[derive(Default)]
pub(crate) struct VgemFence {}

static QUEUE_NAME: &CStr = c_str!("vgem_fence");
static QUEUE_CLASS_KEY: kernel::sync::LockClassKey = kernel::sync::LockClassKey::new();

#[vtable]
impl FenceOps for VgemFence {
    const USE_64BIT_SEQNO: bool = false;

    fn get_driver_name<'a>(self: &'a FenceObject<Self>) -> &'a CStr {
        c_str!("vgem")
    }

    fn get_timeline_name<'a>(self: &'a FenceObject<Self>) -> &'a CStr {
        c_str!("unbound")
    }

    fn fence_value_str(self: &FenceObject<Self>, output: &mut dyn Write) {
        let _ = output.write_fmt(format_args!("{}", self.seqno()));
    }

    fn timeline_value_str(self: &FenceObject<Self>, output: &mut dyn Write) {
        let value = if self.is_signaled() { self.seqno() } else { 0 };
        let _ = output.write_fmt(format_args!("{}", value));
    }
}

impl VgemFence {
    pub(crate) fn create() -> Result<UniqueFence<Self>> {
        let fence_ctx = FenceContexts::new(1, QUEUE_NAME, &QUEUE_CLASS_KEY)?;
        Ok(fence_ctx.new_fence(0, Default::default())?)
    }
}
