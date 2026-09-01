//! Tests for the new data model: scene graph, pixel layers, document,
//! timeline, masks, text and vector helpers.

#[cfg(test)]
mod tests {
    use crate::document::Document;
    use crate::mask::{Mask, SelectionMask};
    use crate::pixel::{Pixel, PixelFormat};
    use crate::pixel_layer::PixelLayer;
    use crate::scene::{NodeContent, Scene};
    use crate::text::TextObject;
    use crate::tile::TiledImage;
    use crate::timeline::{AnimValue, AnimatableProp, Easing, Keyframe, Timeline, Track};
    use crate::types::{Color, DocumentId, NodeId, ResourceId, Transform2D};
    use crate::vector::{PathNode, VectorObject, VectorPath};

    // -- Scene graph ---------------------------------------------------------

    #[test]
    fn scene_add_remove() {
        let mut doc = Document::new(DocumentId(1), "test", 512, 512).unwrap();
        let root = doc.scene.root();
        let layer = doc
            .scene
            .add_node(
                root,
                "Layer 1",
                NodeContent::Pixel(PixelLayer::blank(512, 512, PixelFormat::Rgba8)),
            )
            .unwrap();

        assert_eq!(doc.scene.node_count(), 2);
        assert!(doc.scene.node(layer).is_some());
        assert_eq!(doc.scene.children(root).unwrap().len(), 1);

        assert!(doc.scene.remove_node(layer));
        assert_eq!(doc.scene.node_count(), 1);
        assert!(doc.scene.node(layer).is_none());
        assert!(doc.scene.children(root).unwrap().is_empty());
    }

    #[test]
    fn scene_reject_non_group_parent() {
        let mut doc = Document::new(DocumentId(2), "t", 256, 256).unwrap();
        let root = doc.scene.root();
        let px = doc
            .scene
            .add_node(root, "p", NodeContent::Pixel(PixelLayer::blank(256, 256, PixelFormat::Rgba8)))
            .unwrap();
        // Adding a child under a non-group node must fail.
        let res = doc
            .scene
            .add_node(px, "orphan", NodeContent::Group);
        assert!(res.is_none());
    }

    #[test]
    fn scene_reparent_cycle_rejected() {
        let mut scene = Scene::new();
        let root = scene.root();
        let g1 = scene.add_node(root, "g1", NodeContent::Group).unwrap();
        let g2 = scene.add_node(g1, "g2", NodeContent::Group).unwrap();
        let leaf = scene.add_node(g2, "leaf", NodeContent::Group).unwrap();

        // Moving a node under its own descendant must be rejected.
        assert!(!scene.reparent(root, g2));
        assert!(!scene.reparent(g1, leaf));
        assert!(!scene.reparent(g2, leaf));
        assert!(scene.validate());

        // A legal reparent works and keeps the tree valid.
        assert!(scene.reparent(g2, root));
        assert!(scene.validate());
        assert!(scene.is_descendant_of(g2, root));
    }

    #[test]
    fn scene_reparent_rejects_missing_or_non_group() {
        let mut scene = Scene::new();
        let root = scene.root();
        let g = scene.add_node(root, "g", NodeContent::Group).unwrap();
        let px = scene
            .add_node(root, "px", NodeContent::Pixel(PixelLayer::blank(8, 8, PixelFormat::Rgba8)))
            .unwrap();

        // Moving a group under a non-group parent must fail (px is not a group).
        assert!(!scene.reparent(g, px));
        // Moving a pixel layer under a group is legal.
        assert!(scene.reparent(px, g));
        assert!(!scene.reparent(NodeId(9999), root)); // missing node
        assert!(!scene.reparent(g, NodeId(9998))); // missing parent
        assert!(scene.validate());
    }

    #[test]
    fn scene_descendants_and_reorder() {
        let mut scene = Scene::new();
        let root = scene.root();
        let a = scene.add_node(root, "a", NodeContent::Group).unwrap();
        let b = scene.add_node(a, "b", NodeContent::Group).unwrap();
        let c = scene.add_node(a, "c", NodeContent::Group).unwrap();
        let d = scene.add_node(b, "d", NodeContent::Group).unwrap();

        let mut desc = scene.descendants(root);
        desc.sort();
        assert_eq!(desc, vec![a, b, c, d]);

        assert!(scene.is_descendant_of(d, a));
        assert!(!scene.is_descendant_of(a, d));
        assert_eq!(scene.depth_of(d), Some(3));
        assert_eq!(scene.depth_of(root), Some(0));

        // Reorder c to the bottom of a's children (index 0).
        assert!(scene.reorder_child(a, c, 0));
        assert_eq!(scene.children(a).unwrap(), &vec![c, b]);
        assert!(scene.validate());
    }

    #[test]
    fn scene_remove_subtree() {
        let mut scene = Scene::new();
        let root = scene.root();
        let g = scene.add_node(root, "g", NodeContent::Group).unwrap();
        let c1 = scene.add_node(g, "c1", NodeContent::Group).unwrap();
        let _c2 = scene.add_node(g, "c2", NodeContent::Group).unwrap();
        let c1a = scene.add_node(c1, "c1a", NodeContent::Group).unwrap();

        assert!(scene.remove_node(g));
        assert!(scene.node(c1).is_none());
        assert!(scene.node(c1a).is_none());
        assert!(scene.validate());
        assert_eq!(scene.node_count(), 1);
    }

    // -- Pixel layers --------------------------------------------------------

    #[test]
    fn pixel_layer_frames_cow() {
        let img = TiledImage::new(512, 512, PixelFormat::Rgba8);
        let mut layer = PixelLayer::new(img);
        layer.add_frame();
        assert_eq!(layer.frame_count(), 2);

        // Mutating frame 1 must not affect frame 0 (COW).
        layer
            .frame_mut(1)
            .unwrap()
            .set_pixel(10, 10, Pixel::new(255, 0, 0, 255));
        assert_eq!(
            layer.frame(1).unwrap().get_pixel(10, 10),
            Pixel::new(255, 0, 0, 255)
        );
        assert_eq!(layer.frame(0).unwrap().get_pixel(10, 10), Pixel::new(0, 0, 0, 0));

        layer.remove_last_frame();
        assert_eq!(layer.frame_count(), 1);
    }

    #[test]
    fn pixel_layer_blank_animated_shares_tiles() {
        let mut layer = PixelLayer::blank_animated(256, 256, PixelFormat::Rgba8, 4);
        assert_eq!(layer.frame_count(), 4);
        assert_eq!(layer.width(), 256);

        // Editing one frame must not leak into its siblings (COW).
        layer.frame_mut(2).unwrap().set_pixel(0, 0, Pixel::rgb(1, 2, 3));
        assert_eq!(layer.frame(2).unwrap().get_pixel(0, 0), Pixel::rgb(1, 2, 3));
        assert_eq!(layer.frame(0).unwrap().get_pixel(0, 0), Pixel::new(0, 0, 0, 0));
    }

    #[test]
    fn pixel_layer_from_frames_and_out_of_range() {
        let frames = vec![
            TiledImage::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(1, 0, 0)).unwrap(),
            TiledImage::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(0, 1, 0)).unwrap(),
        ];
        let mut layer = PixelLayer::from_frames(frames);
        assert_eq!(layer.frame_count(), 2);
        assert!(layer.frame(5).is_none());
        assert!(layer.frame_mut(5).is_none());

        assert!(!layer.set_frame(5, TiledImage::new(4, 4, PixelFormat::Rgba8)));
        assert!(layer.set_frame(1, TiledImage::new(4, 4, PixelFormat::Rgba8)));
    }

    // -- Masks & selection ---------------------------------------------------

    #[test]
    fn mask_opaque_allocates_nothing() {
        let mask = Mask::opaque();
        assert!(mask.is_opaque());
        assert!(matches!(mask.data, crate::mask::MaskData::Grayscale(None)));
    }

    #[test]
    fn selection_invert() {
        // All → nothing.
        let mut sel = SelectionMask::all();
        sel.invert(64, 64).unwrap();
        assert!(sel.has_mask());
        assert!(sel.tiles.as_ref().unwrap().get_pixel(0, 0).luminance() < 128);

        // Nothing → all (absent black tiles become white).
        sel.invert(64, 64).unwrap();
        let img = sel.tiles.as_ref().unwrap();
        assert_eq!(img.tile_count(), 1); // one tile covers the 64×64 canvas
        assert!(img.get_pixel(63, 63).luminance() > 200);
    }

    #[test]
    fn selection_clear() {
        let mut sel = SelectionMask::all();
        sel.clear(32, 32);
        assert!(sel.has_mask());
        let img = sel.tiles.as_ref().unwrap();
        // Full-black Gray8 mask: nothing selected.
        assert_eq!(img.get_pixel(0, 0).r, 0);
        assert!(!sel.is_all());
    }

    // -- Timeline ------------------------------------------------------------

    #[test]
    fn timeline_track_sampling_linear() {
        let mut track = Track::new(NodeId(3), AnimatableProp::Opacity);
        track.add_keyframe(Keyframe::new(0, AnimValue::Scalar(0.0), Easing::Linear));
        track.add_keyframe(Keyframe::new(10, AnimValue::Scalar(1.0), Easing::Linear));

        assert_eq!(track.sample(0), Some(AnimValue::Scalar(0.0)));
        assert_eq!(track.sample(10), Some(AnimValue::Scalar(1.0)));
        assert_eq!(track.sample(5), Some(AnimValue::Scalar(0.5)));
        // Before first / after last clamp to the nearest keyframe.
        assert_eq!(track.sample(100), Some(AnimValue::Scalar(1.0)));
    }

    #[test]
    fn timeline_track_sampling_hold_and_insert_order() {
        let mut track = Track::new(NodeId(1), AnimatableProp::Opacity);
        // Insert out of order — must be kept sorted by frame.
        track.add_keyframe(Keyframe::new(20, AnimValue::Scalar(1.0), Easing::Hold));
        track.add_keyframe(Keyframe::new(0, AnimValue::Scalar(0.0), Easing::Hold));
        assert_eq!(
            track.keyframes.iter().map(|k| k.frame).collect::<Vec<_>>(),
            vec![0, 20]
        );
        // Hold keeps the previous value across the span.
        assert_eq!(track.sample(10), Some(AnimValue::Scalar(0.0)));

        // Replace at an existing frame.
        track.add_keyframe(Keyframe::new(20, AnimValue::Scalar(0.5), Easing::Hold));
        assert_eq!(track.sample(20), Some(AnimValue::Scalar(0.5)));
        assert!(track.remove_keyframe_at(20));
        assert!(!track.remove_keyframe_at(20));
    }

    #[test]
    fn timeline_sample_by_node_prop() {
        let mut timeline = Timeline::new(24);
        timeline.add_track(NodeId(7), AnimatableProp::Transform);
        timeline.track_mut(NodeId(7), AnimatableProp::Transform).unwrap()
            .add_keyframe(Keyframe::new(0, AnimValue::Vec2(0.0, 0.0), Easing::Linear));
        timeline.track_mut(NodeId(7), AnimatableProp::Transform).unwrap()
            .add_keyframe(Keyframe::new(24, AnimValue::Vec2(24.0, 24.0), Easing::Linear));

        assert_eq!(
            timeline.sample(NodeId(7), AnimatableProp::Transform, 12),
            Some(AnimValue::Vec2(12.0, 12.0))
        );
        assert_eq!(timeline.frame_to_seconds(48), 2.0);
        assert_eq!(timeline.seconds_to_frame(1.0), 24);
        assert!(timeline.remove_track(NodeId(7), AnimatableProp::Transform));
    }

    #[test]
    fn easing_curve_boundaries() {
        assert_eq!(Easing::Linear.apply(0.5), 0.5);
        assert_eq!(Easing::EaseIn.apply(0.0), 0.0);
        assert_eq!(Easing::EaseIn.apply(1.0), 1.0);
        assert!(Easing::EaseInOut.apply(0.25) < 0.25);
        assert_eq!(Easing::EaseIn.apply(-1.0), 0.0); // clamped
        assert_eq!(Easing::EaseOut.apply(2.0), 1.0); // clamped
    }

    #[test]
    fn anim_value_lerp() {
        let a = AnimValue::Color(Color::new(0.0, 0.0, 0.0, 1.0));
        let b = AnimValue::Color(Color::new(1.0, 0.5, 0.0, 1.0));
        assert_eq!(a.lerp(b, 1.0), b);
        // Mismatched variants keep the earlier value.
        assert_eq!(a.lerp(AnimValue::Scalar(3.0), 0.5), a);
    }

    // -- Text ----------------------------------------------------------------

    #[test]
    fn text_runs_validation() {
        let mut text = TextObject::new(ResourceId(1), 12.0);
        text.text = "hello world".into();
        assert!(text.validate_runs()); // no runs yet → trivially valid
        assert!(text.run_at(1).is_none());

        // Two non-overlapping runs covering the whole string.
        let r0 = crate::text::TextRun {
            start: 0,
            end: 6,
            font: ResourceId(1),
            size: 12.0,
            color: Color::black(),
            bold: false,
            italic: false,
        };
        let r1 = crate::text::TextRun {
            start: 6,
            end: 11,
            font: ResourceId(1),
            size: 12.0,
            color: Color::black(),
            bold: false,
            italic: false,
        };
        assert!(text.add_run(r0));
        assert!(text.add_run(r1));
        assert!(text.validate_runs());
        assert_eq!(text.run_at(1), Some(0));
        assert_eq!(text.run_at(7), Some(1));

        // Overlapping run must be rejected.
        let overlap = crate::text::TextRun {
            start: 5,
            end: 8,
            font: ResourceId(1),
            size: 12.0,
            color: Color::black(),
            bold: false,
            italic: false,
        };
        assert!(!text.add_run(overlap));
        // Out-of-bounds run must be rejected.
        let oob = crate::text::TextRun {
            start: 100,
            end: 110,
            font: ResourceId(1),
            size: 12.0,
            color: Color::black(),
            bold: false,
            italic: false,
        };
        assert!(!text.add_run(oob));

        assert!(text.remove_run_at(7).is_some());
        assert!(text.validate_runs());
        assert_eq!(text.run_at(7), None);
    }

    // -- Vector --------------------------------------------------------------

    #[test]
    fn vector_bounds() {
        let mut obj = VectorObject::new();
        assert!(obj.bounds().is_none());
        assert!(obj.is_empty());

        let mut path = VectorPath::new();
        path.push(PathNode::new(crate::types::Point::new(0.0, 0.0)));
        path.push(PathNode::new(crate::types::Point::new(10.0, 20.0)));
        obj.add_path(path);
        obj.add_path(VectorPath::new()); // empty path contributes nothing

        let (min, max) = obj.bounds().unwrap();
        assert_eq!(min, crate::types::Point::new(0.0, 0.0));
        assert_eq!(max, crate::types::Point::new(10.0, 20.0));
    }

    // -- Transforms ----------------------------------------------------------

    #[test]
    fn transform_point() {
        let t = Transform2D::identity()
            .with_scale(2.0, 2.0)
            .with_translation(10.0, 0.0);
        let p = t.transform_point(crate::types::Point::new(1.0, 1.0));
        assert!((p.x - 12.0).abs() < 1e-5);
        assert!((p.y - 2.0).abs() < 1e-5);
    }

    #[test]
    fn color_lerp() {
        let c = Color::black().lerp(Color::white(), 0.5);
        assert!((c.r - 0.5).abs() < 1e-6);
    }

    // -- Document ------------------------------------------------------------

    #[test]
    fn document_serialize_roundtrip() {
        let mut doc = Document::new(DocumentId(7), "roundtrip", 256, 256).unwrap();
        let root = doc.scene.root();
        doc.scene
            .add_node(root, "bg", NodeContent::Pixel(PixelLayer::blank(256, 256, PixelFormat::Rgba8)))
            .unwrap();

        let json = serde_json::to_string(&doc).unwrap();
        let back: Document = serde_json::from_str(&json).unwrap();
        assert_eq!(doc, back);
        assert_eq!(back.name, "roundtrip");
        assert_eq!(back.scene.node_count(), 2);
    }

    #[test]
    fn document_json_helpers() {
        let mut doc = Document::new(DocumentId(9), "kld", 64, 64).unwrap();
        doc.scene
            .add_node(
                doc.root(),
                "px",
                NodeContent::Pixel(PixelLayer::blank(64, 64, PixelFormat::Rgba8)),
            )
            .unwrap();
        doc.touch();

        let json = doc.to_json_pretty().unwrap();
        let back = Document::from_json(&json).unwrap();
        assert_eq!(back, doc);
    }

    #[test]
    fn history_entry() {
        let entry = crate::document::HistoryEntry::new(42, "Brush stroke");
        assert_eq!(entry.id, 42);
        assert_eq!(entry.label, "Brush stroke");
    }

    #[test]
    fn document_size_validation() {
        // Valid dimensions
        assert!(Document::new(DocumentId(1), "ok", 1, 1).is_ok());
        assert!(Document::new(DocumentId(2), "ok", 32768, 32768).is_ok());
        assert!(Document::new(DocumentId(3), "ok", 1920, 1080).is_ok());

        // Invalid dimensions
        assert!(Document::new(DocumentId(4), "bad", 0, 100).is_err());
        assert!(Document::new(DocumentId(5), "bad", 100, 0).is_err());
        assert!(Document::new(DocumentId(6), "bad", 32769, 100).is_err());
        assert!(Document::new(DocumentId(7), "bad", 100, 99999).is_err());
    }

    #[test]
    fn kld_format_roundtrip() {
        let mut doc = Document::new(DocumentId(10), "kld_test", 128, 128).unwrap();
        doc.scene
            .add_node(
                doc.root(),
                "layer",
                NodeContent::Pixel(PixelLayer::blank(128, 128, PixelFormat::Rgba8)),
            )
            .unwrap();

        // Serialize to binary
        let bytes = doc.to_kld().unwrap();
        assert!(crate::format::KldFormat::is_kld_header(&bytes));
        assert_eq!(&bytes[0..4], b"KALD");

        // Deserialize back
        let back = Document::from_kld(&bytes).unwrap();
        assert_eq!(back, doc);
        assert_eq!(back.name, "kld_test");
        assert_eq!(back.size.width, 128);
    }

    #[test]
    fn kld_format_rejects_invalid_magic() {
        let bytes = b"NOT_A_KLD_FILE";
        assert!(!crate::format::KldFormat::is_kld_header(bytes));
        assert!(Document::from_kld(bytes).is_err());
    }

    #[test]
    fn kld_chunk_serialization() {
        use crate::format::{KldChunk, KldFormat, CHUNK_DOCUMENT};

        let chunk = KldChunk::document(b"test data".to_vec());
        let format = KldFormat::default();
        let bytes = KldFormat::serialize_chunks(&format, &[chunk]);

        let (parsed_format, chunks) = KldFormat::deserialize_chunks(&bytes).unwrap();
        assert_eq!(parsed_format.version, 1);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].chunk_type, CHUNK_DOCUMENT);
        assert_eq!(chunks[0].data, b"test data");
    }
}
