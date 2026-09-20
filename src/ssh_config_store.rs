use crate::database::{FileDatabase, HostDatabaseEntry};
use anyhow::{format_err, Context, Result};
use ssh_cfg::{SshConfig, SshConfigParser, SshHostConfig};
use std::cmp::Reverse;
use std::collections::HashMap;
use std::fmt::Debug;

// Constants for group names
pub const RECENTS_GROUP: &str = "Recents";
const OTHERS_GROUP: &str = "Others";
const RECENTS_LIMIT: usize = 20;

/// Extracts full-line comments that immediately precede a `Host` directive.
fn extract_comments(contents: &str) -> HashMap<String, String> {
    let mut comments = HashMap::new();
    let mut current_comment = String::new();

    for line in contents.lines() {
        let trimmed = line.trim();

        if let Some(comment_text) = trimmed.strip_prefix('#') {
            if !current_comment.is_empty() {
                current_comment.push('\n');
            }
            current_comment.push_str(comment_text.trim());
        } else if let Some(host) = host_pattern(trimmed) {
            if !current_comment.is_empty() {
                comments.insert(host.to_string(), std::mem::take(&mut current_comment));
            }
        } else {
            current_comment.clear();
        }
    }

    comments
}

/// Returns the value of a `Host` line, or `None` when the line is not a
/// `Host` directive. Handles case-insensitive keywords, `=` separators and
/// inline comments. The value is kept as a single string (including multiple
/// patterns), matching how `ssh_cfg` keys hosts.
fn host_pattern(line: &str) -> Option<&str> {
    let line = line
        .split_once('#')
        .map_or(line, |(before, _)| before)
        .trim();

    let (key, rest) = match line.split_once('=') {
        Some((key, value)) => (key.trim(), Some(value.trim())),
        None => {
            let mut parts = line.splitn(2, char::is_whitespace);
            (parts.next().unwrap_or(""), parts.next().map(str::trim))
        }
    };

    if !key.eq_ignore_ascii_case("host") {
        return None;
    }

    match rest {
        Some(value) if !value.is_empty() => Some(value),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct SshGroupItem {
    pub name: String,
    pub full_name: String,
    pub connection_count: i64,
    pub last_used: i64,
    pub host_config: SshHostConfig,
    pub comment: Option<String>,
}

#[derive(Debug)]
pub struct SshGroup {
    pub name: String,
    pub items: Vec<SshGroupItem>,
}

#[derive(Debug)]
pub struct SshConfigStore {
    pub config: SshConfig,
    pub groups: Vec<SshGroup>,
}

impl SshConfigStore {
    pub async fn new(db: &FileDatabase) -> Result<SshConfigStore> {
        let home =
            dirs::home_dir().ok_or_else(|| format_err!("Could not determine home directory"))?;
        let config_path = home.join(".ssh").join("config");

        // Read once and feed both the parser and the comment extractor.
        let contents = tokio::fs::read_to_string(&config_path)
            .await
            .with_context(|| format!("Could not read SSH config at {}", config_path.display()))?;

        let ssh_config = SshConfigParser::parse_config_contents(&contents)?;
        let comments = extract_comments(&contents);

        let mut scs = SshConfigStore {
            config: ssh_config,
            groups: Vec::new(),
        };

        scs.create_ssh_groups(db, &comments);

        if scs.groups.is_empty() {
            return Err(format_err!("Your configuration file contains no entries (or only wildcards) ! Please add at least one."));
        }

        Ok(scs)
    }

    fn create_ssh_groups(&mut self, db: &FileDatabase, comments: &HashMap<String, String>) {
        let mut groups: Vec<SshGroup> = vec![SshGroup {
            name: OTHERS_GROUP.to_string(),
            items: Vec::new(),
        }];
        let mut group_indices: HashMap<&str, usize> = HashMap::new();
        group_indices.insert(OTHERS_GROUP, 0);

        for (key, value) in self.config.iter() {
            // Skip wildcard entries
            if key.contains('*') {
                continue;
            }

            let host_entry = db.get_host_values(key).unwrap_or_else(|e| {
                eprintln!("Warning: Failed to get database entry for '{}': {}", key, e);
                HostDatabaseEntry {
                    connection_count: 0,
                    last_used_date: 0,
                }
            });

            let (group_name, item_name) = match key.split_once('/') {
                Some((group_name, item_name)) => (group_name, item_name),
                None => (OTHERS_GROUP, key.as_str()),
            };

            let group_item = SshGroupItem {
                name: item_name.to_string(),
                full_name: key.to_string(),
                connection_count: host_entry.connection_count,
                last_used: host_entry.last_used_date,
                host_config: value.clone(),
                comment: comments.get(key).cloned(),
            };

            let group_index = match group_indices.get(group_name) {
                Some(&index) => index,
                None => {
                    let index = groups.len();
                    group_indices.insert(group_name, index);
                    groups.push(SshGroup {
                        name: group_name.to_string(),
                        items: Vec::new(),
                    });
                    index
                }
            };
            groups[group_index].items.push(group_item);
        }

        self.groups = groups.into_iter().filter(|g| !g.items.is_empty()).collect();
        self.groups.sort_by_cached_key(|a| a.name.to_lowercase());
        for group in &mut self.groups {
            group.items.sort_by_cached_key(|a| a.name.to_lowercase());
        }

        // Create "Recents" group from used items
        let mut all_used_items: Vec<SshGroupItem> = self
            .groups
            .iter()
            .flat_map(|g| g.items.iter().filter(|i| i.last_used > 0).cloned())
            .collect();

        if !all_used_items.is_empty() {
            all_used_items.sort_unstable_by_key(|b| Reverse(b.last_used));
            all_used_items.truncate(RECENTS_LIMIT);

            self.groups.insert(
                0,
                SshGroup {
                    name: RECENTS_GROUP.to_string(),
                    items: all_used_items,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_host_pattern() {
        assert_eq!(host_pattern("Host foo"), Some("foo"));
        assert_eq!(host_pattern("host foo"), Some("foo"));
        assert_eq!(host_pattern("HOST=foo"), Some("foo"));
        assert_eq!(host_pattern("Host foo bar"), Some("foo bar"));
        assert_eq!(host_pattern("Host foo # trailing"), Some("foo"));
        assert_eq!(host_pattern("HostName foo"), None);
        assert_eq!(host_pattern("Host"), None);
        assert_eq!(host_pattern(""), None);
    }

    #[test]
    fn test_extract_comments() {
        let contents = "\
# first host
# second line
Host alpha
    HostName 10.0.0.1

# lowercase directive
host beta
    HostName 10.0.0.2

# multi pattern
Host gamma delta
    HostName 10.0.0.3
";

        let comments = extract_comments(contents);
        assert_eq!(
            comments.get("alpha").map(String::as_str),
            Some("first host\nsecond line")
        );
        assert_eq!(
            comments.get("beta").map(String::as_str),
            Some("lowercase directive")
        );
        assert_eq!(
            comments.get("gamma delta").map(String::as_str),
            Some("multi pattern")
        );
    }

    #[test]
    fn test_extract_comments_clears_on_other_directives() {
        let contents = "\
# stale comment
Include ~/.ssh/other
Host epsilon
    HostName 10.0.0.4
";

        assert_eq!(extract_comments(contents).get("epsilon"), None);
    }

    #[test]
    fn test_ssh_group_item_creation() {
        let item = SshGroupItem {
            name: "test-server".to_string(),
            full_name: "production/test-server".to_string(),
            connection_count: 5,
            last_used: 1234567890,
            host_config: SshHostConfig::default(),
            comment: Some("Test server".to_string()),
        };

        assert_eq!(item.name, "test-server");
        assert_eq!(item.full_name, "production/test-server");
        assert_eq!(item.connection_count, 5);
        assert_eq!(item.last_used, 1234567890);
        assert_eq!(item.comment, Some("Test server".to_string()));
    }

    #[test]
    fn test_ssh_group_item_no_comment() {
        let item = SshGroupItem {
            name: "test-server".to_string(),
            full_name: "test-server".to_string(),
            connection_count: 0,
            last_used: 0,
            host_config: SshHostConfig::default(),
            comment: None,
        };

        assert_eq!(item.comment, None);
    }

    #[test]
    fn test_ssh_group_creation() {
        let group = SshGroup {
            name: "Production".to_string(),
            items: vec![],
        };

        assert_eq!(group.name, "Production");
        assert_eq!(group.items.len(), 0);
    }

    #[test]
    fn test_ssh_group_with_items() {
        let item1 = SshGroupItem {
            name: "server1".to_string(),
            full_name: "server1".to_string(),
            connection_count: 1,
            last_used: 100,
            host_config: SshHostConfig::default(),
            comment: None,
        };

        let item2 = SshGroupItem {
            name: "server2".to_string(),
            full_name: "server2".to_string(),
            connection_count: 2,
            last_used: 200,
            host_config: SshHostConfig::default(),
            comment: None,
        };

        let group = SshGroup {
            name: "Test".to_string(),
            items: vec![item1, item2],
        };

        assert_eq!(group.items.len(), 2);
        assert_eq!(group.items[0].name, "server1");
        assert_eq!(group.items[1].name, "server2");
    }

    #[test]
    fn test_ssh_group_item_clone() {
        let item = SshGroupItem {
            name: "test".to_string(),
            full_name: "test".to_string(),
            connection_count: 5,
            last_used: 123,
            host_config: SshHostConfig::default(),
            comment: Some("comment".to_string()),
        };

        let cloned = item.clone();
        assert_eq!(item.name, cloned.name);
        assert_eq!(item.full_name, cloned.full_name);
        assert_eq!(item.connection_count, cloned.connection_count);
        assert_eq!(item.last_used, cloned.last_used);
        assert_eq!(item.comment, cloned.comment);
    }
}
