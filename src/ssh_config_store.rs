use crate::database::{FileDatabase, HostDatabaseEntry};
use anyhow::{format_err, Result};
use ssh_cfg::{SshConfig, SshConfigParser, SshHostConfig};
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

        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
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

            let group_item = SshGroupItem {
                connection_count: host_entry.connection_count,
                last_used: host_entry.last_used_date,
                full_name: key.to_string(),
                host_config: value.clone(),
                comment: comments.get(key).cloned(),
                name: String::new(), // Temporary, will be set below
            };

            if let Some(slash_pos) = key.find('/') {
                let (group_name, item_name) = key.split_at(slash_pos);
                let item_name = &item_name[1..]; // Skip the '/'

                let mut group_item = group_item;
                group_item.name = item_name.to_string();

                // Find or create the group
                if let Some(group) = groups.iter_mut().find(|g| g.name == group_name) {
                    group.items.push(group_item);
                } else {
                    groups.push(SshGroup {
                        name: group_name.to_string(),
                        items: vec![group_item],
                    });
                }
            } else {
                // Add to "Others" group (first in vec)
                let mut group_item = group_item;
                group_item.name = key.to_string();
                // Safe: "Others" group is always initialized at position 0
                if let Some(others_group) = groups.first_mut() {
                    others_group.items.push(group_item);
                }
            }
        });

        groups.reverse();
        self.groups = groups.into_iter().filter(|g| !g.items.is_empty()).collect();

        // Create "Recents" group from used items
        let mut all_used_items: Vec<SshGroupItem> = self
            .groups
            .iter()
            .flat_map(|g| g.items.iter().filter(|i| i.last_used > 0).cloned())
            .collect();

        if !all_used_items.is_empty() {
            all_used_items.sort_unstable_by(|a, b| b.last_used.cmp(&a.last_used));
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
