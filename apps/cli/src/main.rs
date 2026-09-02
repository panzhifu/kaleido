//! Kaleido CLI — demonstrates the 12-manager service layer.
//!
//! This is a minimal CLI that boots the new service manager architecture
//! and exercises document lifecycle, layers, selection, rendering, and tasks.

use anyhow::Context;
use clap::{Parser, Subcommand};
use kaleido_core::PixelFormat;
use kaleido_services::app::{AppConfig, KaleidoApp};

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
    /// Boot the service layer and print a status report.
    Status,

    /// Run an end-to-end workflow: document → layers → selection → render → task.
    Demo,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let app = KaleidoApp::boot(AppConfig::default())?;

    match cli.command {
        Commands::Status => cmd_status(&app),
        Commands::Demo => cmd_demo(app),
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn cmd_status(app: &KaleidoApp) -> anyhow::Result<()> {
    let svc = app.app_service();
    println!("Kaleido {}", svc.version());
    println!("App name: {}", svc.name());
    println!("Editing mode: {}", svc.current_mode());

    let data = app.data_service();
    println!("Document open: {}", data.has_document());
    if let Some(size) = data.size() {
        println!("Canvas size: {} × {}", size.width, size.height);
    }
    println!("Has unsaved changes: N/A");

    let layers = app.layer_service();
    println!("Scene nodes: {}", layers.layer_count().unwrap_or(0));

    let selection = app.selection_service();
    println!(
        "Selection: {}",
        match selection.selection().unwrap_or(None) {
            Some(_) => "active",
            None => "none",
        }
    );

    let tasks = app.task_service();
    println!("Tracked tasks: {}", tasks.tasks().len());

    let resources = app.resource_service();
    println!("Resources: {}", resources.count());

    let plugins = app.plugin_service();
    println!("Plugins: {}", plugins.plugin_count());

    let render = app.render_service();
    println!("Render available: yes");

    app.app_service().notify("status check complete");
    Ok(())
}

fn cmd_demo(mut app: KaleidoApp) -> anyhow::Result<()> {
    // 1. Document lifecycle (data manager).
    let data = app.data_service();
    println!("1. Creating document...");
    data.new_document("demo", 64, 32)?;
    assert!(data.has_document());
    let size = data.size().unwrap();
    println!("   Created: {} × {}", size.width, size.height);

    // 2. Layer creation (layer manager).
    println!("2. Adding layers...");
    let layers = app.layer_service();
    let bg = layers.add_pixel_layer("Background", 64, 32, PixelFormat::Rgba8)?;
    println!("   Added 'Background' (id: {:?})", bg);
    let fg = layers.add_pixel_layer("Foreground", 64, 32, PixelFormat::Rgba8)?;
    println!("   Added 'Foreground' (id: {:?})", fg);

    // 3. Selection (selection manager).
    println!("3. Setting selection...");
    let selection = app.selection_service();
    selection.set(Some(kaleido_core::SelectionMask::none(64, 32)))?;
    println!("   Selection set: {}", selection.selection()?.unwrap().has_mask());
    selection.invert()?;
    println!("   Selection inverted");

    // 4. History manager.
    println!("4. History...");
    let history = app.history_service();
    println!("   Can undo: {}", history.can_undo());
    println!("   Can redo: {}", history.can_redo());
    println!("   Undo depth: {}", history.undo_depth());
    history.clear().unwrap();
    println!("   History cleared");

    // 5. Render (render manager).
    println!("5. Rendering...");
    let render = app.render_service();
    let image = render.render()?;
    println!(
        "   Rendered: {} × {} pixels",
        image.width(),
        image.height()
    );

    // 6. Task (task manager).
    println!("6. Background task...");
    let tasks = app.task_service();
    let id = tasks.spawn("demo task", Box::new(|| {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }))?;
    let status = tasks.join(id)?;
    println!("   Task {} finished: {:?}", id, status);

    // 7. Color (color manager).
    println!("7. Color profile...");
    let color = app.color_service();
    println!("   Color profile: {:?}", color.profile());

    // 8. Resource (resource manager).
    println!("8. Resources...");
    let resources = app.resource_service();
    let font_id = resources.register(kaleido_traits::resource::ResourceData::Font {
        name: "Demo Font".into(),
        bytes: vec![0, 1, 2, 3],
    })?;
    println!(
        "   Registered font (id: {:?}), total: {}",
        font_id,
        resources.count()
    );

    // 9. Shortcut (shortcut manager).
    println!("9. Shortcuts...");
    let shortcuts = app.shortcut_service();
    shortcuts.register_global(kaleido_traits::keyboard::ShortcutBinding {
        key: "ctrl+s".into(),
        action: "save".into(),
        source: kaleido_traits::keyboard::ShortcutSource::Default,
    })?;
    println!("   Registered global shortcut: ctrl+s → save");

    // 10. UI (ui manager).
    println!("10. UI...");
    let ui = app.ui_service();
    ui.set_status("demo complete");
    ui.notify("Demo workflow finished");
    println!("   Status: {}", ui.status());

    // 11. App (app manager) — software configuration.
    println!("11. App...");
    let app_svc = app.app_service();
    println!("    Name: {}", app_svc.name());
    println!("    Version: {}", app_svc.version());
    println!("    Mode: {}", app_svc.current_mode());
    let settings = app_svc.settings();
    println!("    Default canvas: {} × {}", settings.default_width, settings.default_height);
    println!("    Undo limit: {}", settings.undo_limit);
    println!("    Auto-save: {}s (0=disabled)", settings.auto_save_interval);
    println!("    Plugin dirs: {:?}", settings.plugin_dirs);
    println!("    Default mode: {}", settings.default_mode);

    // Modify a single setting.
    app_svc.set_setting("undo_limit", "100")?;
    println!("    Updated undo_limit to: {}", app_svc.get_setting("undo_limit").unwrap());

    // Switch editing mode.
    app_svc.set_mode("vector")?;
    println!("    Switched mode to: {}", app_svc.current_mode());

    // Update full settings.
    let mut new_settings = app_svc.settings();
    new_settings.default_width = 1920;
    new_settings.default_height = 1080;
    app_svc.update_settings(new_settings)?;
    println!("    Updated canvas to: {} × {}",
        app_svc.settings().default_width,
        app_svc.settings().default_height);

    // 12. Plugin (plugin manager).
    println!("12. Plugins...");
    let plugins = app.plugin_service();
    println!("    Installed: {}", plugins.plugin_count());

    println!("\nDemo complete!");
    app.dispose().with_context(|| "failed to dispose app")?;
    Ok(())
}
