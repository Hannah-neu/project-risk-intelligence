use crate::{database::Database, repositories::settings};
use tauri::State;

#[tauri::command]
pub fn get_setting(database: State<'_, Database>, key: String) -> Result<Option<String>, String> {
    if key.starts_with("enterprise_llm.") {
        return Err("Use the Enterprise configuration interface for these settings.".into());
    }
    let connection = database.connection().map_err(|error| error.to_string())?;
    settings::get(&connection, &key).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_setting(
    database: State<'_, Database>,
    key: String,
    value: String,
) -> Result<(), String> {
    if key.starts_with("enterprise_llm.") {
        return Err("Use the Enterprise configuration interface for these settings.".into());
    }
    let connection = database.connection().map_err(|error| error.to_string())?;
    settings::upsert(&connection, &key, &value).map_err(|error| error.to_string())
}
