use crate::{database::Database, models::InsightOverviewData, repositories::insights};
use tauri::State;

#[tauri::command]
pub fn get_insight_overview(database: State<'_, Database>) -> Result<InsightOverviewData, String> {
    let connection = database.connection().map_err(|error| error.to_string())?;
    insights::overview(&connection).map_err(|error| error.to_string())
}
