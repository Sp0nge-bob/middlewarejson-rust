use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{Connection, Row};

use crate::db::schema::SCHEMA_SQL;

pub struct Database {
    path: PathBuf,
}

impl Database {
    pub fn new(db_path: impl AsRef<Path>) -> Self {
        let path = db_path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn connect(&self) -> Result<Connection> {
        let conn = Connection::open(&self.path).context("open database")?;
        conn.execute("PRAGMA foreign_keys = ON", [])?;
        Ok(conn)
    }

    pub fn initialize(&self) -> Result<()> {
        let conn = self.connect()?;
        conn.execute_batch(SCHEMA_SQL)?;
        migrate_legacy_schema(&conn)?;
        Ok(())
    }
}

fn migrate_legacy_schema(conn: &Connection) -> Result<()> {
    let tables: HashSet<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table'")?
        .query_map([], |row: &Row| row.get::<_, String>(0))?
        .collect::<Result<Vec<String>, rusqlite::Error>>()?
        .into_iter()
        .collect();

    if tables.contains("inbound_catalog") {
        let catalog_columns: HashSet<String> = conn
            .prepare("PRAGMA table_info(inbound_catalog)")?
            .query_map([], |row: &Row| row.get::<_, String>(1))?
            .collect::<Result<Vec<String>, rusqlite::Error>>()?
            .into_iter()
            .collect();

        if !catalog_columns.contains("panel_inbound_id") {
            conn.execute(
                "ALTER TABLE inbound_catalog ADD COLUMN panel_inbound_id INTEGER",
                [],
            )?;
        }
        if !catalog_columns.contains("endpoint_index") {
            conn.execute(
                "ALTER TABLE inbound_catalog ADD COLUMN endpoint_index INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_catalog_panel_id ON inbound_catalog(panel_inbound_id)",
            [],
        )?;
    }

    if !tables.contains("client_index") {
        conn.execute_batch(
            r#"
            CREATE TABLE client_index (
                sub_id TEXT PRIMARY KEY,
                group_name TEXT NOT NULL DEFAULT '',
                email TEXT NOT NULL DEFAULT '',
                enable INTEGER NOT NULL DEFAULT 1,
                last_seen_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_client_group ON client_index(group_name);
            "#,
        )?;
    }

    if !tables.contains("group_balancers") {
        conn.execute_batch(
            r#"
            CREATE TABLE group_balancers (
                group_name TEXT PRIMARY KEY,
                balancer_tag TEXT NOT NULL REFERENCES balancers(tag) ON DELETE CASCADE
            );
            "#,
        )?;
    }

    if !tables.contains("panel_groups") {
        conn.execute_batch(
            r#"
            CREATE TABLE panel_groups (
                group_name TEXT PRIMARY KEY,
                last_seen_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            "#,
        )?;
        if tables.contains("client_index") {
            conn.execute(
                r#"
                INSERT OR IGNORE INTO panel_groups (group_name)
                SELECT DISTINCT group_name
                FROM client_index
                WHERE group_name != ''
                "#,
                [],
            )?;
        }
    }

    if tables.contains("balancers") {
        let balancer_columns: HashSet<String> = conn
            .prepare("PRAGMA table_info(balancers)")?
            .query_map([], |row: &Row| row.get::<_, String>(1))?
            .collect::<Result<Vec<String>, rusqlite::Error>>()?
            .into_iter()
            .collect();

        if !balancer_columns.contains("scope") {
            conn.execute(
                "ALTER TABLE balancers ADD COLUMN scope TEXT NOT NULL DEFAULT 'disabled'",
                [],
            )?;
        }
        if !balancer_columns.contains("scope_target") {
            conn.execute(
                "ALTER TABLE balancers ADD COLUMN scope_target TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }

        if tables.contains("group_balancers") {
            let mut stmt = conn.prepare("SELECT group_name, balancer_tag FROM group_balancers")?;
            let rows = stmt
                .query_map([], |row: &Row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            for (group_name, balancer_tag) in rows {
                conn.execute(
                    r#"
                    UPDATE balancers
                    SET scope = 'group', scope_target = ?1
                    WHERE tag = ?2 AND scope = 'disabled' AND scope_target = ''
                    "#,
                    rusqlite::params![group_name, balancer_tag],
                )?;
            }
        }
    }

    if !tables.contains("balancers") {
        return Ok(());
    }

    let columns: Vec<String> = conn
        .prepare("PRAGMA table_info(balancers)")?
        .query_map([], |row: &Row| row.get::<_, String>(1))?
        .collect::<Result<Vec<String>, rusqlite::Error>>()?;

    if !columns.contains(&"sub_id".to_string()) {
        return Ok(());
    }

    let old_balancers: Vec<(i64, String, String, String, String)> = conn
        .prepare("SELECT id, sub_id, tag, remarks, strategy FROM balancers")?
        .query_map([], |row: &Row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })?
        .collect::<Result<Vec<(i64, String, String, String, String)>, rusqlite::Error>>()?;

    let old_members: Vec<(i64, String)> = conn
        .prepare("SELECT balancer_id, inbound_fingerprint FROM balancer_members")?
        .query_map([], |row: &Row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<(i64, String)>, rusqlite::Error>>()?;

    let old_id_to_tag: std::collections::HashMap<i64, String> = old_balancers
        .iter()
        .map(|(id, _, tag, _, _)| (*id, tag.clone()))
        .collect();

    conn.execute_batch(
        r#"
        DROP TABLE balancer_members;
        DROP TABLE balancers;
        DROP TABLE IF EXISTS subscription_profiles;
        CREATE TABLE balancers (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            tag TEXT NOT NULL UNIQUE,
            remarks TEXT NOT NULL DEFAULT '',
            strategy TEXT NOT NULL DEFAULT 'roundRobin'
        );
        CREATE TABLE balancer_members (
            balancer_id INTEGER NOT NULL REFERENCES balancers(id) ON DELETE CASCADE,
            inbound_fingerprint TEXT NOT NULL,
            PRIMARY KEY (balancer_id, inbound_fingerprint)
        );
        "#,
    )?;

    let mut tag_to_new_id = std::collections::HashMap::new();
    for (_, _, tag, remarks, strategy) in &old_balancers {
        if tag_to_new_id.contains_key(tag) {
            continue;
        }
        conn.execute(
            "INSERT INTO balancers (tag, remarks, strategy) VALUES (?1, ?2, ?3)",
            rusqlite::params![tag, remarks, strategy],
        )?;
        let new_id: i64 = conn.last_insert_rowid();
        tag_to_new_id.insert(tag.clone(), new_id);
    }

    for (balancer_id, fingerprint) in old_members {
        let Some(tag) = old_id_to_tag.get(&balancer_id) else {
            continue;
        };
        let Some(new_id) = tag_to_new_id.get(tag) else {
            continue;
        };
        conn.execute(
            "INSERT OR IGNORE INTO balancer_members (balancer_id, inbound_fingerprint) VALUES (?1, ?2)",
            rusqlite::params![new_id, fingerprint],
        )?;
    }

    Ok(())
}