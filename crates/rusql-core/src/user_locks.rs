//! Session-scoped named advisory locks (`GET_LOCK` / `RELEASE_LOCK`).

use std::collections::{HashMap, HashSet};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// MySQL user-lock name limit (`ER_USER_LOCK_WRONG_NAME`, errno 1470).
pub const USER_LOCK_NAME_MAX_BYTES: usize = 64;

/// How long `GET_LOCK(name, timeout)` may block when another session holds `name`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserLockWait {
    /// `timeout = 0`: return immediately with SQL `0` if held elsewhere.
    NonBlocking,
    /// `timeout > 0`: wait up to this duration (MySQL uses whole seconds).
    Timed(Duration),
    /// `timeout < 0`: wait until the lock is free or this session already holds it.
    Forever,
}

/// Result of `GET_LOCK(name, timeout)` against the registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GetLockResult {
    /// Lock acquired (or already held by this session). SQL `1`.
    Acquired,
    /// Name is held by another session until timeout (or `timeout = 0`). SQL `0`.
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
pub struct UserLockRegistry {
    inner: Mutex<Inner>,
    cvar: Condvar,
}

impl Default for UserLockRegistry {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            cvar: Condvar::new(),
        }
    }
}

impl std::fmt::Debug for UserLockRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserLockRegistry").finish_non_exhaustive()
    }
}

impl UserLockRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    fn name_error(name: &str) -> bool {
        name.len() > USER_LOCK_NAME_MAX_BYTES
    }

    /// Try to acquire `name` for `session_id`, waiting per [`UserLockWait`].
    ///
    /// Same-session re-acquire returns [`GetLockResult::Acquired`] without nesting.
    pub fn get_lock(&self, session_id: u64, name: &str, wait: UserLockWait) -> GetLockResult {
        if Self::name_error(name) {
            return GetLockResult::NameTooLong;
        }
        let deadline = match wait {
            UserLockWait::NonBlocking => None,
            UserLockWait::Timed(d) => Some(Instant::now() + d),
            UserLockWait::Forever => None,
        };

        loop {
            let mut guard = self.inner.lock().expect("user lock registry");
            if let Some(&holder) = guard.holders.get(name) {
                if holder == session_id {
                    return GetLockResult::Acquired;
                }
                match wait {
                    UserLockWait::NonBlocking => return GetLockResult::Timeout,
                    UserLockWait::Forever => {
                        guard = self.cvar.wait(guard).unwrap_or_else(|e| e.into_inner());
                    }
                    UserLockWait::Timed(_) => {
                        let Some(deadline) = deadline else {
                            return GetLockResult::Timeout;
                        };
                        if Instant::now() >= deadline {
                            return GetLockResult::Timeout;
                        }
                        let remaining = deadline.saturating_duration_since(Instant::now());
                        let (g, result) = match self.cvar.wait_timeout(guard, remaining) {
                            Ok(pair) => pair,
                            Err(_) => return GetLockResult::Timeout,
                        };
                        guard = g;
                        if result.timed_out() {
                            return GetLockResult::Timeout;
                        }
                    }
                }
                drop(guard);
                continue;
            }
            guard.holders.insert(name.to_string(), session_id);
            guard
                .by_session
                .entry(session_id)
                .or_default()
                .insert(name.to_string());
            self.cvar.notify_all();
            return GetLockResult::Acquired;
        }
    }

    /// Release `name` if this session holds it.
    pub fn release_lock(&self, session_id: u64, name: &str) -> ReleaseLockResult {
        if Self::name_error(name) {
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
                self.cvar.notify_all();
                ReleaseLockResult::Released
            }
        }
    }

    /// `IS_FREE_LOCK(name)`: `1` free, `0` in use. `None` when the name is invalid (SQL NULL).
    pub fn is_free_lock(&self, name: &str) -> Option<bool> {
        if Self::name_error(name) {
            return None;
        }
        let guard = self.inner.lock().expect("user lock registry");
        Some(!guard.holders.contains_key(name))
    }

    /// `IS_USED_LOCK(name)`: holder connection id, or `None` when free / invalid name.
    pub fn is_used_lock(&self, name: &str) -> Option<u64> {
        if Self::name_error(name) {
            return None;
        }
        let guard = self.inner.lock().expect("user lock registry");
        guard.holders.get(name).copied()
    }

    /// Release every name held by `session_id` (disconnect / reset / change-user).
    pub fn release_all(&self, session_id: u64) {
        let mut guard = self.inner.lock().expect("user lock registry");
        if let Some(names) = guard.by_session.remove(&session_id) {
            for name in names {
                guard.holders.remove(&name);
            }
            self.cvar.notify_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn get_lock_acquire_and_contention() {
        let reg = UserLockRegistry::new();
        assert_eq!(
            reg.get_lock(1, "gap_lock", UserLockWait::NonBlocking),
            GetLockResult::Acquired
        );
        assert_eq!(
            reg.get_lock(1, "gap_lock", UserLockWait::NonBlocking),
            GetLockResult::Acquired
        );
        assert_eq!(
            reg.get_lock(2, "gap_lock", UserLockWait::NonBlocking),
            GetLockResult::Timeout
        );
        assert_eq!(
            reg.release_lock(2, "gap_lock"),
            ReleaseLockResult::NotHolder
        );
        assert_eq!(reg.release_lock(1, "gap_lock"), ReleaseLockResult::Released);
        assert_eq!(
            reg.release_lock(1, "gap_lock"),
            ReleaseLockResult::NotExists
        );
        assert_eq!(
            reg.get_lock(2, "gap_lock", UserLockWait::NonBlocking),
            GetLockResult::Acquired
        );
    }

    #[test]
    fn release_all_frees_names() {
        let reg = UserLockRegistry::new();
        assert_eq!(
            reg.get_lock(1, "a", UserLockWait::NonBlocking),
            GetLockResult::Acquired
        );
        assert_eq!(
            reg.get_lock(1, "b", UserLockWait::NonBlocking),
            GetLockResult::Acquired
        );
        reg.release_all(1);
        assert_eq!(
            reg.get_lock(2, "a", UserLockWait::NonBlocking),
            GetLockResult::Acquired
        );
        assert_eq!(
            reg.get_lock(2, "b", UserLockWait::NonBlocking),
            GetLockResult::Acquired
        );
    }

    #[test]
    fn name_longer_than_64_bytes_is_rejected() {
        let reg = UserLockRegistry::new();
        let long = "x".repeat(65);
        assert_eq!(
            reg.get_lock(1, &long, UserLockWait::NonBlocking),
            GetLockResult::NameTooLong
        );
        assert_eq!(reg.release_lock(1, &long), ReleaseLockResult::NameTooLong);
        assert!(reg.is_free_lock(&long).is_none());
        assert!(reg.is_used_lock(&long).is_none());
    }

    #[test]
    fn get_lock_timeout_wait_acquires_when_released() {
        let reg = Arc::new(UserLockRegistry::new());
        assert_eq!(
            reg.get_lock(1, "wait_me", UserLockWait::NonBlocking),
            GetLockResult::Acquired
        );
        let waiter = {
            let reg = reg.clone();
            thread::spawn(move || {
                reg.get_lock(2, "wait_me", UserLockWait::Timed(Duration::from_secs(5)))
            })
        };
        thread::sleep(Duration::from_millis(50));
        assert_eq!(reg.release_lock(1, "wait_me"), ReleaseLockResult::Released);
        assert_eq!(waiter.join().unwrap(), GetLockResult::Acquired);
    }

    #[test]
    fn get_lock_timeout_returns_zero_after_deadline() {
        let reg = UserLockRegistry::new();
        assert_eq!(
            reg.get_lock(1, "held", UserLockWait::NonBlocking),
            GetLockResult::Acquired
        );
        let start = Instant::now();
        assert_eq!(
            reg.get_lock(2, "held", UserLockWait::Timed(Duration::from_secs(1))),
            GetLockResult::Timeout
        );
        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_millis(900),
            "expected ~1s wait, got {elapsed:?}"
        );
    }

    #[test]
    fn is_free_and_used_lock() {
        let reg = UserLockRegistry::new();
        assert_eq!(reg.is_free_lock("k"), Some(true));
        assert_eq!(reg.is_used_lock("k"), None);
        assert_eq!(
            reg.get_lock(42, "k", UserLockWait::NonBlocking),
            GetLockResult::Acquired
        );
        assert_eq!(reg.is_free_lock("k"), Some(false));
        assert_eq!(reg.is_used_lock("k"), Some(42));
        reg.release_all(42);
        assert_eq!(reg.is_free_lock("k"), Some(true));
        assert_eq!(reg.is_used_lock("k"), None);
    }
}
