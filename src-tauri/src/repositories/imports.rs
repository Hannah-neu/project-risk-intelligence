use crate::models::{ImportHistoryItem, SyncHistoryItem};
use chrono::Utc;
use rusqlite::{params, Connection, Result};

pub fn count(connection: &Connection) -> Result<i64> {
    connection.query_row("SELECT COUNT(*) FROM import_history", [], |row| row.get(0))
}

pub fn record_failure(
    connection: &Connection,
    file_name: &str,
    file_size: Option<u64>,
    error_message: &str,
) -> Result<()> {
    connection.execute(
        r#"
    INSERT INTO import_history (
      file_name, import_time, total_projects, total_milestones, status, error_message, file_size
    ) VALUES (?1, ?2, 0, 0, 'failed', ?3, ?4)
    "#,
        params![
            file_name,
            Utc::now().to_rfc3339(),
            error_message,
            file_size.map(|size| size as i64)
        ],
    )?;
    Ok(())
}

pub fn list_recent(connection: &Connection, limit: u32) -> Result<Vec<ImportHistoryItem>> {
    let mut statement = connection.prepare(
        r#"
    SELECT id, file_name, import_time, total_projects, total_milestones, status, error_message,
           file_size, warning_count, formula_error_count
    FROM import_history
    ORDER BY import_time DESC, id DESC
    LIMIT ?1
    "#,
    )?;
    let items = statement.query_map([limit as i64], |row| {
        Ok(ImportHistoryItem {
            id: row.get(0)?,
            file_name: row.get(1)?,
            import_time: row.get(2)?,
            total_projects: row.get(3)?,
            total_milestones: row.get(4)?,
            status: row.get(5)?,
            error_message: row.get(6)?,
            file_size: row.get(7)?,
            warning_count: row.get(8)?,
            formula_error_count: row.get(9)?,
        })
    })?;
    items.collect()
}

pub fn list_recent_syncs(connection: &Connection, limit: u32) -> Result<Vec<SyncHistoryItem>> {
    let mut statement = connection.prepare(
        r#"
        SELECT kind, id, file_name, event_time, status, project_count,
               updated_milestones, projects_affected, warning_count
        FROM (
          SELECT 'project_list' AS kind, id, file_name, import_time AS event_time,
                 status, total_projects AS project_count,
                 NULL AS updated_milestones, NULL AS projects_affected, warning_count
          FROM import_history
          UNION ALL
          SELECT 'mcr' AS kind, id, file_name, synced_at AS event_time,
                 'success' AS status, NULL AS project_count,
                 updated_milestones, projects_affected, warning_count
          FROM mcr_sync_history
        )
        ORDER BY event_time DESC, kind DESC, id DESC
        LIMIT ?1
        "#,
    )?;
    let rows = statement.query_map([limit as i64], |row| {
        Ok(SyncHistoryItem {
            kind: row.get(0)?,
            id: row.get(1)?,
            file_name: row.get(2)?,
            event_time: row.get(3)?,
            status: row.get(4)?,
            project_count: row.get(5)?,
            updated_milestones: row.get(6)?,
            projects_affected: row.get(7)?,
            warning_count: row.get(8)?,
        })
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations;

    #[test]
    fn recent_syncs_merge_project_list_and_mcr_in_time_order() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations::run(&mut connection).unwrap();
        connection.execute_batch(
            r#"
            INSERT INTO import_history (file_name, import_time, total_projects, status, warning_count)
              VALUES ('projects.xlsx', '2026-09-20T10:00:00+00:00', 52, 'success', 2);
            INSERT INTO mcr_sync_history (file_name, file_size, synced_at, master_ids, matched_projects, updated_milestones, projects_affected, warning_count)
              VALUES ('mcr.xlsm', 100, '2026-09-20T11:00:00+00:00', 5, 4, 7, 3, 1);
            INSERT INTO import_history (file_name, import_time, status)
              VALUES ('invalid.xlsx', '2026-09-20T12:00:00+00:00', 'failed');
            "#,
        ).unwrap();
        let history = list_recent_syncs(&connection, 8).unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].file_name, "invalid.xlsx");
        assert_eq!(history[1].kind, "mcr");
        assert_eq!(history[1].updated_milestones, Some(7));
        assert_eq!(history[1].projects_affected, Some(3));
        assert_eq!(history[2].kind, "project_list");
        assert_eq!(history[2].project_count, Some(52));
        assert_eq!(list_recent_syncs(&connection, 2).unwrap().len(), 2);
        assert_eq!(list_recent(&connection, 8).unwrap().len(), 2);
    }
}
