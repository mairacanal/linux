// SPDX-License-Identifier: MIT

use kernel::{
    drm::{gem, gem::shmem},
    error::Result,
};

use crate::file::DrmFile;
use crate::{VgemDevice, VgemDriver};

/// Represents the inner data of a GEM object for this driver.
pub(crate) struct DriverObject {}

/// Type alias for the shmem GEM object type for this driver.
pub(crate) type Object = shmem::Object<DriverObject>;

impl gem::BaseDriverObject<Object> for DriverObject {
    /// Callback to create the inner data of a GEM object
    fn new(_dev: &VgemDevice, _size: usize) -> Result<Self> {
        Ok(Self {})
    }

    /// Callback to drop all mappings for a GEM object owned by a given `File`
    fn close(_obj: &Object, _file: &DrmFile) {}
}

impl shmem::DriverObject for DriverObject {
    type Driver = VgemDriver;

    const MAP_WC: bool = true;
}
