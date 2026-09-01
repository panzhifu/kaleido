//! The [`Document`] — root aggregate of the whole data model.
//!
//! One document = one editing session.  It owns the scene graph, the active
//! selection, the undo history state, the animation timeline, the color
//! configuration and references into the global resource manager.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use super::color_profile::ColorProfile;
use super::error::{ImageError, ImageResult};
use super::format::{KldError, KldFormat};
use super::mask::SelectionMask;
use super::scene::Scene;
use super::timeline::Timeline;
use super::types::{DocumentId, ImageSize, ResourceId};

/// Current unix timestamp in seconds.
fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Resource references a document holds (fonts, swatches, brushes…).
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResourceRefs {
    pub fonts: Vec<ResourceId>,
    pub swatches: Vec<ResourceId>,
    pub brushes: Vec<ResourceId>,
}

/// Document-level metadata.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DocumentMeta {
    pub author: Option<String>,
    /// Unix seconds since epoch.
    pub created_at: i64,
    pub modified_at: i64,
    /// Free-form key/value properties (EXIF-ish).
    pub properties: HashMap<String, String>,
}

impl Default for DocumentMeta {
    fn default() -> Self {
        let now = now_secs();
        Self {
            author: None,
            created_at: now,
            modified_at: now,
            properties: HashMap::new(),
        }
    }
}

/// Undo/redo history state.
///
/// The host only stores the *container* here; the command implementation
/// (what a history entry actually does) lives in the history service.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HistoryState {
    pub undo_stack: Vec<HistoryEntry>,
    pub redo_stack: Vec<HistoryEntry>,
    /// Maximum entries kept on the undo stack before merging/dropping.
    pub limit: usize,
}

impl HistoryState {
    /// A history state with the given depth limit.
    #[inline]
    pub fn with_limit(limit: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            limit,
        }
    }
}

/// A single history entry — a description of an undoable operation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HistoryEntry {
    /// Stable id (can be referenced by services for command replay).
    pub id: u64,
    /// Human-readable label ("Brush stroke", "Move node", …).
    pub label: String,
    /// Unix seconds since epoch.
    pub timestamp: i64,
}

impl HistoryEntry {
    /// Creates a history entry with an auto timestamp and a fresh id.
    pub fn new(id: u64, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            timestamp: now_secs(),
        }
    }
}

/// The document — root aggregate.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Document {
    pub id: DocumentId,
    pub name: String,
    pub size: ImageSize,
    pub dpi: f32,
    pub color_profile: ColorProfile,
    /// ★ The scene graph — all editable content lives here.
    pub scene: Scene,
    /// Active selection (grayscale mask; `None` = select all).
    pub selection: Option<SelectionMask>,
    pub history: HistoryState,
    pub timeline: Timeline,
    pub resources: ResourceRefs,
    pub metadata: DocumentMeta,
}

impl Document {
    /// Maximum allowed dimension (width or height) in pixels.
    pub const MAX_DIMENSION: u32 = 32768; // 2^15, GPU texture limit

    /// Minimum allowed dimension (width or height) in pixels.
    pub const MIN_DIMENSION: u32 = 1;

    /// Creates a new blank document with the given canvas size.
    ///
    /// # Errors
    ///
    /// Returns [`ImageError::InvalidDimensions`] if width or height is
    /// outside [`MIN_DIMENSION`, `MAX_DIMENSION`].
    pub fn new(
        id: DocumentId,
        name: impl Into<String>,
        width: u32,
        height: u32,
    ) -> ImageResult<Self> {
        Self::validate_dimensions(width, height)?;
        Ok(Self {
            id,
            name: name.into(),
            size: ImageSize::new(width, height),
            dpi: 96.0,
            color_profile: ColorProfile::default(),
            scene: Scene::new(),
            selection: Some(SelectionMask::all()),
            history: HistoryState::with_limit(100),
            timeline: Timeline::default(),
            resources: ResourceRefs::default(),
            metadata: DocumentMeta::default(),
        })
    }

    /// Validates that dimensions are within allowed bounds.
    pub fn validate_dimensions(width: u32, height: u32) -> ImageResult<()> {
        if width < Self::MIN_DIMENSION
            || width > Self::MAX_DIMENSION
            || height < Self::MIN_DIMENSION
            || height > Self::MAX_DIMENSION
        {
            Err(ImageError::InvalidDimensions { width, height })
        } else {
            Ok(())
        }
    }

    /// Marks the document as modified (bumps `modified_at`).
    pub fn touch(&mut self) {
        self.metadata.modified_at = now_secs();
    }

    /// Convenience: the scene root node id.
    #[inline]
    pub fn root(&self) -> super::types::NodeId {
        self.scene.root()
    }

    /// Serializes the document to a compact JSON string.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }

    /// Serializes the document to a pretty-printed JSON string.
    pub fn to_json_pretty(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// Deserializes a document from a JSON string produced by
    /// [`Self::to_json`] / [`Self::to_json_pretty`].
    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }

    // ── Binary `.kld` format ────────────────────────────────────────────

    /// Serializes the document to binary `.kld` format.
    ///
    /// The output is a chunk-based container:
    /// - Header: magic (`KALD`) + version + flags + chunk count
    /// - Chunk 0 (`DOCM`): document JSON
    /// - Chunk 1 (`THMB`, optional): thumbnail PNG
    pub fn to_kld(&self) -> serde_json::Result<Vec<u8>> {
        let format = super::KldFormat::default();
        let doc_json = self.to_json()?;
        let doc_chunk = super::KldChunk::document(doc_json.into_bytes());
        let chunks = vec![doc_chunk];
        Ok(KldFormat::serialize_chunks(&format, &chunks))
    }

    /// Deserializes a document from binary `.kld` format.
    pub fn from_kld(bytes: &[u8]) -> Result<Self, KldError> {
        let (format, chunks) = KldFormat::deserialize_chunks(bytes)?;
        if format.version > super::KLD_VERSION {
            return Err(KldError::UnsupportedVersion(format.version));
        }
        let doc_chunk = chunks
            .iter()
            .find(|c| c.chunk_type == super::CHUNK_DOCUMENT)
            .ok_or(KldError::MissingDocumentChunk)?;
        let json = String::from_utf8_lossy(&doc_chunk.data);
        Ok(Document::from_json(&json)?)
    }
}
