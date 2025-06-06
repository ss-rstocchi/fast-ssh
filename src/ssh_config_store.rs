use crate::database::FileDatabase;
use anyhow::{format_err, Result};
use ssh_cfg::{SshConfig, SshConfigParser, SshHostConfig};
use std::collections::HashMap;
use std::fmt::Debug;
use std::fs::read_to_string;
use std::path::PathBuf;

trait ConfigComments {
    fn get_comments(&self) -> HashMap<String, String>;
}

impl ConfigComments for SshConfig {
    fn get_comments(&self) -> HashMap<String, String> {
        let mut comments = HashMap::new();

        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let config_path = PathBuf::from(home).join(".ssh").join("config");

        if let Ok(contents) = read_to_string(config_path) {
            let mut current_comment = Vec::new();

            for line in contents.lines() {
                let trimmed = line.trim();

                if trimmed.starts_with('#') {
                    let comment_text = trimmed[1..].trim().to_string();
                    current_comment.push(comment_text);
                } else if trimmed.starts_with("Host ") {
                    let host = trimmed["Host ".len()..].trim().to_string();
                    if !current_comment.is_empty() {
                        comments.insert(host, current_comment.join("\n"));
                        current_comment.clear();
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

#[derive(Debug, Clone)]
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
            name: "Others".to_string(),
            items: Vec::new(),
        }];

        // Map to store groups by name to ensure consistent order
        let mut group_map: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

        self.config.iter().for_each(|(key, value)| {
            let host_entry = db.get_host_values(key).unwrap();

            if key.contains('*') {
                return;
            }

            if key.contains('/') {
                let group_name = key.split('/').next().unwrap();
                let group_key = key.split('/').skip(1).collect::<Vec<&str>>().join("");

                let group = groups.iter_mut().find(|g| g.name == group_name);

                let group_item = SshGroupItem {
                    name: group_key,
                    connection_count: host_entry.connection_count,
                    last_used: host_entry.last_used_date,
                    full_name: key.to_string(),
                    host_config: value.clone(),
                    comment: comments.get(key).cloned(),
                };

                if group.is_none() {
                    // Add this group name to our tracking map if not already there
                    if !group_map.contains_key(group_name) {
                        group_map.insert(group_name.to_string(), groups.len());
                        groups.push(SshGroup {
                            name: group_name.to_string(),
                            items: vec![group_item],
                        });
                    } else {
                        // Group exists but wasn't found by iter_mut, get its index
                        let idx = group_map[group_name];
                        groups[idx].items.push(group_item);
                    }

                    return;
                }

                let group = &mut group.unwrap().items;
                group.push(group_item);

                return;
            }

            groups[0].items.push(SshGroupItem {
                full_name: key.to_string(),
                connection_count: host_entry.connection_count,
                last_used: host_entry.last_used_date,
                name: key.to_string(),
                host_config: value.clone(),
                comment: comments.get(key).cloned(),
            });
        });

        // Instead of reversing (which changes order each time), 
        // sort groups by their first appearance in the config
        self.groups = groups.into_iter().filter(|g| !g.items.is_empty()).collect();

        let mut all_used_items: Vec<SshGroupItem> = self
            .groups
            .iter()
            .flat_map(|g| g.items.iter().filter(|i| i.last_used > 0).cloned())
            .collect();

        all_used_items.sort_by(|a, b| b.last_used.cmp(&a.last_used));

        all_used_items.truncate(20);

        if !all_used_items.is_empty() {
            self.groups.insert(
                0,
                SshGroup {
                    name: "Recents".to_string(),
                    items: all_used_items,
                },
            );
        }
    }
}
