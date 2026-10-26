//! Password-policy decisions for M183.
//!
//! These checks are built in. They are not a `validate_password` component `.so`.
//! Credential hashing stays in `caching_sha2` / `mysql_native_password`.

/// `ER_NOT_VALID_PASSWORD`.
pub const ER_NOT_VALID_PASSWORD: u16 = 1819;
/// `ER_MUST_CHANGE_PASSWORD`.
pub const ER_MUST_CHANGE_PASSWORD: u16 = 1820;
/// `ER_CREDENTIALS_CONTRADICT_TO_HISTORY`.
pub const ER_CREDENTIALS_CONTRADICT_TO_HISTORY: u16 = 3638;

const DAY_SECS: u64 = 86_400;

/// Built-in `validate_password.policy` subset. `Off` matches a server with the
/// component not installed (no complexity check).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidatePasswordLevel {
    Off,
    Low,
    Medium,
    Strong,
}

/// Length plus policy level. `length == 0` means the length check is disabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidatePasswordSettings {
    pub level: ValidatePasswordLevel,
    pub length: u32,
}

impl Default for ValidatePasswordSettings {
    fn default() -> Self {
        Self {
            level: ValidatePasswordLevel::Off,
            length: 8,
        }
    }
}

/// One retained password (current or a previous value) for reuse checks.
#[derive(Debug, Clone, Copy)]
pub struct HistorySample<'a> {
    pub password: &'a str,
    pub changed_unix: u64,
}

/// `true` when the password meets the documented subset.
///
/// `LOW`: length. `MEDIUM`: length plus one ASCII lower, upper, digit, and a
/// non-alphanumeric character. `STRONG`: `MEDIUM` plus the password must not
/// contain the account name when that name is at least 3 characters. There is
/// no dictionary file.
pub fn password_satisfies_policy(
    password: &str,
    user: &str,
    settings: ValidatePasswordSettings,
) -> bool {
    if settings.level == ValidatePasswordLevel::Off {
        return true;
    }
    if password.chars().count() < settings.length as usize {
        return false;
    }
    if matches!(
        settings.level,
        ValidatePasswordLevel::Medium | ValidatePasswordLevel::Strong
    ) && !meets_medium_classes(password)
    {
        return false;
    }
    if settings.level == ValidatePasswordLevel::Strong && contains_account_name(password, user) {
        return false;
    }
    true
}

fn meets_medium_classes(password: &str) -> bool {
    let has_lower = password.chars().any(|c| c.is_ascii_lowercase());
    let has_upper = password.chars().any(|c| c.is_ascii_uppercase());
    let has_digit = password.chars().any(|c| c.is_ascii_digit());
    let has_special = password.chars().any(|c| !c.is_ascii_alphanumeric());
    has_lower && has_upper && has_digit && has_special
}

fn contains_account_name(password: &str, user: &str) -> bool {
    let user = user.trim();
    if user.chars().count() < 3 {
        return false;
    }
    password
        .to_ascii_lowercase()
        .contains(&user.to_ascii_lowercase())
}

/// Expired accounts may still authenticate. `COM_QUERY` is errno 1820 until the
/// account sets a new password.
pub fn password_expire_blocks_statement(must_change: bool, is_self_password_change: bool) -> bool {
    must_change && !is_self_password_change
}

/// `true` when `now_unix` is at least `lifetime_days` after `changed_unix`.
/// A zero lifetime or a zero change time does not expire.
pub fn password_lifetime_elapsed(changed_unix: u64, lifetime_days: u32, now_unix: u64) -> bool {
    if lifetime_days == 0 || changed_unix == 0 {
        return false;
    }
    now_unix.saturating_sub(changed_unix) >= u64::from(lifetime_days) * DAY_SECS
}

/// `true` when `new_password` repeats the current password or a retained one
/// inside the history count or the reuse interval. Both limits `0` disables the check.
///
/// `previous` is newest-first and does not include `current_password`.
pub fn password_contradicts_history(
    new_password: &str,
    current_password: &str,
    current_changed_unix: u64,
    previous: &[HistorySample<'_>],
    history_count: u32,
    reuse_interval_days: u32,
    now_unix: u64,
) -> bool {
    if history_count == 0 && reuse_interval_days == 0 {
        return false;
    }
    let mut index = 0u32;
    if !current_password.is_empty() {
        if contradicts_one(
            new_password,
            current_password,
            current_changed_unix,
            index,
            history_count,
            reuse_interval_days,
            now_unix,
        ) {
            return true;
        }
        index += 1;
    }
    for sample in previous {
        if contradicts_one(
            new_password,
            sample.password,
            sample.changed_unix,
            index,
            history_count,
            reuse_interval_days,
            now_unix,
        ) {
            return true;
        }
        index += 1;
    }
    false
}

fn contradicts_one(
    new_password: &str,
    old: &str,
    changed_unix: u64,
    index: u32,
    history_count: u32,
    reuse_interval_days: u32,
    now_unix: u64,
) -> bool {
    if new_password != old {
        return false;
    }
    let in_count = history_count > 0 && index < history_count;
    let in_interval = reuse_interval_days > 0
        && now_unix.saturating_sub(changed_unix) < u64::from(reuse_interval_days) * DAY_SECS;
    in_count || in_interval
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_expire_blocks_non_change_statement() {
        assert!(password_expire_blocks_statement(true, false));
        assert!(!password_expire_blocks_statement(true, true));
        assert!(!password_expire_blocks_statement(false, false));
        assert_eq!(ER_MUST_CHANGE_PASSWORD, 1820);
    }

    #[test]
    fn password_expire_lifetime_elapsed() {
        assert!(!password_lifetime_elapsed(1_000, 0, 1_000 + DAY_SECS));
        assert!(!password_lifetime_elapsed(0, 1, 1_000 + DAY_SECS));
        assert!(!password_lifetime_elapsed(1_000, 1, 1_000 + DAY_SECS - 1));
        assert!(password_lifetime_elapsed(1_000, 1, 1_000 + DAY_SECS));
    }

    #[test]
    fn password_expire_history_and_validate() {
        let low = ValidatePasswordSettings {
            level: ValidatePasswordLevel::Low,
            length: 8,
        };
        assert!(!password_satisfies_policy("short", "app", low));
        assert!(password_satisfies_policy("longenough", "app", low));
        assert!(password_satisfies_policy(
            "short",
            "app",
            ValidatePasswordSettings::default()
        ));

        let medium = ValidatePasswordSettings {
            level: ValidatePasswordLevel::Medium,
            length: 8,
        };
        assert!(!password_satisfies_policy("longenough", "app", medium));
        assert!(password_satisfies_policy("Longen1!", "app", medium));

        let strong = ValidatePasswordSettings {
            level: ValidatePasswordLevel::Strong,
            length: 8,
        };
        assert!(!password_satisfies_policy("xxAppuser1!", "appuser", strong));
        assert!(password_satisfies_policy("Longen1!", "appuser", strong));
        assert_eq!(ER_NOT_VALID_PASSWORD, 1819);

        let previous = [HistorySample {
            password: "secret2",
            changed_unix: 500,
        }];
        assert!(password_contradicts_history(
            "secret2", "secret3", 2_000, &previous, 2, 0, 3_000,
        ));
        assert!(password_contradicts_history(
            "secret3", "secret3", 2_000, &previous, 1, 0, 3_000,
        ));
        assert!(!password_contradicts_history(
            "secret4", "secret3", 2_000, &previous, 2, 0, 3_000,
        ));
        assert!(!password_contradicts_history(
            "secret3", "secret3", 2_000, &previous, 0, 0, 3_000,
        ));
        let recent = [HistorySample {
            password: "secret2",
            changed_unix: 9_000,
        }];
        assert!(password_contradicts_history(
            "secret2",
            "secret3",
            9_100,
            &recent,
            0,
            1,
            9_000 + DAY_SECS - 1,
        ));
        assert!(!password_contradicts_history(
            "secret2",
            "secret3",
            9_100,
            &recent,
            0,
            1,
            9_000 + DAY_SECS,
        ));
        assert_eq!(ER_CREDENTIALS_CONTRADICT_TO_HISTORY, 3638);
    }
}
