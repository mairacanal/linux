// SPDX-License-Identifier: MIT

use core::fmt::Write;
use core::ops::Deref;
use core::time::Duration;
use kernel::c_str;
use kernel::dma_fence::*;
use kernel::prelude::*;
use kernel::timer::{Timer, TimerOps, UniqueTimer};

#[derive(Default)]
pub(crate) struct VgemFenceOps {}

pub(crate) struct VgemFence {
    timer: Timer<VgemFenceOps, UniqueFence<VgemFenceOps>>,
}

static QUEUE_NAME: &CStr = c_str!("vgem_fence");
static QUEUE_CLASS_KEY: kernel::sync::LockClassKey = kernel::sync::LockClassKey::new();

#[vtable]
impl FenceOps for VgemFenceOps {
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

impl TimerOps for VgemFenceOps {
    type Inner = UniqueFence<Self>;

    fn timer_callback(timer: &UniqueTimer<Self, UniqueFence<Self>>) {
        let _ = timer.inner().signal();
    }
}

impl VgemFence {
    pub(crate) fn create() -> Result<VgemFence> {
        let fence_ctx = FenceContexts::new(1, QUEUE_NAME, &QUEUE_CLASS_KEY)?;
        let fence = fence_ctx.new_fence(0, Default::default())?;

        let timer = Timer::<VgemFenceOps, UniqueFence<VgemFenceOps>>::setup(fence);

        // We force the fence to expire within 10s to prevent driver hangs
        timer.modify(Duration::from_secs(10));

        Ok(VgemFence { timer })
    }
}

impl Deref for VgemFence {
    type Target = UniqueFence<VgemFenceOps>;

    fn deref(&self) -> &Self::Target {
        self.timer.inner()
    }
}
