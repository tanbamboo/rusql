//! Session-scoped named advisory locks (`GET_LOCK` / `RELEASE_LOCK`).

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

/// MySQL user-lock name limit (`ER_USER_LOCK_WRONG_NAME`, errno 1470).
pub const USER_LOCK_NAME_MAX_BYTES: usize = 64;

/// Result of `GET_LOCK(name, timeout)` against the registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GetLockResult {
    /// Lock acquired (or already held by this session). SQL `1`.
    Acquired,
    /// Name is held by another session. SQL `0` (timeout / would-block).
    Timeout,
    /// Name longer than 64 bytes.
    NameTooLong,
}

/// Result of `RELEASE_LOCK(name)` against the registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseLockResult {
    /// This session held the name and it is now free. SQL `1`.
    Released,
    /// Another session holds the name. SQL `0`.
    NotHolder,
    /// Nobody holds the name. SQL NULL.
    NotExists,
    /// Name longer than 64 bytes.
    NameTooLong,
}

#[derive(Debug, Default)]
struct Inner {
    holders: HashMap<String, u64>,
    by_session: HashMap<u64, HashSet<String>>,
}

/// Process-wide named user-lock table shared by connections.
#[derive(Debug, Default)]
pub struct UserLockRegistry {
    inner: Mutex<Inner>,
}

impl UserLockRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Try to acquire `name` for `session_id` without waiting.
    ///
    /// Same-session re-acquire returns [`GetLockResult::Acquired`] without nesting.
    /// Timeout wait (`timeout > 0`) is M164; callers treat a foreign holder as timeout.
    pub fn get_lock(&self, session_id: u64, name: &str) -> GetLockResult {
        if name.len() > USER_LOCK_NAME_MAX_BYTES {
            return GetLockResult::NameTooLong;
        }
        let mut guard = self.inner.lock().expect("user lock registry");
        if let Some(&holder) = guard.holders.get(name) {
            if holder == session_id {
                return GetLockResult::Acquired;
            }
            return GetLockResult::Timeout;
        }
        guard.holders.insert(name.to_string(), session_id);
        guard
            .by_session
            .entry(session_id)
            .or_default()
            .insert(name.to_string());
        GetLockResult::Acquired
    }

    /// Release `name` if this session holds it.
    pub fn release_lock(&self, session_id: u64, name: &str) -> ReleaseLockResult {
        if name.len() > USER_LOCK_NAME_MAX_BYTES {
            return ReleaseLockResult::NameTooLong;
        }
        let mut guard = self.inner.lock().expect("user lock registry");
        match guard.holders.get(name).copied() {
            None => ReleaseLockResult::NotExists,
            Some(holder) if holder != session_id => ReleaseLockResult::NotHolder,
            Some(_) => {
                guard.holders.remove(name);
                if let Some(held) = guard.by_session.get_mut(&session_id) {
                    held.remove(name);
                    if held.is_empty() {
                        guard.by_session.remove(&session_id);
                    }
                }
                ReleaseLockResult::Released
            }
        }
    }

    /// Release every name held by `session_id` (disconnect / reset / change-user).
    pub fn release_all(&self, session_id: u64) {
        let mut guard = self.inner.lock().expect("user lock registry");
        if let Some(names) = guard.by_session.remove(&session_id) {
            for name in names {
                guard.holders.remove(&name);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_lock_acquire_and_contention() {
        let reg = UserLockRegistry::new();
        assert_eq!(reg.get_lock(1, "gap_lock"), GetLockResult::Acquired);
        assert_eq!(reg.get_lock(1, "gap_lock"), GetLockResult::Acquired);
        assert_eq!(reg.get_lock(2, "gap_lock"), GetLockResult::Timeout);
        assert_eq!(
            reg.release_lock(2, "gap_lock"),
            ReleaseLockResult::NotHolder
        );
        assert_eq!(reg.release_lock(1, "gap_lock"), ReleaseLockResult::Released);
        assert_eq!(
            reg.release_lock(1, "gap_lock"),
            ReleaseLockResult::NotExists
        );
        assert_eq!(reg.get_lock(2, "gap_lock"), GetLockResult::Acquired);
    }

    #[test]
    fn release_all_frees_names() {
        let reg = UserLockRegistry::new();
        assert_eq!(reg.get_lock(1, "a"), GetLockResult::Acquired);
        assert_eq!(reg.get_lock(1, "b"), GetLockResult::Acquired);
        reg.release_all(1);
        assert_eq!(reg.get_lock(2, "a"), GetLockResult::Acquired);
        assert_eq!(reg.get_lock(2, "b"), GetLockResult::Acquired);
    }

    #[test]
    fn name_longer_than_64_bytes_is_rejected() {
        let reg = UserLockRegistry::new();
        let long = "x".repeat(65);
        assert_eq!(reg.get_lock(1, &long), GetLockResult::NameTooLong);
        assert_eq!(reg.release_lock(1, &long), ReleaseLockResult::NameTooLong);
    }
}
