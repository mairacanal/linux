// SPDX-License-Identifier: MIT

use crate::fence::VgemFence;
use crate::gem::DriverObject;
use crate::{VgemDevice, VgemDriver};
use core::ops::Deref;
use kernel::dma_fence::RawDmaFence;
use kernel::drm::gem::BaseObject;
use kernel::prelude::*;
use kernel::{bindings, dma_fence::UniqueFence, drm, drm::gem::shmem, xarray};

pub(crate) struct File {
    fences: xarray::XArray<Box<Option<UniqueFence<VgemFence>>>>,
}

/// Convenience type alias for our DRM `File` type.
pub(crate) type DrmFile = drm::file::File<File>;

impl drm::file::DriverFile for File {
    type Driver = VgemDriver;

    fn open(_device: &VgemDevice) -> Result<Box<Self>> {
        pr_info!("Opening...");
        Ok(Box::try_new(Self {
            fences: xarray::XArray::new(xarray::flags::ALLOC1)?,
        })?)
    }
}

impl File {
    /// vgem_fence_attach_ioctl (DRM_IOCTL_VGEM_FENCE_ATTACH):
    ///
    /// Create and attach a fence to the vGEM handle. This fence is then exposed
    /// via the dma-buf reservation object and visible to consumers of the exported
    /// dma-buf.
    ///
    /// If the vGEM handle does not exist, vgem_fence_attach_ioctl returns -ENOENT.
    ///
    pub(crate) fn attach(
        _device: &VgemDevice,
        data: &mut bindings::drm_vgem_fence_attach,
        file: &DrmFile,
    ) -> Result<u32> {
        pr_info!(
            "attach: {{ handle: {}, flags: {}, out_fence: {}, pad: {} }}",
            data.handle,
            data.flags,
            data.out_fence,
            data.pad
        );

        if (data.flags & !bindings::VGEM_FENCE_WRITE) != 0 {
            pr_info!("attach: Returned EINVAL");
            return Err(EINVAL);
        }

        if data.pad != 0 {
            pr_info!("attach: Returned EINVAL");
            return Err(EINVAL);
        }

        let obj = shmem::Object::<DriverObject>::lookup_handle(file, data.handle)?;

        let fence = VgemFence::create()?;

        // Check for a conflicting fence
        let resv = obj.resv();
        let usage = resv.usage_rw(data.flags & bindings::VGEM_FENCE_WRITE != 0);
        if !resv.test_signaled(usage) {
            fence.signal()?;
            return Err(EBUSY);
        }

        // Expose the fence via the dma-buf
        let usage = if (data.flags & bindings::VGEM_FENCE_WRITE) != 0 {
            bindings::dma_resv_usage_DMA_RESV_USAGE_WRITE
        } else {
            bindings::dma_resv_usage_DMA_RESV_USAGE_READ
        };

        // Record the fence in our idr for later signaling
        if resv.add_fences(fence.deref(), 1, usage).is_ok() {
            if let Ok(id) = file.fences.alloc(Some(Box::try_new(Some(fence))?)) {
                data.out_fence = id as u32
            }
        } else {
            fence.signal()?;
        }

        pr_info!(
            "attach: Returned {{ handle: {}, flags: {}, out_fence: {}, pad: {} }}",
            data.handle,
            data.flags,
            data.out_fence,
            data.pad
        );
        pr_info!("attach: Returned 0");
        Ok(0)
    }

    /// vgem_fence_signal_ioctl (DRM_IOCTL_VGEM_FENCE_SIGNAL):
    ///
    /// Signal and consume a fence ealier attached to a vGEM handle using
    /// vgem_fence_attach_ioctl (DRM_IOCTL_VGEM_FENCE_ATTACH).
    ///
    /// Signaling a fence indicates to all consumers of the dma-buf that the
    /// client has completed the operation associated with the fence, and that the
    /// buffer is then ready for consumption.
    ///
    /// If the fence does not exist (or has already been signaled by the client),
    /// vgem_fence_signal_ioctl returns -ENOENT.
    ///
    pub(crate) fn signal(
        _device: &VgemDevice,
        data: &mut bindings::drm_vgem_fence_signal,
        file: &DrmFile,
    ) -> Result<u32> {
        pr_info!("signal: {{ fence: {}, flags: {} }}", data.fence, data.flags);
        if data.flags != 0 {
            pr_info!("signal: Returned EINVAL");
            return Err(EINVAL);
        }

        let fence = file
            .fences
            .replace(data.fence as usize, Box::try_new(None)?);

        let fence = match fence {
            Err(ret) => {
                pr_info!("signal: Returned PTR_ERR");
                return Err(ret);
            }
            Ok(None) => {
                pr_info!("signal: Returned ENOENT");
                return Err(ENOENT);
            }
            Ok(fence) => {
                let fence = fence.unwrap().unwrap();

                if fence.is_signaled() {
                    pr_info!("signal: Returned ETIMEDOUT");
                    return Err(ETIMEDOUT);
                }

                fence
            }
        };

        fence.signal()?;
        pr_info!("signal: Returned 0");
        Ok(0)
    }
}

impl Drop for File {
    fn drop(&mut self) {
        pr_info!("Closing...");
    }
}
