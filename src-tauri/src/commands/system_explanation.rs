use crate::{database::Database, system_explanation};
use tauri::State;

#[tauri::command]
pub fn explain_system_question(
    database: State<'_, Database>,
    intent: String,
    question: String,
    language: String,
) -> Result<String, String> {
    let connection = database.connection().map_err(|error| error.to_string())?;
    system_explanation::answer(&connection, &intent, &question, &language)
}
