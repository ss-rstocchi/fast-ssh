use crate::database::{FileDatabase, HostDatabaseEntry};
use anyhow::{format_err, Result};
use ssh_cfg::{SshConfig, SshConfigParser, SshHostConfig};
use std::cmp::Reverse;
use std::collections::HashMap;
use std::fmt::Debug;
use std::fs::read_to_string;
use std::path::PathBuf;

// Constants for group names
pub const RECENTS_GROUP: &str = "Recents";
const OTHERS_GROUP: &str = "Others";
const RECENTS_LIMIT: usize = 20;

trait ConfigComments {
    fn get_comments(&self) -> HashMap<String, String>;
}

impl ConfigComments for SshConfig {
    fn get_comments(&self) -> HashMap<String, String> {
        let mut comments = HashMap::new();

        let home = std::env::var("HOME").unwrap_or_else(|_| {
            eprintln!("Warning: $HOME is not set, falling back to current directory for SSH config");
            ".".to_string()
        });
        let config_path = PathBuf::from(home).join(".ssh/config");

        if let Ok(contents) = read_to_string(config_path) {
            let mut current_comment = String::new();

            for line in contents.lines() {
                let trimmed = line.trim();

                if let Some(comment_text) = trimmed.strip_prefix('#') {
                    if !current_comment.is_empty() {
                        current_comment.push('\n');
                    }
                    current_comment.push_str(comment_text.trim());
                } else if let Some(host) = trimmed.strip_prefix("Host ") {
                    if !current_comment.is_empty() {
                        comments.insert(host.trim().to_string(), std::mem::take(&mut current_comment));
                    }
                } else if trimmed.is_empty() {
                    current_comment.clear();
                }
            }
        }

        comments
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
        let ssh_config = SshConfigParser::parse_home().await?;

        let comments = ssh_config.get_comments();

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

    fn create_ssh_groups(
        &mut self,
        db: &FileDatabase,
        comments: &std::collections::HashMap<String, String>,
    ) {
        let mut groups: Vec<SshGroup> = vec![SshGroup {
            name: OTHERS_GROUP.to_string(),
            items: Vec::new(),
        }];

        self.config.iter().for_each(|(key, value)| {
            // Skip wildcard entries
            if key.contains('*') {
                return;
            }

            let host_entry = db.get_host_values(key).unwrap_or_else(|e| {
                eprintln!("Warning: Failed to get database entry for '{}': {}", key, e);
                HostDatabaseEntry {
                    connection_count: 0,
                    last_used_date: 0,
                }
            });

            if let Some((group_name, item_name)) = key.split_once('/') {
                let group_item = SshGroupItem {
                    name: item_name.to_string(),
                    full_name: key.to_string(),
                    connection_count: host_entry.connection_count,
                    last_used: host_entry.last_used_date,
                    host_config: value.clone(),
                    comment: comments.get(key).cloned(),
                };

                if let Some(group) = groups.iter_mut().find(|g| g.name == group_name) {
                    group.items.push(group_item);
                } else {
                    groups.push(SshGroup {
                        name: group_name.to_string(),
                        items: vec![group_item],
                    });
                }
            } else {
                let group_item = SshGroupItem {
                    name: key.to_string(),
                    full_name: key.to_string(),
                    connection_count: host_entry.connection_count,
                    last_used: host_entry.last_used_date,
                    host_config: value.clone(),
                    comment: comments.get(key).cloned(),
                };
                if let Some(others_group) = groups.first_mut() {
                    others_group.items.push(group_item);
                }
            }
        });

        self.groups = groups.into_iter().filter(|g| !g.items.is_empty()).collect();
        self.groups.sort_by_key(|a| a.name.to_lowercase());
        for group in &mut self.groups {
            group.items.sort_by_key(|a| a.name.to_lowercase());
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
