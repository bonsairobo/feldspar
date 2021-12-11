use crate::{SdfChunk, PaletteIdChunk, ChunkShape, CHUNK_SHAPE_IVEC3};

use ilattice::glam::IVec3;
use ilattice::prelude::Extent;
use ndshape::ConstShape;
use std::mem;

pub struct OctantKernel {
    strides: [usize; 8],
    mode_counter: OctantModeCounter,
}

impl Default for OctantKernel {
    fn default() -> Self {
        Self::new()
    }
}

impl OctantKernel {
    // TODO: would be nice for this to be const, but it requires const trait methods
    pub fn new() -> Self {
        let mut strides = [0; 8];
        let mut i = 0;
        for z in [0, 1] {
            for y in [0, 1] {
                for x in [0, 1] {
                    strides[i] = ChunkShape::linearize([x, y, z]) as usize;
                    i += 1;
                }
            }
        }

        Self { strides, mode_counter: OctantModeCounter::default() }
    }

    /// Takes the **mean** of each octant in `src` to achieve half resolution; result is written to `dst`.
    pub fn downsample_sdf(&self, src: &SdfChunk, dst_offset: usize, dst: &mut SdfChunk) {
        // Not only do we get the mean signed distance value by dividing by the octant volume, but we also re-normalize by
        // dividing by 2.
        const RESCALE: f32 = 1.0 / (2.0 * 8.0);

        let iter_extent = Extent::from_min_and_shape(IVec3::ZERO, CHUNK_SHAPE_IVEC3 >> 1);
        for p in iter_extent.iter3() {
            let dst_i = ChunkShape::linearize(p.to_array()) as usize;
            let src_i = dst_i << 1;

            let mut sum = 0.0;
            for stride in self.strides {
                sum += f32::from(src[src_i + stride]);
            }
            dst[dst_offset + dst_i] = (sum * RESCALE).into();
        }
    }

    /// Takes the **mode** of each octant to achieve half resolution.
    pub fn downsample_labels(&mut self, src: &PaletteIdChunk, dst_offset: usize, dst: &mut PaletteIdChunk) {
        let iter_extent = Extent::from_min_and_shape(IVec3::ZERO, CHUNK_SHAPE_IVEC3 >> 1);
        for p in iter_extent.iter3() {
            let dst_i = ChunkShape::linearize(p.to_array()) as usize;
            let src_i = dst_i << 1;

            for stride in self.strides {
                self.mode_counter.add(src[src_i + stride]);
            }
            dst[dst_offset + dst_i] = self.mode_counter.get_mode_and_reset().label;
        }
    }
}

type Label = u8;

type Slot = u8;
const NULL_SLOT: Slot = Slot::MAX;

/// Counts occurrences of [`Label`]s in a population of exactly 8. Calculates the mode in linear time while only scanning an
/// array of 8 elements.
///
/// Attempts to use more than 8 [`Label`]s per counter will panic.
struct OctantModeCounter {
    counts: [Option<LabelCount>; 8],
    /// An array map determines which count slot, if any, each label is assigned.
    label_to_slot: [Slot; 256],
    /// Should only go up to 8!
    slots_vended: Slot,
}

impl Default for OctantModeCounter {
    fn default() -> Self {
        Self {
            counts: [None; 8],
            label_to_slot: [NULL_SLOT; 256],
            slots_vended: 0,
        }
    }
}

impl OctantModeCounter {
    pub fn add(&mut self, label: Label) {
        let label_i = label as usize;

        let mut slot = self.label_to_slot[label_i];
        if slot == NULL_SLOT {
            slot = self.slots_vended;
            self.label_to_slot[label_i] = slot;
            self.counts[slot as usize] = Some(LabelCount { label, count: 0 });
            self.slots_vended += 1;
        }

        self.counts[slot as usize].as_mut().unwrap().count += 1;
    }

    pub fn get_mode_and_reset(&mut self) -> LabelCount {
        let old_counts = mem::take(&mut self.counts);
        let mut max_count = 0;
        let mut max_elem = None;
        for elem in old_counts.into_iter().flatten() {
            self.label_to_slot[elem.label as usize] = NULL_SLOT;
            if elem.count > max_count {
                max_count = elem.count;
                max_elem = Some(elem);
            }
        }
        max_elem.unwrap()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LabelCount {
    pub count: usize,
    pub label: Label,
}

// ████████╗███████╗███████╗████████╗
// ╚══██╔══╝██╔════╝██╔════╝╚══██╔══╝
//    ██║   █████╗  ███████╗   ██║
//    ██║   ██╔══╝  ╚════██║   ██║
//    ██║   ███████╗███████║   ██║
//    ╚═╝   ╚══════╝╚══════╝   ╚═╝

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_label_is_mode() {
        let mut counter = OctantModeCounter::default();
        counter.add(1);

        assert_eq!(
            counter.counts,
            [
                Some(LabelCount { label: 1, count: 1 }),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ]);
        assert_eq!(counter.get_mode_and_reset(), LabelCount { label: 1, count: 1 });
    }

    #[test]
    fn single_label_twice_is_mode_with_count_two() {
        let mut counter = OctantModeCounter::default();
        counter.add(1);
        counter.add(1);

        assert_eq!(
            counter.counts,
            [
                Some(LabelCount { label: 1, count: 2 }),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ]);
            assert_eq!(counter.get_mode_and_reset(), LabelCount { label: 1, count: 2 });
    }

    #[test]
    fn two_labels_tie_for_mode() {
        let mut counter = OctantModeCounter::default();
        counter.add(1);
        counter.add(0);

        assert_eq!(
            counter.counts,
            [
                Some(LabelCount { label: 1, count: 1 }),
                Some(LabelCount { label: 0, count: 1 }),
                None,
                None,
                None,
                None,
                None,
                None,
            ]);
        assert_eq!(counter.get_mode_and_reset(), LabelCount { label: 1, count: 1 });
    }

    #[test]
    fn many_labels() {
        let mut counter = OctantModeCounter::default();
        for label in [1, 8, 2, 4, 4, 4, 3, 3, 3, 3] {
            counter.add(label);
        }
        assert_eq!(
            counter.counts,
            [
                Some(LabelCount { label: 1, count: 1 }),
                Some(LabelCount { label: 8, count: 1 }),
                Some(LabelCount { label: 2, count: 1 }),
                Some(LabelCount { label: 4, count: 3 }),
                Some(LabelCount { label: 3, count: 4 }),
                None,
                None,
                None,
            ]);
        assert_eq!(counter.get_mode_and_reset(), LabelCount { label: 3, count: 4 });
    }
}
