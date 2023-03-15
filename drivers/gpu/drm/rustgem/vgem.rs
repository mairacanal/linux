// SPDX-License-Identifier: MIT

//! Driver for a Virtual GEM service.

use kernel::driver::DeviceRemoval;
use kernel::macros::vtable;
use kernel::{
    c_str, device, drm,
    drm::{drv, ioctl},
    error::Result,
    platform,
    prelude::*,
    sync::Arc,
};

mod fence;
mod file;
mod gem;

/// Driver metadata
const INFO: drv::DriverInfo = drv::DriverInfo {
    major: 1,
    minor: 0,
    patchlevel: 0,
    name: c_str!("vgem"),
    desc: c_str!("Virtual GEM provider"),
    date: c_str!("20230201"),
};

struct Vgem {
    data: Arc<DeviceData>,
    _resource: device::Resource,
    _pdev: platform::Device,
}

/// Empty struct representing this driver.
pub(crate) struct VgemDriver;

/// Convenience type alias for the DRM device type for this driver.
pub(crate) type VgemDevice = kernel::drm::device::Device<VgemDriver>;

///// Convenience type alias for the `device::Data` type for this driver.
type DeviceData = device::Data<drv::Registration<VgemDriver>, (), ()>;

#[vtable]
impl drv::Driver for VgemDriver {
    /// Our `DeviceData` type, reference-counted
    type Data = Arc<DeviceData>;
    /// Our `File` type.
    type File = file::File;
    /// Our `Object` type.
    type Object = gem::Object;

    const INFO: drv::DriverInfo = INFO;
    const FEATURES: u32 = drv::FEAT_GEM | drv::FEAT_RENDER;

    kernel::declare_drm_ioctls! {
        (VGEM_DUMMY, drm_vgem_dummy, ioctl::RENDER_ALLOW, file::File::dummy),
        (VGEM_FENCE_ATTACH, drm_vgem_fence_attach, ioctl::RENDER_ALLOW, file::File::attach),
        (VGEM_FENCE_SIGNAL, drm_vgem_fence_signal, ioctl::RENDER_ALLOW, file::File::signal),
    }
}

impl kernel::Module for Vgem {
    fn init(_name: &'static CStr, _module: &'static ThisModule) -> Result<Self> {
        let mut pdev = platform::Device::register(c_str!("vgem"), -1)?;
        let dev = device::Device::from_dev(&pdev);

        let resource = dev.open_group(core::ptr::null_mut() as *mut core::ffi::c_void)?;

        pdev.coerse_dma_masks(u64::MAX)?;

        let reg = drm::drv::Registration::<VgemDriver>::new(&dev)?;

        let data = kernel::new_device_data!(reg, (), (), "Vgem::Registrations")?;

        let data = Arc::<DeviceData>::from(data);

        kernel::drm_device_register!(
            data.registrations().ok_or(ENXIO)?.as_pinned_mut(),
            data.clone(),
            0
        )?;

        Ok(Vgem {
            _pdev: pdev,
            _resource: resource,
            data,
        })
    }
}

impl Drop for Vgem {
    fn drop(&mut self) {
        self.data.device_remove();
    }
}

module! {
    type: Vgem,
    name: "vgem",
    author: "Maíra Canal",
    description: "Virtual GEM provider",
    license: "GPL",
}
