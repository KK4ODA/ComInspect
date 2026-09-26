//! Versioned schema migrations.
//!
//! * The schema version is stored in `PRAGMA user_version`.
//! * `meta.compat_schema` records the lowest schema version an application
//!   must understand to safely *write* the database. Additive migrations (new
//!   nullable columns or tables) keep it unchanged so that an older build can
//!   still use a database touched by a newer one; breaking migrations raise it.
//! * Migrations are forward-only and each runs in its own transaction. The
//!   caller backs up the database file before running any of them.

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::Result;

/// Schema version written by this build.
pub const SCHEMA_VERSION: u32 = 1;

pub struct Migration {
    /// Version the database has after this migration.
    pub version: u32,
    /// Minimum schema version able to write the database after this
    /// migration.
    pub compat: u32,
    pub description: &'static str,
    pub sql: &'static str,
}

pub const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    compat: 1,
    description: "initial schema",
    sql: r#"
CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE devices (
    id                INTEGER PRIMARY KEY,
    uuid              TEXT NOT NULL UNIQUE,
    -- user-assigned logical identity
    nickname          TEXT,
    equipment         TEXT,
    category          TEXT,
    purpose           TEXT,
    cat_status        TEXT NOT NULL DEFAULT 'unknown',
    notes             TEXT,
    ignored           INTEGER NOT NULL DEFAULT 0,
    -- hardware summary (latest observation)
    transport         TEXT NOT NULL DEFAULT 'unknown',
    vid               INTEGER,
    pid               INTEGER,
    serial_number     TEXT,
    interface_number  INTEGER,
    manufacturer      TEXT,
    product           TEXT,
    description       TEXT,
    bt_address        TEXT,
    virtual_provider  TEXT,
    last_snapshot     TEXT,
    snapshot_presence TEXT,
    snapshot_hash     TEXT,
    -- machine-specific state
    last_port         TEXT,
    connected         INTEGER NOT NULL DEFAULT 0,
    first_seen        INTEGER NOT NULL,
    last_seen         INTEGER,
    last_observed     INTEGER NOT NULL,
    awaiting          INTEGER NOT NULL DEFAULT 0,
    imported_from     TEXT,
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL
);

CREATE TABLE identity_keys (
    device_id  INTEGER NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    kind       TEXT NOT NULL,
    value      TEXT NOT NULL,
    strength   INTEGER NOT NULL,
    first_seen INTEGER NOT NULL,
    last_seen  INTEGER NOT NULL,
    PRIMARY KEY (device_id, kind, value)
) WITHOUT ROWID;
CREATE INDEX identity_keys_lookup ON identity_keys(kind, value);

CREATE TABLE port_assignments (
    device_id      INTEGER NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    port_name      TEXT NOT NULL,
    first_observed INTEGER NOT NULL,
    last_observed  INTEGER NOT NULL,
    last_connected INTEGER,
    PRIMARY KEY (device_id, port_name)
) WITHOUT ROWID;

CREATE TABLE events (
    id            INTEGER PRIMARY KEY,
    ts            INTEGER NOT NULL,
    device_id     INTEGER REFERENCES devices(id) ON DELETE CASCADE,
    kind          TEXT NOT NULL,
    port_name     TEXT,
    previous_port TEXT,
    detail        TEXT
);
CREATE INDEX events_device ON events(device_id, ts);
CREATE INDEX events_ts ON events(ts);

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) WITHOUT ROWID;
"#,
}];

pub fn user_version(conn: &Connection) -> Result<u32> {
    Ok(conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))? as u32)
}

/// Compatibility level recorded in the database, if the meta table exists.
pub fn compat_schema(conn: &Connection) -> Result<Option<u32>> {
    let has_meta: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'meta')",
        [],
        |r| r.get(0),
    )?;
    if !has_meta {
        return Ok(None);
    }
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'compat_schema'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    Ok(value.and_then(|v| v.parse().ok()))
}

/// Applies all migrations newer than the current version.
/// Returns the versions that were applied.
pub fn migrate(conn: &mut Connection, app_version: &str) -> Result<Vec<u32>> {
    let current = user_version(conn)?;
    let mut applied = Vec::new();
    for migration in MIGRATIONS.iter().filter(|m| m.version > current) {
        let tx = conn.transaction()?;
        tx.execute_batch(migration.sql)?;
        let now = cominspect_core::time::now_ms().to_string();
        let previous_compat: u32 = tx
            .query_row(
                "SELECT value FROM meta WHERE key = 'compat_schema'",
                [],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let compat = previous_compat.max(migration.compat);
        tx.execute(
            "INSERT INTO meta(key, value) VALUES ('compat_schema', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![compat.to_string()],
        )?;
        tx.execute(
            "INSERT INTO meta(key, value) VALUES ('created_at', ?1) ON CONFLICT(key) DO NOTHING",
            params![now],
        )?;
        tx.execute(
            "INSERT INTO meta(key, value) VALUES ('created_by_version', ?1)
             ON CONFLICT(key) DO NOTHING",
            params![app_version],
        )?;
        tx.execute_batch(&format!("PRAGMA user_version = {}", migration.version))?;
        tx.commit()?;
        log::info!(
            "database migrated to schema {} ({})",
            migration.version,
            migration.description
        );
        applied.push(migration.version);
    }
    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_ordered_and_reach_current_version() {
        let mut last = 0;
        for m in MIGRATIONS {
            assert!(m.version > last, "migrations must be strictly increasing");
            assert!(m.compat <= m.version);
            last = m.version;
        }
        assert_eq!(last, SCHEMA_VERSION);
    }

    #[test]
    fn migrating_fresh_database() {
        let mut conn = Connection::open_in_memory().unwrap();
        assert_eq!(user_version(&conn).unwrap(), 0);
        assert_eq!(compat_schema(&conn).unwrap(), None);
        let applied = migrate(&mut conn, "0.1.0").unwrap();
        assert_eq!(applied, vec![1]);
        assert_eq!(user_version(&conn).unwrap(), SCHEMA_VERSION);
        assert_eq!(compat_schema(&conn).unwrap(), Some(1));
        // Idempotent.
        assert!(migrate(&mut conn, "0.1.0").unwrap().is_empty());
    }
}
