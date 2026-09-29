use rusqlite::{Connection, Result};

pub fn count(connection: &Connection) -> Result<i64> {
    connection.query_row("SELECT COUNT(*) FROM project_milestones", [], |row| {
        row.get(0)
    })
}
