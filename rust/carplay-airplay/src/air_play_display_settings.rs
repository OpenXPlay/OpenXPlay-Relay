// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::*;
/// Source configuration utilities used by /info, without an Android settings UI.
pub struct AirPlayDisplaySettings {
    _private: (),
}
/// Axis used to compute the receiver's advertised physical display size.
#[derive(Clone, Copy, Debug)]
pub enum AirPlayPhysicalSizeBasis {
    Width,
    Height,
}
/// Physical size of the primary display, in millimeters.
#[derive(Clone, Debug)]
pub struct AirPlayPhysicalSizeMm {
    /// Horizontal size.
    pub width_mm: u32,
    /// Vertical size.
    pub height_mm: u32,
}
impl AirPlayDisplaySettings {
    /// Bound and snap FPS to the advertised values.
    pub fn sanitize_fps(value: u32) -> u32 {
        ((value.clamp(MIN_FPS, MAX_FPS) - MIN_FPS + FPS_STEP / 2) / FPS_STEP) * FPS_STEP + MIN_FPS
    }
    /// Bound and snap the configured physical width.
    pub fn sanitize_width_physical_mm(value: u32) -> u32 {
        ((value.clamp(MIN_WIDTH_PHYSICAL_MM, MAX_WIDTH_PHYSICAL_MM) - MIN_WIDTH_PHYSICAL_MM
            + WIDTH_PHYSICAL_MM_STEP / 2)
            / WIDTH_PHYSICAL_MM_STEP)
            * WIDTH_PHYSICAL_MM_STEP
            + MIN_WIDTH_PHYSICAL_MM
    }
    /// Clamp a physical size advertised in /info.
    pub fn sanitize_reported_physical_mm(value: u32) -> u32 {
        value.clamp(MIN_REPORTED_PHYSICAL_MM, MAX_REPORTED_PHYSICAL_MM)
    }
    /// Compute physical dimensions from aspect ratio and the selected axis.
    pub fn resolve_physical_size_mm(
        width_pixels: u32,
        height_pixels: u32,
        physical_mm: u32,
        basis: AirPlayPhysicalSizeBasis,
    ) -> AirPlayPhysicalSizeMm {
        let (w, h) = match basis {
            AirPlayPhysicalSizeBasis::Width => (
                physical_mm.max(1) as f64,
                physical_mm.max(1) as f64 * height_pixels as f64 / width_pixels.max(1) as f64,
            ),
            AirPlayPhysicalSizeBasis::Height => (
                physical_mm.max(1) as f64 * width_pixels as f64 / height_pixels.max(1) as f64,
                physical_mm.max(1) as f64,
            ),
        };
        AirPlayPhysicalSizeMm {
            width_mm: Self::sanitize_reported_physical_mm(w.round() as u32),
            height_mm: Self::sanitize_reported_physical_mm(h.round() as u32),
        }
    }
}
/// Display configuration constant.
pub const MIN_FPS: u32 = 30;
/// Display configuration constant.
pub const MAX_FPS: u32 = 60;
/// Display configuration constant.
pub const FPS_STEP: u32 = 5;
/// Display configuration constant.
pub const MIN_WIDTH_PHYSICAL_MM: u32 = 100;
/// Display configuration constant.
pub const MAX_WIDTH_PHYSICAL_MM: u32 = 400;
/// Display configuration constant.
pub const WIDTH_PHYSICAL_MM_STEP: u32 = 25;
/// Display configuration constant.
pub const DEFAULT_WIDTH_PHYSICAL_MM: u32 = 200;
/// Display configuration constant.
pub const MIN_REPORTED_PHYSICAL_MM: u32 = 1;
/// Display configuration constant.
pub const MAX_REPORTED_PHYSICAL_MM: u32 = 2_000;
/// Default frame rate in frames per second.
pub const DEFAULT_FPS: u32 = MAX_FPS;

impl AirPlayDisplaySettings {
    pub fn resolve_physical_size_mm_with_maximum(
        current_width_pixels: u32,
        current_height_pixels: u32,
        maximum_width_pixels: u32,
        maximum_height_pixels: u32,
        reference_millimeters: u32,
        basis: AirPlayPhysicalSizeBasis,
    ) -> Result<AirPlayPhysicalSizeMm> {
        if current_width_pixels == 0 || current_height_pixels == 0 {
            return Err(std::io::Error::other(
                "Current display dimensions must be positive",
            ));
        }
        if maximum_width_pixels == 0 || maximum_height_pixels == 0 {
            return Err(std::io::Error::other(
                "Maximum display dimensions must be positive",
            ));
        }
        let scale = match basis {
            AirPlayPhysicalSizeBasis::Width => {
                current_width_pixels as f64 / maximum_width_pixels as f64
            }
            AirPlayPhysicalSizeBasis::Height => {
                current_height_pixels as f64 / maximum_height_pixels as f64
            }
        };
        let reference = reference_millimeters.max(1) as f64 * scale;
        let (width, height) = match basis {
            AirPlayPhysicalSizeBasis::Width => (
                reference,
                reference * current_height_pixels as f64 / current_width_pixels as f64,
            ),
            AirPlayPhysicalSizeBasis::Height => (
                reference * current_width_pixels as f64 / current_height_pixels as f64,
                reference,
            ),
        };
        Ok(AirPlayPhysicalSizeMm {
            width_mm: Self::sanitize_reported_physical_mm(width.round() as u32),
            height_mm: Self::sanitize_reported_physical_mm(height.round() as u32),
        })
    }
}
