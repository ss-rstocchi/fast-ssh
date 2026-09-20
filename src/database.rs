use anyhow::{Context, Result};
use rustbreak::{deser::Ron, FileDatabase as _FileDatabase, RustbreakError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub struct FileDatabase {
    db: _FileDatabase<HashMap<String, HostDatabaseEntry>, Ron>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, Copy)]
pub struct HostDatabaseEntry {
    pub connection_count: i64,
    pub last_used_date: i64,
}

impl FileDatabase {
    pub fn new(filename: &str) -> Result<FileDatabase> {
        let load = || {
            _FileDatabase::<HashMap<String, HostDatabaseEntry>, Ron>::load_from_path_or_default(
                filename,
            )
        };

        match load() {
            Ok(db) => Ok(FileDatabase { db }),
            Err(error) => {
                // A corrupt database should not make the app unusable: move it
                // aside and start from a fresh one.
                let backup = format!("{}.bak", filename);
                std::fs::rename(filename, &backup).with_context(|| {
                    format!(
                        "Database at {} is unreadable ({error}) and could not be backed up to {}",
                        filename, backup
                    )
                })?;
                eprintln!(
                    "Warning: unreadable database at {} moved to {} ({error})",
                    filename, backup
                );

                let db = load().with_context(|| {
                    format!("Error while creating a fresh database at {}", filename)
                })?;
                Ok(FileDatabase { db })
            }
        }
    }

    pub fn get_host_values(&self, host_key: &str) -> Result<HostDatabaseEntry, RustbreakError> {
        self.db
            .read(|db| db.get(host_key).copied().unwrap_or_default())
    }

    pub fn save_host_values(
        &self,
        host_key: &str,
        connection_count: i64,
        last_used_date: i64,
    ) -> Result<(), RustbreakError> {
        self.db.write(|db| {
            db.insert(
                host_key.to_owned(),
                HostDatabaseEntry {
                    connection_count,
                    last_used_date,
                },
            );
        })?;

        self.db.save()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Each test gets its own database file so parallel tests cannot clobber
    /// or delete each other's data.
    fn get_test_db_path() -> String {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);

        let test_dir = std::env::temp_dir().join("fast_ssh_tests");
        fs::create_dir_all(&test_dir).unwrap();
        let path = test_dir.join(format!(
            "test_db_{}_{}.ron",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        path.to_str().unwrap().to_string()
    }

    fn cleanup_test_db(path: &str) {
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_host_database_entry_default() {
        let entry = HostDatabaseEntry::default();
        assert_eq!(entry.connection_count, 0);
        assert_eq!(entry.last_used_date, 0);
    }

    #[test]
    fn test_host_database_entry_clone() {
        let entry = HostDatabaseEntry {
            connection_count: 5,
            last_used_date: 9876543210,
        };
        let cloned = entry;
        assert_eq!(entry.connection_count, cloned.connection_count);
        assert_eq!(entry.last_used_date, cloned.last_used_date);
    }

    #[test]
    fn test_file_database_new() {
        let path = get_test_db_path();
        let result = FileDatabase::new(&path);
        assert!(
            result.is_ok(),
            "failed to create database: {:?}",
            result.err()
        );
        cleanup_test_db(&path);
    }

    #[test]
    fn test_get_host_values_nonexistent() {
        let path = get_test_db_path();
        let db = FileDatabase::new(&path).expect("failed to create database");

        let entry = db.get_host_values("nonexistent-host").expect("read failed");
        assert_eq!(entry.connection_count, 0);
        assert_eq!(entry.last_used_date, 0);

        cleanup_test_db(&path);
    }

    #[test]
    fn test_save_and_get_host_values() {
        let path = get_test_db_path();
        let db = FileDatabase::new(&path).expect("failed to create database");

        db.save_host_values("test-host", 42, 1234567890)
            .expect("save failed");

        let entry = db.get_host_values("test-host").expect("read failed");
        assert_eq!(entry.connection_count, 42);
        assert_eq!(entry.last_used_date, 1234567890);

        cleanup_test_db(&path);
    }

    #[test]
    fn test_update_host_values() {
        let path = get_test_db_path();
        let db = FileDatabase::new(&path).expect("failed to create database");

        db.save_host_values("test-host", 1, 100)
            .expect("save failed");
        db.save_host_values("test-host", 5, 500)
            .expect("save failed");

        let entry = db.get_host_values("test-host").expect("read failed");
        assert_eq!(entry.connection_count, 5);
        assert_eq!(entry.last_used_date, 500);

        cleanup_test_db(&path);
    }

    #[test]
    fn test_multiple_hosts() {
        let path = get_test_db_path();
        let db = FileDatabase::new(&path).expect("failed to create database");

        db.save_host_values("host1", 10, 1000).expect("save failed");
        db.save_host_values("host2", 20, 2000).expect("save failed");
        db.save_host_values("host3", 30, 3000).expect("save failed");

        assert_eq!(db.get_host_values("host1").unwrap().connection_count, 10);
        assert_eq!(db.get_host_values("host2").unwrap().connection_count, 20);
        assert_eq!(db.get_host_values("host3").unwrap().connection_count, 30);

        cleanup_test_db(&path);
    }

    #[test]
    fn test_corrupt_database_is_backed_up_and_reset() {
        let path = get_test_db_path();
        fs::write(&path, "this is not valid ron").unwrap();

        let db = FileDatabase::new(&path).expect("should recover from a corrupt database");
        assert_eq!(db.get_host_values("host").unwrap().connection_count, 0);

        let backup = format!("{}.bak", path);
        assert!(std::path::Path::new(&backup).exists());

        cleanup_test_db(&path);
        let _ = fs::remove_file(backup);
    }

    #[test]
    fn test_database_persistence() {
        let path = get_test_db_path();

        {
            let db = FileDatabase::new(&path).expect("failed to create database");
            db.save_host_values("persistent-host", 99, 9999)
                .expect("save failed");
        }

        let db = FileDatabase::new(&path).expect("failed to reopen database");
        let entry = db.get_host_values("persistent-host").expect("read failed");
        assert_eq!(entry.connection_count, 99);
        assert_eq!(entry.last_used_date, 9999);

        cleanup_test_db(&path);
    }
}
