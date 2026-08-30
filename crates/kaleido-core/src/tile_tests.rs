//! Tests for [`TiledImage`] — kept separate to keep [`tile.rs] focused.

#[cfg(test)]
mod tests {
    use crate::pixel::{ImageMetadata, Pixel, PixelFormat};
    use crate::tile::TiledImage;

    #[test]
    fn test_tiled_image_create() {
        let img = TiledImage::new(256, 256, PixelFormat::Rgba8);
        assert_eq!(img.width(), 256);
        assert_eq!(img.height(), 256);
        assert_eq!(img.tile_count(), 0);
        assert_eq!(img.tile_cols(), 2);
        assert_eq!(img.tile_rows(), 2);
    }

    #[test]
    fn test_tiled_image_with_color() {
        let img =
            TiledImage::with_color(256, 256, PixelFormat::Rgba8, Pixel::new(255, 0, 0, 255)).unwrap();
        assert_eq!(img.tile_count(), 4);
        assert_eq!(img.get_pixel(0, 0), Pixel::new(255, 0, 0, 255));
        assert_eq!(img.get_pixel(255, 255), Pixel::new(255, 0, 0, 255));
    }

    #[test]
    fn test_from_rgba() {
        let data = vec![255, 0, 0, 255].repeat(25);
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
            TiledImage::with_color(200, 200, PixelFormat::Rgba8, Pixel::rgb(10, 20, 30)).unwrap();
        assert_eq!(img.tile_cols(), 2);
        assert_eq!(img.tile_rows(), 2);
        assert_eq!(img.get_pixel(199, 199), Pixel::rgb(10, 20, 30));
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
}
