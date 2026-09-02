//! The **layer manager** implementation.
//!
//! Manages scene-graph layer operations. Works with [`super::data::DataService`]
//! to access and modify the document.

use std::sync::{Arc, RwLock};


use crate::{impl_service, service_plugin};
use kaleido_core::{BlendMode, NodeContent, NodeId, PixelFormat, PixelLayer, Transform2D};
use kaleido_traits::data::error::{ServiceError, ServiceResult};
use kaleido_traits::data::DataService;
use kaleido_traits::layer::{LayerInfo, LayerService};

/// Default implementation of [`LayerService`].
pub struct LayerServiceImpl {
    data_service: Arc<dyn DataService>,
    /// Active layer id.
    active_layer: RwLock<Option<NodeId>>,
}

impl LayerServiceImpl {
    pub fn new(data_service: Arc<dyn DataService>) -> Self {
        Self {
            data_service,
            active_layer: RwLock::new(None),
        }
    }

    fn with_doc<F, T>(&self, f: F) -> ServiceResult<T>
    where
        F: FnOnce(&kaleido_core::Document) -> ServiceResult<T>,
    {
        let doc = self
            .data_service
            .document()?
            .ok_or(ServiceError::NoDocument)?;
        f(&doc)
    }

    fn modify_doc<F>(&self, f: F) -> ServiceResult<()>
    where
        F: FnOnce(&mut kaleido_core::Document) -> ServiceResult<()>
    {
        let doc = self
            .data_service
            .document()?
            .ok_or(ServiceError::NoDocument)?;
        let mut new_doc = doc;
        f(&mut new_doc)?;
        self.data_service.restore(new_doc);
        Ok(())
    }
}

impl_service!(LayerServiceImpl, "layer_service");

service_plugin!(LayerServiceImpl, "layer_service",
    deps: none,
    build: |ctx, _config| {
        let data_service: Arc<dyn DataService> = ctx
            .get::<crate::data::DataServiceImpl>("data_service")?
            .ok_or_else(|| -> cordis::CordisError {
                cordis::CordisError::with_message(
                    cordis::ErrorCode::Other,
                    String::from("data_service not found"),
                )
            })?;
        Ok(LayerServiceImpl::new(data_service))
    }
);

// ── LayerService trait implementation ──────────────────────────────────────

impl LayerService for LayerServiceImpl {
    fn add_pixel_layer(
        &self,
        name: &str,
        width: u32,
        height: u32,
        format: PixelFormat,
    ) -> ServiceResult<NodeId> {
        let doc = self
            .data_service
            .document()?
            .ok_or(ServiceError::NoDocument)?;
        let layer = PixelLayer::blank(width, height, format);
        let root = doc.scene.root();
        let mut new_doc = doc;
        let id = new_doc
            .scene
            .add_node(root, name, NodeContent::Pixel(layer))
            .ok_or_else(|| ServiceError::Other("failed to add node".into()))?;
        self.data_service.restore(new_doc);
        Ok(id)
    }

    fn add_group(&self, name: &str) -> ServiceResult<NodeId> {
        let doc = self
            .data_service
            .document()?
            .ok_or(ServiceError::NoDocument)?;
        let root = doc.scene.root();
        let mut new_doc = doc;
        let id = new_doc
            .scene
            .add_node(root, name, NodeContent::Group)
            .ok_or_else(|| ServiceError::Other("failed to add node".into()))?;
        self.data_service.restore(new_doc);
        Ok(id)
    }

    fn remove(&self, id: NodeId) -> ServiceResult<()> {
        self.modify_doc(|doc| {
            doc.scene.remove_node(id);
            Ok(())
        })
    }

    fn rename(&self, id: NodeId, name: &str) -> ServiceResult<()> {
        self.modify_doc(|doc| {
            let node = doc
                .scene
                .node_mut(id)
                .ok_or(ServiceError::NodeNotFound(id.0))?;
            node.name = name.into();
            Ok(())
        })
    }

    fn reorder(&self, child: NodeId, to_index: usize) -> ServiceResult<()> {
        self.modify_doc(|doc| {
            let parent = doc
                .scene
                .node(child)
                .and_then(|n| n.parent)
                .ok_or(ServiceError::NodeNotFound(child.0))?;
            doc.scene.reorder_child(parent, child, to_index);
            Ok(())
        })
    }

    fn reparent(&self, id: NodeId, new_parent: NodeId) -> ServiceResult<()> {
        self.modify_doc(|doc| {
            doc.scene.reparent(id, new_parent);
            Ok(())
        })
    }

    fn set_visible(&self, id: NodeId, visible: bool) -> ServiceResult<()> {
        self.modify_doc(|doc| {
            let node = doc
                .scene
                .node_mut(id)
                .ok_or(ServiceError::NodeNotFound(id.0))?;
            node.visible = visible;
            Ok(())
        })
    }

    fn set_opacity(&self, id: NodeId, opacity: f32) -> ServiceResult<()> {
        self.modify_doc(|doc| {
            let node = doc
                .scene
                .node_mut(id)
                .ok_or(ServiceError::NodeNotFound(id.0))?;
            node.opacity = opacity.clamp(0.0, 1.0);
            Ok(())
        })
    }

    fn set_blend(&self, id: NodeId, blend: BlendMode) -> ServiceResult<()> {
        self.modify_doc(|doc| {
            let node = doc
                .scene
                .node_mut(id)
                .ok_or(ServiceError::NodeNotFound(id.0))?;
            node.blend_mode = blend;
            Ok(())
        })
    }

    fn set_transform(&self, id: NodeId, transform: Transform2D) -> ServiceResult<()> {
        self.modify_doc(|doc| {
            let node = doc
                .scene
                .node_mut(id)
                .ok_or(ServiceError::NodeNotFound(id.0))?;
            node.transform = transform;
            Ok(())
        })
    }

    fn children(&self, id: NodeId) -> ServiceResult<Vec<NodeId>> {
        self.with_doc(|doc| Ok(doc.scene.children(id).cloned().unwrap_or_default()))
    }

    fn layer(&self, id: NodeId) -> ServiceResult<Option<LayerInfo>> {
        self.with_doc(|doc| {
            let node = match doc.scene.node(id) {
                Some(n) => n,
                None => return Ok(None),
            };
            Ok(Some(LayerInfo {
                id: node.id,
                name: node.name.clone(),
                visible: node.visible,
                opacity: node.opacity,
                blend_mode: node.blend_mode,
                locked: node.locked,
                is_group: matches!(node.content, NodeContent::Group),
            }))
        })
    }

    fn layer_count(&self) -> ServiceResult<usize> {
        self.with_doc(|doc| Ok(doc.scene.node_count()))
    }

    fn active_layer(&self) -> Option<NodeId> {
        self.active_layer
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn set_active(&self, id: NodeId) -> ServiceResult<()> {
        let mut active = self
            .active_layer
            .write()
            .unwrap_or_else(|p| p.into_inner());
        *active = Some(id);
        Ok(())
    }

    fn layer_ids(&self) -> ServiceResult<Vec<NodeId>> {
        self.with_doc(|doc| {
            let root = doc.scene.root();
            Ok(doc.scene.children(root).cloned().unwrap_or_default())
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use kaleido_core::DocumentId;

    struct FakeDataService {
        doc: std::sync::RwLock<Option<kaleido_core::Document>>,
    }

    impl FakeDataService {
        fn new(doc: kaleido_core::Document) -> Self {
            Self {
                doc: std::sync::RwLock::new(Some(doc)),
            }
        }
    }

    impl DataService for FakeDataService {
        fn new_document(
            &self,
            _name: &str,
            _w: u32,
            _h: u32,
        ) -> ServiceResult<kaleido_core::DocumentId> {
            Err(ServiceError::Other("not implemented".into()))
        }
        fn open(&self, _path: &std::path::Path) -> ServiceResult<()> {
            Err(ServiceError::Other("not implemented".into()))
        }
        fn save(&self) -> ServiceResult<()> {
            Err(ServiceError::Other("not implemented".into()))
        }
        fn save_as(&self, _path: &std::path::Path) -> ServiceResult<()> {
            Err(ServiceError::Other("not implemented".into()))
        }
        fn close(&self) -> ServiceResult<()> {
            Err(ServiceError::Other("not implemented".into()))
        }
        fn document(&self) -> ServiceResult<Option<kaleido_core::Document>> {
            Ok(self
                .doc
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .clone())
        }
        fn has_document(&self) -> bool {
            self.doc
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .is_some()
        }
        fn path(&self) -> Option<std::path::PathBuf> {
            None
        }
        fn size(&self) -> Option<kaleido_core::ImageSize> {
            None
        }
        fn restore(&self, snapshot: kaleido_core::Document) {
            *self.doc.write().unwrap_or_else(|e| e.into_inner()) = Some(snapshot);
        }
        fn render_for_export(&self) -> ServiceResult<kaleido_core::TiledImage> {
            Err(ServiceError::Other("not implemented".into()))
        }
    }

    fn make_service() -> LayerServiceImpl {
        let doc = kaleido_core::Document::new(DocumentId(1), "test", 100, 100).unwrap();
        let fake = Arc::new(FakeDataService::new(doc));
        LayerServiceImpl::new(fake)
    }

    #[test]
    fn test_new_service() {
        let svc = make_service();
        assert!(svc.active_layer().is_none());
    }

    #[test]
    fn test_set_active() {
        let svc = make_service();
        svc.set_active(NodeId(1)).unwrap();
        assert_eq!(svc.active_layer(), Some(NodeId(1)));
    }

    #[test]
    fn test_add_pixel_layer() {
        let svc = make_service();
        let id = svc
            .add_pixel_layer("Layer 1", 100, 100, PixelFormat::Rgba8)
            .unwrap();
        assert!(id.0 >= 2); // Root is 1, layers start at 2
        let info = svc.layer(id).unwrap();
        assert!(info.is_some());
        assert_eq!(info.unwrap().name, "Layer 1");
    }

    #[test]
    fn test_add_group() {
        let svc = make_service();
        let id = svc.add_group("Group 1").unwrap();
        assert!(id.0 >= 2);
        let info = svc.layer(id).unwrap().unwrap();
        assert!(info.is_group);
    }

    #[test]
    fn test_layer_count() {
        let svc = make_service();
        let initial = svc.layer_count().unwrap();
        svc.add_pixel_layer("Layer 1", 100, 100, PixelFormat::Rgba8)
            .unwrap();
        assert_eq!(svc.layer_count().unwrap(), initial + 1);
    }

    #[test]
    fn test_rename() {
        let svc = make_service();
        let id = svc
            .add_pixel_layer("Old Name", 100, 100, PixelFormat::Rgba8)
            .unwrap();
        svc.rename(id, "New Name").unwrap();
        assert_eq!(svc.layer(id).unwrap().unwrap().name, "New Name");
    }

    #[test]
    fn test_remove() {
        let svc = make_service();
        let id = svc
            .add_pixel_layer("Layer 1", 100, 100, PixelFormat::Rgba8)
            .unwrap();
        svc.remove(id).unwrap();
        assert!(svc.layer(id).unwrap().is_none());
    }

    #[test]
    fn test_set_visible_opacity() {
        let svc = make_service();
        let id = svc
            .add_pixel_layer("Layer 1", 100, 100, PixelFormat::Rgba8)
            .unwrap();
        svc.set_visible(id, false).unwrap();
        svc.set_opacity(id, 0.5).unwrap();
        let info = svc.layer(id).unwrap().unwrap();
        assert!(!info.visible);
        assert!((info.opacity - 0.5).abs() < f32::EPSILON);
    }
}
