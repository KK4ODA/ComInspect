//! Database file handling: opening, integrity checks, migrations, backups,
//! meta values and settings.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, ErrorCode, OpenFlags, OptionalExtension, params};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::{Result, StoreError};
use crate::schema::{self, SCHEMA_VERSION};

/// Number of backup files kept in the `backups` directory.
pub const BACKUP_RETENTION: usize = 10;

/// What happened while opening the database.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenReport {
    pub path: PathBuf,
    pub created: bool,
    pub schema_before: u32,
    pub schema_after: u32,
    pub migrations_applied: Vec<u32>,
    /// Backup taken before migrating.
    pub backup: Option<PathBuf>,
    /// A damaged database file was moved to this path and a new one created.
    pub recovered_corrupt_file: Option<PathBuf>,
    /// Set when the app runs on a temporary in-memory database (the file on
    /// disk was left untouched).
    pub temporary_reason: Option<String>,
    /// Schema version of a database written by a newer, incompatible build.
    pub newer_schema: Option<u32>,
}

/// A backup file in the `backups` directory.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub path: PathBuf,
    pub file_name: String,
    pub size: u64,
    pub modified: Option<i64>,
    pub schema: Option<u32>,
}

pub struct Store {
    pub(crate) conn: Connection,
    path: Option<PathBuf>,
    temporary_reason: Option<String>,
}

fn is_damaged(error: &StoreError) -> bool {
    matches!(
        error,
        StoreError::Sqlite(rusqlite::Error::SqliteFailure(e, _))
            if matches!(e.code, ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt)
    )
}

/// `YYYYMMDD-HHMMSS` in UTC.
fn timestamp_label() -> String {
    let now = time::OffsetDateTime::now_utc();
    now.format(time::macros::format_description!(
        "[year][month][day]-[hour][minute][second]"
    ))
    .unwrap_or_else(|_| cominspect_core::time::now_ms().to_string())
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Moves `db` and its WAL/SHM side files to `<stem>.<label>-<ts>.db`.
fn move_aside(db: &Path, label: &str) -> Result<PathBuf> {
    let stem = db
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "inventory".into());
    let target = db.with_file_name(format!("{stem}.{label}-{}.db", timestamp_label()));
    if db.exists() {
        fs::rename(db, &target)?;
    }
    for suffix in ["-wal", "-shm"] {
        let side = sidecar(db, suffix);
        if side.exists() {
            let side_target = sidecar(&target, suffix);
            if let Err(e) = fs::rename(&side, &side_target) {
                log::warn!("could not move {}: {e}", side.display());
                let _ = fs::remove_file(&side);
            }
        }
    }
    Ok(target)
}

impl Store {
    fn configure(conn: &Connection) -> Result<()> {
        conn.busy_timeout(Duration::from_secs(5))?;
        let _mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        Ok(())
    }

    fn open_checked(path: &Path, check: bool) -> Result<Connection> {
        let conn = Connection::open(path)?;
        Self::configure(&conn)?;
        if check {
            let result: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
            if result != "ok" {
                return Err(StoreError::Sqlite(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CORRUPT),
                    Some(result),
                )));
            }
        }
        Ok(conn)
    }

    /// Opens (creating if necessary) the database at `path`, migrating it to
    /// the current schema. A backup is taken before any migration.
    pub fn open(path: &Path, app_version: &str) -> Result<(Store, OpenReport)> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let created = !path.exists();
        let mut report = OpenReport {
            path: path.to_path_buf(),
            created,
            ..Default::default()
        };

        let mut conn = match Self::open_checked(path, !created) {
            Ok(conn) => conn,
            Err(e) if is_damaged(&e) => {
                log::error!(
                    "database {} is damaged ({e}); starting a new one",
                    path.display()
                );
                let moved = move_aside(path, "damaged")?;
                report.recovered_corrupt_file = Some(moved);
                report.created = true;
                Self::open_checked(path, false)?
            }
            Err(e) => return Err(e),
        };

        let version = schema::user_version(&conn)?;
        report.schema_before = version;

        if version > SCHEMA_VERSION {
            let compat = schema::compat_schema(&conn)?.unwrap_or(version);
            if compat > SCHEMA_VERSION {
                drop(conn);
                let reason = format!(
                    "The device database was written by a newer version of ComInspect \
                     (schema {version}; this version understands schema {SCHEMA_VERSION}). \
                     It has been left untouched and changes made now will not be saved. \
                     Update ComInspect, or restore the backup taken before the upgrade."
                );
                log::warn!("{reason}");
                let store = Store::open_temporary(app_version, reason.clone())?;
                report.schema_after = SCHEMA_VERSION;
                report.temporary_reason = Some(reason);
                report.newer_schema = Some(version);
                return Ok((store, report));
            }
            log::info!("database schema {version} is newer but compatible (compat {compat})");
        } else if version < SCHEMA_VERSION {
            if version > 0 {
                let store = Store {
                    conn,
                    path: Some(path.to_path_buf()),
                    temporary_reason: None,
                };
                report.backup = Some(store.create_backup(&format!("schema{version}"))?);
                conn = store.conn;
            }
            report.migrations_applied = schema::migrate(&mut conn, app_version)?;
        }

        report.schema_after = schema::user_version(&conn)?;
        let store = Store {
            conn,
            path: Some(path.to_path_buf()),
            temporary_reason: None,
        };
        store.set_meta("last_opened_by_version", app_version)?;
        Ok((store, report))
    }

    /// In-memory database at the current schema.
    pub fn open_in_memory() -> Result<Store> {
        let mut conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", true)?;
        schema::migrate(&mut conn, env!("CARGO_PKG_VERSION"))?;
        Ok(Store {
            conn,
            path: None,
            temporary_reason: None,
        })
    }

    fn open_temporary(app_version: &str, reason: String) -> Result<Store> {
        let mut conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", true)?;
        schema::migrate(&mut conn, app_version)?;
        Ok(Store {
            conn,
            path: None,
            temporary_reason: Some(reason),
        })
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Why the store is running on a temporary database, if it is.
    pub fn temporary_reason(&self) -> Option<&str> {
        self.temporary_reason.as_deref()
    }

    pub fn schema_version(&self) -> Result<u32> {
        schema::user_version(&self.conn)
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    // --- meta -----------------------------------------------------------

    pub fn get_meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| {
                r.get(0)
            })
            .optional()?)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn delete_meta(&self, key: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM meta WHERE key = ?1", params![key])?;
        Ok(())
    }

    // --- settings -------------------------------------------------------

    pub fn get_setting<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?;
        match raw {
            None => Ok(None),
            Some(raw) => match serde_json::from_str(&raw) {
                Ok(v) => Ok(Some(v)),
                Err(e) => {
                    log::warn!("ignoring unreadable setting {key}: {e}");
                    Ok(None)
                }
            },
        }
    }

    pub fn set_setting<T: Serialize + ?Sized>(&self, key: &str, value: &T) -> Result<()> {
        let json = serde_json::to_string(value)?;
        self.conn.execute(
            "INSERT INTO settings(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, json],
        )?;
        Ok(())
    }

    // --- backups --------------------------------------------------------

    pub fn backups_dir(&self) -> Option<PathBuf> {
        self.path.as_deref().map(backups_dir_for)
    }

    /// Writes a consistent copy of the database to the `backups` directory.
    pub fn create_backup(&self, label: &str) -> Result<PathBuf> {
        let dir = self.backups_dir().ok_or_else(|| {
            StoreError::Invalid("an in-memory database cannot be backed up".into())
        })?;
        fs::create_dir_all(&dir)?;
        let safe_label: String = label
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let target = dir.join(format!("inventory-{safe_label}-{}.db", timestamp_label()));
        if target.exists() {
            fs::remove_file(&target)?;
        }
        self.conn.execute(
            "VACUUM INTO ?1",
            params![target.to_string_lossy().into_owned()],
        )?;
        prune_backups(&dir, BACKUP_RETENTION)?;
        log::info!("database backup written to {}", target.display());
        Ok(target)
    }

    /// Writes a consistent copy of the database to an arbitrary path.
    pub fn backup_to(&self, target: &Path) -> Result<()> {
        if target.exists() {
            fs::remove_file(target)?;
        }
        self.conn.execute(
            "VACUUM INTO ?1",
            params![target.to_string_lossy().into_owned()],
        )?;
        Ok(())
    }
}

pub fn backups_dir_for(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .map(|p| p.join("backups"))
        .unwrap_or_else(|| PathBuf::from("backups"))
}

fn prune_backups(dir: &Path, keep: usize) -> Result<()> {
    let mut files: Vec<_> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.starts_with("inventory-") && name.ends_with(".db")
        })
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    files.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    for (_, path) in files.into_iter().skip(keep) {
        if let Err(e) = fs::remove_file(&path) {
            log::warn!("could not remove old backup {}: {e}", path.display());
        }
    }
    Ok(())
}

/// Lists backups for the database at `db_path`, newest first.
pub fn list_backups(db_path: &Path) -> Result<Vec<BackupInfo>> {
    let dir = backups_dir_for(db_path);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(&dir)?.filter_map(|e| e.ok()) {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if !file_name.ends_with(".db") {
            continue;
        }
        let meta = entry.metadata()?;
        let modified = meta
            .modified()
            .ok()
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64);
        let schema = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .ok()
            .and_then(|c| schema::user_version(&c).ok());
        out.push(BackupInfo {
            path,
            file_name,
            size: meta.len(),
            modified,
            schema,
        });
    }
    out.sort_by(|a, b| {
        b.modified
            .cmp(&a.modified)
            .then(b.file_name.cmp(&a.file_name))
    });
    Ok(out)
}

/// Replaces the database at `db_path` with `backup`. The current file (and
/// its WAL/SHM files) is moved aside first, and its new location returned.
/// Must be called while no connection to the database is open.
pub fn restore_backup(db_path: &Path, backup: &Path) -> Result<Option<PathBuf>> {
    if !backup.exists() {
        return Err(StoreError::Invalid(format!(
            "backup {} does not exist",
            backup.display()
        )));
    }
    // Validate the backup before touching anything.
    {
        let conn = Connection::open_with_flags(backup, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let check: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if check != "ok" {
            return Err(StoreError::Invalid("the backup file is damaged".into()));
        }
    }
    let moved = if db_path.exists() {
        Some(move_aside(db_path, "replaced")?)
    } else {
        None
    };
    fs::copy(backup, db_path)?;
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_and_reopens_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("inventory.db");
        let (store, report) = Store::open(&path, "0.1.0").unwrap();
        assert!(report.created);
        assert_eq!(report.schema_before, 0);
        assert_eq!(report.schema_after, SCHEMA_VERSION);
        assert_eq!(report.backup, None, "no backup for a brand new database");
        store.set_setting("update.autoCheck", &false).unwrap();
        drop(store);

        let (store, report) = Store::open(&path, "0.1.0").unwrap();
        assert!(!report.created);
        assert!(report.migrations_applied.is_empty());
        assert_eq!(
            store.get_setting::<bool>("update.autoCheck").unwrap(),
            Some(false)
        );
        assert_eq!(store.get_setting::<bool>("missing").unwrap(), None);
    }

    #[test]
    fn older_schema_is_backed_up_before_migration() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("inventory.db");
        {
            // A database at schema 0 with some foreign content.
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch("CREATE TABLE legacy(x); INSERT INTO legacy VALUES (1);")
                .unwrap();
            conn.execute_batch("PRAGMA user_version = 0").unwrap();
        }
        // Schema 0 databases are treated as new: migrations run, no backup.
        let (_store, report) = Store::open(&path, "0.1.0").unwrap();
        assert_eq!(report.migrations_applied, vec![1]);
    }

    #[test]
    fn newer_incompatible_schema_runs_on_temporary_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("inventory.db");
        {
            let (store, _) = Store::open(&path, "0.1.0").unwrap();
            store.set_meta("compat_schema", "99").unwrap();
            store
                .conn
                .execute_batch("PRAGMA user_version = 99")
                .unwrap();
        }
        let before = fs::read(&path).unwrap();
        let (store, report) = Store::open(&path, "0.1.0").unwrap();
        assert_eq!(report.newer_schema, Some(99));
        assert!(store.temporary_reason().is_some());
        assert!(store.path().is_none());
        store.set_setting("x", &1).unwrap();
        drop(store);
        assert_eq!(fs::read(&path).unwrap(), before, "file must be untouched");
    }

    #[test]
    fn newer_compatible_schema_is_used_normally() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("inventory.db");
        {
            let (store, _) = Store::open(&path, "0.1.0").unwrap();
            store.conn.execute_batch("PRAGMA user_version = 5").unwrap();
        }
        let (store, report) = Store::open(&path, "0.1.0").unwrap();
        assert_eq!(report.newer_schema, None);
        assert!(store.temporary_reason().is_none());
        assert_eq!(store.schema_version().unwrap(), 5, "never down-migrated");
    }

    #[test]
    fn damaged_file_is_moved_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("inventory.db");
        fs::write(
            &path,
            b"this is definitely not an sqlite database, just junk bytes",
        )
        .unwrap();
        let (store, report) = Store::open(&path, "0.1.0").unwrap();
        let moved = report.recovered_corrupt_file.expect("damaged file moved");
        assert!(moved.exists());
        assert!(store.path().is_some());
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn backups_are_listed_pruned_and_restorable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("inventory.db");
        let (store, _) = Store::open(&path, "0.1.0").unwrap();
        store.set_setting("marker", &"original").unwrap();
        let backup = store.create_backup("manual").unwrap();
        assert!(backup.exists());
        store.set_setting("marker", &"changed").unwrap();

        let backups = list_backups(&path).unwrap();
        assert_eq!(backups.len(), 1);
        assert_eq!(backups[0].schema, Some(SCHEMA_VERSION));
        drop(store);

        let moved = restore_backup(&path, &backup).unwrap();
        assert!(moved.unwrap().exists());
        let (store, _) = Store::open(&path, "0.1.0").unwrap();
        assert_eq!(
            store.get_setting::<String>("marker").unwrap().as_deref(),
            Some("original")
        );

        // Retention.
        let backups_dir = backups_dir_for(&path);
        for i in 0..(BACKUP_RETENTION + 3) {
            fs::write(backups_dir.join(format!("inventory-old{i:02}-x.db")), b"x").unwrap();
        }
        prune_backups(&backups_dir, BACKUP_RETENTION).unwrap();
        assert_eq!(
            fs::read_dir(&backups_dir).unwrap().count(),
            BACKUP_RETENTION
        );
    }
}
