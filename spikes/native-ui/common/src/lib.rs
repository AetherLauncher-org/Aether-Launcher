//! What both toolkit spikes share: the same data, the same form logic, the same cube and the
//! same measurements, so the only thing that differs between them is the toolkit.
//!
//! The toolkits use different `wgpu` versions (iced 0.14: 27, egui 0.37: 30), so nothing here
//! touches `wgpu` types: this crate holds the mesh, the matrices and the WGSL source as plain
//! data and each spike builds its own pipeline from it.

pub mod bench;
pub mod cube;
pub mod host;
pub mod model;
pub mod stats;
