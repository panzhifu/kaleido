//! Tests for [`TiledImage`] — kept separate to keep [`tile.rs] focused.

#[cfg(test)]
mod tests {
    use crate::pixel::{ImageMetadata, Pixel, PixelFormat};
    use crate::tile::TiledImage;

    #[test]
    fn test_tiled_image_create() {
        let img = TiledImage::new(512, 512, PixelFormat::Rgba8);
        assert_eq!(img.width(), 512);
        assert_eq!(img.height(), 512);
        assert_eq!(img.tile_count(), 0);
        assert_eq!(img.tile_cols(), 2);
        assert_eq!(img.tile_rows(), 2);
    }

    #[test]
    fn test_tiled_image_with_color() {
        let img =
            TiledImage::with_color(512, 512, PixelFormat::Rgba8, Pixel::new(255, 0, 0, 255)).unwrap();
        assert_eq!(img.tile_count(), 4);
        assert_eq!(img.get_pixel(0, 0), Pixel::new(255, 0, 0, 255));
        assert_eq!(img.get_pixel(511, 511), Pixel::new(255, 0, 0, 255));
    }

    #[test]
    fn test_from_rgba() {
        let data = [255, 0, 0, 255].repeat(25);
        let img = TiledImage::from_rgba(5, 5, data).unwrap();
        assert_eq!(img.width(), 5);
        assert_eq!(img.height(), 5);
        assert_eq!(img.get_pixel(0, 0), Pixel::new(255, 0, 0, 255));
    }

    #[test]
    fn test_tiled_image_sparse() {
        let mut img = TiledImage::new(1024, 1024, PixelFormat::Rgba8);
        assert_eq!(img.tile_count(), 0);
        img.set_pixel(500, 500, Pixel::new(1, 2, 3, 4));
        assert_eq!(img.tile_count(), 1);
        assert_eq!(img.get_pixel(500, 500), Pixel::new(1, 2, 3, 4));
        assert_eq!(img.get_pixel(0, 0), Pixel::new(0, 0, 0, 0));
    }

    #[test]
    fn test_tiled_image_non_multiple() {
        let img =
            TiledImage::with_color(300, 300, PixelFormat::Rgba8, Pixel::rgb(10, 20, 30)).unwrap();
        assert_eq!(img.tile_cols(), 2);
        assert_eq!(img.tile_rows(), 2);
        assert_eq!(img.get_pixel(299, 299), Pixel::rgb(10, 20, 30));
    }

    #[test]
    fn test_crop() {
        let mut img =
            TiledImage::with_color(10, 10, PixelFormat::Rgba8, Pixel::rgb(0, 0, 0)).unwrap();
        img.set_pixel(5, 5, Pixel::rgb(255, 0, 0));
        let cropped = img.crop(3, 3, 5, 5).unwrap();
        assert_eq!(cropped.width(), 5);
        assert_eq!(cropped.height(), 5);
        assert_eq!(cropped.get_pixel(2, 2), Pixel::rgb(255, 0, 0));
    }

    #[test]
    fn test_copy_from() {
        let mut dst =
            TiledImage::with_color(10, 10, PixelFormat::Rgba8, Pixel::rgb(0, 0, 0)).unwrap();
        let src = TiledImage::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(255, 0, 0)).unwrap();
        dst.copy_from(&src, 0, 0, 3, 3, 4, 4).unwrap();
        assert_eq!(dst.get_pixel(3, 3), Pixel::rgb(255, 0, 0));
    }

    #[test]
    fn test_fill_rect() {
        let mut img = TiledImage::new(10, 10, PixelFormat::Rgba8);
        img.fill_rect(2, 2, 4, 4, Pixel::rgb(255, 0, 0));
        assert_eq!(img.get_pixel(3, 3), Pixel::rgb(255, 0, 0));
        assert_eq!(img.get_pixel(0, 0), Pixel::new(0, 0, 0, 0));
    }

    #[test]
    fn test_to_raw_vec_roundtrip() {
        let mut tiled = TiledImage::new(200, 200, PixelFormat::Rgba8);
        tiled.set_pixel(50, 50, Pixel::new(100, 150, 200, 255));
        tiled.set_pixel(199, 199, Pixel::new(10, 20, 30, 40));

        let raw = tiled.to_raw_vec();
        assert_eq!(raw.len(), 200 * 200 * 4);

        let tiled2 = TiledImage::from_data(200, 200, PixelFormat::Rgba8, raw).unwrap();
        assert_eq!(tiled2.get_pixel(50, 50), Pixel::new(100, 150, 200, 255));
        assert_eq!(tiled2.get_pixel(199, 199), Pixel::new(10, 20, 30, 40));
    }

    #[test]
    fn test_set_pixels_from_buffer() {
        let mut img = TiledImage::new(2, 2, PixelFormat::Rgba8);
        let buffer = vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
        ];
        img.set_pixels_from_buffer(&buffer).unwrap();
        assert_eq!(img.get_pixel(0, 0), Pixel::rgb(255, 0, 0));
        assert_eq!(img.get_pixel(1, 0), Pixel::rgb(0, 255, 0));
        assert_eq!(img.get_pixel(0, 1), Pixel::rgb(0, 0, 255));
        assert_eq!(img.get_pixel(1, 1), Pixel::rgb(255, 255, 0));
    }

    #[test]
    fn test_metadata() {
        let mut img =
            TiledImage::with_color(10, 10, PixelFormat::Rgba8, Pixel::rgb(255, 0, 0)).unwrap();
        img.metadata = ImageMetadata::new()
            .with_property("author", "kaleido")
            .with_description("test image");

        assert_eq!(img.metadata().get_property("author").unwrap(), "kaleido");
        assert_eq!(img.metadata().description.as_deref(), Some("test image"));
    }

    #[test]
    fn test_zero_dimensions() {
        assert!(TiledImage::new(0, 10, PixelFormat::Rgba8).is_empty());
        assert!(TiledImage::with_color(0, 10, PixelFormat::Rgba8, Pixel::rgb(0, 0, 0)).is_err());
    }

    #[test]
    fn test_dirty_tile_tracking() {
        let mut img = TiledImage::new(512, 512, PixelFormat::Rgba8);
        assert!(!img.has_dirty_tiles());

        img.set_pixel(10, 10, Pixel::rgb(1, 2, 3));
        img.set_pixel(300, 300, Pixel::rgb(4, 5, 6));
        assert_eq!(img.dirty_tile_count(), 2);

        let coords: Vec<_> = img.dirty_tile_coords().collect();
        assert!(coords.contains(&crate::tile_core::TileCoord::new(0, 0)));
        assert!(coords.contains(&crate::tile_core::TileCoord::new(1, 1)));

        img.clear_dirty();
        assert_eq!(img.dirty_tile_count(), 0);
    }

    #[test]
    fn test_invert_gray() {
        // Gray8 stores luminance, so use neutral-grey pixels for exact values.
        let mut img = TiledImage::with_color(10, 10, PixelFormat::Gray8, Pixel::rgb(200, 200, 200)).unwrap();
        img.set_pixel(0, 0, Pixel::rgb(10, 10, 10));
        img.invert_gray().unwrap();
        // 10 → 245, 200 → 55.
        assert_eq!(img.get_pixel(0, 0).r, 245);
        assert_eq!(img.get_pixel(5, 5).r, 55);

        let mut img2 = TiledImage::new(10, 10, PixelFormat::Gray8);
        img2.invert_gray().unwrap();
        assert_eq!(img2.tile_count(), 1); // absent black tiles materialized white
        assert_eq!(img2.get_pixel(9, 9).r, 255);

        // Non-gray formats are rejected.
        let mut rgba = TiledImage::new(10, 10, PixelFormat::Rgba8);
        assert!(rgba.invert_gray().is_err());
    }

    #[test]
    fn test_to_rgba_vec_non_rgba_format() {
        let img = TiledImage::with_color(300, 300, PixelFormat::Gray8, Pixel::rgb(128, 128, 128)).unwrap();
        let rgba = img.to_rgba_vec();
        assert_eq!(rgba.len(), 300 * 300 * 4);
        assert_eq!(&rgba[0..4], &[128, 128, 128, 255]);
        assert_eq!(&rgba[(300 * 299 + 299) * 4..(300 * 299 + 299) * 4 + 4], &[128, 128, 128, 255]);
    }

    #[test]
    fn test_to_rgba_vec_rgba8_sparse() {
        let mut img = TiledImage::new(300, 300, PixelFormat::Rgba8);
        img.set_pixel(0, 0, Pixel::rgb(9, 8, 7));
        let rgba = img.to_rgba_vec();
        assert_eq!(&rgba[0..4], &[9, 8, 7, 255]);
        // Unallocated pixels read as transparent black.
        assert_eq!(&rgba[4..8], &[0, 0, 0, 0]);
    }

    #[test]
    fn test_fill_rect_across_tiles() {
        let mut img = TiledImage::new(600, 600, PixelFormat::Rgba8);
        img.fill_rect(250, 250, 100, 100, Pixel::rgb(255, 0, 0));
        assert_eq!(img.get_pixel(250, 250), Pixel::rgb(255, 0, 0));
        assert_eq!(img.get_pixel(349, 349), Pixel::rgb(255, 0, 0));
        assert_eq!(img.get_pixel(350, 250), Pixel::new(0, 0, 0, 0)); // outside
        assert_eq!(img.get_pixel(250, 350), Pixel::new(0, 0, 0, 0)); // outside
    }

    #[test]
    fn test_crop_across_tiles() {
        // A crop that spans tile boundaries must copy correctly.
        let mut src = TiledImage::new(600, 600, PixelFormat::Rgba8);
        src.fill_rect(200, 200, 200, 200, Pixel::rgb(1, 2, 3));

        let cropped = src.crop(180, 180, 240, 240).unwrap();
        assert_eq!(cropped.width(), 240);
        assert_eq!(cropped.get_pixel(20, 20), Pixel::rgb(1, 2, 3)); // inside the rect
        assert_eq!(cropped.get_pixel(0, 0), Pixel::new(0, 0, 0, 0)); // outside
        assert_eq!(cropped.get_pixel(239, 239), Pixel::new(0, 0, 0, 0));

        assert!(src.crop(100, 100, 1000, 10).is_err());
        assert!(src.crop(0, 0, 0, 10).is_err());
    }

    #[test]
    fn test_copy_from_cross_tile() {
        let mut dst = TiledImage::new(600, 600, PixelFormat::Rgba8);
        let mut src = TiledImage::new(300, 300, PixelFormat::Rgba8);
        src.fill_rect(100, 100, 100, 100, Pixel::rgb(9, 9, 9));

        // Copy the *filled* region across a tile boundary.
        dst.copy_from(&src, 100, 100, 500, 500, 100, 100).unwrap();
        assert_eq!(dst.get_pixel(500, 500), Pixel::rgb(9, 9, 9));
        assert_eq!(dst.get_pixel(599, 599), Pixel::rgb(9, 9, 9));
        assert_eq!(dst.get_pixel(600, 600), Pixel::new(0, 0, 0, 0));

        // Format mismatch is rejected.
        let gray = TiledImage::new(10, 10, PixelFormat::Gray8);
        assert!(dst.copy_from(&gray, 0, 0, 0, 0, 10, 10).is_err());
    }

    #[test]
    fn test_tile_cow_sharing() {
        let mut a = TiledImage::with_color(256, 256, PixelFormat::Rgba8, Pixel::rgb(1, 1, 1)).unwrap();
        let b = a.clone();
        assert!(a.is_shared());
        // Writing must break sharing (COW) and not affect the clone.
        a.set_pixel(0, 0, Pixel::rgb(2, 2, 2));
        assert_eq!(b.get_pixel(0, 0), Pixel::rgb(1, 1, 1));
    }
}
