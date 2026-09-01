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
}

/// Parsed account DDL (CREATE/DROP USER).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountDdl {
    CreateUser {
        accounts: Vec<Account>,
        auth_plugin: String,
        password: String,
        if_not_exists: bool,
    },
    DropUser {
        accounts: Vec<Account>,
        if_exists: bool,
    },
}

/// In-memory privilege catalog persisted under the server data directory.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PrivilegeStore {
    grants: Vec<GrantRecord>,
    #[serde(default)]
    accounts: Vec<UserAccountRecord>,
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
        });
    }

    pub fn create_user(
        &mut self,
        account: &Account,
        password: impl Into<String>,
        auth_plugin: impl Into<String>,
        if_not_exists: bool,
    ) -> Result<(), String> {
        if self
            .find_auth_account(&account.user, &account.host)
            .is_some()
        {
            if if_not_exists {
                return Ok(());
            }
            return Err(format!(
                "Operation CREATE USER failed for {} as it already exists",
                account.display()
            ));
        }
        self.ensure_account(account, password, auth_plugin);
        Ok(())
    }

    pub fn drop_user(&mut self, account: &Account, if_exists: bool) -> Result<(), String> {
        let before = self.accounts.len();
        self.accounts
            .retain(|a| !(a.user.eq_ignore_ascii_case(&account.user) && a.host == account.host));
        self.grants
            .retain(|g| !(g.user.eq_ignore_ascii_case(&account.user) && g.host == account.host));
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
        if Self::is_superuser(&account.user) {
            return true;
        }
        self.grants.iter().any(|record| {
            if record.user != account.user || record.host != account.host {
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

    pub fn show_grants(&self, account: &Account) -> Vec<String> {
        let mut lines: Vec<String> = self
            .grants
            .iter()
            .filter(|g| g.user == account.user && g.host == account.host)
            .map(format_grant_line)
            .collect();
        if lines.is_empty() {
            lines.push(format!("GRANT USAGE ON *.* TO {}", account.display()));
        }
        lines.sort();
        lines
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

/// Parse `CREATE USER` / `DROP USER` (MySQL subset).
pub fn parse_account_ddl(sql: &str) -> Option<AccountDdl> {
    let trimmed = sql.trim().trim_end_matches(';').trim();
    let upper = trimmed.to_ascii_uppercase();
    if upper.starts_with("CREATE USER") {
        return parse_create_user(trimmed);
    }
    if upper.starts_with("DROP USER") {
        return parse_drop_user(trimmed);
    }
    None
}

fn parse_create_user(sql: &str) -> Option<AccountDdl> {
    let upper = sql.to_ascii_uppercase();
    let mut rest = sql["CREATE USER".len()..].trim_start();
    let upper_rest = &upper["CREATE USER".len()..];
    let if_not_exists = upper_rest.starts_with("IF NOT EXISTS");
    if if_not_exists {
        rest = rest["IF NOT EXISTS".len()..].trim_start();
    }
    let (accounts, rest) = parse_account_list(rest)?;
    let upper_rest = rest.to_ascii_uppercase();
    let (auth_plugin, password) = if let Some(idx) = upper_rest.find("IDENTIFIED BY") {
        let pass = parse_quoted_literal(rest[idx + "IDENTIFIED BY".len()..].trim_start())?;
        (AUTH_PLUGIN_CACHING_SHA2.to_string(), pass)
    } else if let Some(idx) = upper_rest.find("IDENTIFIED WITH") {
        let tail = rest[idx + "IDENTIFIED WITH".len()..].trim_start();
        let (plugin, password) = parse_auth_plugin_and_password(tail)?;
        (plugin, password?)
    } else {
        return None;
    };
    Some(AccountDdl::CreateUser {
        accounts,
        auth_plugin,
        password,
        if_not_exists,
    })
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

fn parse_auth_plugin_and_password(input: &str) -> Option<(String, Option<String>)> {
    let upper = input.to_ascii_uppercase();
    if upper.starts_with("MYSQL_NATIVE_PASSWORD") {
        let tail = input["mysql_native_password".len()..].trim_start();
        let password = parse_by_password(tail)?;
        return Some((AUTH_PLUGIN_NATIVE.to_string(), Some(password)));
    }
    if upper.starts_with("CACHING_SHA2_PASSWORD") {
        let tail = input["caching_sha2_password".len()..].trim_start();
        let password = parse_by_password(tail)?;
        return Some((AUTH_PLUGIN_CACHING_SHA2.to_string(), Some(password)));
    }
    let plugin = parse_bare_token(input)?;
    let tail = input[plugin.len()..].trim_start();
    let password = parse_by_password(tail)?;
    Some((plugin, Some(password)))
}

fn parse_by_password(input: &str) -> Option<String> {
    let upper = input.to_ascii_uppercase();
    if !upper.starts_with("BY") {
        return None;
    }
    parse_quoted_literal(input[2..].trim_start())
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

fn parse_quoted_literal(input: &str) -> Option<String> {
    parse_quoted_literal_with_tail(input).map(|(value, _)| value)
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
}
