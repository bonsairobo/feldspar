use crate::prelude::{MaterialLayer, MaterialVoxel};

use building_blocks::storage::{prelude::Sd8, IsEmpty};
use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

/// Identifies the type of voxel.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct VoxelType(pub u8);

unsafe impl Zeroable for VoxelType {}
unsafe impl Pod for VoxelType {}

impl VoxelType {
    pub const EMPTY: Self = VoxelType(0);
}

pub const EMPTY_SIGNED_DISTANCE: Sd8 = Sd8::ONE;
pub const EMPTY_SDF_VOXEL: (VoxelType, Sd8) = (VoxelType::EMPTY, EMPTY_SIGNED_DISTANCE);

/// Metadata about a specific type of voxel.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct VoxelTypeInfo {
    pub is_empty: bool,
    pub material: VoxelMaterial,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct VoxelMaterial(pub u8);

impl VoxelMaterial {
    pub const NULL: Self = Self(std::u8::MAX);
}

impl IsEmpty for &VoxelTypeInfo {
    #[inline]
    fn is_empty(&self) -> bool {
        self.is_empty
    }
}

impl MaterialVoxel for &VoxelTypeInfo {
    #[inline]
    fn material(&self) -> MaterialLayer {
        MaterialLayer(self.material.0)
    }
}
