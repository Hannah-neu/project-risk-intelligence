use crate::{database::Database, models::DatabaseStatus, repositories};
use tauri::State;

#[tauri::command]
pub fn get_database_status(database: State<'_, Database>) -> Result<DatabaseStatus, String> {
    let connection = database.connection().map_err(|error| error.to_string())?;
    let schema_version = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;

    Ok(DatabaseStatus {
        path: database.path().display().to_string(),
        schema_version,
        database_size_bytes: database.size_bytes().map_err(|error| error.to_string())?,
        total_projects: repositories::projects::count(&connection)
            .map_err(|error| error.to_string())?,
        total_milestones: repositories::milestones::count(&connection)
            .map_err(|error| error.to_string())?,
        total_risks: repositories::risks::count(&connection).map_err(|error| error.to_string())?,
        total_insights: repositories::insights::count(&connection)
            .map_err(|error| error.to_string())?,
        total_imports: repositories::imports::count(&connection)
            .map_err(|error| error.to_string())?,
    })
}
