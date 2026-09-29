use crate::{
    database::Database,
    models::{ProjectDetail, ProjectListQuery, ProjectListResponse},
    repositories::projects,
};
use tauri::State;

#[tauri::command]
pub fn get_projects(
    database: State<'_, Database>,
    query: ProjectListQuery,
) -> Result<ProjectListResponse, String> {
    let connection = database.connection().map_err(|error| error.to_string())?;
    projects::list(&connection, &query).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_project_detail(
    database: State<'_, Database>,
    project_id: i64,
) -> Result<ProjectDetail, String> {
    let connection = database.connection().map_err(|error| error.to_string())?;
    projects::detail(&connection, project_id).map_err(|error| error.to_string())
}
