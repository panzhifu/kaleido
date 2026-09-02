//! The **resource** manager — document resources (fonts / swatches / brushes).

use kaleido_core::ResourceId;

use super::ServiceResult;

/// The kind of a registered resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceKind {
    /// A font resource (name + binary data).
    Font,
    /// A colour swatch.
    Swatch,
    /// A brush preset.
    Brush,
}

/// A registered resource's data.
///
/// Each variant carries the payload appropriate to its [`ResourceKind`];
/// use [`Self::kind`] to determine which variant this is.
#[derive(Debug, Clone, PartialEq)]
pub enum ResourceData {
    /// A font: name + binary data (e.g. OTF/TTF bytes).
    Font {
        name: String,
        bytes: Vec<u8>,
    },
    /// A colour swatch.
    Swatch {
        color: kaleido_core::Color,
    },
    /// A brush preset (name references parameters stored elsewhere).
    Brush {
        name: String,
    },
}

impl ResourceData {
    /// Returns the [`ResourceKind`] of this resource.
    pub fn kind(&self) -> ResourceKind {
        match self {
            ResourceData::Font { .. } => ResourceKind::Font,
            ResourceData::Swatch { .. } => ResourceKind::Swatch,
            ResourceData::Brush { .. } => ResourceKind::Brush,
        }
    }
}

/// The resource management service.
///
/// Provides a generic key-value store for document resources such as fonts,
/// colour swatches, and brush presets. Resources are addressed by the
/// monotonic [`ResourceId`] handle returned at registration time.
pub trait ResourceService: Send + Sync + 'static {
    // ── CRUD ─────────────────────────────────────────────────────────────

    /// Registers a new resource and returns its unique handle.
    ///
    /// Ids are allocated monotonically from 1 and never reused, so a handle
    /// stays valid for the lifetime of the service (or until
    /// [`Self::remove`] is called).
    fn register(&self, data: ResourceData) -> ServiceResult<ResourceId>;

    /// Returns the data for `id`, or `None` when the id is not registered.
    fn get(&self, id: ResourceId) -> Option<ResourceData>;

    /// Replaces the data of an existing resource in place.
    ///
    /// The id and total count are unchanged; only the payload is replaced.
    fn update(&self, id: ResourceId, data: ResourceData) -> ServiceResult<()>;

    /// Removes a resource by id.
    fn remove(&self, id: ResourceId) -> ServiceResult<()>;

    // ── Query ────────────────────────────────────────────────────────────

    /// Lists all resources of a given kind, sorted by id.
    fn list(&self, kind: ResourceKind) -> Vec<(ResourceId, ResourceData)>;

    /// The total number of registered resources.
    fn count(&self) -> usize;
}
