use anyhow::Context;
use clap::{Parser, Subcommand};
use kaleido_core::{Image, Pixel, PixelFormat};
use kaleido_services::app::{AppConfig, KaleidoApp};
use kaleido_tool_brightness::{BrightnessToolConfig, brightness_tool_plugin};
use kaleido_tool_invert::invert_tool_plugin;
use kaleido_traits::{FileCodec, ImageFormat, ToolRegistry};
use serde_json::json;
use std::path::Path;

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Parses a string into an [`ImageFormat`] for CLI argument handling.
fn parse_image_format(s: &str) -> Result<ImageFormat, String> {
    ImageFormat::from_extension(s).ok_or_else(|| {
        format!(
            "Unknown format '{}'. Supported: jpg, jpeg, png, webp, bmp, gif",
            s
        )
    })
}

// ---------------------------------------------------------------------------
// CLI definition
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "kaleido",
    about = "Kaleido — AI-native image workstation (CLI)",
    version = "0.1.0",
    infer_subcommands = true
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Display image information (dimensions, format, etc.).
    Info {
        /// Path to the image file.
        path: String,
    },

    /// Convert an image to a different format.
    Convert {
        /// Input image path.
        input: String,
        /// Output image path (format inferred from extension).
        output: String,
        /// Explicitly specify the output format (overrides extension).
        #[arg(long, value_name = "FORMAT", value_parser = parse_image_format)]
        format: Option<ImageFormat>,
    },

    /// List supported read / write formats.
    ListFormats,

    /// Adjust image brightness.
    Brightness {
        /// Input image path.
        input: String,
        /// Output image path.
        output: String,
        /// Brightness adjustment value (-255 to 255).
        #[arg(long, default_value = "0")]
        value: i32,
    },

    /// Resize an image to the given dimensions.
    Resize {
        /// Input image path.
        input: String,
        /// Output image path.
        output: String,
        /// Target width in pixels.
        #[arg(long)]
        width: u32,
        /// Target height in pixels.
        #[arg(long)]
        height: u32,
    },

    /// Convert an image to grayscale.
    Grayscale {
        /// Input image path.
        input: String,
        /// Output image path.
        output: String,
    },

    /// Invert all pixel colours (negative).
    Invert {
        /// Input image path.
        input: String,
        /// Output image path.
        output: String,
    },
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Boot the Cordis-managed application container and grab the codec
    // service from it (single source of truth for file I/O).
    let app = KaleidoApp::boot(AppConfig::default())?;

    // Install the tool plugins: commands are provided by plugins, not
    // hard-coded. Installing/uninstalling a plugin adds/removes commands.
    app.context()
        .plugin(brightness_tool_plugin(), BrightnessToolConfig::default());
    app.context().plugin(invert_tool_plugin(), ());

    let codec = app.file_codec();
    let tools = app.tool_registry();

    match cli.command {
        Commands::Info { path } => cmd_info(codec.as_ref(), &path),
        Commands::Convert {
            input,
            output,
            format,
        } => cmd_convert(codec.as_ref(), &input, &output, format),
        Commands::ListFormats => cmd_list_formats(codec.as_ref()),
        Commands::Brightness {
            input,
            output,
            value,
        } => cmd_brightness(codec.as_ref(), tools.as_ref(), &input, &output, value),
        Commands::Resize {
            input,
            output,
            width,
            height,
        } => cmd_resize(codec.as_ref(), &input, &output, width, height),
        Commands::Grayscale { input, output } => cmd_grayscale(codec.as_ref(), &input, &output),
        Commands::Invert { input, output } => cmd_invert(codec.as_ref(), tools.as_ref(), &input, &output),
    }
}

// ---------------------------------------------------------------------------
// Command handlers
// ---------------------------------------------------------------------------

fn cmd_info(codec: &dyn FileCodec, path: &str) -> anyhow::Result<()> {
    let image = codec
        .load(Path::new(path))
        .with_context(|| format!("Failed to load image: {}", path))?;

    let metadata = codec.read_metadata(Path::new(path))?;

    println!("Image: {}", path);
    println!("  Dimensions: {} × {}", image.width(), image.height());
    println!("  Pixel format: {:?}", image.format());
    println!("  Pixel count: {}", image.pixel_count());
    println!(
        "  Memory: {} bytes",
        image.pixel_count() as usize * image.format().bytes_per_pixel()
    );
    println!(
        "  Created at: {}",
        metadata.created_at.as_deref().unwrap_or("N/A")
    );
    println!(
        "  Description: {}",
        metadata.description.as_deref().unwrap_or("N/A")
    );

    Ok(())
}

fn cmd_convert(
    codec: &dyn FileCodec,
    input: &str,
    output: &str,
    format: Option<ImageFormat>,
) -> anyhow::Result<()> {
    let image = codec
        .load(Path::new(input))
        .with_context(|| format!("Failed to load image: {}", input))?;

    match format {
        Some(fmt) => codec
            .save_with_format(Path::new(output), &image, fmt)
            .with_context(|| format!("Failed to save image: {}", output))?,
        None => codec
            .save(Path::new(output), &image)
            .with_context(|| format!("Failed to save image: {}", output))?,
    }

    println!("Converted {} → {}", input, output);
    Ok(())
}

fn cmd_list_formats(codec: &dyn FileCodec) -> anyhow::Result<()> {
    println!("Supported read formats:");
    for format in codec.supported_read_formats() {
        println!("  {:<8} ({})", format.extension(), format.mime_type());
    }

    println!("\nSupported write formats:");
    for format in codec.supported_write_formats() {
        println!("  {:<8} ({})", format.extension(), format.mime_type());
    }

    Ok(())
}

fn cmd_brightness(
    codec: &dyn FileCodec,
    tools: &dyn ToolRegistry,
    input: &str,
    output: &str,
    value: i32,
) -> anyhow::Result<()> {
    let mut image = codec
        .load(Path::new(input))
        .with_context(|| format!("Failed to load image: {}", input))?;

    let tool = tools
        .get("brightness")
        .context("brightness plugin is not installed")?;
    tool.apply(&mut image, &json!({ "value": value }))
        .with_context(|| format!("Failed to run brightness tool: {}", input))?;

    codec
        .save(Path::new(output), &image)
        .with_context(|| format!("Failed to save image: {}", output))?;

    println!("Adjusted brightness by {value} (via plugin): {input} → {output}");
    Ok(())
}

fn cmd_resize(
    codec: &dyn FileCodec,
    input: &str,
    output: &str,
    width: u32,
    height: u32,
) -> anyhow::Result<()> {
    let image = codec
        .load(Path::new(input))
        .with_context(|| format!("Failed to load image: {}", input))?;

    let mut resized = Image::new(width, height, PixelFormat::Rgba8)
        .with_context(|| format!("Failed to create target image: {}×{}", width, height))?;

    let x_ratio = image.width() as f32 / width as f32;
    let y_ratio = image.height() as f32 / height as f32;

    for y in 0..height {
        for x in 0..width {
            let src_x = ((x as f32 * x_ratio) as u32).min(image.width() - 1);
            let src_y = ((y as f32 * y_ratio) as u32).min(image.height() - 1);
            let pixel = image
                .get_pixel(src_x, src_y)
                .with_context(|| format!("Failed to read pixel ({}, {})", src_x, src_y))?;
            resized
                .set_pixel(x, y, pixel)
                .with_context(|| format!("Failed to write pixel ({}, {})", x, y))?;
        }
    }

    codec
        .save(Path::new(output), &resized)
        .with_context(|| format!("Failed to save image: {}", output))?;

    println!(
        "Resized {}×{} → {}×{}: {input} → {output}",
        image.width(),
        image.height(),
        width,
        height
    );
    Ok(())
}

fn cmd_grayscale(codec: &dyn FileCodec, input: &str, output: &str) -> anyhow::Result<()> {
    let mut image = codec
        .load(Path::new(input))
        .with_context(|| format!("Failed to load image: {}", input))?;

    for y in 0..image.height() {
        for x in 0..image.width() {
            let pixel = image
                .get_pixel(x, y)
                .with_context(|| format!("Failed to read pixel ({}, {})", x, y))?;

            // ITU-R BT.601 luma coefficients: R*0.299 + G*0.587 + B*0.114
            let gray =
                ((pixel.r as u32 * 299 + pixel.g as u32 * 587 + pixel.b as u32 * 114) / 1000) as u8;

            image
                .set_pixel(x, y, Pixel::new(gray, gray, gray, pixel.a))
                .with_context(|| format!("Failed to write pixel ({}, {})", x, y))?;
        }
    }

    codec
        .save(Path::new(output), &image)
        .with_context(|| format!("Failed to save image: {}", output))?;

    println!("Converted to grayscale: {input} → {output}");
    Ok(())
}

fn cmd_invert(
    codec: &dyn FileCodec,
    tools: &dyn ToolRegistry,
    input: &str,
    output: &str,
) -> anyhow::Result<()> {
    let mut image = codec
        .load(Path::new(input))
        .with_context(|| format!("Failed to load image: {}", input))?;

    let tool = tools
        .get("invert")
        .context("invert plugin is not installed")?;
    tool.apply(&mut image, &json!({}))
        .with_context(|| format!("Failed to run invert tool: {}", input))?;

    codec
        .save(Path::new(output), &image)
        .with_context(|| format!("Failed to save image: {}", output))?;

    println!("Inverted colours (via plugin): {input} → {output}");
    Ok(())
}
