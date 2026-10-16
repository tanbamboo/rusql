//! InnoDB-style row exclusive locks for `SELECT … FOR UPDATE` and conflicting DML (M157, M160).

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

/// Wait-for graph cycle detected (MySQL errno 1213).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowLockDeadlock;

/// Result of attempting to acquire a row exclusive lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowLockAcquireError {
    Timeout(RowLockWaitTimeout),
    Deadlock(RowLockDeadlock),
}

#[derive(Debug, Default)]
struct Inner {
    holders: HashMap<RowKey, u64>,
    by_session: HashMap<u64, HashSet<RowKey>>,
    /// Session blocked in `acquire_exclusive`, waiting for this key.
    waiting_for: HashMap<u64, RowKey>,
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
    ///
    /// On a wait-for cycle, the **younger** transaction (higher `session_id` in the cycle) is
    /// aborted with [`RowLockDeadlock`]; other waiters keep waiting.
    pub fn acquire_exclusive(
        &self,
        session_id: u64,
        key: &RowKey,
        timeout: Duration,
    ) -> Result<(), RowLockAcquireError> {
        let deadline = Instant::now() + timeout;
        loop {
            let mut guard = self.inner.lock().expect("row lock registry");
            match guard.holders.get(key) {
                Some(&holder) if holder == session_id => {
                    guard.waiting_for.remove(&session_id);
                    return Ok(());
                }
                Some(&holder) => {
                    guard.waiting_for.insert(session_id, key.clone());
                    if let Some(victim) = deadlock_victim(&guard, session_id, holder) {
                        guard.waiting_for.remove(&session_id);
                        if session_id == victim {
                            return Err(RowLockAcquireError::Deadlock(RowLockDeadlock));
                        }
                    }
                }
                None => {
                    guard.waiting_for.remove(&session_id);
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
                let mut guard = self.inner.lock().expect("row lock registry");
                guard.waiting_for.remove(&session_id);
                return Err(RowLockAcquireError::Timeout(RowLockWaitTimeout));
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
        guard.waiting_for.remove(&session_id);
        if let Some(keys) = guard.by_session.remove(&session_id) {
            for key in keys {
                if guard.holders.get(&key) == Some(&session_id) {
                    guard.holders.remove(&key);
                }
            }
        }
    }
}

/// If `waiter` waiting on `holder` completes a cycle, return the victim session id (max id in cycle).
fn deadlock_victim(inner: &Inner, waiter: u64, holder: u64) -> Option<u64> {
    if !waits_for_session(inner, holder, waiter) {
        return None;
    }
    let mut cycle = vec![waiter, holder];
    let mut current = holder;
    while current != waiter {
        let key = inner.waiting_for.get(&current)?;
        let next = *inner.holders.get(key)?;
        if next == waiter {
            break;
        }
        if cycle.contains(&next) {
            break;
        }
        cycle.push(next);
        current = next;
    }
    cycle.into_iter().max()
}

/// Whether `start` is waiting (directly or transitively) for a lock held by `target`.
fn waits_for_session(inner: &Inner, start: u64, target: u64) -> bool {
    let mut current = start;
    let mut seen = HashSet::new();
    while seen.insert(current) {
        if current == target {
            return true;
        }
        let Some(key) = inner.waiting_for.get(&current) else {
            return false;
        };
        let Some(&holder) = inner.holders.get(key) else {
            return false;
        };
        current = holder;
    }
    false
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
            Err(RowLockAcquireError::Timeout(RowLockWaitTimeout))
        );
        reg.release_all(1);
    }

    #[test]
    fn deadlock_opposite_row_order() {
        let reg = Arc::new(RowLockRegistry::new());
        let k1 = RowKey::new("rusql.dl_t", vec!["1".into()]);
        let k2 = RowKey::new("rusql.dl_t", vec!["2".into()]);
        reg.acquire_exclusive(1, &k1, Duration::from_secs(5))
            .expect("session 1 locks row 1");
        reg.acquire_exclusive(2, &k2, Duration::from_secs(5))
            .expect("session 2 locks row 2");

        let reg_a = reg.clone();
        let k2a = k2.clone();
        let t1 = thread::spawn(move || reg_a.acquire_exclusive(1, &k2a, Duration::from_secs(5)));

        thread::sleep(Duration::from_millis(20));

        let reg_b = reg.clone();
        let k1b = k1.clone();
        let t2 = thread::spawn(move || reg_b.acquire_exclusive(2, &k1b, Duration::from_secs(5)));

        let r2 = t2.join().expect("thread 2");
        assert_eq!(
            r2,
            Err(RowLockAcquireError::Deadlock(RowLockDeadlock)),
            "younger session (2) should be deadlock victim"
        );
        reg.release_all(2);
        let r1 = t1.join().expect("thread 1");
        assert!(
            r1.is_ok(),
            "older session should acquire after victim releases locks"
        );

        reg.release_all(1);
        reg.release_all(2);
    }
}
