use crate::{database::Database, models::DashboardData, repositories::dashboard};
use tauri::State;

#[tauri::command]
pub fn get_dashboard_data(database: State<'_, Database>) -> Result<DashboardData, String> {
    let connection = database.connection().map_err(|error| error.to_string())?;
    dashboard::get(&connection).map_err(|error| error.to_string())
}
