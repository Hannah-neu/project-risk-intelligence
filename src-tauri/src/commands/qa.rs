use crate::{
    database::Database,
    qa::{self, Answer},
};
use tauri::State;

#[tauri::command]
pub fn ask_project_data(database: State<'_, Database>, question: String) -> Result<Answer, String> {
    if question.trim().is_empty() || question.chars().count() > 500 {
        return Err("Enter a question of 1–500 characters.".into());
    }
    let connection = database.connection().map_err(|error| error.to_string())?;
    qa::answer(&connection, &question).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn ask_structured_project_data(
    database: State<'_, Database>,
    query: qa::structured::StructuredQuery,
) -> Result<Answer, String> {
    let connection = database.connection().map_err(|error| error.to_string())?;
    qa::structured::answer(&connection, &query)
}

#[tauri::command]
pub fn get_dataset_version(database: State<'_, Database>) -> Result<String, String> {
    let connection = database.connection().map_err(|error| error.to_string())?;
    qa::dataset_version(&connection).map_err(|error| error.to_string())
}
