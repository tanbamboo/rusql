//! MySQL-style account privileges (GRANT/REVOKE).

use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

/// Privilege kind (MySQL subset).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Privilege {
    Select,
    Insert,
    Update,
    Delete,
    Create,
    Drop,
    Index,
    Alter,
}

impl Privilege {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Select => "SELECT",
            Self::Insert => "INSERT",
            Self::Update => "UPDATE",
            Self::Delete => "DELETE",
            Self::Create => "CREATE",
            Self::Drop => "DROP",
            Self::Index => "INDEX",
            Self::Alter => "ALTER",
        }
    }

    pub fn parse_name(name: &str) -> Option<Self> {
        match name.to_ascii_uppercase().as_str() {
            "SELECT" => Some(Self::Select),
            "INSERT" => Some(Self::Insert),
            "UPDATE" => Some(Self::Update),
            "DELETE" => Some(Self::Delete),
            "CREATE" => Some(Self::Create),
            "DROP" => Some(Self::Drop),
            "INDEX" => Some(Self::Index),
            "ALTER" => Some(Self::Alter),
            _ => None,
        }
    }
}

/// Target of a grant (`db.*` or `db.table`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GrantTarget {
    pub schema: String,
    /// `None` means all tables in schema (`schema.*`).
    pub table: Option<String>,
}

/// One persisted grant row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantRecord {
    pub user: String,
    pub host: String,
    pub target: GrantTarget,
    pub privileges: BTreeSet<Privilege>,
}

/// Login-account fields for `SELECT mysql.user` (M186).
///
/// Password text and password history are not copied into this view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MysqlUserAccountView {
    pub user: String,
    pub host: String,
    pub auth_plugin: String,
    pub require_ssl: bool,
}

/// Account key (`user`@`host`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Account {
    pub user: String,
    pub host: String,
}

impl Account {
    pub fn new(user: impl Into<String>, host: impl Into<String>) -> Self {
        Self {
            user: user.into(),
            host: host.into(),
        }
    }

    pub fn display(&self) -> String {
        format!("'{}'@'{}'", self.user, self.host)
    }
}

pub const AUTH_PLUGIN_CACHING_SHA2: &str = "caching_sha2_password";
pub const AUTH_PLUGIN_NATIVE: &str = "mysql_native_password";

/// Persisted login account (`user`@`host` + password + plugin).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserAccountRecord {
    pub user: String,
    pub host: String,
    pub auth_plugin: String,
    pub password: String,
    /// `PASSWORD EXPIRE` / must-change. Absent in catalogs written before M183.
    #[serde(default)]
    pub password_expired: bool,
    /// Account lifetime. Absent means the global default.
    #[serde(default)]
    pub password_lifetime: PasswordLifetime,
    /// Unix seconds of the last password change. `0` means unknown (no time expiry).
    #[serde(default)]
    pub password_changed_unix: u64,
    /// Previous passwords, newest first. Same representation as `password`.
    #[serde(default)]
    pub password_history: Vec<StoredPassword>,
    /// `None` follows the global `password_history`.
    #[serde(default)]
    pub password_history_len: Option<u32>,
    /// `None` follows the global `password_reuse_interval` (days).
    #[serde(default)]
    pub password_reuse_interval_days: Option<u32>,
    /// `REQUIRE SSL`. Absent in catalogs written before M184 means plaintext is allowed.
    #[serde(default)]
    pub require_ssl: bool,
}

/// One retained password on the account row (not a separate history table).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredPassword {
    pub password: String,
    pub changed_unix: u64,
}

/// How long an account password stays valid before it must be changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PasswordLifetime {
    #[default]
    Default,
    Never,
    Days(u32),
}

/// Global password policy persisted in `mysql.user.json`.
///
/// `validate_policy = OFF` does not check complexity (MySQL without the
/// `validate_password` component). History and lifetime default to `0` (off).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PasswordPolicyConfig {
    #[serde(default = "default_validate_length")]
    pub validate_length: u32,
    #[serde(default = "default_validate_policy")]
    pub validate_policy: String,
    #[serde(default)]
    pub password_history: u32,
    #[serde(default)]
    pub password_reuse_interval: u32,
    #[serde(default)]
    pub default_password_lifetime: u32,
}

fn default_validate_length() -> u32 {
    8
}

fn default_validate_policy() -> String {
    "OFF".to_string()
}

impl Default for PasswordPolicyConfig {
    fn default() -> Self {
        Self {
            validate_length: default_validate_length(),
            validate_policy: default_validate_policy(),
            password_history: 0,
            password_reuse_interval: 0,
            default_password_lifetime: 0,
        }
    }
}

/// Optional clauses on `CREATE USER` / `ALTER USER`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PasswordOptions {
    pub expire: Option<PasswordExpireMode>,
    /// `Some(None)` is `PASSWORD HISTORY DEFAULT`. `Some(Some(n))` is a count.
    pub history: Option<Option<u32>>,
    /// `Some(None)` is `PASSWORD REUSE INTERVAL DEFAULT`.
    pub reuse_interval_days: Option<Option<u32>>,
    /// `None` omits the clause. `Some(true)` is `REQUIRE SSL`. `Some(false)` is `REQUIRE NONE`.
    pub require_ssl: Option<bool>,
}

/// `PASSWORD EXPIRE` forms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordExpireMode {
    Immediate,
    Never,
    Default,
    Days(u32),
}

/// Account named in `ALTER USER`, or `USER()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountSpec {
    Current,
    Named(Account),
}

/// Password-changing or policy-only `ALTER USER`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlterUserAction {
    Identified {
        /// `None` keeps the account plugin (`IDENTIFIED BY` without `WITH`).
        auth_plugin: Option<String>,
        password: String,
        options: PasswordOptions,
    },
    Options(PasswordOptions),
}

/// `SET GLOBAL` assignment for the M183 policy variables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasswordPolicyAssign {
    ValidateLength(u32),
    ValidatePolicy(String),
    PasswordHistory(u32),
    PasswordReuseInterval(u32),
    DefaultPasswordLifetime(u32),
}

/// Parse result for a policy `SET`. `None` from the parser means "not this statement".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasswordPolicySetParse {
    Assign(PasswordPolicyAssign),
    NeedsGlobal(String),
    BadValue { name: String, value: String },
}

/// One `SELECT @@policy_var` read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasswordPolicyField {
    ValidateLength,
    ValidatePolicy,
    PasswordHistory,
    PasswordReuseInterval,
    DefaultPasswordLifetime,
}

/// `SELECT @@…` of a policy variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordPolicyRead {
    pub column: String,
    pub field: PasswordPolicyField,
}

/// Fields the server needs to enforce policy without cloning the whole catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordStatus {
    pub current: String,
    pub auth_plugin: String,
    pub changed_unix: u64,
    pub expired: bool,
    pub lifetime_days: u32,
    pub history: Vec<StoredPassword>,
    pub history_limit: u32,
    pub reuse_interval_days: u32,
}

const PASSWORD_HISTORY_CAP: usize = 64;

/// Named privilege bundle (`CREATE ROLE`). Not a login account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleRecord {
    pub user: String,
    pub host: String,
}

/// `GRANT role TO grantee` edge. Grantee is a user account or another role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleEdge {
    pub role_user: String,
    pub role_host: String,
    pub grantee_user: String,
    pub grantee_host: String,
}

/// Role catalog failures. Callers map these to MySQL errnos and i18n text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivilegeError {
    AlreadyExists {
        operation: &'static str,
        account: Account,
    },
    DoesNotExist {
        operation: &'static str,
        account: Account,
    },
    UnknownAuthId(Account),
    RoleNotGranted {
        role: Account,
        grantee: Account,
    },
    AnonymousGrantee,
}

/// Parsed account DDL (CREATE/DROP USER).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountDdl {
    CreateUser {
        accounts: Vec<Account>,
        auth_plugin: String,
        password: String,
        if_not_exists: bool,
        options: PasswordOptions,
    },
    DropUser {
        accounts: Vec<Account>,
        if_exists: bool,
    },
    AlterUser {
        accounts: Vec<AccountSpec>,
        action: AlterUserAction,
    },
    SetPassword {
        /// `None` is the current account (`SET PASSWORD = …`).
        account: Option<Account>,
        password: String,
    },
}

/// In-memory privilege catalog persisted under the server data directory.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PrivilegeStore {
    grants: Vec<GrantRecord>,
    #[serde(default)]
    accounts: Vec<UserAccountRecord>,
    /// `CREATE ROLE` rows. Absent in catalogs written before M182.
    #[serde(default)]
    roles: Vec<RoleRecord>,
    /// Role graph. Absent in catalogs written before M182.
    #[serde(default)]
    role_edges: Vec<RoleEdge>,
    /// Password policy. Absent in catalogs written before M183.
    #[serde(default)]
    password_policy: PasswordPolicyConfig,
}

impl PrivilegeStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn grants_file(data_dir: &Path) -> PathBuf {
        data_dir.join("mysql.user.json")
    }

    pub fn load(data_dir: &Path) -> std::io::Result<Self> {
        let path = Self::grants_file(data_dir);
        if !path.exists() {
            return Ok(Self::default());
        }
        let bytes = std::fs::read(&path)?;
        serde_json::from_slice(&bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    pub fn save(&self, data_dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(data_dir)?;
        let path = Self::grants_file(data_dir);
        let json = serde_json::to_vec_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }

    pub fn is_superuser(user: &str) -> bool {
        user.eq_ignore_ascii_case("root")
    }

    pub fn has_accounts(&self) -> bool {
        !self.accounts.is_empty()
    }

    pub fn ensure_account(
        &mut self,
        account: &Account,
        password: impl Into<String>,
        auth_plugin: impl Into<String>,
    ) {
        let password = password.into();
        let auth_plugin = auth_plugin.into();
        if let Some(record) = self
            .accounts
            .iter_mut()
            .find(|a| a.user.eq_ignore_ascii_case(&account.user) && a.host == account.host)
        {
            record.password = password;
            record.auth_plugin = auth_plugin;
            return;
        }
        self.accounts.push(UserAccountRecord {
            user: account.user.clone(),
            host: account.host.clone(),
            auth_plugin,
            password,
            password_expired: false,
            password_lifetime: PasswordLifetime::Default,
            password_changed_unix: 0,
            password_history: Vec::new(),
            password_history_len: None,
            password_reuse_interval_days: None,
            require_ssl: false,
        });
    }

    pub fn create_user(
        &mut self,
        account: &Account,
        password: impl Into<String>,
        auth_plugin: impl Into<String>,
        if_not_exists: bool,
    ) -> Result<(), String> {
        self.create_user_with_options(
            account,
            password,
            auth_plugin,
            if_not_exists,
            &PasswordOptions::default(),
            unix_now(),
        )
    }

    pub fn create_user_with_options(
        &mut self,
        account: &Account,
        password: impl Into<String>,
        auth_plugin: impl Into<String>,
        if_not_exists: bool,
        options: &PasswordOptions,
        now: u64,
    ) -> Result<(), String> {
        if self
            .find_auth_account(&account.user, &account.host)
            .is_some()
            || self.is_role(account)
        {
            if if_not_exists {
                return Ok(());
            }
            return Err(format!(
                "Operation CREATE USER failed for {} as it already exists",
                account.display()
            ));
        }
        let mut record = UserAccountRecord {
            user: account.user.clone(),
            host: account.host.clone(),
            auth_plugin: auth_plugin.into(),
            password: password.into(),
            password_expired: false,
            password_lifetime: PasswordLifetime::Default,
            password_changed_unix: now,
            password_history: Vec::new(),
            password_history_len: None,
            password_reuse_interval_days: None,
            require_ssl: false,
        };
        apply_password_options(&mut record, options);
        self.accounts.push(record);
        Ok(())
    }

    pub fn password_policy(&self) -> &PasswordPolicyConfig {
        &self.password_policy
    }

    pub fn apply_password_policy_assign(&mut self, assign: PasswordPolicyAssign) {
        match assign {
            PasswordPolicyAssign::ValidateLength(n) => self.password_policy.validate_length = n,
            PasswordPolicyAssign::ValidatePolicy(policy) => {
                self.password_policy.validate_policy = policy;
            }
            PasswordPolicyAssign::PasswordHistory(n) => self.password_policy.password_history = n,
            PasswordPolicyAssign::PasswordReuseInterval(n) => {
                self.password_policy.password_reuse_interval = n;
            }
            PasswordPolicyAssign::DefaultPasswordLifetime(n) => {
                self.password_policy.default_password_lifetime = n;
            }
        }
    }

    pub fn password_policy_text(&self, field: &PasswordPolicyField) -> String {
        match field {
            PasswordPolicyField::ValidateLength => self.password_policy.validate_length.to_string(),
            PasswordPolicyField::ValidatePolicy => self.password_policy.validate_policy.clone(),
            PasswordPolicyField::PasswordHistory => {
                self.password_policy.password_history.to_string()
            }
            PasswordPolicyField::PasswordReuseInterval => {
                self.password_policy.password_reuse_interval.to_string()
            }
            PasswordPolicyField::DefaultPasswordLifetime => {
                self.password_policy.default_password_lifetime.to_string()
            }
        }
    }

    /// Exact account lookup used by password changes (not client-host matching).
    pub fn password_status(&self, user: &str, host: &str) -> Option<PasswordStatus> {
        let record = self.get_account(user, host)?;
        Some(self.status_of(record))
    }

    fn status_of(&self, record: &UserAccountRecord) -> PasswordStatus {
        let lifetime_days = match record.password_lifetime {
            PasswordLifetime::Never => 0,
            PasswordLifetime::Default => self.password_policy.default_password_lifetime,
            PasswordLifetime::Days(n) => n,
        };
        PasswordStatus {
            current: record.password.clone(),
            auth_plugin: record.auth_plugin.clone(),
            changed_unix: record.password_changed_unix,
            expired: record.password_expired,
            lifetime_days,
            history: record.password_history.clone(),
            history_limit: record
                .password_history_len
                .unwrap_or(self.password_policy.password_history),
            reuse_interval_days: record
                .password_reuse_interval_days
                .unwrap_or(self.password_policy.password_reuse_interval),
        }
    }

    /// Limits that apply to a password change, including clauses on this statement.
    pub fn effective_reuse_limits(
        &self,
        status: &PasswordStatus,
        options: &PasswordOptions,
    ) -> (u32, u32) {
        let history = match options.history {
            Some(Some(n)) => n,
            Some(None) => self.password_policy.password_history,
            None => status.history_limit,
        };
        let reuse = match options.reuse_interval_days {
            Some(Some(n)) => n,
            Some(None) => self.password_policy.password_reuse_interval,
            None => status.reuse_interval_days,
        };
        (history, reuse)
    }

    pub fn apply_password_options_only(
        &mut self,
        account: &Account,
        options: &PasswordOptions,
    ) -> bool {
        let Some(record) = self.find_exact_mut(account) else {
            return false;
        };
        apply_password_options(record, options);
        true
    }

    pub fn commit_new_password(
        &mut self,
        account: &Account,
        password: String,
        auth_plugin: Option<String>,
        options: &PasswordOptions,
        now: u64,
    ) -> bool {
        let Some(record) = self.find_exact_mut(account) else {
            return false;
        };
        let old = record.password.clone();
        let old_ts = if record.password_changed_unix == 0 {
            now
        } else {
            record.password_changed_unix
        };
        if !old.is_empty() {
            record.password_history.insert(
                0,
                StoredPassword {
                    password: old,
                    changed_unix: old_ts,
                },
            );
            record.password_history.truncate(PASSWORD_HISTORY_CAP);
        }
        record.password = password;
        if let Some(plugin) = auth_plugin {
            record.auth_plugin = plugin;
        }
        record.password_changed_unix = now;
        record.password_expired = false;
        apply_password_options(record, options);
        true
    }

    fn find_exact_mut(&mut self, account: &Account) -> Option<&mut UserAccountRecord> {
        self.accounts
            .iter_mut()
            .find(|a| a.user.eq_ignore_ascii_case(&account.user) && a.host == account.host)
    }

    pub fn drop_user(&mut self, account: &Account, if_exists: bool) -> Result<(), String> {
        let before = self.accounts.len();
        self.accounts
            .retain(|a| !(a.user.eq_ignore_ascii_case(&account.user) && a.host == account.host));
        self.grants
            .retain(|g| !(g.user.eq_ignore_ascii_case(&account.user) && g.host == account.host));
        self.role_edges.retain(|edge| {
            !same_account(
                &edge.grantee_user,
                &edge.grantee_host,
                &account.user,
                &account.host,
            )
        });
        if self.accounts.len() == before {
            if if_exists {
                return Ok(());
            }
            return Err(format!(
                "Operation DROP USER failed for {}. User does not exist",
                account.display()
            ));
        }
        Ok(())
    }

    pub fn resolve_auth(&self, username: &str, client_host: &str) -> Option<UserAccountRecord> {
        self.find_auth_account(username, client_host).cloned()
    }

    /// Exact `user`@`host` lookup for `SHOW CREATE USER` (not client-host matching).
    pub fn get_account(&self, user: &str, host: &str) -> Option<&UserAccountRecord> {
        self.accounts
            .iter()
            .find(|a| a.user.eq_ignore_ascii_case(user) && a.host == host)
    }

    fn find_auth_account(&self, username: &str, client_host: &str) -> Option<&UserAccountRecord> {
        let mut matches: Vec<&UserAccountRecord> = self
            .accounts
            .iter()
            .filter(|a| a.user.eq_ignore_ascii_case(username) && host_matches(&a.host, client_host))
            .collect();
        matches.sort_by_key(|a| host_specificity(&a.host));
        matches.first().copied()
    }

    pub fn grant(
        &mut self,
        account: &Account,
        target: GrantTarget,
        privileges: BTreeSet<Privilege>,
    ) {
        if privileges.is_empty() {
            return;
        }
        if let Some(record) = self
            .grants
            .iter_mut()
            .find(|g| g.user == account.user && g.host == account.host && g.target == target)
        {
            record.privileges.extend(privileges);
            return;
        }
        self.grants.push(GrantRecord {
            user: account.user.clone(),
            host: account.host.clone(),
            target,
            privileges,
        });
    }

    pub fn revoke(
        &mut self,
        account: &Account,
        target: GrantTarget,
        privileges: BTreeSet<Privilege>,
    ) {
        if privileges.is_empty() {
            return;
        }
        self.grants.retain_mut(|record| {
            if record.user != account.user || record.host != account.host || record.target != target
            {
                return true;
            }
            for p in &privileges {
                record.privileges.remove(p);
            }
            !record.privileges.is_empty()
        });
    }

    pub fn has_privilege(
        &self,
        account: &Account,
        schema: &str,
        table: Option<&str>,
        required: Privilege,
    ) -> bool {
        self.has_effective_privilege(account, schema, table, required, &[])
    }

    /// Direct grants plus privileges of `active_roles` and roles granted to those roles.
    /// A repeated node in the role graph stops the walk (cycle detection).
    pub fn has_effective_privilege(
        &self,
        account: &Account,
        schema: &str,
        table: Option<&str>,
        required: Privilege,
        active_roles: &[Account],
    ) -> bool {
        if Self::is_superuser(&account.user) {
            return true;
        }
        if self.direct_privilege(account, schema, table, required) {
            return true;
        }
        let mut visited = BTreeSet::new();
        for role in active_roles {
            if !self.is_role_granted_to(role, account) {
                continue;
            }
            if self.privilege_through_role(role, schema, table, required, &mut visited) {
                return true;
            }
        }
        false
    }

    pub fn show_grants(&self, account: &Account) -> Vec<String> {
        let mut lines: Vec<String> = self
            .grants
            .iter()
            .filter(|g| same_account(&g.user, &g.host, &account.user, &account.host))
            .map(format_grant_line)
            .collect();
        if lines.is_empty() {
            lines.push(format!("GRANT USAGE ON *.* TO {}", account.display()));
        }
        for edge in &self.role_edges {
            if same_account(
                &edge.grantee_user,
                &edge.grantee_host,
                &account.user,
                &account.host,
            ) {
                lines.push(format!(
                    "GRANT `{}`@`{}` TO {}",
                    edge.role_user,
                    edge.role_host,
                    account.display()
                ));
            }
        }
        lines.sort();
        lines
    }

    /// Login accounts for the virtual `mysql.user` table, ordered by `Host`, `User`.
    ///
    /// Roles are not rows. The password stored on the account is not returned.
    pub fn mysql_user_accounts(&self) -> Vec<MysqlUserAccountView> {
        let mut rows: Vec<MysqlUserAccountView> = self
            .accounts
            .iter()
            .map(|account| MysqlUserAccountView {
                user: account.user.clone(),
                host: account.host.clone(),
                auth_plugin: account.auth_plugin.clone(),
                require_ssl: account.require_ssl,
            })
            .collect();
        rows.sort_by(|a, b| a.host.cmp(&b.host).then(a.user.cmp(&b.user)));
        rows
    }

    pub fn mysql_user_rows(&self) -> Vec<(String, String, String)> {
        let mut by_account: HashMap<(String, String), BTreeSet<Privilege>> = HashMap::new();
        for record in &self.grants {
            by_account
                .entry((record.user.clone(), record.host.clone()))
                .or_default()
                .extend(record.privileges.iter().copied());
        }
        let mut rows: Vec<(String, String, String)> = by_account
            .into_iter()
            .map(|((user, host), privs)| {
                let list = privs
                    .iter()
                    .map(|p| p.as_str())
                    .collect::<Vec<_>>()
                    .join(",");
                (user, host, list)
            })
            .collect();
        rows.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        rows
    }
}

impl PrivilegeStore {
    pub fn is_role(&self, account: &Account) -> bool {
        self.roles
            .iter()
            .any(|role| same_account(&role.user, &role.host, &account.user, &account.host))
    }

    pub fn create_roles(
        &mut self,
        names: &[Account],
        if_not_exists: bool,
    ) -> Result<(), PrivilegeError> {
        for name in names {
            if self.role_or_user_exists(name) && !if_not_exists {
                return Err(PrivilegeError::AlreadyExists {
                    operation: "CREATE ROLE",
                    account: name.clone(),
                });
            }
        }
        for name in names {
            if !self.is_role(name) && self.get_account(&name.user, &name.host).is_none() {
                self.roles.push(RoleRecord {
                    user: name.user.clone(),
                    host: name.host.clone(),
                });
            }
        }
        Ok(())
    }

    pub fn drop_roles(&mut self, names: &[Account], if_exists: bool) -> Result<(), PrivilegeError> {
        for name in names {
            if !self.is_role(name) && !if_exists {
                return Err(PrivilegeError::DoesNotExist {
                    operation: "DROP ROLE",
                    account: name.clone(),
                });
            }
        }
        for name in names {
            self.roles
                .retain(|role| !same_account(&role.user, &role.host, &name.user, &name.host));
            self.role_edges.retain(|edge| {
                !same_account(&edge.role_user, &edge.role_host, &name.user, &name.host)
                    && !same_account(
                        &edge.grantee_user,
                        &edge.grantee_host,
                        &name.user,
                        &name.host,
                    )
            });
            self.grants
                .retain(|grant| !same_account(&grant.user, &grant.host, &name.user, &name.host));
        }
        Ok(())
    }

    pub fn grant_roles(
        &mut self,
        roles: &[Account],
        grantees: &[Account],
    ) -> Result<(), PrivilegeError> {
        for grantee in grantees {
            if grantee.user.is_empty() {
                return Err(PrivilegeError::AnonymousGrantee);
            }
            if !self.grantee_exists(grantee) {
                return Err(PrivilegeError::UnknownAuthId(grantee.clone()));
            }
        }
        for role in roles {
            if !self.is_role(role) {
                return Err(PrivilegeError::UnknownAuthId(role.clone()));
            }
        }
        for role in roles {
            for grantee in grantees {
                if self.is_role_granted_to(role, grantee) {
                    continue;
                }
                self.role_edges.push(RoleEdge {
                    role_user: role.user.clone(),
                    role_host: role.host.clone(),
                    grantee_user: grantee.user.clone(),
                    grantee_host: grantee.host.clone(),
                });
            }
        }
        Ok(())
    }

    pub fn revoke_roles(
        &mut self,
        roles: &[Account],
        grantees: &[Account],
    ) -> Result<(), PrivilegeError> {
        for role in roles {
            for grantee in grantees {
                if !self.is_role_granted_to(role, grantee) {
                    return Err(PrivilegeError::RoleNotGranted {
                        role: role.clone(),
                        grantee: grantee.clone(),
                    });
                }
            }
        }
        for role in roles {
            for grantee in grantees {
                self.role_edges.retain(|edge| {
                    !(same_account(&edge.role_user, &edge.role_host, &role.user, &role.host)
                        && same_account(
                            &edge.grantee_user,
                            &edge.grantee_host,
                            &grantee.user,
                            &grantee.host,
                        ))
                });
            }
        }
        Ok(())
    }

    /// Direct `GRANT role TO grantee` edge. Nested roles are not included.
    pub fn is_role_granted_to(&self, role: &Account, grantee: &Account) -> bool {
        self.role_edges.iter().any(|edge| {
            same_account(&edge.role_user, &edge.role_host, &role.user, &role.host)
                && same_account(
                    &edge.grantee_user,
                    &edge.grantee_host,
                    &grantee.user,
                    &grantee.host,
                )
        })
    }

    pub fn roles_granted_to(&self, grantee: &Account) -> Vec<Account> {
        let mut roles: Vec<Account> = self
            .role_edges
            .iter()
            .filter(|edge| {
                same_account(
                    &edge.grantee_user,
                    &edge.grantee_host,
                    &grantee.user,
                    &grantee.host,
                )
            })
            .map(|edge| Account::new(edge.role_user.clone(), edge.role_host.clone()))
            .collect();
        roles.sort_by(|a, b| a.user.cmp(&b.user).then(a.host.cmp(&b.host)));
        roles
    }

    /// True when following `GRANT role TO grantee` returns to a node already on the stack.
    /// MySQL stores such edges. Privilege checks stop at the repeat instead of looping.
    pub fn role_graph_has_cycle(&self) -> bool {
        let mut nodes = BTreeSet::new();
        for edge in &self.role_edges {
            nodes.insert(account_key(&edge.role_user, &edge.role_host));
            nodes.insert(account_key(&edge.grantee_user, &edge.grantee_host));
        }
        let mut color: HashMap<String, u8> = HashMap::new();
        for node in &nodes {
            if self.role_graph_visit(node, &mut color) {
                return true;
            }
        }
        false
    }

    fn role_graph_visit(&self, node: &str, color: &mut HashMap<String, u8>) -> bool {
        match color.get(node).copied().unwrap_or(0) {
            1 => return true,
            2 => return false,
            _ => {}
        }
        color.insert(node.to_string(), 1);
        for edge in &self.role_edges {
            let grantee = account_key(&edge.grantee_user, &edge.grantee_host);
            if grantee != node {
                continue;
            }
            let next = account_key(&edge.role_user, &edge.role_host);
            if self.role_graph_visit(&next, color) {
                return true;
            }
        }
        color.insert(node.to_string(), 2);
        false
    }

    fn grantee_exists(&self, account: &Account) -> bool {
        self.is_role(account)
            || self.get_account(&account.user, &account.host).is_some()
            || Self::is_superuser(&account.user)
    }

    fn role_or_user_exists(&self, account: &Account) -> bool {
        self.is_role(account) || self.get_account(&account.user, &account.host).is_some()
    }

    fn direct_privilege(
        &self,
        account: &Account,
        schema: &str,
        table: Option<&str>,
        required: Privilege,
    ) -> bool {
        self.grants.iter().any(|record| {
            if !same_account(&record.user, &record.host, &account.user, &account.host) {
                return false;
            }
            if !record.privileges.contains(&required) {
                return false;
            }
            if record.target.schema != schema && record.target.schema != "*" {
                return false;
            }
            match (&record.target.table, table) {
                (None, _) => true,
                (Some(granted_table), Some(table)) => granted_table == table,
                (Some(_), None) => false,
            }
        })
    }

    fn privilege_through_role(
        &self,
        role: &Account,
        schema: &str,
        table: Option<&str>,
        required: Privilege,
        visited: &mut BTreeSet<String>,
    ) -> bool {
        let key = account_key(&role.user, &role.host);
        if !visited.insert(key) {
            return false;
        }
        if self.direct_privilege(role, schema, table, required) {
            return true;
        }
        for nested in self.roles_granted_to(role) {
            if self.privilege_through_role(&nested, schema, table, required, visited) {
                return true;
            }
        }
        false
    }
}

fn same_account(user: &str, host: &str, other_user: &str, other_host: &str) -> bool {
    user.eq_ignore_ascii_case(other_user) && host == other_host
}

fn account_key(user: &str, host: &str) -> String {
    format!("{}@{}", user.to_ascii_lowercase(), host)
}

/// MySQL `CURRENT_ROLE()` text. `NONE` is the server token, not a translated label.
pub fn format_current_role(roles: &[Account]) -> String {
    if roles.is_empty() {
        return "NONE".to_string();
    }
    roles
        .iter()
        .map(|role| format!("`{}`@`{}`", role.user, role.host))
        .collect::<Vec<_>>()
        .join(",")
}

fn host_matches(account_host: &str, client_host: &str) -> bool {
    if account_host == "%" {
        return true;
    }
    account_host == client_host
}

fn host_specificity(host: &str) -> u8 {
    if host == "%" {
        1
    } else {
        0
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn password_options_empty(options: &PasswordOptions) -> bool {
    options.expire.is_none()
        && options.history.is_none()
        && options.reuse_interval_days.is_none()
        && options.require_ssl.is_none()
}

fn apply_password_options(record: &mut UserAccountRecord, options: &PasswordOptions) {
    if let Some(mode) = options.expire {
        match mode {
            PasswordExpireMode::Immediate => record.password_expired = true,
            PasswordExpireMode::Never => record.password_lifetime = PasswordLifetime::Never,
            PasswordExpireMode::Default => record.password_lifetime = PasswordLifetime::Default,
            PasswordExpireMode::Days(days) => {
                record.password_lifetime = PasswordLifetime::Days(days)
            }
        }
    }
    if let Some(history) = options.history {
        record.password_history_len = history;
    }
    if let Some(reuse) = options.reuse_interval_days {
        record.password_reuse_interval_days = reuse;
    }
    if let Some(require_ssl) = options.require_ssl {
        record.require_ssl = require_ssl;
    }
}

/// Parse `CREATE USER` / `DROP USER` / `ALTER USER` / `SET PASSWORD` (MySQL subset).
pub fn parse_account_ddl(sql: &str) -> Option<AccountDdl> {
    let trimmed = sql.trim().trim_end_matches(';').trim();
    let upper = trimmed.to_ascii_uppercase();
    if upper.starts_with("CREATE USER") {
        return parse_create_user(trimmed);
    }
    if upper.starts_with("DROP USER") {
        return parse_drop_user(trimmed);
    }
    if upper.starts_with("ALTER USER") {
        return parse_alter_user(trimmed);
    }
    if is_set_password(&upper) {
        return parse_set_password(trimmed);
    }
    None
}

/// `SET GLOBAL` of a password-policy variable. Other `SET` statements return `None`.
pub fn parse_password_policy_set(sql: &str) -> Option<PasswordPolicySetParse> {
    let trimmed = sql.trim().trim_end_matches(';').trim();
    let rest = strip_policy_set_prefix(trimmed)?;
    let (global, rest) = split_set_scope(rest)?;
    let (name, rest) = read_var_name(rest.trim_start())?;
    let name = name.to_ascii_lowercase();
    if !is_policy_var(&name) {
        return None;
    }
    if !global {
        return Some(PasswordPolicySetParse::NeedsGlobal(name));
    }
    let rest = rest.trim_start();
    if !rest.starts_with('=') {
        return Some(PasswordPolicySetParse::BadValue {
            name,
            value: rest.to_string(),
        });
    }
    let Some(value) = parse_set_value(rest[1..].trim_start()) else {
        return Some(PasswordPolicySetParse::BadValue {
            name,
            value: rest[1..].trim().to_string(),
        });
    };
    match assign_policy_value(&name, &value) {
        Some(assign) => Some(PasswordPolicySetParse::Assign(assign)),
        None => Some(PasswordPolicySetParse::BadValue { name, value }),
    }
}

/// `SELECT @@policy_var` with no other clauses.
pub fn parse_password_policy_select(sql: &str) -> Option<PasswordPolicyRead> {
    let trimmed = sql.trim().trim_end_matches(';').trim();
    let upper = trimmed.to_ascii_uppercase();
    let after = upper.strip_prefix("SELECT")?;
    if !after.starts_with(|c: char| c.is_whitespace()) {
        return None;
    }
    let expr = trimmed["SELECT".len()..].trim();
    if !expr.starts_with("@@") {
        return None;
    }
    let mut body = &expr[2..];
    let body_upper = body.to_ascii_uppercase();
    for prefix in ["GLOBAL.", "SESSION.", "LOCAL."] {
        if body_upper.starts_with(prefix) {
            body = &body[prefix.len()..];
            break;
        }
    }
    let end = body.find(|c: char| c.is_whitespace()).unwrap_or(body.len());
    if end == 0 || !body[end..].trim().is_empty() {
        return None;
    }
    let name = body[..end].to_ascii_lowercase();
    let field = policy_field(&name)?;
    Some(PasswordPolicyRead {
        column: expr.to_string(),
        field,
    })
}

fn parse_create_user(sql: &str) -> Option<AccountDdl> {
    let upper = sql.to_ascii_uppercase();
    let mut rest = sql["CREATE USER".len()..].trim_start();
    let upper_rest = &upper["CREATE USER".len()..];
    let if_not_exists = upper_rest.starts_with("IF NOT EXISTS");
    if if_not_exists {
        rest = rest["IF NOT EXISTS".len()..].trim_start();
    }
    let (accounts, mut rest) = parse_account_list(rest)?;
    let mut identified: Option<(Option<String>, String)> = None;
    let mut options = PasswordOptions::default();
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        let upper = rest.to_ascii_uppercase();
        if upper.starts_with("IDENTIFIED") && !continues_ident(&rest["IDENTIFIED".len()..]) {
            if identified.is_some() {
                return None;
            }
            let (plugin, password, tail) = parse_identified_clause(rest)?;
            identified = Some((plugin, password));
            rest = tail;
            continue;
        }
        if upper.starts_with("PASSWORD") && !continues_ident(&rest["PASSWORD".len()..]) {
            let (more, tail) = parse_one_password_clause(rest)?;
            if !merge_password_option(&mut options, more) {
                return None;
            }
            rest = tail;
            continue;
        }
        if upper.starts_with("REQUIRE") && !continues_ident(&rest["REQUIRE".len()..]) {
            let (require_ssl, tail) = parse_require_ssl_clause(rest)?;
            if options.require_ssl.is_some() {
                return None;
            }
            options.require_ssl = Some(require_ssl);
            rest = tail;
            continue;
        }
        return None;
    }
    let (plugin, password) = identified?;
    Some(AccountDdl::CreateUser {
        accounts,
        auth_plugin: plugin.unwrap_or_else(|| AUTH_PLUGIN_CACHING_SHA2.to_string()),
        password,
        if_not_exists,
        options,
    })
}

fn parse_alter_user(sql: &str) -> Option<AccountDdl> {
    let mut rest = sql["ALTER USER".len()..].trim_start();
    if rest.to_ascii_uppercase().starts_with("IF EXISTS") {
        rest = rest["IF EXISTS".len()..].trim_start();
    }
    let (accounts, rest) = parse_alter_account_list(rest)?;
    let action = parse_alter_action(rest)?;
    Some(AccountDdl::AlterUser { accounts, action })
}

fn parse_alter_action(input: &str) -> Option<AlterUserAction> {
    let mut rest = input.trim_start();
    if rest.is_empty() {
        return None;
    }
    let mut identified: Option<(Option<String>, String)> = None;
    let mut options = PasswordOptions::default();
    while !rest.is_empty() {
        let upper = rest.to_ascii_uppercase();
        if upper.starts_with("IDENTIFIED") {
            if identified.is_some() {
                return None;
            }
            let (plugin, password, tail) = parse_identified_clause(rest)?;
            identified = Some((plugin, password));
            rest = tail.trim_start();
            continue;
        }
        if upper.starts_with("PASSWORD") {
            let (more, tail) = parse_one_password_clause(rest)?;
            if !merge_password_option(&mut options, more) {
                return None;
            }
            rest = tail.trim_start();
            continue;
        }
        if upper.starts_with("REQUIRE") && !continues_ident(&rest["REQUIRE".len()..]) {
            let (require_ssl, tail) = parse_require_ssl_clause(rest)?;
            if options.require_ssl.is_some() {
                return None;
            }
            options.require_ssl = Some(require_ssl);
            rest = tail.trim_start();
            continue;
        }
        return None;
    }
    match identified {
        Some((auth_plugin, password)) => Some(AlterUserAction::Identified {
            auth_plugin,
            password,
            options,
        }),
        None if !password_options_empty(&options) => Some(AlterUserAction::Options(options)),
        None => None,
    }
}

fn parse_set_password(sql: &str) -> Option<AccountDdl> {
    let rest = sql["SET PASSWORD".len()..].trim_start();
    let upper = rest.to_ascii_uppercase();
    if upper.starts_with('=') || upper.starts_with("= ") {
        let (password, tail) = parse_quoted_literal_with_tail(rest[1..].trim_start())?;
        if !tail.trim().is_empty() {
            return None;
        }
        return Some(AccountDdl::SetPassword {
            account: None,
            password,
        });
    }
    if !upper.starts_with("FOR") || continues_ident(&rest["FOR".len()..]) {
        return None;
    }
    let rest = rest["FOR".len()..].trim_start();
    if rest.to_ascii_uppercase().starts_with("USER()") {
        let rest = rest["USER()".len()..].trim_start();
        if !rest.starts_with('=') {
            return None;
        }
        let (password, tail) = parse_quoted_literal_with_tail(rest[1..].trim_start())?;
        if !tail.trim().is_empty() {
            return None;
        }
        return Some(AccountDdl::SetPassword {
            account: None,
            password,
        });
    }
    let (account, rest) = parse_account_literal(rest)?;
    let rest = rest.trim_start();
    if !rest.starts_with('=') {
        return None;
    }
    let (password, tail) = parse_quoted_literal_with_tail(rest[1..].trim_start())?;
    if !tail.trim().is_empty() {
        return None;
    }
    Some(AccountDdl::SetPassword {
        account: Some(account),
        password,
    })
}

fn is_set_password(upper: &str) -> bool {
    let Some(rest) = upper.strip_prefix("SET PASSWORD") else {
        return false;
    };
    !continues_ident(rest)
}

fn parse_identified_clause(input: &str) -> Option<(Option<String>, String, &str)> {
    let rest = input.trim_start();
    let upper = rest.to_ascii_uppercase();
    if !upper.starts_with("IDENTIFIED") || continues_ident(&rest["IDENTIFIED".len()..]) {
        return None;
    }
    let rest = rest["IDENTIFIED".len()..].trim_start();
    let upper = rest.to_ascii_uppercase();
    if upper.starts_with("BY") && !continues_ident(&rest["BY".len()..]) {
        let (password, tail) = parse_quoted_literal_with_tail(rest["BY".len()..].trim_start())?;
        return Some((None, password, tail));
    }
    if upper.starts_with("WITH") && !continues_ident(&rest["WITH".len()..]) {
        let rest = rest["WITH".len()..].trim_start();
        let (plugin, password, tail) = parse_auth_plugin_password_tail(rest)?;
        return Some((Some(plugin), password, tail));
    }
    None
}

fn parse_auth_plugin_password_tail(input: &str) -> Option<(String, String, &str)> {
    let upper = input.to_ascii_uppercase();
    let (plugin, after) = if upper.starts_with("MYSQL_NATIVE_PASSWORD") {
        (
            AUTH_PLUGIN_NATIVE.to_string(),
            input["mysql_native_password".len()..].trim_start(),
        )
    } else if upper.starts_with("CACHING_SHA2_PASSWORD") {
        (
            AUTH_PLUGIN_CACHING_SHA2.to_string(),
            input["caching_sha2_password".len()..].trim_start(),
        )
    } else {
        let plugin = parse_bare_token(input)?;
        (plugin.clone(), input[plugin.len()..].trim_start())
    };
    let upper = after.to_ascii_uppercase();
    if !upper.starts_with("BY") || continues_ident(&after["BY".len()..]) {
        return None;
    }
    let (password, tail) = parse_quoted_literal_with_tail(after["BY".len()..].trim_start())?;
    Some((plugin, password, tail))
}

/// `REQUIRE SSL` is `true`. `REQUIRE NONE` is `false`. X509, issuer, subject, and cipher are rejected.
fn parse_require_ssl_clause(input: &str) -> Option<(bool, &str)> {
    let rest = input.trim_start();
    let upper = rest.to_ascii_uppercase();
    if !upper.starts_with("REQUIRE") || continues_ident(&rest["REQUIRE".len()..]) {
        return None;
    }
    let after = rest["REQUIRE".len()..].trim_start();
    let upper = after.to_ascii_uppercase();
    if upper.starts_with("SSL") && !continues_ident(&after["SSL".len()..]) {
        return Some((true, &after["SSL".len()..]));
    }
    if upper.starts_with("NONE") && !continues_ident(&after["NONE".len()..]) {
        return Some((false, &after["NONE".len()..]));
    }
    None
}

fn parse_one_password_clause(input: &str) -> Option<(PasswordOptions, &str)> {
    let rest = input.trim_start();
    let upper = rest.to_ascii_uppercase();
    if !upper.starts_with("PASSWORD") || continues_ident(&rest["PASSWORD".len()..]) {
        return None;
    }
    let after = rest["PASSWORD".len()..].trim_start();
    let upper = after.to_ascii_uppercase();
    if upper.starts_with("EXPIRE") && !continues_ident(&after["EXPIRE".len()..]) {
        let (mode, tail) = parse_expire_mode(after["EXPIRE".len()..].trim_start())?;
        return Some((
            PasswordOptions {
                expire: Some(mode),
                ..PasswordOptions::default()
            },
            tail,
        ));
    }
    if upper.starts_with("HISTORY") && !continues_ident(&after["HISTORY".len()..]) {
        let (value, tail) = parse_optional_count(after["HISTORY".len()..].trim_start())?;
        return Some((
            PasswordOptions {
                history: Some(value),
                ..PasswordOptions::default()
            },
            tail,
        ));
    }
    if upper.starts_with("REUSE") && !continues_ident(&after["REUSE".len()..]) {
        let after = after["REUSE".len()..].trim_start();
        let upper = after.to_ascii_uppercase();
        if !upper.starts_with("INTERVAL") || continues_ident(&after["INTERVAL".len()..]) {
            return None;
        }
        let (value, tail) = parse_reuse_interval(after["INTERVAL".len()..].trim_start())?;
        return Some((
            PasswordOptions {
                reuse_interval_days: Some(value),
                ..PasswordOptions::default()
            },
            tail,
        ));
    }
    None
}

fn merge_password_option(into: &mut PasswordOptions, more: PasswordOptions) -> bool {
    if more.expire.is_some() {
        if into.expire.is_some() {
            return false;
        }
        into.expire = more.expire;
    }
    if more.history.is_some() {
        if into.history.is_some() {
            return false;
        }
        into.history = more.history;
    }
    if more.reuse_interval_days.is_some() {
        if into.reuse_interval_days.is_some() {
            return false;
        }
        into.reuse_interval_days = more.reuse_interval_days;
    }
    true
}

fn parse_expire_mode(input: &str) -> Option<(PasswordExpireMode, &str)> {
    let upper = input.to_ascii_uppercase();
    if upper.starts_with("NEVER") && !continues_ident(&input["NEVER".len()..]) {
        return Some((PasswordExpireMode::Never, &input["NEVER".len()..]));
    }
    if upper.starts_with("DEFAULT") && !continues_ident(&input["DEFAULT".len()..]) {
        return Some((PasswordExpireMode::Default, &input["DEFAULT".len()..]));
    }
    if upper.starts_with("INTERVAL") && !continues_ident(&input["INTERVAL".len()..]) {
        let after = input["INTERVAL".len()..].trim_start();
        let (days, tail) = parse_u32_prefix(after)?;
        let tail = tail.trim_start();
        let upper = tail.to_ascii_uppercase();
        if !upper.starts_with("DAY") || continues_ident(&tail["DAY".len()..]) {
            return None;
        }
        return Some((PasswordExpireMode::Days(days), &tail["DAY".len()..]));
    }
    Some((PasswordExpireMode::Immediate, input))
}

fn parse_optional_count(input: &str) -> Option<(Option<u32>, &str)> {
    let upper = input.to_ascii_uppercase();
    if upper.starts_with("DEFAULT") && !continues_ident(&input["DEFAULT".len()..]) {
        return Some((None, &input["DEFAULT".len()..]));
    }
    let (n, tail) = parse_u32_prefix(input)?;
    Some((Some(n), tail))
}

fn parse_reuse_interval(input: &str) -> Option<(Option<u32>, &str)> {
    let upper = input.to_ascii_uppercase();
    if upper.starts_with("DEFAULT") && !continues_ident(&input["DEFAULT".len()..]) {
        return Some((None, &input["DEFAULT".len()..]));
    }
    let (days, tail) = parse_u32_prefix(input)?;
    let tail = tail.trim_start();
    let upper = tail.to_ascii_uppercase();
    if !upper.starts_with("DAY") || continues_ident(&tail["DAY".len()..]) {
        return None;
    }
    Some((Some(days), &tail["DAY".len()..]))
}

fn parse_alter_account_list(input: &str) -> Option<(Vec<AccountSpec>, &str)> {
    let mut accounts = Vec::new();
    let mut rest = input.trim_start();
    if rest.is_empty() {
        return None;
    }
    loop {
        if rest.len() >= 6 && rest[..6].eq_ignore_ascii_case("USER()") {
            accounts.push(AccountSpec::Current);
            rest = rest["USER()".len()..].trim_start();
        } else {
            let (account, tail) = parse_account_literal(rest)?;
            accounts.push(AccountSpec::Named(account));
            rest = tail.trim_start();
        }
        if rest.starts_with(',') {
            rest = rest[1..].trim_start();
            continue;
        }
        break;
    }
    Some((accounts, rest))
}

fn continues_ident(tail: &str) -> bool {
    tail.chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn parse_u32_prefix(input: &str) -> Option<(u32, &str)> {
    let input = input.trim_start();
    let end = input
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(input.len());
    if end == 0 {
        return None;
    }
    let n = input[..end].parse().ok()?;
    Some((n, &input[end..]))
}

fn strip_policy_set_prefix(sql: &str) -> Option<&str> {
    let upper = sql.to_ascii_uppercase();
    if !upper.starts_with("SET") || continues_ident(&sql["SET".len()..]) {
        return None;
    }
    let rest = sql["SET".len()..].trim_start();
    let upper = rest.to_ascii_uppercase();
    if upper.starts_with("PASSWORD") && !continues_ident(&rest["PASSWORD".len()..]) {
        return None;
    }
    if upper.starts_with("NAMES")
        || upper.starts_with("ROLE")
        || upper.starts_with("TRANSACTION")
        || upper.starts_with("CHARACTER")
    {
        return None;
    }
    Some(rest)
}

fn split_set_scope(input: &str) -> Option<(bool, &str)> {
    let upper = input.to_ascii_uppercase();
    if upper.starts_with("GLOBAL") && !continues_ident(&input["GLOBAL".len()..]) {
        return Some((true, input["GLOBAL".len()..].trim_start()));
    }
    if (upper.starts_with("SESSION") && !continues_ident(&input["SESSION".len()..]))
        || (upper.starts_with("LOCAL") && !continues_ident(&input["LOCAL".len()..]))
    {
        let keyword = if upper.starts_with("SESSION") {
            "SESSION"
        } else {
            "LOCAL"
        };
        return Some((false, input[keyword.len()..].trim_start()));
    }
    if let Some(stripped) = input.strip_prefix("@@") {
        let upper = stripped.to_ascii_uppercase();
        if upper.starts_with("GLOBAL.") {
            return Some((true, &stripped["GLOBAL.".len()..]));
        }
        if upper.starts_with("SESSION.") || upper.starts_with("LOCAL.") {
            let prefix = if upper.starts_with("SESSION.") {
                "SESSION."
            } else {
                "LOCAL."
            };
            return Some((false, &stripped[prefix.len()..]));
        }
        return Some((false, stripped));
    }
    Some((false, input))
}

fn read_var_name(input: &str) -> Option<(String, &str)> {
    let end = input
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
        .unwrap_or(input.len());
    if end == 0 {
        return None;
    }
    Some((input[..end].to_string(), &input[end..]))
}

fn is_policy_var(name: &str) -> bool {
    policy_field(name).is_some()
}

fn policy_field(name: &str) -> Option<PasswordPolicyField> {
    match name {
        "validate_password.length" => Some(PasswordPolicyField::ValidateLength),
        "validate_password.policy" => Some(PasswordPolicyField::ValidatePolicy),
        "password_history" => Some(PasswordPolicyField::PasswordHistory),
        "password_reuse_interval" => Some(PasswordPolicyField::PasswordReuseInterval),
        "default_password_lifetime" => Some(PasswordPolicyField::DefaultPasswordLifetime),
        _ => None,
    }
}

fn parse_set_value(input: &str) -> Option<String> {
    let input = input.trim();
    if input.starts_with('\'') || input.starts_with('"') {
        let (value, tail) = parse_quoted_literal_with_tail(input)?;
        if tail.trim().is_empty() {
            return Some(value);
        }
        return None;
    }
    let token = parse_bare_token(input)?;
    if input[token.len()..].trim().is_empty() {
        return Some(token);
    }
    None
}

fn assign_policy_value(name: &str, value: &str) -> Option<PasswordPolicyAssign> {
    match name {
        "validate_password.length" => {
            Some(PasswordPolicyAssign::ValidateLength(value.parse().ok()?))
        }
        "password_history" => Some(PasswordPolicyAssign::PasswordHistory(value.parse().ok()?)),
        "password_reuse_interval" => Some(PasswordPolicyAssign::PasswordReuseInterval(
            value.parse().ok()?,
        )),
        "default_password_lifetime" => Some(PasswordPolicyAssign::DefaultPasswordLifetime(
            value.parse().ok()?,
        )),
        "validate_password.policy" => {
            let policy = match value.to_ascii_uppercase().as_str() {
                "OFF" => "OFF",
                "LOW" | "0" => "LOW",
                "MEDIUM" | "1" => "MEDIUM",
                "STRONG" | "2" => "STRONG",
                _ => return None,
            };
            Some(PasswordPolicyAssign::ValidatePolicy(policy.to_string()))
        }
        _ => None,
    }
}

fn parse_drop_user(sql: &str) -> Option<AccountDdl> {
    let upper = sql.to_ascii_uppercase();
    let mut rest = sql["DROP USER".len()..].trim_start();
    let upper_rest = &upper["DROP USER".len()..];
    let if_exists = upper_rest.starts_with("IF EXISTS");
    if if_exists {
        rest = rest["IF EXISTS".len()..].trim_start();
    }
    let (accounts, tail) = parse_account_list(rest)?;
    if !tail.trim().is_empty() {
        return None;
    }
    Some(AccountDdl::DropUser {
        accounts,
        if_exists,
    })
}

fn parse_account_list(input: &str) -> Option<(Vec<Account>, &str)> {
    let mut accounts = Vec::new();
    let mut rest = input.trim_start();
    loop {
        let (account, tail) = parse_account_literal(rest)?;
        accounts.push(account);
        rest = tail.trim_start();
        if rest.starts_with(',') {
            rest = rest[1..].trim_start();
            continue;
        }
        break;
    }
    Some((accounts, rest))
}

fn parse_account_literal(input: &str) -> Option<(Account, &str)> {
    let (user, rest) = parse_quoted_literal_with_tail(input)?;
    let rest = rest.trim_start();
    if !rest.starts_with('@') {
        return Some((Account::new(user, "%"), rest));
    }
    let rest = rest[1..].trim_start();
    if rest.starts_with('\'') || rest.starts_with('"') {
        let (host, tail) = parse_quoted_literal_with_tail(rest)?;
        return Some((Account::new(user, host), tail));
    }
    let host = parse_bare_token(rest)?;
    let tail = &rest[host.len()..];
    Some((Account::new(user, host), tail))
}

fn parse_quoted_literal_with_tail(input: &str) -> Option<(String, &str)> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let quote = bytes[0];
    if quote != b'\'' && quote != b'"' {
        return None;
    }
    let mut value = String::new();
    let mut escaped = false;
    for (idx, ch) in input[1..].char_indices() {
        if escaped {
            value.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch as u8 == quote {
            return Some((value, &input[idx + 2..]));
        }
        value.push(ch);
    }
    None
}

fn parse_bare_token(input: &str) -> Option<String> {
    let end = input
        .find(|c: char| c.is_whitespace() || c == ',')
        .unwrap_or(input.len());
    if end == 0 {
        return None;
    }
    Some(input[..end].to_string())
}

fn format_grant_line(record: &GrantRecord) -> String {
    let privs = record
        .privileges
        .iter()
        .map(|p| p.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let object = match &record.target.table {
        None => format!("`{}`.*", record.target.schema),
        Some(table) => format!("`{}`.`{}`", record.target.schema, table),
    };
    format!(
        "GRANT {} ON {} TO {}",
        privs,
        object,
        Account::new(&record.user, &record.host).display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grant_revoke_and_check() {
        let mut store = PrivilegeStore::new();
        let account = Account::new("app", "%");
        let target = GrantTarget {
            schema: "rusql".into(),
            table: None,
        };
        store.grant(
            &account,
            target.clone(),
            BTreeSet::from([Privilege::Select, Privilege::Insert]),
        );
        assert!(store.has_privilege(&account, "rusql", Some("t"), Privilege::Select));
        assert!(store.has_privilege(&account, "rusql", Some("t"), Privilege::Insert));
        assert!(!store.has_privilege(&account, "rusql", Some("t"), Privilege::Update));
        store.revoke(&account, target, BTreeSet::from([Privilege::Insert]));
        assert!(!store.has_privilege(&account, "rusql", Some("t"), Privilege::Insert));
    }

    #[test]
    fn create_and_resolve_user() {
        let mut store = PrivilegeStore::new();
        let account = Account::new("app", "%");
        store
            .create_user(&account, "secret", AUTH_PLUGIN_NATIVE, false)
            .unwrap();
        let resolved = store.resolve_auth("app", "127.0.0.1").unwrap();
        assert_eq!(resolved.password, "secret");
        assert_eq!(resolved.auth_plugin, AUTH_PLUGIN_NATIVE);
        let exact = store.get_account("app", "%").unwrap();
        assert_eq!(exact.auth_plugin, AUTH_PLUGIN_NATIVE);
        assert!(store.get_account("app", "127.0.0.1").is_none());
        store.drop_user(&account, false).unwrap();
        assert!(store.resolve_auth("app", "127.0.0.1").is_none());
    }

    #[test]
    fn parse_create_user_ddl() {
        let ddl = parse_account_ddl(
            "CREATE USER 'app'@'%' IDENTIFIED WITH mysql_native_password BY 'secret'",
        )
        .unwrap();
        match ddl {
            AccountDdl::CreateUser {
                accounts,
                auth_plugin,
                password,
                ..
            } => {
                assert_eq!(accounts.len(), 1);
                assert_eq!(auth_plugin, AUTH_PLUGIN_NATIVE);
                assert_eq!(password, "secret");
            }
            _ => panic!("expected create user"),
        }
    }

    #[test]
    fn show_grants_shape() {
        let mut store = PrivilegeStore::new();
        let account = Account::new("app", "%");
        store.grant(
            &account,
            GrantTarget {
                schema: "rusql".into(),
                table: None,
            },
            BTreeSet::from([Privilege::Select, Privilege::Insert]),
        );
        let lines = store.show_grants(&account);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("SELECT, INSERT"));
        assert!(lines[0].contains("`rusql`.*"));
        assert!(lines[0].contains("'app'@'%'"));
    }

    #[test]
    fn roles_grant_activates_only_after_set_role_list() {
        let mut store = PrivilegeStore::new();
        let role = Account::new("r", "%");
        let user = Account::new("app", "%");
        store
            .create_roles(std::slice::from_ref(&role), false)
            .unwrap();
        store
            .create_user(&user, "secret", AUTH_PLUGIN_NATIVE, false)
            .unwrap();
        store.grant(
            &role,
            GrantTarget {
                schema: "rusql".into(),
                table: None,
            },
            BTreeSet::from([Privilege::Select]),
        );
        store
            .grant_roles(std::slice::from_ref(&role), std::slice::from_ref(&user))
            .unwrap();
        assert!(!store.has_effective_privilege(&user, "rusql", Some("t"), Privilege::Select, &[]));
        assert!(store.has_effective_privilege(
            &user,
            "rusql",
            Some("t"),
            Privilege::Select,
            std::slice::from_ref(&role)
        ));
        let lines = store.show_grants(&user);
        assert!(lines.iter().any(|line| line.contains("GRANT `r`@`%` TO")));
        store
            .revoke_roles(std::slice::from_ref(&role), std::slice::from_ref(&user))
            .unwrap();
        assert!(!store.has_effective_privilege(
            &user,
            "rusql",
            Some("t"),
            Privilege::Select,
            &[role]
        ));
    }

    #[test]
    fn role_graph_cycle_is_detected_and_privilege_walk_stops() {
        let mut store = PrivilegeStore::new();
        let r1 = Account::new("r1", "%");
        let r2 = Account::new("r2", "%");
        let user = Account::new("app", "%");
        store
            .create_roles(&[r1.clone(), r2.clone()], false)
            .unwrap();
        store
            .create_user(&user, "secret", AUTH_PLUGIN_NATIVE, false)
            .unwrap();
        store.grant(
            &r2,
            GrantTarget {
                schema: "rusql".into(),
                table: None,
            },
            BTreeSet::from([Privilege::Select]),
        );
        store
            .grant_roles(std::slice::from_ref(&r2), std::slice::from_ref(&r1))
            .unwrap();
        store
            .grant_roles(std::slice::from_ref(&r1), std::slice::from_ref(&r2))
            .unwrap();
        store
            .grant_roles(std::slice::from_ref(&r1), std::slice::from_ref(&user))
            .unwrap();
        assert!(store.role_graph_has_cycle());
        assert!(store.has_effective_privilege(&user, "rusql", Some("t"), Privilege::Select, &[r1]));
        assert_eq!(format_current_role(&[]), "NONE");
        assert_eq!(format_current_role(&[Account::new("r", "%")]), "`r`@`%`");
    }

    #[test]
    fn role_catalog_roundtrip_keeps_edges() {
        let dir = std::env::temp_dir().join(format!("rusql-roles-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut store = PrivilegeStore::new();
        let role = Account::new("r", "%");
        store
            .create_roles(std::slice::from_ref(&role), false)
            .unwrap();
        store
            .grant_roles(&[role], &[Account::new("root", "%")])
            .unwrap();
        store.save(&dir).unwrap();
        let loaded = PrivilegeStore::load(&dir).unwrap();
        assert!(loaded.is_role(&Account::new("r", "%")));
        assert!(loaded.is_role_granted_to(&Account::new("R", "%"), &Account::new("root", "%")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_password_expire_alter_and_set() {
        let ddl = parse_account_ddl(
            "ALTER USER 'app'@'%' PASSWORD EXPIRE INTERVAL 30 DAY PASSWORD HISTORY 2",
        )
        .unwrap();
        match ddl {
            AccountDdl::AlterUser {
                action: AlterUserAction::Options(options),
                ..
            } => {
                assert_eq!(options.expire, Some(PasswordExpireMode::Days(30)));
                assert_eq!(options.history, Some(Some(2)));
            }
            other => panic!("alter options: {other:?}"),
        }
        let ddl = parse_account_ddl("ALTER USER USER() IDENTIFIED BY 'secret2'").unwrap();
        match ddl {
            AccountDdl::AlterUser {
                accounts,
                action:
                    AlterUserAction::Identified {
                        auth_plugin,
                        password,
                        ..
                    },
            } => {
                assert_eq!(accounts, vec![AccountSpec::Current]);
                assert!(auth_plugin.is_none());
                assert_eq!(password, "secret2");
            }
            other => panic!("alter identified: {other:?}"),
        }
        let ddl = parse_account_ddl("SET PASSWORD FOR 'app'@'%' = 'secret3'").unwrap();
        match ddl {
            AccountDdl::SetPassword {
                account: Some(account),
                password,
            } => {
                assert_eq!(account.user, "app");
                assert_eq!(password, "secret3");
            }
            other => panic!("set password: {other:?}"),
        }
        match parse_password_policy_set("SET GLOBAL validate_password.policy = LOW").unwrap() {
            PasswordPolicySetParse::Assign(PasswordPolicyAssign::ValidatePolicy(policy)) => {
                assert_eq!(policy, "LOW");
            }
            other => panic!("policy set: {other:?}"),
        }
        assert!(matches!(
            parse_password_policy_set("SET validate_password.length = 8").unwrap(),
            PasswordPolicySetParse::NeedsGlobal(_)
        ));
        let read = parse_password_policy_select("SELECT @@global.password_history").unwrap();
        assert_eq!(read.field, PasswordPolicyField::PasswordHistory);
    }

    #[test]
    fn password_expire_catalog_roundtrip() {
        let mut store = PrivilegeStore::new();
        let account = Account::new("app", "%");
        let options = PasswordOptions {
            expire: Some(PasswordExpireMode::Immediate),
            history: Some(Some(2)),
            ..PasswordOptions::default()
        };
        store
            .create_user_with_options(
                &account,
                "secret",
                AUTH_PLUGIN_CACHING_SHA2,
                false,
                &options,
                50,
            )
            .unwrap();
        let status = store.password_status("app", "%").unwrap();
        assert!(status.expired);
        assert_eq!(status.history_limit, 2);
        assert_eq!(status.changed_unix, 50);
        assert!(store.commit_new_password(
            &account,
            "secret2".into(),
            None,
            &PasswordOptions::default(),
            80,
        ));
        let status = store.password_status("app", "%").unwrap();
        assert!(!status.expired);
        assert_eq!(status.current, "secret2");
        assert_eq!(status.history[0].password, "secret");
        let (limit, _) = store.effective_reuse_limits(&status, &PasswordOptions::default());
        assert_eq!(limit, 2);
    }

    #[test]
    fn require_ssl_parse_persists_and_rejects_x509() {
        let ddl = parse_account_ddl(
            "CREATE USER 'app'@'%' REQUIRE SSL IDENTIFIED BY 'secret' PASSWORD EXPIRE",
        )
        .unwrap();
        match ddl {
            AccountDdl::CreateUser {
                password, options, ..
            } => {
                assert_eq!(password, "secret");
                assert_eq!(options.require_ssl, Some(true));
                assert_eq!(options.expire, Some(PasswordExpireMode::Immediate));
            }
            other => panic!("create: {other:?}"),
        }
        assert!(
            parse_account_ddl("CREATE USER 'app'@'%' IDENTIFIED BY 'secret' REQUIRE X509")
                .is_none()
        );
        assert!(parse_account_ddl(
            "ALTER USER 'app'@'%' REQUIRE ISSUER '/CN=ca' REQUIRE SUBJECT '/CN=app'"
        )
        .is_none());
        assert!(parse_account_ddl("ALTER USER 'app'@'%' REQUIRE CIPHER 'AES128-SHA'").is_none());

        let mut store = PrivilegeStore::new();
        let account = Account::new("app", "%");
        let options = PasswordOptions {
            require_ssl: Some(true),
            ..PasswordOptions::default()
        };
        store
            .create_user_with_options(
                &account,
                "secret",
                AUTH_PLUGIN_CACHING_SHA2,
                false,
                &options,
                10,
            )
            .unwrap();
        assert!(store.get_account("app", "%").unwrap().require_ssl);
        let cleared = PasswordOptions {
            require_ssl: Some(false),
            ..PasswordOptions::default()
        };
        assert!(store.apply_password_options_only(&account, &cleared));
        assert!(!store.get_account("app", "%").unwrap().require_ssl);
        assert!(store.apply_password_options_only(&account, &options));

        let json = serde_json::to_string(store.get_account("app", "%").unwrap()).unwrap();
        let restored: UserAccountRecord = serde_json::from_str(&json).unwrap();
        assert!(restored.require_ssl);
        let legacy =
            r#"{"user":"old","host":"%","auth_plugin":"caching_sha2_password","password":"x"}"#;
        let old: UserAccountRecord = serde_json::from_str(legacy).unwrap();
        assert!(!old.require_ssl);

        match parse_account_ddl("ALTER USER 'app'@'%' REQUIRE NONE").unwrap() {
            AccountDdl::AlterUser {
                action: AlterUserAction::Options(options),
                ..
            } => assert_eq!(options.require_ssl, Some(false)),
            other => panic!("alter none: {other:?}"),
        }
        match parse_account_ddl("ALTER USER 'app'@'%' IDENTIFIED BY 'next' REQUIRE SSL").unwrap() {
            AccountDdl::AlterUser {
                action:
                    AlterUserAction::Identified {
                        options, password, ..
                    },
                ..
            } => {
                assert_eq!(password, "next");
                assert_eq!(options.require_ssl, Some(true));
            }
            other => panic!("alter identified: {other:?}"),
        }
    }
}
