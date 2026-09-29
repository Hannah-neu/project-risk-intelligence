use super::DatabaseError;
use rusqlite::Connection;

const LATEST_SCHEMA_VERSION: i32 = 7;

pub fn run(connection: &mut Connection) -> Result<(), DatabaseError> {
    let current_version: i32 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;

    if current_version < 1 {
        migrate_to_v1(connection)?;
    }

    if current_version < 2 {
        migrate_to_v2(connection)?;
    }

    if current_version < 3 {
        migrate_to_v3(connection)?;
    }

    if current_version < 4 {
        migrate_to_v4(connection)?;
    }

    if current_version < 5 {
        migrate_to_v5(connection)?;
    }
    if current_version < 6 {
        migrate_to_v6(connection)?;
    }
    if current_version < 7 {
        migrate_to_v7(connection)?;
    }

    let migrated_version: i32 =
        connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if migrated_version != LATEST_SCHEMA_VERSION {
        return Err(rusqlite::Error::InvalidQuery.into());
    }

    Ok(())
}

fn migrate_to_v7(connection: &mut Connection) -> Result<(), DatabaseError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        r#"
        CREATE TABLE project_ai_analysis (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          project_id INTEGER NOT NULL,
          project_mcr_id TEXT NOT NULL,
          dataset_version TEXT NOT NULL,
          model TEXT NOT NULL,
          language TEXT NOT NULL CHECK (language IN ('en', 'zh')),
          content TEXT NOT NULL,
          risk_score_at_generation INTEGER NOT NULL,
          risk_level_at_generation TEXT NOT NULL,
          created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );
        CREATE INDEX idx_project_ai_analysis_latest ON project_ai_analysis(project_mcr_id, id DESC);
        PRAGMA user_version = 7;
    "#,
    )?;
    transaction.commit()?;
    Ok(())
}

fn migrate_to_v6(connection: &mut Connection) -> Result<(), DatabaseError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        r#"
        CREATE TABLE mcr_sync_history (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          file_name TEXT NOT NULL,
          file_size INTEGER NOT NULL,
          synced_at TEXT NOT NULL,
          master_ids INTEGER NOT NULL,
          matched_projects INTEGER NOT NULL,
          updated_milestones INTEGER NOT NULL,
          projects_affected INTEGER NOT NULL,
          warning_count INTEGER NOT NULL
        );
        CREATE INDEX idx_mcr_sync_history_time ON mcr_sync_history(synced_at DESC);
        PRAGMA user_version = 6;
        "#,
    )?;
    transaction.commit()?;
    Ok(())
}

fn migrate_to_v5(connection: &mut Connection) -> Result<(), DatabaseError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        r#"
        UPDATE projects
        SET project_status = UPPER(TRIM(project_status))
        WHERE project_status IS NOT NULL;
        PRAGMA user_version = 5;
        "#,
    )?;
    transaction.commit()?;
    Ok(())
}

fn migrate_to_v4(connection: &mut Connection) -> Result<(), DatabaseError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        r#"
        CREATE TABLE project_risks_v4 (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          project_id INTEGER NOT NULL UNIQUE,
          risk_score INTEGER NOT NULL CHECK (risk_score BETWEEN 0 AND 100),
          risk_level TEXT NOT NULL CHECK (risk_level IN ('Low', 'Medium', 'High', 'Critical')),
          primary_risk_driver TEXT,
          risk_reasons TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(risk_reasons)),
          calculated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
          FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
        );

        INSERT INTO project_risks_v4 (
          id, project_id, risk_score, risk_level, primary_risk_driver, risk_reasons, calculated_at
        )
        SELECT id, project_id, risk_score, risk_level, primary_risk_driver, risk_reasons, calculated_at
        FROM project_risks;

        DROP TABLE project_risks;
        ALTER TABLE project_risks_v4 RENAME TO project_risks;
        CREATE INDEX idx_risks_level ON project_risks(risk_level);
        CREATE INDEX idx_risks_score ON project_risks(risk_score DESC);
        PRAGMA user_version = 4;
        "#,
    )?;
    transaction.commit()?;
    Ok(())
}

fn migrate_to_v3(connection: &mut Connection) -> Result<(), DatabaseError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        r#"
        ALTER TABLE import_history ADD COLUMN warning_count INTEGER NOT NULL DEFAULT 0;
        ALTER TABLE import_history ADD COLUMN formula_error_count INTEGER NOT NULL DEFAULT 0;
        PRAGMA user_version = 3;
        "#,
    )?;
    transaction.commit()?;
    Ok(())
}

fn migrate_to_v2(connection: &mut Connection) -> Result<(), DatabaseError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        r#"
        CREATE TABLE project_components (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          project_id INTEGER NOT NULL,
          component_name TEXT NOT NULL,
          component_type TEXT NOT NULL CHECK (component_type IN ('existing', 'new_released')),
          FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
          UNIQUE (project_id, component_name, component_type)
        );

        CREATE INDEX idx_project_components_project ON project_components(project_id);
        CREATE INDEX idx_project_components_name ON project_components(component_name);
        CREATE INDEX idx_project_components_type ON project_components(component_type);

        PRAGMA user_version = 2;
        "#,
    )?;
    transaction.commit()?;
    Ok(())
}

fn migrate_to_v1(connection: &mut Connection) -> Result<(), DatabaseError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
    r#"
    CREATE TABLE projects (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      project_no INTEGER,
      mcr_id TEXT NOT NULL UNIQUE,
      project_name TEXT NOT NULL,
      customer TEXT,
      p_id TEXT,
      acq_manager TEXT,
      project_manager TEXT,
      engine_segment TEXT,
      product_category TEXT,
      emission TEXT,
      apic TEXT,
      project_type TEXT,
      system TEXT,
      components TEXT,
      new_released_components TEXT,
      has_injector INTEGER NOT NULL DEFAULT 0 CHECK (has_injector IN (0, 1)),
      has_pump INTEGER NOT NULL DEFAULT 0 CHECK (has_pump IN (0, 1)),
      has_rail INTEGER NOT NULL DEFAULT 0 CHECK (has_rail IN (0, 1)),
      has_ecu INTEGER NOT NULL DEFAULT 0 CHECK (has_ecu IN (0, 1)),
      has_dnox INTEGER NOT NULL DEFAULT 0 CHECK (has_dnox IN (0, 1)),
      other_component TEXT,
      project_status TEXT,
      b_sample TEXT,
      c_sample TEXT,
      d_sample TEXT,
      source_file TEXT NOT NULL,
      source_row INTEGER NOT NULL,
      created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
      updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    );

    CREATE TABLE project_milestones (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      project_id INTEGER NOT NULL,
      milestone_type TEXT NOT NULL CHECK (milestone_type IN ('SE2', 'SE3', 'SE4', 'QGC0', 'QGC4')),
      planned_date TEXT,
      actual_date TEXT,
      plan_source TEXT NOT NULL,
      actual_source TEXT NOT NULL,
      created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
      updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
      FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
      UNIQUE (project_id, milestone_type)
    );

    CREATE TABLE project_risks (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      project_id INTEGER NOT NULL UNIQUE,
      risk_score INTEGER NOT NULL CHECK (risk_score BETWEEN 0 AND 100),
      risk_level TEXT NOT NULL CHECK (risk_level IN ('Low', 'Medium', 'High', 'Critical')),
      primary_risk_driver TEXT,
      risk_reasons TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(risk_reasons)),
      calculated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
      FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
    );

    CREATE TABLE ai_insights (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      project_id INTEGER,
      insight_type TEXT NOT NULL CHECK (insight_type IN ('warning', 'milestone', 'portfolio', 'recommendation', 'summary')),
      title TEXT NOT NULL,
      description TEXT NOT NULL,
      severity TEXT,
      created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
      FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
    );

    CREATE TABLE import_history (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      file_name TEXT NOT NULL,
      import_time TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
      total_projects INTEGER NOT NULL DEFAULT 0,
      total_milestones INTEGER NOT NULL DEFAULT 0,
      status TEXT NOT NULL CHECK (status IN ('success', 'failed')),
      error_message TEXT,
      file_size INTEGER
    );

    CREATE TABLE app_settings (
      key TEXT PRIMARY KEY,
      value TEXT NOT NULL,
      updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    );

    CREATE INDEX idx_projects_mcr_id ON projects(mcr_id);
    CREATE INDEX idx_projects_name ON projects(project_name);
    CREATE INDEX idx_projects_customer ON projects(customer);
    CREATE INDEX idx_projects_manager ON projects(project_manager);
    CREATE INDEX idx_projects_status ON projects(project_status);
    CREATE INDEX idx_milestones_project ON project_milestones(project_id);
    CREATE INDEX idx_milestones_planned_date ON project_milestones(planned_date);
    CREATE INDEX idx_risks_level ON project_risks(risk_level);
    CREATE INDEX idx_risks_score ON project_risks(risk_score DESC);
    CREATE INDEX idx_insights_type ON ai_insights(insight_type);
    CREATE INDEX idx_import_history_time ON import_history(import_time DESC);

    PRAGMA user_version = 1;
    "#,
  )?;
    transaction.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v5_normalizes_existing_project_status_values() {
        let mut connection = Connection::open_in_memory().expect("in-memory database should open");
        run(&mut connection).expect("initial migration should succeed");
        connection
            .execute_batch(
                r#"
                INSERT INTO projects (mcr_id, project_name, project_status, source_file, source_row)
                VALUES
                  ('MCR-1', 'One', 'work', 'test.xlsx', 2),
                  ('MCR-2', 'Two', ' WORK ', 'test.xlsx', 3);
                DROP TABLE mcr_sync_history;
                DROP TABLE project_ai_analysis;
                PRAGMA user_version = 4;
                "#,
            )
            .expect("legacy status rows should insert");

        run(&mut connection).expect("v5 migration should succeed");

        let values: Vec<String> = connection
            .prepare("SELECT project_status FROM projects ORDER BY id")
            .expect("status query should prepare")
            .query_map([], |row| row.get(0))
            .expect("status query should run")
            .collect::<Result<Vec<_>, _>>()
            .expect("statuses should collect");
        assert_eq!(values, vec!["WORK", "WORK"]);
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i32>(0))
                .expect("schema version should be readable"),
            7
        );
    }
}
