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

/// In-memory privilege catalog persisted under the server data directory.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PrivilegeStore {
    grants: Vec<GrantRecord>,
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
