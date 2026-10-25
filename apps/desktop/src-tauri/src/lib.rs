mod commands;
mod menu;
mod palettes;
mod session;

use tauri::{Emitter, Manager};

use session::{AppState, AUTOSAVE_INTERVAL};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .menu(menu::build)
        .on_menu_event(|app, event| {
            // The UI owns menu behaviour (it may need to confirm first).
            let _ = app.emit("menu", event.id().0.as_str());
        })
        .setup(|app| {
            let recovery_dir = app.path().app_data_dir()?.join("recovery");
            app.manage(AppState::new(recovery_dir));
            app.manage(palettes::PaletteStore::new(app.path().app_data_dir()?.join("palettes.json")));
            let handle = app.handle().clone();
            std::thread::Builder::new().name("autosave".into()).spawn(move || loop {
                std::thread::sleep(AUTOSAVE_INTERVAL);
                if let Err(e) = handle.state::<AppState>().autosave() {
                    eprintln!("autosave failed: {e}");
                }
            })?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::doc_view,
            commands::doc_new,
            commands::doc_open,
            commands::doc_save,
            commands::doc_save_as,
            commands::execute,
            commands::undo,
            commands::redo,
            commands::artboard_activate,
            commands::read_cel,
            commands::artboard_composite,
            commands::read_frame,
            commands::export_png,
            commands::export_all,
            commands::export_sheet,
            commands::export_gif,
            commands::export_godot,
            commands::recovery_status,
            commands::recovery_restore,
            commands::recovery_discard,
            palettes::palette_library,
            palettes::palette_import,
            palettes::palette_save,
            palettes::palette_delete,
            palettes::palette_export,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Dotify");
}
