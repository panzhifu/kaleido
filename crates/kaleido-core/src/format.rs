//! Kaleido document format (`.kld`) — binary container layout.
//!
//! The format is a chunk-based binary container:
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────┐
//! │ Header (16 bytes)                                        │
//! │   magic:       b"KALD"  (4 bytes)                        │
//! │   version:     u32 LE (current: 1)                       │
//! │   flags:       u32 LE (reserved, 0)                      │
//! │   chunk_count: u32 LE                                    │
//! ├──────────────────────────────────────────────────────────┤
//! │ Chunks                                                   │
//! │   ┌────────────────────────────────────────────────────┐ │
//! │   │ Chunk 0: DOCM — full document JSON                 │ │
//! │   │   type:    b"DOCM" (4 bytes)                       │ │
//! │   │   size:    u64 LE (data length)                    │ │
//! │   │   data:    [u8; size]                              │ │
//! │   ├────────────────────────────────────────────────────┤ │
//! │   │ Chunk 1: THMB — thumbnail (optional)               │ │
//! │   │   type:    b"THMB" (4 bytes)                       │ │
//! │   │   size:    u64 LE                                  │ │
//! │   │   data:    [u8; size] (PNG)                        │ │
//! │   ├────────────────────────────────────────────────────┤ │
//! │   │ Chunk N: ... (future: fonts, ICC profiles)         │ │
//! │   └────────────────────────────────────────────────────┘ │
//! └──────────────────────────────────────────────────────────┘
//! ```

use thiserror::Error;

/// Kaleido file magic number: `KALD`.
pub const KLD_MAGIC: [u8; 4] = *b"KALD";

/// Current format version.
pub const KLD_VERSION: u32 = 1;

/// Chunk type tags.
pub const CHUNK_DOCUMENT: [u8; 4] = *b"DOCM";
pub const CHUNK_THUMBNAIL: [u8; 4] = *b"THMB";

/// Errors that can occur when parsing `.kld` files.
#[derive(Debug, Error)]
pub enum KldError {
    /// The file does not start with the `KALD` magic.
    #[error("not a .kld file (invalid magic)")]
    InvalidMagic,
    /// The file version is newer than what this library supports.
    #[error("unsupported .kld version: {0}")]
    UnsupportedVersion(u32),
    /// The document chunk (`DOCM`) is missing.
    #[error("missing document chunk in .kld file")]
    MissingDocumentChunk,
    /// The file is truncated or malformed.
    #[error("corrupt .kld file: {0}")]
    Corrupt(String),
    /// JSON (de)serialization failure.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Document format descriptor — the `.kld` binary layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KldFormat {
    /// Format version.
    pub version: u32,
    /// Flags (reserved for future use).
    pub flags: u32,
}

impl Default for KldFormat {
    fn default() -> Self {
        Self {
            version: KLD_VERSION,
            flags: 0,
        }
    }
}

/// A single chunk in the `.kld` container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KldChunk {
    /// 4-byte type tag (e.g. `DOCM`, `THMB`).
    pub chunk_type: [u8; 4],
    /// Chunk data.
    pub data: Vec<u8>,
}

impl KldChunk {
    /// Creates a new chunk.
    pub fn new(chunk_type: [u8; 4], data: Vec<u8>) -> Self {
        Self { chunk_type, data }
    }

    /// Creates a document chunk.
    pub fn document(data: Vec<u8>) -> Self {
        Self::new(CHUNK_DOCUMENT, data)
    }

    /// Creates a thumbnail chunk.
    pub fn thumbnail(data: Vec<u8>) -> Self {
        Self::new(CHUNK_THUMBNAIL, data)
    }
}

impl KldFormat {
    /// Returns true if the bytes start with the Kaldld magic.
    pub fn is_kld_header(bytes: &[u8]) -> bool {
        bytes.len() >= 4 && bytes[0..4] == KLD_MAGIC
    }

    /// Serializes chunks to binary `.kld` format.
    pub fn serialize_chunks(format: &KldFormat, chunks: &[KldChunk]) -> Vec<u8> {
        let mut out = Vec::with_capacity(16 + chunks.iter().map(|c| 12 + c.data.len()).sum::<usize>());
        // Header
        out.extend_from_slice(&KLD_MAGIC);
        out.extend_from_slice(&format.version.to_le_bytes());
        out.extend_from_slice(&format.flags.to_le_bytes());
        out.extend_from_slice(&(chunks.len() as u32).to_le_bytes());
        // Chunks
        for chunk in chunks {
            out.extend_from_slice(&chunk.chunk_type);
            out.extend_from_slice(&(chunk.data.len() as u64).to_le_bytes());
            out.extend_from_slice(&chunk.data);
        }
        out
    }

    /// Deserializes binary `.kld` format to chunks.
    pub fn deserialize_chunks(bytes: &[u8]) -> Result<(KldFormat, Vec<KldChunk>), KldError> {
        if !Self::is_kld_header(bytes) {
            return Err(KldError::InvalidMagic);
        }
        if bytes.len() < 16 {
            return Err(KldError::Corrupt("truncated header".into()));
        }
        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let flags = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let chunk_count = u32::from_le_bytes(bytes[12..16].try_into().unwrap());

        let mut offset = 16;
        let mut chunks = Vec::with_capacity(chunk_count as usize);

        for _ in 0..chunk_count {
            if offset + 12 > bytes.len() {
                return Err(KldError::Corrupt("truncated chunk header".into()));
            }
            let chunk_type: [u8; 4] = bytes[offset..offset + 4].try_into().unwrap();
            let size = u64::from_le_bytes(bytes[offset + 4..offset + 12].try_into().unwrap());
            offset += 12;

            if offset + size as usize > bytes.len() {
                return Err(KldError::Corrupt("truncated chunk data".into()));
            }
            let data = bytes[offset..offset + size as usize].to_vec();
            offset += size as usize;

            chunks.push(KldChunk::new(chunk_type, data));
        }

        Ok((
            KldFormat { version, flags },
            chunks,
        ))
    }
}
