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
// Tool command helpers
// ---------------------------------------------------------------------------

/// Parses a JSON string into a serde_json::Value for tool params.
fn parse_tool_params(s: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str(s).map_err(|e| format!("Invalid JSON params: {e}"))
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Parses a string into an [`ImageFormat`] for CLI argument handling.
fn parse_image_format(s: &str) -> Result<ImageFormat, String> {
    ImageFormat::from_extension(s).ok_or_else(|| {
        format!(
            "Unknown format '{}'. Supported: jpg, jpeg, png, webp, bmp, gif, tif, tiff",
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

    /// List all registered tools with their schemas.
    ListTools,

    /// Show the JSON schema for a specific tool.
    ToolSchema {
        /// Tool name.
        name: String,
    },

    /// Run a tool on an image with custom parameters.
    Run {
        /// Input image path.
        input: String,
        /// Output image path.
        output: String,
        /// Tool name.
        #[arg(long)]
        tool: String,
        /// Tool parameters as a JSON string.
        #[arg(long, value_name = "JSON", value_parser = parse_tool_params)]
        params: Option<serde_json::Value>,
    },

    /// Generate a new tool from an AI description (JSON).
    CreateTool {
        /// Tool description as JSON.
        #[arg(long, value_name = "JSON", value_parser = parse_tool_params)]
        description: serde_json::Value,
        /// Input image path (for testing the tool).
        #[arg(long)]
        input: Option<String>,
        /// Output image path (for testing the tool).
        #[arg(long)]
        output: Option<String>,
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
        Commands::Invert { input, output } => {
            cmd_invert(codec.as_ref(), tools.as_ref(), &input, &output)
        }
        Commands::ListTools => cmd_list_tools(tools.as_ref()),
        Commands::ToolSchema { name } => cmd_tool_schema(tools.as_ref(), &name),
        Commands::Run {
            input,
            output,
            tool,
            params,
        } => cmd_run(
            codec.as_ref(),
            tools.as_ref(),
            &input,
            &output,
            &tool,
            params,
        ),
        Commands::CreateTool {
            description,
            input,
            output,
        } => cmd_create_tool(
            app.clone(),
            codec.as_ref(),
            &description,
            input.as_deref(),
            output.as_deref(),
        ),
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

// ---------------------------------------------------------------------------
// New command handlers for tool management
// ---------------------------------------------------------------------------

fn cmd_list_tools(tools: &dyn ToolRegistry) -> anyhow::Result<()> {
    let all_tools = tools.tools();
    if all_tools.is_empty() {
        println!("No tools registered.");
        return Ok(());
    }

    println!("Registered tools ({}):", all_tools.len());
    println!("{:<20} {:<20} {}", "NAME", "MENU PATH", "DESCRIPTION");
    println!("{:-<70}", "");
    for tool in &all_tools {
        println!(
            "{:<20} {:<20} {}",
            tool.name(),
            tool.menu_path(),
            tool.description()
        );
    }
    Ok(())
}

fn cmd_tool_schema(tools: &dyn ToolRegistry, name: &str) -> anyhow::Result<()> {
    let tool = tools.get(name).with_context(|| {
        format!("Tool '{name}' not found. Use 'list-tools' to see available tools.")
    })?;

    let schema = tool.schema();
    let json_schema = schema.to_json_schema();
    let pretty = serde_json::to_string_pretty(&json_schema)?;
    println!("Tool: {}", tool.name());
    println!("Menu: {}", tool.menu_path());
    println!("Description: {}", tool.description());
    println!("\nSchema:");
    println!("{pretty}");
    Ok(())
}

fn cmd_run(
    codec: &dyn FileCodec,
    tools: &dyn ToolRegistry,
    input: &str,
    output: &str,
    tool_name: &str,
    params: Option<serde_json::Value>,
) -> anyhow::Result<()> {
    let mut image = codec
        .load(Path::new(input))
        .with_context(|| format!("Failed to load image: {input}"))?;

    let tool = tools.get(tool_name).with_context(|| {
        format!("Tool '{tool_name}' not found. Use 'list-tools' to see available tools.")
    })?;

    let params = params.unwrap_or_else(|| json!({}));

    // Validate params against schema.
    let schema = tool.schema();
    if let Err(e) = schema.validate_params(&params) {
        anyhow::bail!("Invalid parameters for tool '{tool_name}': {e}");
    }

    // Apply defaults.
    let params_with_defaults = schema.apply_defaults(&params);

    tool.apply(&mut image, &params_with_defaults)
        .with_context(|| format!("Failed to run tool: {tool_name}"))?;

    codec
        .save(Path::new(output), &image)
        .with_context(|| format!("Failed to save image: {output}"))?;

    println!("Applied tool '{tool_name}': {input} → {output}");
    Ok(())
}

fn cmd_create_tool(
    app: KaleidoApp,
    codec: &dyn FileCodec,
    description: &serde_json::Value,
    input: Option<&str>,
    output: Option<&str>,
) -> anyhow::Result<()> {
    let _tool_name = description["name"]
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "ai_tool".to_string());

    // Create the AI tool with a default apply function that converts to grayscale.
    // In a real scenario, the AI would provide the actual implementation.
    let tool = app
        .create_ai_tool(description, |image, _params| {
            // Default implementation: convert to grayscale.
            // The AI would replace this with actual logic.
            for y in 0..image.height() {
                for x in 0..image.width() {
                    let p = image.get_pixel(x, y)?;
                    let gray =
                        ((p.r as u32 * 299 + p.g as u32 * 587 + p.b as u32 * 114) / 1000) as u8;
                    image.set_pixel(x, y, Pixel::new(gray, gray, gray, p.a))?;
                }
            }
            Ok(())
        })
        .with_context(|| "Failed to create AI tool")?;

    println!("Created AI tool: {}", tool.name());
    println!("Description: {}", tool.description());

    // If input/output provided, test the tool.
    if let (Some(input), Some(output)) = (input, output) {
        let mut image = codec
            .load(Path::new(input))
            .with_context(|| format!("Failed to load image: {input}"))?;

        let schema = tool.schema();
        let params = schema.apply_defaults(&json!({}));

        tool.apply(&mut image, &params)
            .with_context(|| format!("Failed to apply tool: {}", tool.name()))?;

        codec
            .save(Path::new(output), &image)
            .with_context(|| format!("Failed to save image: {output}"))?;

        println!(
            "Applied tool '{}' and saved: {input} → {output}",
            tool.name()
        );
    }

    Ok(())
}
