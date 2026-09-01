//! Color management — the document's working color configuration.
//!
//! The heavy lifting (conversions between spaces, ICC handling) lives in
//! the color-management service; this module only defines the data model.

use super::types::ResourceId;

/// Working color space of a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ColorSpace {
    /// Standard RGB with sRGB transfer function (default).
    SRgb,
    /// Linear-light RGB (rendering / HDR friendly).
    LinearRgb,
    /// CMYK — print workflow.
    Cmyk,
    /// CIE Lab.
    Lab,
}

/// The document-level color configuration.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ColorProfile {
    pub space: ColorSpace,
    /// Bits per channel: 8, 16 or 32 (float).
    pub bit_depth: u8,
    /// Optional embedded ICC profile (referenced from the resource manager).
    pub icc: Option<ResourceId>,
}

impl ColorProfile {
    /// A profile with the given working space and bit depth.
    #[inline]
    pub const fn new(space: ColorSpace, bit_depth: u8) -> Self {
        Self {
            space,
            bit_depth,
            icc: None,
        }
    }
}

impl Default for ColorProfile {
    fn default() -> Self {
        Self {
            space: ColorSpace::SRgb,
            bit_depth: 8,
            icc: None,
        }
    }
}
