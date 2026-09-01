//! Format codecs — one file per format category, plus a registry.
//!
//! # Architecture
//!
//! ```text
//! DataService.open("photo.png")
//!         │
//!         ▼
//! FormatRegistry.load(path)
//!         │
//!         ├── find codec for ".png" → RasterCodec
//!         └── codec.load(path) → TiledImage
//! ```
//!
//! # File layout
//!
//! | File | Formats | Read | Write |
//! |------|---------|------|-------|
//! | `raster.rs` | PNG, JPEG, WebP, TIFF | ✅ | ✅ |
//! | `bmp.rs` | BMP | ✅ | ❌ |
//! | `gif.rs` | GIF | ✅ | ❌ |
//! | `registry.rs` | All formats | — | — |

pub mod bmp;
pub mod gif;
pub mod raster;
pub mod registry;

pub use registry::FormatRegistry;

/// Re-export the capability struct for use in format files.
pub use kaleido_traits::data::codec::CodecCapability;
