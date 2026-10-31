//! Tauri commands for opening and converting an instance's schematics.
//!
//! Only IPC glue; the parsing, the limits on hostile files and the conversion rules are in
//! `aether_app::schematics`, `aether-schematic` and `aether-storage`.

use aether_app::schematics::{self, ConversionTargets, ConversionView, SchematicView};

use super::host::run;

/// Opens a schematic for the 3D viewer.
#[tauri::command]
pub async fn read_instance_schematic(
    app: tauri::AppHandle,
    id: String,
    name: String,
) -> Result<SchematicView, String> {
    run(app, move |host| schematics::open(host, &id, &name)).await
}

/// Converts a schematic to another format and, optionally, another Minecraft data version,
/// saving the result as a new file in the same folder.
#[tauri::command]
pub async fn convert_instance_schematic(
    app: tauri::AppHandle,
    id: String,
    name: String,
    format: String,
    data_version: Option<i32>,
) -> Result<ConversionView, String> {
    run(app, move |host| {
        schematics::convert(host, &id, &name, &format, data_version)
    })
    .await
}

/// The formats and Minecraft versions the conversion dialog offers.
#[tauri::command]
pub fn schematic_conversion_targets() -> ConversionTargets {
    schematics::conversion_targets()
}
