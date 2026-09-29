mod ai_orchestrator;
#[cfg(test)]
mod ai_prompt_eval;
mod ai_prompts;
mod ai_query_plan;
mod ai_query_tools;
mod commands;
mod database;
mod enterprise_config;
mod enterprise_context;
mod enterprise_llm;
mod enterprise_secret;
mod importer;
mod insight_engine;
mod models;
mod project_analysis;
mod project_list_export;
mod qa;
mod repositories;
mod risk_engine;
mod system_explanation;

use database::Database;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            #[cfg(windows)]
            match_title_bar_to_sidebar(app);
            let app_data_dir = app.path().app_data_dir()?;
            let database = Database::initialize(&app_data_dir)?;
            app.manage(database);
            app.manage(commands::import::ImportSession::default());
            app.manage(commands::mcr::McrSession::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::database::get_database_status,
            commands::export::export_project_list,
            commands::dashboard::get_dashboard_data,
            commands::import::get_import_history,
            commands::import::get_sync_history,
            commands::import::preview_excel,
            commands::import::replace_excel_preview,
            commands::import::cancel_import_preview,
            commands::mcr::preview_mcr_excel,
            commands::mcr::apply_mcr_preview,
            commands::mcr::cancel_mcr_preview,
            commands::insights::get_insight_overview,
            commands::qa::ask_project_data,
            commands::qa::ask_structured_project_data,
            commands::system_explanation::explain_system_question,
            commands::qa::get_dataset_version,
            commands::enterprise::get_enterprise_config,
            commands::enterprise::save_enterprise_config,
            commands::enterprise::invoke_enterprise_llm,
            commands::enterprise::interpret_enterprise_question,
            commands::enterprise::test_enterprise_connection,
            commands::projects::get_projects,
            commands::projects::get_project_detail,
            commands::project_analysis::load_project_ai_analysis,
            commands::project_analysis::generate_project_ai_analysis,
            commands::risks::get_risk_overview,
            commands::settings::get_setting,
            commands::settings::set_setting
        ])
        .run(tauri::generate_context!())
        .expect("error while running Project Risk Intelligence");
}

#[cfg(windows)]
fn match_title_bar_to_sidebar(app: &tauri::App) {
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR,
    };

    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let Ok(hwnd) = window.hwnd() else { return };
    // COLORREF is BGR: the caption matches the white sidebar; text remains dark in either OS theme.
    for (attribute, color) in [
        (DWMWA_CAPTION_COLOR, 0x00FF_FFFF_u32),
        (DWMWA_TEXT_COLOR, 0x0054_4034_u32),
    ] {
        let result = unsafe {
            DwmSetWindowAttribute(
                hwnd.0,
                attribute as u32,
                (&color as *const u32).cast(),
                std::mem::size_of::<u32>() as u32,
            )
        };
        if result < 0 {
            eprintln!(
                "Windows caption attribute {attribute} could not be set (HRESULT {result:#x})"
            );
        }
    }
}
