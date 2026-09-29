use rusqlite::{params, Connection, OptionalExtension, Result};

pub fn get(connection: &Connection, key: &str) -> Result<Option<String>> {
    connection
        .query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            [key],
            |row| row.get(0),
        )
        .optional()
}

pub fn upsert(connection: &Connection, key: &str, value: &str) -> Result<()> {
    connection.execute(
        r#"
    INSERT INTO app_settings (key, value)
    VALUES (?1, ?2)
    ON CONFLICT(key) DO UPDATE SET
      value = excluded.value,
      updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    "#,
        params![key, value],
    )?;
    Ok(())
}
