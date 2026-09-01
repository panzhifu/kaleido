//! [`LayerStore`] implementation backed by a [`LayerStack`].
//!
//! The store owns the document's layer stack plus the active-layer id.
//! Every mutation re-composites and publishes the flattened result to the
//! [`ImageStore`], so hosts (and tools) see the canvas update automatically.
//!
//! This is the *host-side* counterpart of the plugin-facing
//! [`kaleido_traits::LayerStore`] contract.

use std::sync::{Arc, Mutex, Weak};

use cordis::Context;
use kaleido_core::{ImageError, ImageResult, Pixel, PixelFormat, TiledImage};
use kaleido_traits::{
    BlendMode, ImageStore, KaleidoEmitter, LayerAddedEvent, LayerId, LayerInfo, LayerRemovedEvent,
};

use crate::services::layer::legacy_layer_stack::{Layer, LayerContent, LayerStack};

/// Shared state behind the [`LayerStoreImpl`] facade.
#[derive(Debug, Default)]
struct Inner {
    stack: Option<LayerStack>,
    active: Option<LayerId>,
}

/// Default implementation of [`kaleido_traits::LayerStore`].
///
/// Registered as the Cordis `"layer_store"` service; depends on
/// `"image_store"` so it can publish composited frames.
pub struct LayerStoreImpl {
    inner: Mutex<Inner>,
    image_store: Weak<dyn ImageStore>,
    ctx: Context,
}

impl cordis::Service for LayerStoreImpl {
    const NAME: &'static str = "layer_store";
}

impl LayerStoreImpl {
    /// Creates a new layer store that publishes composites to `image_store`.
    pub fn new(image_store: Arc<dyn ImageStore>, ctx: Context) -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            image_store: Arc::downgrade(&image_store),
            ctx,
        }
    }

    /// Adds a layer to the stack (top of stack).
    fn push_layer(&self, layer: Layer) -> ImageResult<LayerId> {
        let id = layer.id;
        let name = layer.name.clone();
        let mut inner = self.inner.lock().map_err(|_| lock_err())?;
        let stack = self.stack_mut(&mut inner)?;
        stack.add_layer(layer);
        // The newly added layer becomes the active (editable) layer.
        inner.active = Some(id);
        drop(inner);
        self.ctx.emit_layer_added(LayerAddedEvent {
            layer_id: format!("{}", id.0),
            name,
        });
        self.publish()?;
        Ok(id)
    }

    /// Composites the stack and writes the result into the image store.
    fn publish(&self) -> ImageResult<()> {
        let mut inner = self.inner.lock().map_err(|_| lock_err())?;
        let stack = match self.stack_mut(&mut inner) {
            Ok(s) => s,
            Err(_) => return Ok(()), // no document yet
        };
        // `composite()` borrows the stack cache; own the pixels so the
        // lock can be released before touching the image store.
        let composited = stack.composite()?.clone();
        drop(inner);
        let Some(store) = self.image_store.upgrade() else {
            return Ok(());
        };
        store.set_image(composited)?;
        Ok(())
    }

    fn stack_mut<'a>(&self, inner: &'a mut Inner) -> ImageResult<&'a mut LayerStack> {
        inner.stack.as_mut().ok_or_else(|| ImageError::EmptyImage)
    }

    fn stack_ref<'a>(&self, inner: &'a Inner) -> Option<&'a LayerStack> {
        inner.stack.as_ref()
    }

    fn layer_mut<'a>(
        inner: &'a mut Inner,
        id: LayerId,
    ) -> ImageResult<&'a mut Layer> {
        let stack = inner
            .stack
            .as_mut()
            .ok_or(ImageError::EmptyImage)?;
        let index = stack.layer_index(id).ok_or_else(|| {
            ImageError::OperationFailed {
                reason: format!("no layer with id {id:?}"),
            }
        })?;
        stack.layer_mut(index).ok_or_else(|| {
            ImageError::OperationFailed {
                reason: format!("layer index {index} out of range"),
            }
        })
    }
}

fn lock_err() -> ImageError {
    ImageError::OperationFailed {
        reason: "layer_store lock poisoned".to_string(),
    }
}

fn clone_image(source: &TiledImage) -> ImageResult<TiledImage> {
    let width = source.width();
    let height = source.height();
    let data = source.to_rgba_vec();
    TiledImage::from_rgba(width, height, data)
}

impl kaleido_traits::LayerStore for LayerStoreImpl {
    fn layers(&self) -> Vec<LayerInfo> {
        let inner = match self.inner.lock() {
            Ok(inner) => inner,
            Err(_) => return Vec::new(),
        };
        let Some(stack) = inner.stack.as_ref() else {
            return Vec::new();
        };
        stack
            .iter()
            .map(|l| LayerInfo {
                id: l.id,
                name: l.name.clone(),
                visible: l.visible,
                opacity: l.opacity,
                blend_mode: l.blend_mode,
                is_pixels: l.is_pixels(),
            })
            .collect()
    }

    fn import_image(&self, name: &str, image: TiledImage) -> ImageResult<()> {
        let width = image.width();
        let height = image.height();
        let mut inner = self.inner.lock().map_err(|_| lock_err())?;
        let mut stack = LayerStack::new(width, height);
        let layer = Layer::new_pixels(name.to_string(), image);
        let id = layer.id;
        stack.add_layer(layer);
        inner.stack = Some(stack);
        inner.active = Some(id);
        drop(inner);
        self.publish()?;
        Ok(())
    }

    fn active_layer(&self) -> Option<LayerId> {
        self.inner.lock().ok()?.active
    }

    fn set_active_layer(&self, id: LayerId) -> ImageResult<()> {
        let mut inner = self.inner.lock().map_err(|_| lock_err())?;
        let stack = self.stack_mut(&mut inner)?;
        if stack.layer_by_id(id).is_none() {
            return Err(ImageError::OperationFailed {
                reason: format!("no layer with id {id:?}"),
            });
        }
        inner.active = Some(id);
        Ok(())
    }

    fn add_pixel_layer(&self, name: &str) -> ImageResult<LayerId> {
        let (width, height) = self.document_size();
        let image = TiledImage::new(width, height, PixelFormat::Rgba8);
        self.push_layer(Layer::new_pixels(name.to_string(), image))
    }

    fn add_solid_layer(&self, name: &str, color: Pixel) -> ImageResult<LayerId> {
        let (width, height) = self.document_size();
        let image = TiledImage::with_color(width, height, PixelFormat::Rgba8, color)?;
        self.push_layer(Layer::new_pixels(name.to_string(), image))
    }

    fn remove_layer(&self, id: LayerId) -> ImageResult<()> {
        let mut inner = self.inner.lock().map_err(|_| lock_err())?;
        {
            let stack = self.stack_mut(&mut inner)?;
            if stack.remove_layer(id).is_none() {
                return Err(ImageError::OperationFailed {
                    reason: format!("no layer with id {id:?}"),
                });
            }
        }
        // Fall back to the topmost remaining layer (or none) when the
        // active layer was removed.
        if inner.active == Some(id) {
            inner.active = match self.stack_ref(&inner) {
                Some(stack) if stack.layer_count() > 0 => {
                    stack.layer(stack.layer_count() - 1).map(|l| l.id)
                }
                _ => None,
            };
        }
        let removed_id = id;
        drop(inner);
        self.ctx.emit_layer_removed(LayerRemovedEvent {
            layer_id: format!("{}", removed_id.0),
        });
        self.publish()?;
        Ok(())
    }

    fn reorder(&self, from: usize, to: usize) -> ImageResult<()> {
        let mut inner = self.inner.lock().map_err(|_| lock_err())?;
        let stack = self.stack_mut(&mut inner)?;
        if from >= stack.layer_count() || to >= stack.layer_count() {
            return Err(ImageError::OperationFailed {
                reason: format!("reorder out of range: {from} -> {to}"),
            });
        }
        stack.reorder(from, to);
        drop(inner);
        self.publish()
    }

    fn set_opacity(&self, id: LayerId, opacity: f32) -> ImageResult<()> {
        let mut inner = self.inner.lock().map_err(|_| lock_err())?;
        Self::layer_mut(&mut inner, id)?.opacity = opacity.clamp(0.0, 1.0);
        drop(inner);
        self.publish()
    }

    fn set_visible(&self, id: LayerId, visible: bool) -> ImageResult<()> {
        let mut inner = self.inner.lock().map_err(|_| lock_err())?;
        Self::layer_mut(&mut inner, id)?.visible = visible;
        drop(inner);
        self.publish()
    }

    fn set_blend_mode(&self, id: LayerId, mode: BlendMode) -> ImageResult<()> {
        let mut inner = self.inner.lock().map_err(|_| lock_err())?;
        Self::layer_mut(&mut inner, id)?.blend_mode = mode;
        drop(inner);
        self.publish()
    }

    fn set_layer_name(&self, id: LayerId, name: &str) -> ImageResult<()> {
        let mut inner = self.inner.lock().map_err(|_| lock_err())?;
        Self::layer_mut(&mut inner, id)?.name = name.to_string();
        Ok(())
    }

    fn edit_active_layer(&self, f: &mut dyn FnMut(&mut TiledImage)) -> ImageResult<()> {
        let mut inner = self.inner.lock().map_err(|_| lock_err())?;
        let active = inner.active.ok_or(ImageError::EmptyImage)?;
        let stack = self.stack_mut(&mut inner)?;
        let index = stack.layer_index(active).ok_or_else(|| {
            ImageError::OperationFailed {
                reason: format!("active layer {active:?} missing"),
            }
        })?;
        let layer = stack.layer_mut(index).ok_or_else(|| {
            ImageError::OperationFailed {
                reason: format!("active layer {active:?} missing"),
            }
        })?;
        let LayerContent::Pixels(ref mut image) = layer.content else {
            return Err(ImageError::OperationFailed {
                reason: "active layer is not a pixel layer".to_string(),
            });
        };
        f(image);
        // Direct pixel edits bypass the structural mutators, so the
        // composite cache must be invalidated explicitly.
        stack.invalidate();
        drop(inner);
        self.publish()
    }

    fn composite(&self) -> ImageResult<TiledImage> {
        let mut inner = self.inner.lock().map_err(|_| lock_err())?;
        let stack = self.stack_mut(&mut inner)?;
        let composited = stack.composite()?;
        Ok(clone_image(composited)?)
    }

    fn document_size(&self) -> (u32, u32) {
        let inner = match self.inner.lock() {
            Ok(inner) => inner,
            Err(_) => return (0, 0),
        };
        match inner.stack.as_ref() {
            Some(stack) => (stack.width(), stack.height()),
            None => (0, 0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::data::legacy::file_codec_impl::FileCodecImpl;
    use crate::services::data::legacy::image_store_impl::ImageStoreImpl;
    use cordis::Context;
    use kaleido_core::Pixel;
    use kaleido_traits::LayerStore as _;

    fn setup() -> (Arc<LayerStoreImpl>, Arc<ImageStoreImpl>) {
        let ctx = Context::new();
        let codec: Arc<dyn kaleido_traits::FileCodec> = Arc::new(FileCodecImpl::new());
        let store: Arc<ImageStoreImpl> = Arc::new(ImageStoreImpl::new(codec, ctx.clone()));
        let image: TiledImage =
            TiledImage::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(0, 0, 0)).unwrap();
        store.set_image(image).unwrap();
        let layers = Arc::new(LayerStoreImpl::new(store.clone(), ctx));
        (layers, store)
    }

    #[test]
    fn test_import_image_creates_background() {
        let (layers, _store) = setup();
        let image =
            TiledImage::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(10, 20, 30)).unwrap();
        layers.import_image("背景", image).unwrap();

        assert_eq!(layers.layer_count(), 1);
        let info = &layers.layers()[0];
        assert_eq!(info.name, "背景");
        assert!(info.visible);
        assert_eq!(info.opacity, 1.0);
        assert!(layers.active_layer().is_some());
    }

    #[test]
    fn test_add_and_remove_layers() {
        let (layers, _store) = setup();
        layers
            .import_image(
                "背景",
                TiledImage::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(0, 0, 0)).unwrap(),
            )
            .unwrap();

        let id = layers.add_pixel_layer("空层").unwrap();
        assert_eq!(layers.layer_count(), 2);
        assert_eq!(layers.active_layer(), Some(id));

        layers.remove_layer(id).unwrap();
        assert_eq!(layers.layer_count(), 1);
        // Active falls back to the remaining layer.
        assert!(layers.active_layer().is_some());
    }

    #[test]
    fn test_set_opacity_and_visibility() {
        let (layers, _store) = setup();
        layers
            .import_image(
                "背景",
                TiledImage::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(0, 0, 0)).unwrap(),
            )
            .unwrap();
        let id = layers.layers()[0].id;

        layers.set_opacity(id, 0.5).unwrap();
        assert_eq!(layers.layers()[0].opacity, 0.5);

        layers.set_visible(id, false).unwrap();
        assert!(!layers.layers()[0].visible);
    }

    #[test]
    fn test_solid_layer_publishes_to_store() {
        let (layers, store) = setup();
        layers
            .import_image(
                "背景",
                TiledImage::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(0, 0, 0)).unwrap(),
            )
            .unwrap();

        layers.add_solid_layer("红色", Pixel::rgb(255, 0, 0)).unwrap();

        let published = store.get_image().unwrap().unwrap();
        let px = published.get_pixel(2, 2);
        assert_eq!(px.r, 255);
        assert_eq!(px.g, 0);
        assert_eq!(px.b, 0);
    }

    #[test]
    fn test_edit_active_layer() {
        let (layers, store) = setup();
        layers
            .import_image(
                "背景",
                TiledImage::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(0, 0, 0)).unwrap(),
            )
            .unwrap();

        layers
            .edit_active_layer(&mut |img| {
                img.set_pixel(0, 0, Pixel::rgb(9, 9, 9));
            })
            .unwrap();

        let published = store.get_image().unwrap().unwrap();
        assert_eq!(published.get_pixel(0, 0), Pixel::rgb(9, 9, 9));
        assert_eq!(published.get_pixel(3, 3), Pixel::rgb(0, 0, 0));
    }
}
