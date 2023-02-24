// SPDX-License-Identifier: MIT

use crate::fence::VgemFence;
use crate::gem::DriverObject;
use crate::{VgemDevice, VgemDriver};
use core::ops::Deref;
use kernel::dma_fence::RawDmaFence;
use kernel::drm::gem::BaseObject;
use kernel::prelude::*;
use kernel::{bindings, drm, drm::gem::shmem, xarray};

pub(crate) struct File {
    fences: xarray::XArray<Box<Option<VgemFence>>>,
}

/// Convenience type alias for our DRM `File` type.
pub(crate) type DrmFile = drm::file::File<File>;

impl drm::file::DriverFile for File {
    type Driver = VgemDriver;

    fn open(_device: &VgemDevice) -> Result<Box<Self>> {
        Ok(Box::try_new(Self {
            fences: xarray::XArray::new(xarray::flags::ALLOC1)?,
        })?)
    }
}

impl File {
    pub(crate) fn dummy(
        _device: &VgemDevice,
        _data: &mut bindings::drm_vgem_dummy,
        _file: &DrmFile,
    ) -> Result<u32> {
        Err(EINVAL)
    }

    /// vgem_fence_attach_ioctl (DRM_IOCTL_VGEM_FENCE_ATTACH):
    ///
    /// Create and attach a fence to the vGEM handle. This fence is then exposed
    /// via the dma-buf reservation object and visible to consumers of the exported
    /// dma-buf.
    ///
    /// This returns the handle for the new fence that must be signaled within 10
    /// seconds (or otherwise it will automatically expire). See
    /// signal (DRM_IOCTL_VGEM_FENCE_SIGNAL).
    ///
    /// If the vGEM handle does not exist, attach returns -ENOENT.
    ///
    pub(crate) fn attach(
        _device: &VgemDevice,
        data: &mut bindings::drm_vgem_fence_attach,
        file: &DrmFile,
    ) -> Result<u32> {
        if (data.flags & !bindings::VGEM_FENCE_WRITE) != 0 {
            return Err(EINVAL);
        }

        if data.pad != 0 {
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

        let usage = if (data.flags & bindings::VGEM_FENCE_WRITE) != 0 {
            bindings::dma_resv_usage_DMA_RESV_USAGE_WRITE
        } else {
            bindings::dma_resv_usage_DMA_RESV_USAGE_READ
        };

        // Expose the fence via the dma-buf
        if resv.add_fences(fence.deref(), 1, usage).is_ok() {
            // Record the fence in our xarray for later signaling
            if let Ok(id) = file.fences.alloc(Some(Box::try_new(Some(fence))?)) {
                data.out_fence = id as u32
            }
        } else {
            fence.signal()?;
        }

        Ok(0)
    }

    /// vgem_fence_signal_ioctl (DRM_IOCTL_VGEM_FENCE_SIGNAL):
    ///
    /// Signal and consume a fence earlier attached to a vGEM handle using
    /// attach (DRM_IOCTL_VGEM_FENCE_ATTACH).
    ///
    /// All fences must be signaled within 10s of attachment or otherwise they
    /// will automatically expire (and signal returns -ETIMEDOUT).
    ///
    /// Signaling a fence indicates to all consumers of the dma-buf that the
    /// client has completed the operation associated with the fence, and that the
    /// buffer is then ready for consumption.
    ///
    /// If the fence does not exist (or has already been signaled by the client),
    /// signal returns -ENOENT.
    ///
    pub(crate) fn signal(
        _device: &VgemDevice,
        data: &mut bindings::drm_vgem_fence_signal,
        file: &DrmFile,
    ) -> Result<u32> {
        if data.flags != 0 {
            return Err(EINVAL);
        }

        let fence = file
            .fences
            .replace(data.fence as usize, Box::try_new(None)?);

        let fence = match fence {
            Err(ret) => {
                return Err(ret);
            }
            Ok(None) => {
                return Err(ENOENT);
            }
            Ok(fence) => {
                let fence = fence.unwrap().unwrap();

                if fence.is_signaled() {
                    return Err(ETIMEDOUT);
                }

                fence
            }
        };

        fence.signal()?;
        Ok(0)
    }
}
