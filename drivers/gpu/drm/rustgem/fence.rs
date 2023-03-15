// SPDX-License-Identifier: MIT

use core::fmt::Write;
use core::ops::Deref;
use core::time::Duration;
use kernel::dma_fence::*;
use kernel::prelude::*;
use kernel::sync::Arc;
use kernel::time::timer::*;
use kernel::time::*;
use kernel::{bindings, c_str, timer_init};

static QUEUE_NAME: &CStr = c_str!("vgem_fence");
static QUEUE_CLASS_KEY: kernel::sync::LockClassKey = kernel::sync::LockClassKey::new();

pub(crate) struct Fence {}

#[vtable]
impl FenceOps for Fence {
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

pub(crate) struct VgemFence {
    fence: Arc<UniqueFence<Fence>>,
    _timer: Box<FnTimer<Box<dyn FnMut() -> Result<Next> + Sync>>>,
}

impl VgemFence {
    pub(crate) fn create() -> Result<Self> {
        let fence_ctx = FenceContexts::new(1, QUEUE_NAME, &QUEUE_CLASS_KEY)?;
        let fence = Arc::try_new(fence_ctx.new_fence(0, Fence {})?)?;

        // SAFETY: The caller calls [`FnTimer::init_timer`] before using the timer.
        let t = Box::try_new(unsafe {
            FnTimer::new(Box::try_new({
                let fence = fence.clone();
                move || {
                    let _ = fence.signal();
                    Ok(Next::Done)
                }
            })? as Box<_>)
        })?;

        // SAFETY: As FnTimer is inside a Box, it won't be moved.
        let ptr = unsafe { core::pin::Pin::new_unchecked(&*t) };

        timer_init!(ptr, 0, "vgem_timer");

        // SAFETY: Duration.as_millis() returns a valid total number of whole milliseconds.
        let timeout =
            unsafe { bindings::msecs_to_jiffies(Duration::from_secs(10).as_millis().try_into()?) };

        // We force the fence to expire within 10s to prevent driver hangs
        ptr.raw_timer().schedule_at(jiffies_later(timeout));

        Ok(Self { fence, _timer: t })
    }
}

impl Deref for VgemFence {
    type Target = UniqueFence<Fence>;

    fn deref(&self) -> &Self::Target {
        &*self.fence
    }
}
