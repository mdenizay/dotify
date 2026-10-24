//! Native application menu. Item IDs are forwarded to the UI as `menu`
//! events; keep them in sync with `Controller.menu` in `src/app/controller.ts`.

use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::{AppHandle, Runtime};

fn item<R: Runtime>(
    app: &AppHandle<R>,
    id: &str,
    text: &str,
    accel: Option<&str>,
) -> tauri::Result<tauri::menu::MenuItem<R>> {
    let mut b = MenuItemBuilder::with_id(id, text);
    if let Some(a) = accel {
        b = b.accelerator(a);
    }
    b.build(app)
}

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let app_menu = SubmenuBuilder::new(app, "Dotify")
        .item(&PredefinedMenuItem::about(app, Some("About Dotify"), None)?)
        .item(&item(app, "check_updates", "Check for Updates…", None)?)
        .separator()
        .item(&PredefinedMenuItem::services(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::hide(app, None)?)
        .item(&PredefinedMenuItem::hide_others(app, None)?)
        .item(&PredefinedMenuItem::show_all(app, None)?)
        .separator()
        .item(&item(app, "quit", "Quit Dotify", Some("CmdOrCtrl+Q"))?)
        .build()?;

    let file = SubmenuBuilder::new(app, "File")
        .item(&item(app, "new", "New…", Some("CmdOrCtrl+N"))?)
        .item(&item(app, "open", "Open…", Some("CmdOrCtrl+O"))?)
        .separator()
        .item(&item(app, "save", "Save", Some("CmdOrCtrl+S"))?)
        .item(&item(app, "save_as", "Save As…", Some("CmdOrCtrl+Shift+S"))?)
        .separator()
        .item(&item(app, "export_png", "Export PNG…", Some("CmdOrCtrl+E"))?)
        .item(&item(app, "export_all", "Export All Artboards…", Some("CmdOrCtrl+Shift+E"))?)
        .separator()
        .item(&item(app, "close", "Close Window", Some("CmdOrCtrl+W"))?)
        .build()?;

    let edit = SubmenuBuilder::new(app, "Edit")
        .item(&item(app, "undo", "Undo", Some("CmdOrCtrl+Z"))?)
        .item(&item(app, "redo", "Redo", Some("CmdOrCtrl+Shift+Z"))?)
        .separator()
        .item(&PredefinedMenuItem::cut(app, None)?)
        .item(&PredefinedMenuItem::copy(app, None)?)
        .item(&PredefinedMenuItem::paste(app, None)?)
        .item(&PredefinedMenuItem::select_all(app, None)?)
        .build()?;

    let view = SubmenuBuilder::new(app, "View")
        .item(&item(app, "zoom_in", "Zoom In", Some("CmdOrCtrl+="))?)
        .item(&item(app, "zoom_out", "Zoom Out", Some("CmdOrCtrl+-"))?)
        .item(&item(app, "zoom_actual", "Actual Size", Some("CmdOrCtrl+1"))?)
        .item(&item(app, "zoom_fit", "Fit to Window", Some("CmdOrCtrl+0"))?)
        .separator()
        .item(&item(app, "toggle_overview", "Artboard Overview", Some("CmdOrCtrl+Shift+O"))?)
        .item(&item(app, "toggle_library", "Show Library", Some("CmdOrCtrl+Shift+L"))?)
        .separator()
        .item(&item(app, "toggle_grid", "Show Pixel Grid", Some("CmdOrCtrl+'"))?)
        .item(&item(app, "toggle_rulers", "Show Rulers", Some("CmdOrCtrl+Shift+R"))?)
        .item(&item(app, "toggle_theme", "Toggle Light/Dark", None)?)
        .separator()
        .item(&PredefinedMenuItem::fullscreen(app, None)?)
        .build()?;

    let window = SubmenuBuilder::new(app, "Window")
        .item(&PredefinedMenuItem::minimize(app, None)?)
        .item(&PredefinedMenuItem::maximize(app, None)?)
        .build()?;

    MenuBuilder::new(app).items(&[&app_menu, &file, &edit, &view, &window]).build()
}
