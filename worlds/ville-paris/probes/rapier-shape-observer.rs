// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Measurement exports for the exact Rapier rlib linked into the inspected release.
use rapier3d::prelude::{Collider, ColliderBuilder};

#[unsafe(no_mangle)]
pub extern "C" fn osage_builder_trimesh(b: &ColliderBuilder) -> usize {
    b.shape.as_trimesh().map_or(0, |m| m as *const _ as usize)
}
#[unsafe(no_mangle)]
pub extern "C" fn osage_builder_trimesh_bytes(b: &ColliderBuilder) -> usize {
    b.shape.as_trimesh().map_or(0, |m| m.total_memory_size())
}
#[unsafe(no_mangle)]
pub extern "C" fn osage_installed_trimesh(c: &Collider) -> usize {
    c.shape().as_trimesh().map_or(0, |m| m as *const _ as usize)
}
#[unsafe(no_mangle)]
pub extern "C" fn osage_installed_trimesh_bytes(c: &Collider) -> usize {
    c.shape().as_trimesh().map_or(0, |m| m.total_memory_size())
}
