//! SQLite persistence for historical JVM metrics.
//!
//! Stores time-series data (heap, CPU, GC, threads, HTTP) in a local SQLite
//! database so charts survive process restarts.
//!
//! Feature-gated behind `persistence` to keep the binary small when not needed.
//!
//! Usage:
//!   cargo build -p drishti-core --features persistence

#[cfg(feature = "persistence")]
pub mod db {
    use crate::model::JvmSnapshot;
    use rusqlite::{params, Connection};
    use std::path::Path;
    use tracing;

    pub struct MetricsDb {
        conn: Connection,
    }

    impl MetricsDb {
        /// Open or create the metrics database.
        pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
            let conn = Connection::open(path)?;
            let db = Self { conn };
            db.create_tables()?;
            Ok(db)
        }

        /// Open an in-memory database (for testing).
        pub fn open_memory() -> Result<Self, rusqlite::Error> {
            let conn = Connection::open_in_memory()?;
            let db = Self { conn };
            db.create_tables()?;
            Ok(db)
        }

        fn create_tables(&self) -> Result<(), rusqlite::Error> {
            self.conn.execute_batch(
                "
                CREATE TABLE IF NOT EXISTS snapshots (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    timestamp TEXT NOT NULL,
                    heap_used INTEGER NOT NULL,
                    heap_max INTEGER NOT NULL,
                    non_heap_used INTEGER NOT NULL,
                    process_cpu REAL NOT NULL,
                    system_cpu REAL NOT NULL,
                    threads_live INTEGER NOT NULL,
                    threads_daemon INTEGER NOT NULL,
                    gc_count INTEGER NOT NULL,
                    gc_time_ms INTEGER NOT NULL,
                    http_total_requests INTEGER NOT NULL,
                    http_total_errors INTEGER NOT NULL,
                    http_avg_latency_ms REAL NOT NULL,
                    hikari_active INTEGER,
                    hikari_max INTEGER,
                    hikari_pending INTEGER
                );

                CREATE INDEX IF NOT EXISTS idx_snapshots_ts ON snapshots(timestamp);

                CREATE TABLE IF NOT EXISTS gc_events (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    timestamp TEXT NOT NULL,
                    gc_id INTEGER NOT NULL,
                    collector TEXT NOT NULL,
                    cause TEXT NOT NULL,
                    phase TEXT NOT NULL,
                    heap_before INTEGER NOT NULL,
                    heap_after INTEGER NOT NULL,
                    pause_ms REAL NOT NULL
                );

                CREATE INDEX IF NOT EXISTS idx_gc_events_ts ON gc_events(timestamp);

                CREATE TABLE IF NOT EXISTS alerts (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    timestamp TEXT NOT NULL,
                    alert_id TEXT NOT NULL,
                    severity TEXT NOT NULL,
                    title TEXT NOT NULL,
                    detail TEXT NOT NULL,
                    confidence REAL NOT NULL
                );
            ",
            )?;
            Ok(())
        }

        /// Store a snapshot as a compact row.
        pub fn insert_snapshot(&self, snap: &JvmSnapshot) -> Result<(), rusqlite::Error> {
            let gc_count: i64 = snap.gc_collectors.iter().map(|c| c.collection_count).sum();
            let gc_time: i64 = snap
                .gc_collectors
                .iter()
                .map(|c| c.collection_time_ms)
                .sum();

            self.conn.execute(
                "INSERT INTO snapshots (timestamp, heap_used, heap_max, non_heap_used,
                    process_cpu, system_cpu, threads_live, threads_daemon,
                    gc_count, gc_time_ms, http_total_requests, http_total_errors,
                    http_avg_latency_ms, hikari_active, hikari_max, hikari_pending)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                params![
                    snap.timestamp.to_rfc3339(),
                    snap.heap.used,
                    snap.heap.max,
                    snap.non_heap.used,
                    snap.cpu.process_cpu,
                    snap.cpu.system_cpu,
                    snap.thread_summary.live,
                    snap.thread_summary.daemon,
                    gc_count,
                    gc_time,
                    snap.http.total_requests,
                    snap.http.total_errors,
                    snap.http.avg_latency_ms,
                    snap.hikari.as_ref().map(|h| h.active as i64),
                    snap.hikari.as_ref().map(|h| h.max as i64),
                    snap.hikari.as_ref().map(|h| h.pending as i64),
                ],
            )?;
            Ok(())
        }

        /// Store a GC event.
        pub fn insert_gc_event(&self, ev: &crate::model::GcEvent) -> Result<(), rusqlite::Error> {
            self.conn.execute(
                "INSERT INTO gc_events (timestamp, gc_id, collector, cause, phase, heap_before, heap_after, pause_ms)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    ev.timestamp.to_rfc3339(), ev.id as i64, ev.collector, ev.cause,
                    format!("{:?}", ev.phase), ev.heap_before_bytes, ev.heap_after_bytes, ev.pause_ms,
                ],
            )?;
            Ok(())
        }

        /// Query historical heap usage for charting.
        pub fn query_heap_history(
            &self,
            hours: u32,
        ) -> Result<Vec<(String, i64, i64)>, rusqlite::Error> {
            let cutoff = chrono::Utc::now() - chrono::Duration::hours(hours as i64);
            let mut stmt = self.conn.prepare(
                "SELECT timestamp, heap_used, heap_max FROM snapshots
                 WHERE timestamp > ?1 ORDER BY timestamp",
            )?;
            let rows = stmt.query_map(params![cutoff.to_rfc3339()], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?;
            rows.collect()
        }

        /// Query historical CPU usage for charting.
        pub fn query_cpu_history(
            &self,
            hours: u32,
        ) -> Result<Vec<(String, f64, f64)>, rusqlite::Error> {
            let cutoff = chrono::Utc::now() - chrono::Duration::hours(hours as i64);
            let mut stmt = self.conn.prepare(
                "SELECT timestamp, process_cpu, system_cpu FROM snapshots
                 WHERE timestamp > ?1 ORDER BY timestamp",
            )?;
            let rows = stmt.query_map(params![cutoff.to_rfc3339()], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?;
            rows.collect()
        }

        /// Prune old data beyond retention period.
        pub fn prune(&self, retain_hours: u32) -> Result<usize, rusqlite::Error> {
            let cutoff = chrono::Utc::now() - chrono::Duration::hours(retain_hours as i64);
            let ts = cutoff.to_rfc3339();
            let mut total = 0;
            total += self
                .conn
                .execute("DELETE FROM snapshots WHERE timestamp < ?1", params![ts])?;
            total += self
                .conn
                .execute("DELETE FROM gc_events WHERE timestamp < ?1", params![ts])?;
            total += self
                .conn
                .execute("DELETE FROM alerts WHERE timestamp < ?1", params![ts])?;
            Ok(total)
        }

        /// Get total snapshot count.
        pub fn snapshot_count(&self) -> Result<i64, rusqlite::Error> {
            self.conn
                .query_row("SELECT COUNT(*) FROM snapshots", [], |row| row.get(0))
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::model::JvmSnapshot;
        use chrono::Utc;

        #[test]
        fn insert_and_query() {
            let db = MetricsDb::open_memory().unwrap();
            let mut snap = JvmSnapshot::default();
            snap.timestamp = Utc::now();
            snap.heap.used = 256 * 1024 * 1024;
            snap.heap.max = 512 * 1024 * 1024;
            snap.cpu.process_cpu = 0.42;

            db.insert_snapshot(&snap).unwrap();
            assert_eq!(db.snapshot_count().unwrap(), 1);

            let history = db.query_heap_history(1).unwrap();
            assert_eq!(history.len(), 1);
            assert_eq!(history[0].1, 256 * 1024 * 1024);
        }

        #[test]
        fn prune_removes_old_data() {
            let db = MetricsDb::open_memory().unwrap();
            let mut snap = JvmSnapshot::default();
            snap.timestamp = Utc::now() - chrono::Duration::hours(48);
            db.insert_snapshot(&snap).unwrap();

            snap.timestamp = Utc::now();
            db.insert_snapshot(&snap).unwrap();

            assert_eq!(db.snapshot_count().unwrap(), 2);
            let removed = db.prune(24).unwrap();
            assert!(removed >= 1);
            assert_eq!(db.snapshot_count().unwrap(), 1);
        }
    }
}
