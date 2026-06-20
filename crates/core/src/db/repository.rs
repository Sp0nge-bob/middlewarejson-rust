use std::collections::HashSet;

use anyhow::{anyhow, Result};
use chrono::Utc;
use rusqlite::{params, Row};

use crate::db::database::Database;
use crate::models::balancer::{normalize_scope, normalize_strategy};
use crate::models::inbound::InboundDescriptor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BalancerRecord {
    pub id: i64,
    pub tag: String,
    pub remarks: String,
    pub strategy: String,
    pub scope: String,
    pub scope_target: String,
    pub member_fingerprints: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientRecord {
    pub sub_id: String,
    pub group_name: String,
    pub email: String,
    pub enable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupAssignment {
    pub group_name: String,
    pub balancer_tag: String,
    pub client_count: i64,
}

pub struct CatalogRepository {
    db: Database,
}

impl CatalogRepository {
    pub fn new(database: Database) -> Result<Self> {
        database.initialize()?;
        Ok(Self { db: database })
    }

    pub fn database(&self) -> &Database {
        &self.db
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.db.connect()?;
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.db.connect()?;
        conn.execute(
            r#"
            INSERT INTO settings (key, value) VALUES (?1, ?2)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value
            "#,
            params![key, value],
        )?;
        Ok(())
    }

    pub fn upsert_inbounds(&self, inbounds: &[InboundDescriptor]) -> Result<(usize, usize)> {
        let now = Utc::now().to_rfc3339();
        let mut seen = HashSet::new();
        let conn = self.db.connect()?;

        for inbound in inbounds {
            seen.insert(inbound.fingerprint.clone());
            conn.execute(
                r#"
                INSERT INTO inbound_catalog (
                    fingerprint, remarks, protocol, network, address,
                    path, port, security, panel_inbound_id, endpoint_index,
                    is_active, last_seen_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1, ?11)
                ON CONFLICT(fingerprint) DO UPDATE SET
                    remarks = excluded.remarks,
                    protocol = excluded.protocol,
                    network = excluded.network,
                    address = excluded.address,
                    path = excluded.path,
                    port = excluded.port,
                    security = excluded.security,
                    panel_inbound_id = excluded.panel_inbound_id,
                    endpoint_index = excluded.endpoint_index,
                    is_active = 1,
                    last_seen_at = excluded.last_seen_at
                "#,
                params![
                    inbound.fingerprint,
                    inbound.remarks,
                    inbound.protocol,
                    inbound.network,
                    inbound.address,
                    inbound.path,
                    inbound.port,
                    inbound.security,
                    inbound.panel_inbound_id,
                    inbound.endpoint_index,
                    now,
                ],
            )?;
        }

        let deactivated = if seen.is_empty() {
            conn.execute(
                "UPDATE inbound_catalog SET is_active = 0 WHERE is_active = 1",
                [],
            )?
        } else {
            let placeholders = seen.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "UPDATE inbound_catalog SET is_active = 0 \
                 WHERE fingerprint NOT IN ({placeholders}) AND is_active = 1"
            );
            let params: Vec<&dyn rusqlite::ToSql> = seen
                .iter()
                .map(|s| s as &dyn rusqlite::ToSql)
                .collect();
            conn.execute(&sql, params.as_slice())?
        };

        Ok((inbounds.len(), deactivated))
    }

    pub fn list_inbounds(&self, active_only: bool) -> Result<Vec<serde_json::Map<String, serde_json::Value>>> {
        let mut query = String::from("SELECT * FROM inbound_catalog");
        if active_only {
            query.push_str(" WHERE is_active = 1");
        }
        query.push_str(" ORDER BY remarks, panel_inbound_id, endpoint_index, fingerprint");

        let conn = self.db.connect()?;
        let mut stmt = conn.prepare(&query)?;
        let rows = stmt
            .query_map([], row_to_map)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn get_fingerprints_by_panel_ids(
        &self,
        panel_ids: &[i64],
        active_only: bool,
    ) -> Result<Vec<String>> {
        if panel_ids.is_empty() {
            return Ok(Vec::new());
        }

        let placeholders = panel_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let mut query = format!(
            "SELECT fingerprint FROM inbound_catalog WHERE panel_inbound_id IN ({placeholders})"
        );
        if active_only {
            query.push_str(" AND is_active = 1");
        }
        query.push_str(" ORDER BY panel_inbound_id, endpoint_index, fingerprint");

        let conn = self.db.connect()?;
        let mut stmt = conn.prepare(&query)?;
        let params: Vec<&dyn rusqlite::ToSql> = panel_ids
            .iter()
            .map(|id| id as &dyn rusqlite::ToSql)
            .collect();
        let rows = stmt
            .query_map(params.as_slice(), |row: &Row| row.get(0))?
            .collect::<std::result::Result<Vec<String>, _>>()?;
        Ok(rows)
    }

    pub fn create_balancer(
        &self,
        tag: &str,
        remarks: &str,
        strategy: &str,
        member_fingerprints: &[String],
        scope: &str,
        scope_target: &str,
    ) -> Result<i64> {
        let normalized_scope = normalize_scope(scope)?;
        let normalized_strategy = normalize_strategy(strategy)?;
        let target = scope_target.trim().to_string();
        let conn = self.db.connect()?;

        conn.execute(
            r#"
            INSERT INTO balancers (tag, remarks, strategy, scope, scope_target)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(tag) DO UPDATE SET
                remarks = excluded.remarks,
                strategy = excluded.strategy,
                scope = excluded.scope,
                scope_target = excluded.scope_target
            "#,
            params![tag, remarks, normalized_strategy, normalized_scope, target],
        )?;

        let mut balancer_id = conn.last_insert_rowid();
        if balancer_id == 0 {
            balancer_id = conn.query_row(
                "SELECT id FROM balancers WHERE tag = ?1",
                params![tag],
                |row| row.get(0),
            )?;
            conn.execute(
                "DELETE FROM balancer_members WHERE balancer_id = ?1",
                params![balancer_id],
            )?;
        }

        for fingerprint in member_fingerprints {
            conn.execute(
                "INSERT OR IGNORE INTO balancer_members (balancer_id, inbound_fingerprint) VALUES (?1, ?2)",
                params![balancer_id, fingerprint],
            )?;
        }

        Ok(balancer_id)
    }

    pub fn update_balancer(
        &self,
        tag: &str,
        remarks: Option<&str>,
        strategy: Option<&str>,
        scope: Option<&str>,
        scope_target: Option<&str>,
        member_fingerprints: Option<&[String]>,
    ) -> Result<bool> {
        let balancer = match self.get_balancer_by_tag(tag)? {
            Some(b) => b,
            None => return Ok(false),
        };

        let new_remarks = remarks.unwrap_or(&balancer.remarks);
        let new_strategy = match strategy {
            Some(s) => normalize_strategy(s)?,
            None => balancer.strategy.as_str(),
        };
        let new_scope = match scope {
            Some(s) => normalize_scope(s)?,
            None => balancer.scope.as_str(),
        };
        let mut new_target = match scope_target {
            Some(t) => t.trim().to_string(),
            None => balancer.scope_target.clone(),
        };
        if new_scope == "disabled" || new_scope == "all" {
            new_target.clear();
        }

        let conn = self.db.connect()?;
        conn.execute(
            r#"
            UPDATE balancers
            SET remarks = ?1, strategy = ?2, scope = ?3, scope_target = ?4
            WHERE tag = ?5
            "#,
            params![new_remarks, new_strategy, new_scope, new_target, tag],
        )?;

        if let Some(fingerprints) = member_fingerprints {
            let balancer_id: i64 = conn.query_row(
                "SELECT id FROM balancers WHERE tag = ?1",
                params![tag],
                |row| row.get(0),
            )?;
            conn.execute(
                "DELETE FROM balancer_members WHERE balancer_id = ?1",
                params![balancer_id],
            )?;
            for fingerprint in fingerprints {
                conn.execute(
                    "INSERT OR IGNORE INTO balancer_members (balancer_id, inbound_fingerprint) VALUES (?1, ?2)",
                    params![balancer_id, fingerprint],
                )?;
            }
        }

        Ok(true)
    }

    pub fn set_balancer_scope(
        &self,
        tag: &str,
        scope: &str,
        scope_target: &str,
    ) -> Result<bool> {
        self.update_balancer(tag, None, None, Some(scope), Some(scope_target), None)
    }

    pub fn delete_balancer(&self, tag: &str) -> Result<bool> {
        let conn = self.db.connect()?;
        let affected = conn.execute("DELETE FROM balancers WHERE tag = ?1", params![tag])?;
        Ok(affected > 0)
    }

    pub fn list_balancers(&self) -> Result<Vec<BalancerRecord>> {
        let conn = self.db.connect()?;
        let mut stmt = conn.prepare(
            "SELECT id, tag, remarks, strategy, scope, scope_target FROM balancers ORDER BY tag",
        )?;
        let rows = stmt
            .query_map([], |row: &Row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let mut records = Vec::new();
        for (id, tag, remarks, strategy, scope, scope_target) in rows {
            let mut member_stmt = conn.prepare(
                "SELECT inbound_fingerprint FROM balancer_members WHERE balancer_id = ?1 ORDER BY inbound_fingerprint",
            )?;
            let members = member_stmt
                .query_map(params![id], |row: &Row| row.get(0))?
                .collect::<std::result::Result<Vec<String>, _>>()?;
            records.push(BalancerRecord {
                id,
                tag,
                remarks,
                strategy,
                scope: normalize_scope(&scope)?.to_string(),
                scope_target,
                member_fingerprints: members,
            });
        }
        Ok(records)
    }

    pub fn get_balancer_by_tag(&self, tag: &str) -> Result<Option<BalancerRecord>> {
        Ok(self
            .list_balancers()?
            .into_iter()
            .find(|balancer| balancer.tag == tag))
    }

    pub fn has_balancers(&self) -> Result<bool> {
        let conn = self.db.connect()?;
        let exists: Option<i32> = conn
            .query_row("SELECT 1 FROM balancers LIMIT 1", [], |row| row.get(0))
            .ok();
        Ok(exists.is_some())
    }

    pub fn upsert_clients(&self, clients: &[ClientRecord]) -> Result<(usize, usize)> {
        let now = Utc::now().to_rfc3339();
        let mut seen = HashSet::new();
        let conn = self.db.connect()?;

        for client in clients {
            seen.insert(client.sub_id.clone());
            conn.execute(
                r#"
                INSERT INTO client_index (sub_id, group_name, email, enable, last_seen_at)
                VALUES (?1, ?2, ?3, ?4, ?5)
                ON CONFLICT(sub_id) DO UPDATE SET
                    group_name = excluded.group_name,
                    email = excluded.email,
                    enable = excluded.enable,
                    last_seen_at = excluded.last_seen_at
                "#,
                params![
                    client.sub_id,
                    client.group_name,
                    client.email,
                    if client.enable { 1 } else { 0 },
                    now,
                ],
            )?;
        }

        let removed = if seen.is_empty() {
            conn.execute("DELETE FROM client_index", [])?
        } else {
            let placeholders = seen.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!("DELETE FROM client_index WHERE sub_id NOT IN ({placeholders})");
            let params: Vec<&dyn rusqlite::ToSql> = seen
                .iter()
                .map(|s| s as &dyn rusqlite::ToSql)
                .collect();
            conn.execute(&sql, params.as_slice())?
        };

        Ok((clients.len(), removed))
    }

    pub fn get_group_for_sub_id(&self, sub_id: &str) -> Result<Option<String>> {
        let conn = self.db.connect()?;
        let result: Option<String> = conn
            .query_row(
                "SELECT group_name FROM client_index WHERE sub_id = ?1 AND enable = 1",
                params![sub_id],
                |row| row.get(0),
            )
            .ok();
        Ok(result.and_then(|group| {
            let trimmed = group.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }))
    }

    pub fn list_clients_by_group(&self, group_name: &str) -> Result<Vec<ClientRecord>> {
        let conn = self.db.connect()?;
        let mut stmt = conn.prepare(
            "SELECT sub_id, group_name, email, enable FROM client_index WHERE group_name = ?1 ORDER BY email, sub_id",
        )?;
        let rows = stmt
            .query_map(params![group_name], client_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn upsert_panel_groups(&self, group_names: &[String]) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let seen: HashSet<String> = group_names
            .iter()
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
            .collect();
        let conn = self.db.connect()?;

        let mut sorted: Vec<_> = seen.iter().cloned().collect();
        sorted.sort();
        for group_name in sorted {
            conn.execute(
                r#"
                INSERT INTO panel_groups (group_name, last_seen_at)
                VALUES (?1, ?2)
                ON CONFLICT(group_name) DO UPDATE SET last_seen_at = excluded.last_seen_at
                "#,
                params![group_name, now],
            )?;
        }

        if seen.is_empty() {
            conn.execute("DELETE FROM panel_groups", [])?;
        } else {
            let placeholders = seen.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!("DELETE FROM panel_groups WHERE group_name NOT IN ({placeholders})");
            let params: Vec<&dyn rusqlite::ToSql> = seen
                .iter()
                .map(|s| s as &dyn rusqlite::ToSql)
                .collect();
            conn.execute(&sql, params.as_slice())?;
        }

        Ok(())
    }

    pub fn list_groups(&self) -> Result<Vec<String>> {
        let conn = self.db.connect()?;
        let mut stmt = conn.prepare("SELECT group_name FROM panel_groups ORDER BY group_name")?;
        let rows = stmt
            .query_map([], |row: &Row| row.get(0))?
            .collect::<std::result::Result<Vec<String>, _>>()?;
        Ok(rows)
    }

    pub fn list_all_clients(&self, enabled_only: bool) -> Result<Vec<ClientRecord>> {
        let mut query =
            String::from("SELECT sub_id, group_name, email, enable FROM client_index");
        if enabled_only {
            query.push_str(" WHERE enable = 1");
        }
        query.push_str(" ORDER BY email, sub_id");

        let conn = self.db.connect()?;
        let mut stmt = conn.prepare(&query)?;
        let rows = stmt
            .query_map([], client_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn get_balancer_tags_for_sub_id(&self, sub_id: &str) -> Result<Vec<String>> {
        let balancers: std::collections::HashMap<String, BalancerRecord> = self
            .list_balancers()?
            .into_iter()
            .map(|balancer| (balancer.tag.clone(), balancer))
            .collect();

        let mut tags = Vec::new();
        let mut seen = HashSet::new();
        let conn = self.db.connect()?;

        let mut append_active = |candidate_tags: Vec<String>| {
            for tag in candidate_tags {
                if seen.contains(&tag) {
                    continue;
                }
                if let Some(balancer) = balancers.get(&tag) {
                    if !balancer.member_fingerprints.is_empty() {
                        tags.push(tag.clone());
                        seen.insert(tag);
                    }
                }
            }
        };

        let client_tags: Vec<String> = conn
            .prepare(
                "SELECT tag FROM balancers WHERE scope = 'client' AND scope_target = ?1 ORDER BY tag",
            )?
            .query_map(params![sub_id], |row: &Row| row.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        append_active(client_tags);

        if let Some(group_name) = self.get_group_for_sub_id(sub_id)? {
            let group_tags: Vec<String> = conn
                .prepare(
                    "SELECT tag FROM balancers WHERE scope = 'group' AND scope_target = ?1 ORDER BY tag",
                )?
                .query_map(params![group_name], |row: &Row| row.get(0))?
                .collect::<std::result::Result<_, _>>()?;
            append_active(group_tags);
        }

        let all_tags: Vec<String> = conn
            .prepare("SELECT tag FROM balancers WHERE scope = 'all' ORDER BY tag")?
            .query_map([], |row: &Row| row.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        append_active(all_tags);

        Ok(tags)
    }

    pub fn get_balancer_tag_for_sub_id(&self, sub_id: &str) -> Result<Option<String>> {
        Ok(self.get_balancer_tags_for_sub_id(sub_id)?.into_iter().next())
    }

    pub fn assign_group_balancer(&self, group_name: &str, balancer_tag: &str) -> Result<()> {
        if self.get_balancer_by_tag(balancer_tag)?.is_none() {
            return Err(anyhow!("balancer '{balancer_tag}' not found"));
        }
        if !self.set_balancer_scope(balancer_tag, "group", group_name)? {
            return Err(anyhow!("balancer '{balancer_tag}' not found"));
        }
        Ok(())
    }

    pub fn unassign_group_balancer(
        &self,
        group_name: &str,
        balancer_tag: Option<&str>,
    ) -> Result<bool> {
        let conn = self.db.connect()?;
        let affected = if let Some(tag) = balancer_tag {
            conn.execute(
                "UPDATE balancers SET scope = 'disabled', scope_target = '' \
                 WHERE scope = 'group' AND scope_target = ?1 AND tag = ?2",
                params![group_name, tag],
            )?
        } else {
            conn.execute(
                "UPDATE balancers SET scope = 'disabled', scope_target = '' \
                 WHERE scope = 'group' AND scope_target = ?1",
                params![group_name],
            )?
        };
        Ok(affected > 0)
    }

    pub fn list_balancers_for_group(&self, group_name: &str) -> Result<Vec<String>> {
        let conn = self.db.connect()?;
        let mut stmt = conn.prepare(
            "SELECT tag FROM balancers WHERE scope = 'group' AND scope_target = ?1 ORDER BY tag",
        )?;
        let rows = stmt
            .query_map(params![group_name], |row: &Row| row.get(0))?
            .collect::<std::result::Result<Vec<String>, _>>()?;
        Ok(rows)
    }

    pub fn get_balancer_for_group(&self, group_name: &str) -> Result<Option<String>> {
        Ok(self.list_balancers_for_group(group_name)?.into_iter().next())
    }

    pub fn list_group_assignments(&self) -> Result<Vec<GroupAssignment>> {
        let conn = self.db.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT
                b.scope_target AS group_name,
                b.tag AS balancer_tag,
                COUNT(ci.sub_id) AS client_count
            FROM balancers b
            LEFT JOIN client_index ci
                ON ci.group_name = b.scope_target AND ci.enable = 1
            WHERE b.scope = 'group' AND b.scope_target != ''
            GROUP BY b.scope_target, b.tag
            ORDER BY b.scope_target
            "#,
        )?;
        let rows = stmt
            .query_map([], |row: &Row| {
                Ok(GroupAssignment {
                    group_name: row.get(0)?,
                    balancer_tag: row.get(1)?,
                    client_count: row.get(2)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

fn client_from_row(row: &Row) -> rusqlite::Result<ClientRecord> {
    Ok(ClientRecord {
        sub_id: row.get(0)?,
        group_name: row.get(1)?,
        email: row.get(2)?,
        enable: row.get::<_, i32>(3)? != 0,
    })
}

fn row_to_map(row: &Row) -> rusqlite::Result<serde_json::Map<String, serde_json::Value>> {
    let column_count = row.as_ref().column_count();
    let mut map = serde_json::Map::new();
    for index in 0..column_count {
        let name = row.as_ref().column_name(index)?.to_string();
        let value: rusqlite::types::Value = row.get(index)?;
        map.insert(name, sqlite_value_to_json(value));
    }
    Ok(map)
}

fn sqlite_value_to_json(value: rusqlite::types::Value) -> serde_json::Value {
    match value {
        rusqlite::types::Value::Null => serde_json::Value::Null,
        rusqlite::types::Value::Integer(i) => serde_json::Value::Number(i.into()),
        rusqlite::types::Value::Real(f) => serde_json::Number::from_f64(f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        rusqlite::types::Value::Text(s) => serde_json::Value::String(s),
        rusqlite::types::Value::Blob(b) => {
            serde_json::Value::String(String::from_utf8_lossy(&b).to_string())
        }
    }
}