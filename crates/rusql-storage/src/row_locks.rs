//! InnoDB-style row exclusive locks for `SELECT … FOR UPDATE` and conflicting DML (M157).

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

/// Stable row identity: table storage key + primary-key column values in catalog order.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RowKey {
    pub table: String,
    pub pk_values: Vec<String>,
}

impl RowKey {
    pub fn new(table: impl Into<String>, pk_values: Vec<String>) -> Self {
        Self {
            table: table.into(),
            pk_values,
        }
    }
}

/// Lock could not be acquired before `innodb_lock_wait_timeout` (MySQL errno 1205).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowLockWaitTimeout;

#[derive(Debug, Default)]
struct Inner {
    holders: HashMap<RowKey, u64>,
    by_session: HashMap<u64, HashSet<RowKey>>,
}

/// Process-wide row lock table shared by connections.
#[derive(Debug)]
pub struct RowLockRegistry {
    inner: Mutex<Inner>,
}

impl Default for RowLockRegistry {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
        }
    }
}

impl RowLockRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Acquire an exclusive lock on `key` for `session_id`, waiting up to `timeout`.
    ///
    /// Re-acquire by the same session succeeds without nesting.
    pub fn acquire_exclusive(
        &self,
        session_id: u64,
        key: &RowKey,
        timeout: Duration,
    ) -> Result<(), RowLockWaitTimeout> {
        let deadline = Instant::now() + timeout;
        loop {
            let mut guard = self.inner.lock().expect("row lock registry");
            match guard.holders.get(key) {
                Some(&holder) if holder == session_id => return Ok(()),
                Some(_) => {}
                None => {
                    guard.holders.insert(key.clone(), session_id);
                    guard
                        .by_session
                        .entry(session_id)
                        .or_default()
                        .insert(key.clone());
                    return Ok(());
                }
            }
            drop(guard);
            if Instant::now() >= deadline {
                return Err(RowLockWaitTimeout);
            }
            thread::sleep(Duration::from_millis(1));
        }
    }

    /// Whether `key` is exclusively held by `session_id`.
    pub fn is_held_by(&self, session_id: u64, key: &RowKey) -> bool {
        let guard = self.inner.lock().expect("row lock registry");
        guard.holders.get(key) == Some(&session_id)
    }

    /// Release every row lock held by `session_id` (commit / rollback / disconnect).
    pub fn release_all(&self, session_id: u64) {
        let mut guard = self.inner.lock().expect("row lock registry");
        if let Some(keys) = guard.by_session.remove(&session_id) {
            for key in keys {
                if guard.holders.get(&key) == Some(&session_id) {
                    guard.holders.remove(&key);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn for_update_wait_blocks_until_release() {
        let reg = Arc::new(RowLockRegistry::new());
        let key = RowKey::new("rusql.fu_t", vec!["1".into()]);
        reg.acquire_exclusive(1, &key, Duration::from_secs(5))
            .expect("holder acquires");
        let reg2 = reg.clone();
        let key2 = key.clone();
        let waiter =
            thread::spawn(move || reg2.acquire_exclusive(2, &key2, Duration::from_secs(5)));
        thread::sleep(Duration::from_millis(30));
        assert!(!waiter.is_finished(), "second session should block");
        reg.release_all(1);
        waiter
            .join()
            .expect("waiter thread")
            .expect("proceeds after release");
    }

    #[test]
    fn for_update_wait_times_out() {
        let reg = RowLockRegistry::new();
        let key = RowKey::new("rusql.fu_t", vec!["1".into()]);
        reg.acquire_exclusive(1, &key, Duration::from_secs(5))
            .expect("holder acquires");
        assert_eq!(
            reg.acquire_exclusive(2, &key, Duration::from_millis(50)),
            Err(RowLockWaitTimeout)
        );
        reg.release_all(1);
    }
}
