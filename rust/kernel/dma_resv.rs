// SPDX-License-Identifier: GPL-2.0 OR MIT

//! DMA resv abstraction
//!
//! C header: [`include/linux/dma-resv.h`](../../include/linux/dma-resv.h)

use crate::bindings;
use crate::dma_fence::RawDmaFence;
use crate::error::{Error, Result};

/// A generic DMA Resv Object
///
/// # Invariants
/// ptr is a valid pointer to a dma_resv and we own a reference to it.
pub struct DmaResv {
    ptr: *mut bindings::dma_resv,
}

impl DmaResv {
    /// Create a new DmaResv object from a raw pointer to a dma_resv.
    ///
    /// # Safety
    /// The caller must own a reference to the dma_resv, which is transferred to the new object.
    pub unsafe fn from_raw(ptr: *mut bindings::dma_resv) -> Self {
        Self { ptr }
    }

    /// Returns the implicit synchronization usage for write or read accesses.
    pub fn usage_rw(&self, write: bool) -> bindings::dma_resv_usage {
        // SAFETY: write is a valid bool.
        unsafe { bindings::dma_resv_usage_rw(write) }
    }

    /// Reserve space to add fences to a dma_resv object.
    pub fn reserve_fences(&self, num_fences: u32) -> Result {
        // SAFETY: We own a reference to this dma_resv.
        let ret = unsafe { bindings::dma_resv_reserve_fences(self.ptr, num_fences) };

        if ret != 0 {
            return Err(Error::from_kernel_errno(ret));
        }
        Ok(())
    }

    /// Add a fence to the dma_resv object
    pub fn add_fences(
        &self,
        fence: &dyn RawDmaFence,
        num_fences: u32,
        usage: bindings::dma_resv_usage,
    ) -> Result {
        // SAFETY: We own a reference to this dma_resv.
        unsafe { bindings::dma_resv_lock(self.ptr, core::ptr::null_mut()) };

        let ret = self.reserve_fences(num_fences);
        if ret.is_ok() {
            // SAFETY: ptr is locked with dma_resv_lock(), and dma_resv_reserve_fences()
            // has been called.
            unsafe {
                bindings::dma_resv_add_fence(self.ptr, fence.raw(), usage);
            }
        }

        // SAFETY: We own a reference to this dma_resv.
        unsafe { bindings::dma_resv_unlock(self.ptr) };

        ret
    }

    /// Test if a reservation object’s fences have been signaled.
    pub fn test_signaled(&self, usage: bindings::dma_resv_usage) -> bool {
        // SAFETY: We own a reference to this dma_resv.
        unsafe { bindings::dma_resv_test_signaled(self.ptr, usage) }
    }
}
