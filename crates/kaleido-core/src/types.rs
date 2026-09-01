//! Foundational types shared across the whole data model:
//! geometry ([`Point`], [`Size`], [`Transform2D`]), color ([`Color`]),
//! blending ([`BlendMode`]) and the stable ID types
//! ([`NodeId`], [`DocumentId`], [`ResourceId`], [`EffectId`]).

// ---------------------------------------------------------------------------
// Stable IDs
// ---------------------------------------------------------------------------

/// Stable handle for a node in the [`crate::scene::Scene`] tree.
///
/// Implemented as a monotonically increasing counter so that deletion of a
/// node never invalidates other nodes' handles (slotmap-style semantics).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct NodeId(pub u64);

/// Stable handle for a [`crate::document::Document`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct DocumentId(pub u64);

/// Handle referencing an asset held by the (global) resource manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct ResourceId(pub u64);

/// Handle referencing a plugin-registered effect implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct EffectId(pub u64);

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

/// A 2D point in document space (f32, resolution independent).
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    #[inline]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// A 2D size in abstract units.
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    #[inline]
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

/// Canvas dimensions in integer pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ImageSize {
    pub width: u32,
    pub height: u32,
}

impl ImageSize {
    #[inline]
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Number of pixels in the canvas.
    #[inline]
    pub const fn pixel_count(self) -> u64 {
        self.width as u64 * self.height as u64
    }
}

// ---------------------------------------------------------------------------
// Color
// ---------------------------------------------------------------------------

/// A linear RGBA color with float channels (0.0 – 1.0).
///
/// Float channels are the canonical representation for the render pipeline
/// (blending, gradients, vector fills) and for 16f/32f pixel formats.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    #[inline]
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Fully transparent black — the default "empty" color.
    #[inline]
    pub const fn transparent() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0)
    }

    /// Opaque black.
    #[inline]
    pub const fn black() -> Self {
        Self::new(0.0, 0.0, 0.0, 1.0)
    }

    /// Opaque white.
    #[inline]
    pub const fn white() -> Self {
        Self::new(1.0, 1.0, 1.0, 1.0)
    }

    /// Converts from an 8-bit RGBA pixel (0–255 channels).
    #[inline]
    pub fn from_rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::new(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        )
    }

    /// Returns a copy with the given alpha.
    #[inline]
    pub fn with_alpha(self, a: f32) -> Self {
        Self::new(self.r, self.g, self.b, a)
    }

    /// Component-wise linear interpolation toward `other` at progress
    /// `t` (0.0 = self, 1.0 = other).  Drives animated colors.
    #[inline]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self::new(
            self.r + (other.r - self.r) * t,
            self.g + (other.g - self.g) * t,
            self.b + (other.b - self.b) * t,
            self.a + (other.a - self.a) * t,
        )
    }
}

impl From<crate::pixel::Pixel> for Color {
    #[inline]
    fn from(p: crate::pixel::Pixel) -> Self {
        Self::from_rgba8(p.r, p.g, p.b, p.a)
    }
}

// ---------------------------------------------------------------------------
// Transform
// ---------------------------------------------------------------------------

/// Affine-ish node transform, decomposed for animation-friendliness.
///
/// Applied as: translate → rotate → scale.  Kept decomposed (instead of a raw
/// matrix) because the animation timeline drives these components directly.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Transform2D {
    /// Translation in document units.
    pub tx: f32,
    pub ty: f32,
    /// Rotation in radians (counter-clockwise).
    pub rotation: f32,
    /// Non-uniform scale.
    pub sx: f32,
    pub sy: f32,
}

impl Default for Transform2D {
    fn default() -> Self {
        Self {
            tx: 0.0,
            ty: 0.0,
            rotation: 0.0,
            sx: 1.0,
            sy: 1.0,
        }
    }
}

impl Transform2D {
    /// The identity transform.
    #[inline]
    pub const fn identity() -> Self {
        Self {
            tx: 0.0,
            ty: 0.0,
            rotation: 0.0,
            sx: 1.0,
            sy: 1.0,
        }
    }

    /// A pure translation.
    #[inline]
    pub const fn translation(tx: f32, ty: f32) -> Self {
        Self {
            tx,
            ty,
            rotation: 0.0,
            sx: 1.0,
            sy: 1.0,
        }
    }

    /// Returns a copy with the given translation.
    #[inline]
    pub const fn with_translation(self, tx: f32, ty: f32) -> Self {
        Self { tx, ty, ..self }
    }

    /// Returns a copy with the given rotation (radians, counter-clockwise).
    #[inline]
    pub const fn with_rotation(self, rotation: f32) -> Self {
        Self { rotation, ..self }
    }

    /// Returns a copy with the given (non-uniform) scale.
    #[inline]
    pub const fn with_scale(self, sx: f32, sy: f32) -> Self {
        Self { sx, sy, ..self }
    }

    /// Applies the transform to a point: `translate → rotate → scale`.
    pub fn transform_point(&self, p: Point) -> Point {
        // Scale first, then rotate, then translate.
        let sx = p.x * self.sx;
        let sy = p.y * self.sy;
        let (sin, cos) = self.rotation.sin_cos();
        Point::new(
            sx * cos - sy * sin + self.tx,
            sx * sin + sy * cos + self.ty,
        )
    }
}

// ---------------------------------------------------------------------------
// Blending
// ---------------------------------------------------------------------------

/// Layer blend mode — the host defines the core set; plugin effects may add
/// their own compositing operators on top of these.
///
/// This is the **single** blend-mode definition for the whole project; the
/// service layer re-exports it (it used to carry a second, 12-variant copy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl Default for BlendMode {
    fn default() -> Self {
        Self::Normal
    }
}
