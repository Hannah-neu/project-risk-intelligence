use crate::{
    database::Database,
    importer::{
        mcr::{self, McrApplyResult, McrPreview, PreparedMcr},
        ImportFailure,
    },
};
use chrono::Utc;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};
use tauri::State;

#[derive(Default)]
pub struct McrSession {
    pending: Mutex<Option<PreparedMcr>>,
    sequence: AtomicU64,
}

#[tauri::command]
pub fn preview_mcr_excel(
    database: State<'_, Database>,
    session: State<'_, McrSession>,
    file_path: String,
) -> Result<McrPreview, ImportFailure> {
    let connection = database
        .connection()
        .map_err(|error| ImportFailure::new("database_unavailable", error.to_string(), vec![]))?;
    let mut prepared = mcr::prepare(&PathBuf::from(file_path), &connection)?;
    drop(connection);
    prepared.preview.preview_token = format!(
        "mcr-{}-{}",
        Utc::now().timestamp_millis(),
        session.sequence.fetch_add(1, Ordering::Relaxed)
    );
    let preview = prepared.preview.clone();
    *session.pending.lock().map_err(|_| session_error())? = Some(prepared);
    Ok(preview)
}

#[tauri::command]
pub fn apply_mcr_preview(
    database: State<'_, Database>,
    session: State<'_, McrSession>,
    preview_token: String,
) -> Result<McrApplyResult, ImportFailure> {
    let mut pending = session.pending.lock().map_err(|_| session_error())?;
    let prepared = pending.as_ref().ok_or_else(expired_error)?;
    if prepared.preview.preview_token != preview_token {
        return Err(expired_error());
    }
    let mut connection = database
        .connection()
        .map_err(|error| ImportFailure::new("database_unavailable", error.to_string(), vec![]))?;
    let result = mcr::apply(&mut connection, prepared)?;
    *pending = None;
    Ok(result)
}

#[tauri::command]
pub fn cancel_mcr_preview(
    session: State<'_, McrSession>,
    preview_token: String,
) -> Result<(), ImportFailure> {
    let mut pending = session.pending.lock().map_err(|_| session_error())?;
    if pending
        .as_ref()
        .is_some_and(|prepared| prepared.preview.preview_token == preview_token)
    {
        *pending = None;
    }
    Ok(())
}

fn expired_error() -> ImportFailure {
    ImportFailure::new(
        "preview_expired",
        "This MCR preview is no longer active. Select the file again.",
        vec![],
    )
}

fn session_error() -> ImportFailure {
    ImportFailure::new(
        "preview_session_unavailable",
        "The MCR preview session is unavailable; no data was changed.",
        vec![],
    )
}
