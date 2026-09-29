use crate::{
    database::Database,
    importer::{self, ImportFailure, ImportPreview, PreparedImport},
    models::{ImportHistoryItem, ImportResult, SyncHistoryItem},
    repositories::imports,
};
use chrono::Utc;
use rusqlite::ErrorCode;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};
use tauri::State;

#[derive(Default)]
pub struct ImportSession {
    pending: Mutex<Option<PreparedImport>>,
    sequence: AtomicU64,
}

impl ImportSession {
    fn stage(&self, mut prepared: PreparedImport) -> Result<ImportPreview, ImportFailure> {
        let token = format!(
            "preview-{}-{}",
            Utc::now().timestamp_millis(),
            self.sequence.fetch_add(1, Ordering::Relaxed)
        );
        prepared.preview.preview_token = token;
        let preview = prepared.preview.clone();
        let mut pending = self.pending.lock().map_err(|_| session_error())?;
        *pending = Some(prepared);
        Ok(preview)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Option<PreparedImport>>, ImportFailure> {
        self.pending.lock().map_err(|_| session_error())
    }
}

#[tauri::command]
pub fn preview_excel(
    session: State<'_, ImportSession>,
    file_path: String,
) -> Result<ImportPreview, ImportFailure> {
    let prepared = importer::prepare_workbook(&PathBuf::from(file_path))?;
    session.stage(prepared)
}

#[tauri::command]
pub fn replace_excel_preview(
    database: State<'_, Database>,
    session: State<'_, ImportSession>,
    preview_token: String,
) -> Result<ImportResult, ImportFailure> {
    let mut pending = session.lock()?;
    let prepared = pending.as_ref().ok_or_else(expired_preview_error)?;
    if prepared.preview.preview_token != preview_token {
        return Err(expired_preview_error());
    }
    importer::ensure_preview_replaceable(&prepared.preview)?;

    let mut connection = database.connection().map_err(|error| {
        eprintln!("database unavailable during dataset replacement: {error}");
        ImportFailure::new(
            "database_unavailable",
            "The database is currently unavailable. The previous dataset was preserved.",
            Vec::new(),
        )
    })?;
    let result = importer::replace_dataset(
        &mut connection,
        &prepared.dataset,
        prepared.preview.warning_count as i64,
        prepared.preview.formula_errors as i64,
    );

    match result {
        Ok(result) => {
            *pending = None;
            Ok(result)
        }
        Err(error) => {
            eprintln!("dataset replacement transaction failed: {error}");
            let message = match &error {
                rusqlite::Error::SqliteFailure(sqlite_error, _)
                    if sqlite_error.code == ErrorCode::ConstraintViolation =>
                {
                    "A database constraint rejected the validated data. The previous dataset was preserved."
                }
                _ => {
                    "The dataset could not be replaced. The previous dataset was preserved."
                }
            };
            let _ = imports::record_failure(
                &connection,
                &prepared.dataset.source_file,
                Some(prepared.dataset.file_size),
                message,
            );
            Err(ImportFailure::new(
                "replacement_failed",
                message,
                Vec::new(),
            ))
        }
    }
}

#[tauri::command]
pub fn cancel_import_preview(
    session: State<'_, ImportSession>,
    preview_token: String,
) -> Result<(), ImportFailure> {
    let mut pending = session.lock()?;
    if pending
        .as_ref()
        .is_some_and(|prepared| prepared.preview.preview_token == preview_token)
    {
        *pending = None;
    }
    Ok(())
}

#[tauri::command]
pub fn get_import_history(
    database: State<'_, Database>,
    limit: Option<u32>,
) -> Result<Vec<ImportHistoryItem>, String> {
    let connection = database.connection().map_err(|error| error.to_string())?;
    imports::list_recent(&connection, limit.unwrap_or(10).min(50))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_sync_history(
    database: State<'_, Database>,
    limit: Option<u32>,
) -> Result<Vec<SyncHistoryItem>, String> {
    let connection = database.connection().map_err(|error| error.to_string())?;
    imports::list_recent_syncs(&connection, limit.unwrap_or(10).min(50))
        .map_err(|error| error.to_string())
}

fn expired_preview_error() -> ImportFailure {
    ImportFailure::new(
        "preview_expired",
        "This import preview is no longer active. Select the Excel file again.",
        Vec::new(),
    )
}

fn session_error() -> ImportFailure {
    ImportFailure::new(
        "preview_session_unavailable",
        "The import preview session is unavailable. The current dataset was not changed.",
        Vec::new(),
    )
}
