//! Session `LOCK TABLES` / `UNLOCK TABLES` metadata locks (M163).

use std::collections::{HashMap, HashSet};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// READ vs WRITE table lock from `LOCK TABLES`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableLockMode {
    Read,
    Write,
}

/// Lock could not be acquired before `innodb_lock_wait_timeout` (MySQL errno 1205).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableLockWaitTimeout;

#[derive(Debug, Default)]
struct TableEntry {
    write: Option<u64>,
    reads: HashSet<u64>,
}

#[derive(Debug, Default)]
struct Inner {
    tables: HashMap<String, TableEntry>,
    /// Locks held per session (table key → mode).
    by_session: HashMap<u64, HashMap<String, TableLockMode>>,
}

/// Process-wide table lock registry shared by connections.
pub struct TableLockRegistry {
    inner: Mutex<Inner>,
    cvar: Condvar,
}

impl Default for TableLockRegistry {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            cvar: Condvar::new(),
        }
    }
}

impl std::fmt::Debug for TableLockRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TableLockRegistry").finish_non_exhaustive()
    }
}

impl TableLockRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace this session's locks with `spec` (sorted acquisition order).
    pub fn lock_tables(
        &self,
        session_id: u64,
        spec: &[(String, TableLockMode)],
        timeout: Duration,
    ) -> Result<(), TableLockWaitTimeout> {
        let deadline = Instant::now() + timeout;
        let mut pending: Vec<(String, TableLockMode)> = spec.to_vec();
        pending.sort_by(|a, b| a.0.cmp(&b.0));

        loop {
            let mut guard = self.inner.lock().expect("table lock registry");
            self.release_session_locked(&mut guard, session_id);

            let mut acquired = Vec::new();
            let mut blocked = false;
            for (table, mode) in &pending {
                if self.can_acquire(&guard, session_id, table, *mode) {
                    self.grant_locked(&mut guard, session_id, table, *mode);
                    acquired.push(table.clone());
                } else {
                    blocked = true;
                    break;
                }
            }
            if blocked {
                for table in acquired {
                    self.revoke_one_locked(&mut guard, session_id, &table);
                }
                if Instant::now() >= deadline {
                    return Err(TableLockWaitTimeout);
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                match self.cvar.wait_timeout(guard, remaining) {
                    Ok((_, result)) if result.timed_out() => return Err(TableLockWaitTimeout),
                    Ok(_) => {}
                    Err(_) => return Err(TableLockWaitTimeout),
                }
                continue;
            }
            guard.by_session.insert(
                session_id,
                pending.iter().map(|(t, m)| (t.clone(), *m)).collect(),
            );
            self.cvar.notify_all();
            return Ok(());
        }
    }

    pub fn unlock_tables(&self, session_id: u64) {
        let mut guard = self.inner.lock().expect("table lock registry");
        self.release_session_locked(&mut guard, session_id);
    }

    pub fn release_all(&self, session_id: u64) {
        self.unlock_tables(session_id);
    }

    /// Snapshot of locks held by `session_id` (empty when not in lock mode).
    pub fn session_locks(&self, session_id: u64) -> Option<HashMap<String, TableLockMode>> {
        let guard = self.inner.lock().expect("table lock registry");
        guard.by_session.get(&session_id).cloned()
    }

    pub fn in_lock_tables_mode(&self, session_id: u64) -> bool {
        let guard = self.inner.lock().expect("table lock registry");
        guard.by_session.contains_key(&session_id)
    }

    /// Wait until `session_id` may read or write `table` per other sessions' table locks.
    pub fn wait_for_access(
        &self,
        session_id: u64,
        table: &str,
        need_write: bool,
        timeout: Duration,
    ) -> Result<(), TableLockWaitTimeout> {
        if timeout.is_zero() {
            let guard = self.inner.lock().expect("table lock registry");
            return if self.must_wait(&guard, session_id, table, need_write) {
                Err(TableLockWaitTimeout)
            } else {
                Ok(())
            };
        }
        let deadline = Instant::now() + timeout;
        let mut guard = self.inner.lock().expect("table lock registry");
        loop {
            if !self.must_wait(&guard, session_id, table, need_write) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(TableLockWaitTimeout);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.cvar.wait_timeout(guard, remaining) {
                Ok((g, result)) => {
                    guard = g;
                    if result.timed_out() {
                        return Err(TableLockWaitTimeout);
                    }
                }
                Err(_) => return Err(TableLockWaitTimeout),
            }
        }
    }

    fn must_wait(&self, guard: &Inner, session_id: u64, table: &str, need_write: bool) -> bool {
        let Some(entry) = guard.tables.get(table) else {
            return false;
        };
        if let Some(w) = entry.write {
            if w != session_id {
                return true;
            }
            return false;
        }
        if need_write {
            return entry.reads.iter().any(|&r| r != session_id);
        }
        false
    }

    fn can_acquire(
        &self,
        guard: &Inner,
        session_id: u64,
        table: &str,
        mode: TableLockMode,
    ) -> bool {
        let Some(entry) = guard.tables.get(table) else {
            return true;
        };
        match mode {
            TableLockMode::Write => {
                if let Some(w) = entry.write {
                    return w == session_id;
                }
                entry.reads.is_empty() || entry.reads == HashSet::from([session_id])
            }
            TableLockMode::Read => entry.write.is_none() || entry.write == Some(session_id),
        }
    }

    fn grant_locked(&self, guard: &mut Inner, session_id: u64, table: &str, mode: TableLockMode) {
        let entry = guard.tables.entry(table.to_string()).or_default();
        match mode {
            TableLockMode::Write => {
                entry.write = Some(session_id);
                entry.reads.remove(&session_id);
            }
            TableLockMode::Read => {
                entry.reads.insert(session_id);
            }
        }
    }

    fn revoke_one_locked(&self, guard: &mut Inner, session_id: u64, table: &str) {
        if let Some(entry) = guard.tables.get_mut(table) {
            if entry.write == Some(session_id) {
                entry.write = None;
            }
            entry.reads.remove(&session_id);
            if entry.write.is_none() && entry.reads.is_empty() {
                guard.tables.remove(table);
            }
        }
    }

    fn release_session_locked(&self, guard: &mut Inner, session_id: u64) {
        if let Some(held) = guard.by_session.remove(&session_id) {
            for table in held.keys() {
                self.revoke_one_locked(guard, session_id, table);
            }
            self.cvar.notify_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn write_blocks_other_write_and_read() {
        let reg = TableLockRegistry::new();
        reg.lock_tables(
            1,
            &[("t".into(), TableLockMode::Write)],
            Duration::from_secs(1),
        )
        .unwrap();
        assert!(matches!(
            reg.wait_for_access(2, "t", false, Duration::ZERO),
            Err(TableLockWaitTimeout)
        ));
        assert!(matches!(
            reg.wait_for_access(2, "t", true, Duration::ZERO),
            Err(TableLockWaitTimeout)
        ));
        reg.unlock_tables(1);
        reg.wait_for_access(2, "t", true, Duration::ZERO).unwrap();
    }

    #[test]
    fn read_allows_concurrent_read_blocks_write() {
        let reg = TableLockRegistry::new();
        reg.lock_tables(
            1,
            &[("t".into(), TableLockMode::Read)],
            Duration::from_secs(1),
        )
        .unwrap();
        reg.wait_for_access(2, "t", false, Duration::ZERO).unwrap();
        assert!(matches!(
            reg.wait_for_access(2, "t", true, Duration::ZERO),
            Err(TableLockWaitTimeout)
        ));
        reg.lock_tables(
            2,
            &[("t".into(), TableLockMode::Read)],
            Duration::from_secs(1),
        )
        .unwrap();
        reg.unlock_tables(1);
        reg.unlock_tables(2);
    }

    #[test]
    fn write_lock_released_on_unlock() {
        use std::sync::Arc;
        let reg = Arc::new(TableLockRegistry::new());
        reg.lock_tables(
            1,
            &[("t".into(), TableLockMode::Write)],
            Duration::from_secs(1),
        )
        .unwrap();
        let reg2 = reg.clone();
        let h = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            reg2.unlock_tables(1);
        });
        reg.wait_for_access(2, "t", true, Duration::from_secs(2))
            .unwrap();
        h.join().unwrap();
    }
}
