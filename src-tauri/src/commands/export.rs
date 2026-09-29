use crate::{
    database::Database,
    project_list_export::{self, ExportResult},
};
use std::path::PathBuf;
use tauri::State;

#[tauri::command]
pub fn export_project_list(
    file_path: String,
    database: State<'_, Database>,
) -> Result<ExportResult, String> {
    let path = PathBuf::from(file_path);
    let connection = database.connection().map_err(|error| error.to_string())?;
    project_list_export::export(&connection, &path)
}
