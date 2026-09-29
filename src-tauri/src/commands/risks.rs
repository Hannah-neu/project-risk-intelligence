use crate::{
    database::Database,
    models::{RiskOverviewData, RiskOverviewQuery},
    repositories::risks,
};
use tauri::State;

#[tauri::command]
pub fn get_risk_overview(
    database: State<'_, Database>,
    query: RiskOverviewQuery,
) -> Result<RiskOverviewData, String> {
    let connection = database.connection().map_err(|error| error.to_string())?;
    risks::overview(&connection, &query).map_err(|error| error.to_string())
}
