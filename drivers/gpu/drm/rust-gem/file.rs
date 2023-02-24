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
    pub(crate) fn mock(
        _device: &VgemDevice,
        _data: &mut bindings::drm_vgem_mock,
        _file: &DrmFile,
    ) -> Result<u32> {
        Ok(0)
    }

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
