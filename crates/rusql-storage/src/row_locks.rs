//! InnoDB-style row and gap exclusive locks for `SELECT … FOR UPDATE` and conflicting DML (M157, M160, M161).

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::{Condvar, Mutex};
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

/// Gap lock on a single-column primary key interval (M161).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GapLockKey {
    pub table: String,
    pub low: Option<String>,
    pub low_inclusive: bool,
    pub high: Option<String>,
    pub high_inclusive: bool,
}

impl GapLockKey {
    pub fn between(table: impl Into<String>, low: String, high: String) -> Self {
        Self {
            table: table.into(),
            low: Some(low),
            low_inclusive: true,
            high: Some(high),
            high_inclusive: true,
        }
    }

    pub fn contains_pk(&self, pk: &str) -> bool {
        if let Some(ref low) = self.low {
            match pk_cmp(pk, low) {
                Ordering::Less => return false,
                Ordering::Equal if !self.low_inclusive => return false,
                _ => {}
            }
        }
        if let Some(ref high) = self.high {
            match pk_cmp(pk, high) {
                Ordering::Greater => return false,
                Ordering::Equal if !self.high_inclusive => return false,
                _ => {}
            }
        }
        true
    }

    pub fn overlaps(&self, other: &GapLockKey) -> bool {
        if self.table != other.table {
            return false;
        }
        !range_before(self, other) && !range_before(other, self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LockTarget {
    Row(RowKey),
    Gap(GapLockKey),
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
    holders: HashMap<LockTarget, u64>,
    by_session: HashMap<u64, HashSet<LockTarget>>,
    /// Session blocked in acquire, waiting for this target.
    waiting_for: HashMap<u64, LockTarget>,
}

/// Process-wide row and gap lock table shared by connections.
pub struct RowLockRegistry {
    inner: Mutex<Inner>,
    cvar: Condvar,
}

impl Default for RowLockRegistry {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            cvar: Condvar::new(),
        }
    }
}

impl std::fmt::Debug for RowLockRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RowLockRegistry").finish_non_exhaustive()
    }
}

impl RowLockRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Acquire an exclusive lock on `target` for `session_id`, waiting up to `timeout`.
    pub fn acquire_exclusive(
        &self,
        session_id: u64,
        target: &LockTarget,
        timeout: Duration,
    ) -> Result<(), RowLockAcquireError> {
        let deadline = Instant::now() + timeout;
        loop {
            let mut guard = self.inner.lock().expect("row lock registry");
            match guard.holders.get(target) {
                Some(&holder) if holder == session_id => {
                    guard.waiting_for.remove(&session_id);
                    return Ok(());
                }
                Some(&holder) => {
                    guard.waiting_for.insert(session_id, target.clone());
                    if let Some(victim) = deadlock_victim(&guard, session_id, holder) {
                        guard.waiting_for.remove(&session_id);
                        if session_id == victim {
                            return Err(RowLockAcquireError::Deadlock(RowLockDeadlock));
                        }
                    }
                }
                None => {
                    if let LockTarget::Gap(gap) = target {
                        if gap_conflict_with_other(&guard, session_id, gap) {
                            if let Some((holder, wait_target)) =
                                first_gap_conflict(&guard, session_id, gap)
                            {
                                guard.waiting_for.insert(session_id, wait_target);
                                if let Some(victim) = deadlock_victim(&guard, session_id, holder) {
                                    guard.waiting_for.remove(&session_id);
                                    if session_id == victim {
                                        return Err(RowLockAcquireError::Deadlock(RowLockDeadlock));
                                    }
                                }
                            }
                            drop(guard);
                            if self.wait_until(deadline) {
                                let mut guard = self.inner.lock().expect("row lock registry");
                                guard.waiting_for.remove(&session_id);
                                return Err(RowLockAcquireError::Timeout(RowLockWaitTimeout));
                            }
                            continue;
                        }
                    }
                    guard.waiting_for.remove(&session_id);
                    guard.holders.insert(target.clone(), session_id);
                    guard
                        .by_session
                        .entry(session_id)
                        .or_default()
                        .insert(target.clone());
                    return Ok(());
                }
            }
            drop(guard);
            if self.wait_until(deadline) {
                let mut guard = self.inner.lock().expect("row lock registry");
                guard.waiting_for.remove(&session_id);
                return Err(RowLockAcquireError::Timeout(RowLockWaitTimeout));
            }
        }
    }

    /// Wait until `pk` may be inserted: no other session's gap covers `pk`, then take row lock.
    pub fn acquire_insert_pk(
        &self,
        session_id: u64,
        table: &str,
        pk: &str,
        timeout: Duration,
    ) -> Result<(), RowLockAcquireError> {
        let row = LockTarget::Row(RowKey::new(table, vec![pk.into()]));
        let deadline = Instant::now() + timeout;
        loop {
            let mut guard = self.inner.lock().expect("row lock registry");
            if let Some((holder, wait_target)) = insert_gap_blocker(&guard, session_id, table, pk) {
                guard.waiting_for.insert(session_id, wait_target);
                if let Some(victim) = deadlock_victim(&guard, session_id, holder) {
                    guard.waiting_for.remove(&session_id);
                    if session_id == victim {
                        return Err(RowLockAcquireError::Deadlock(RowLockDeadlock));
                    }
                }
                drop(guard);
                if self.wait_until(deadline) {
                    let mut guard = self.inner.lock().expect("row lock registry");
                    guard.waiting_for.remove(&session_id);
                    return Err(RowLockAcquireError::Timeout(RowLockWaitTimeout));
                }
                continue;
            }
            match guard.holders.get(&row) {
                Some(&holder) if holder == session_id => {
                    guard.waiting_for.remove(&session_id);
                    return Ok(());
                }
                Some(&holder) => {
                    guard.waiting_for.insert(session_id, row.clone());
                    if let Some(victim) = deadlock_victim(&guard, session_id, holder) {
                        guard.waiting_for.remove(&session_id);
                        if session_id == victim {
                            return Err(RowLockAcquireError::Deadlock(RowLockDeadlock));
                        }
                    }
                }
                None => {
                    guard.waiting_for.remove(&session_id);
                    guard.holders.insert(row.clone(), session_id);
                    guard.by_session.entry(session_id).or_default().insert(row);
                    return Ok(());
                }
            }
            drop(guard);
            if self.wait_until(deadline) {
                let mut guard = self.inner.lock().expect("row lock registry");
                guard.waiting_for.remove(&session_id);
                return Err(RowLockAcquireError::Timeout(RowLockWaitTimeout));
            }
        }
    }

    /// Whether `key` is exclusively held by `session_id`.
    pub fn is_held_by(&self, session_id: u64, key: &RowKey) -> bool {
        let guard = self.inner.lock().expect("row lock registry");
        guard.holders.get(&LockTarget::Row(key.clone())) == Some(&session_id)
    }

    /// Whether `gap` is exclusively held by `session_id`.
    pub fn is_gap_held_by(&self, session_id: u64, gap: &GapLockKey) -> bool {
        let guard = self.inner.lock().expect("row lock registry");
        guard.holders.get(&LockTarget::Gap(gap.clone())) == Some(&session_id)
    }

    /// Release every row and gap lock held by `session_id` (commit / rollback / disconnect).
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
        drop(guard);
        self.cvar.notify_all();
    }

    /// Wait until `deadline`; returns true if timed out.
    fn wait_until(&self, deadline: Instant) -> bool {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return true;
        }
        let guard = self.inner.lock().expect("row lock registry");
        let (_guard, wait_result) = self
            .cvar
            .wait_timeout(guard, remaining)
            .expect("row lock registry");
        wait_result.timed_out()
    }
}

fn pk_cmp(a: &str, b: &str) -> Ordering {
    match (a.parse::<i64>(), b.parse::<i64>()) {
        (Ok(x), Ok(y)) => x.cmp(&y),
        _ => a.cmp(b),
    }
}

fn range_before(a: &GapLockKey, b: &GapLockKey) -> bool {
    let a_high = a.high.as_deref();
    let b_low = b.low.as_deref();
    match (a_high, b_low) {
        (Some(h), Some(l)) => match pk_cmp(h, l) {
            Ordering::Less => true,
            Ordering::Equal => !(a.high_inclusive && b.low_inclusive),
            Ordering::Greater => false,
        },
        (Some(_), None) => false,
        (None, Some(_)) => true,
        (None, None) => false,
    }
}

fn gap_conflict_with_other(inner: &Inner, session_id: u64, gap: &GapLockKey) -> bool {
    inner.holders.iter().any(|(target, &holder)| {
        holder != session_id
            && matches!(target, LockTarget::Gap(existing) if existing.overlaps(gap))
    })
}

fn first_gap_conflict(
    inner: &Inner,
    session_id: u64,
    gap: &GapLockKey,
) -> Option<(u64, LockTarget)> {
    inner
        .holders
        .iter()
        .find(|(target, &holder)| {
            holder != session_id
                && matches!(target, LockTarget::Gap(existing) if existing.overlaps(gap))
        })
        .map(|(target, &holder)| (holder, target.clone()))
}

fn insert_gap_blocker(
    inner: &Inner,
    session_id: u64,
    table: &str,
    pk: &str,
) -> Option<(u64, LockTarget)> {
    inner
        .holders
        .iter()
        .find(|(target, &holder)| {
            if holder == session_id {
                return false;
            }
            match target {
                LockTarget::Gap(gap) => gap.table == table && gap.contains_pk(pk),
                LockTarget::Row(_) => false,
            }
        })
        .map(|(target, &holder)| (holder, target.clone()))
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

    fn row_target(table: &str, pk: &str) -> LockTarget {
        LockTarget::Row(RowKey::new(table, vec![pk.into()]))
    }

    fn gap_target(table: &str, low: &str, high: &str) -> LockTarget {
        LockTarget::Gap(GapLockKey::between(table, low.into(), high.into()))
    }

    #[test]
    fn for_update_wait_blocks_until_release() {
        let reg = Arc::new(RowLockRegistry::new());
        let key = row_target("rusql.fu_t", "1");
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
        let key = row_target("rusql.fu_t", "1");
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
        let k1 = row_target("rusql.dl_t", "1");
        let k2 = row_target("rusql.dl_t", "2");
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

    #[test]
    fn gap_lock_blocks_insert_in_range() {
        let reg = Arc::new(RowLockRegistry::new());
        let gap = gap_target("gap_t", "10", "20");
        reg.acquire_exclusive(1, &gap, Duration::from_secs(5))
            .expect("holder gap");

        let reg2 = reg.clone();
        let inserter =
            thread::spawn(move || reg2.acquire_insert_pk(2, "gap_t", "15", Duration::from_secs(5)));
        thread::sleep(Duration::from_millis(40));
        assert!(
            !inserter.is_finished(),
            "insert into locked gap should block"
        );
        reg.release_all(1);
        inserter.join().expect("join").expect("insert proceeds");
    }

    #[test]
    fn gap_lock_insert_outside_range_proceeds() {
        let reg = RowLockRegistry::new();
        let gap = gap_target("gap_t", "10", "20");
        reg.acquire_exclusive(1, &gap, Duration::from_secs(5))
            .expect("holder gap");
        reg.acquire_insert_pk(2, "gap_t", "25", Duration::from_millis(50))
            .expect("insert outside gap");
        reg.release_all(1);
        reg.release_all(2);
    }
}
