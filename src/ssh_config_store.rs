use crate::database::{FileDatabase, HostDatabaseEntry};
use anyhow::{format_err, Context, Result};
use std::cmp::Reverse;
use std::collections::HashMap;
use std::fmt::Debug;
use std::fs;
use std::path::{Path, PathBuf};

// Constants for group names
pub const RECENTS_GROUP: &str = "Recents";
pub const OTHERS_GROUP: &str = "Others";
const RECENTS_LIMIT: usize = 20;
/// Maximum `Include` nesting, same limit as OpenSSH.
const MAX_INCLUDE_DEPTH: usize = 16;

/// Options of a single host, in file order, with keys as written.
#[derive(Debug, Clone, Default)]
pub struct HostConfig {
    options: Vec<(String, String)>,
}

impl HostConfig {
    /// Value of `key` (case-insensitive). Like OpenSSH, the first value wins.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.options
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.options.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    pub fn insert(&mut self, key: &str, value: &str) {
        self.options.push((key.to_string(), value.to_string()));
    }
}

/// A concrete (non-wildcard) host parsed from the SSH config.
#[derive(Debug)]
struct ParsedHost {
    name: String,
    config: HostConfig,
    comment: Option<String>,
}

/// Lenient SSH config parser: unknown options, global options, `Match`
/// blocks and malformed lines are tolerated instead of failing the whole
/// file, `Host` lines with several patterns yield one host per concrete
/// pattern, and `Include` directives are followed.
struct ConfigParser {
    home: PathBuf,
    hosts: Vec<ParsedHost>,
    index: HashMap<String, usize>,
}

impl ConfigParser {
    fn new(home: &Path) -> ConfigParser {
        ConfigParser {
            home: home.to_path_buf(),
            hosts: Vec::new(),
            index: HashMap::new(),
        }
    }

    fn parse(&mut self, contents: &str, depth: usize) {
        // Hosts the current block applies to; empty for global options and
        // `Match` blocks, whose options are ignored.
        let mut current: Vec<usize> = Vec::new();
        // Full-line comments immediately preceding a `Host` line.
        let mut comment = String::new();

        for line in contents.lines() {
            let trimmed = line.trim();

            if let Some(text) = trimmed.strip_prefix('#') {
                if !comment.is_empty() {
                    comment.push('\n');
                }
                comment.push_str(text.trim());
                continue;
            }

            let Some((key, value)) = split_directive(trimmed) else {
                comment.clear();
                continue;
            };

            if key.eq_ignore_ascii_case("host") {
                let comment = Some(std::mem::take(&mut comment)).filter(|c| !c.is_empty());
                current = value
                    .split_whitespace()
                    .filter(|pattern| is_concrete_host(pattern))
                    .map(|pattern| self.entry(pattern, comment.as_deref()))
                    .collect();
                continue;
            }

            comment.clear();

            if key.eq_ignore_ascii_case("match") {
                current.clear();
            } else if key.eq_ignore_ascii_case("include") {
                if depth < MAX_INCLUDE_DEPTH {
                    for path in self.include_paths(value) {
                        self.parse_file(&path, depth + 1);
                    }
                }
            } else {
                for &idx in &current {
                    self.hosts[idx].config.insert(key, value);
                }
            }
        }
    }

    fn parse_file(&mut self, path: &Path, depth: usize) {
        match fs::read_to_string(path) {
            Ok(contents) => self.parse(&contents, depth),
            // Like OpenSSH, a missing include is not an error
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => eprintln!("Warning: could not read {}: {}", path.display(), e),
        }
    }

    /// Index of the host named `name`, created on first sight. A repeated
    /// `Host` block appends to the existing entry.
    fn entry(&mut self, name: &str, comment: Option<&str>) -> usize {
        if let Some(&idx) = self.index.get(name) {
            let host = &mut self.hosts[idx];
            if host.comment.is_none() {
                host.comment = comment.map(str::to_string);
            }
            return idx;
        }

        let idx = self.hosts.len();
        self.hosts.push(ParsedHost {
            name: name.to_string(),
            config: HostConfig::default(),
            comment: comment.map(str::to_string),
        });
        self.index.insert(name.to_string(), idx);
        idx
    }

    /// Files named by an `Include` value. Relative paths are resolved against
    /// `~/.ssh`; wildcards are supported in the file name.
    fn include_paths(&self, value: &str) -> Vec<PathBuf> {
        let ssh_dir = self.home.join(".ssh");

        value
            .split_whitespace()
            .flat_map(|raw| {
                let path = match raw.strip_prefix("~/") {
                    Some(rest) => self.home.join(rest),
                    None if Path::new(raw).is_absolute() => PathBuf::from(raw),
                    None => ssh_dir.join(raw),
                };
                expand_file_glob(path)
            })
            .collect()
    }
}

/// Splits a config line into `(keyword, arguments)`, dropping trailing
/// comments. Keyword and arguments are separated by whitespace and/or a
/// single `=`, as in OpenSSH; later `=` signs belong to the value.
fn split_directive(line: &str) -> Option<(&str, &str)> {
    let line = strip_trailing_comment(line).trim();
    if line.is_empty() {
        return None;
    }

    let key_end = line
        .find(|c: char| c.is_whitespace() || c == '=')
        .unwrap_or(line.len());
    let (key, rest) = line.split_at(key_end);
    let rest = rest.trim_start();
    let value = rest.strip_prefix('=').unwrap_or(rest).trim_start();

    Some((key, value))
}

/// Cuts the line at a `#` that starts a word.
fn strip_trailing_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'#' && (i == 0 || bytes[i - 1].is_ascii_whitespace()) {
            return &line[..i];
        }
    }
    line
}

/// Whether a `Host` pattern names a single host one can connect to.
fn is_concrete_host(pattern: &str) -> bool {
    !pattern.contains(['*', '?', '!'])
}

/// Expands `*` / `?` in the last path component, sorted like glob(3).
fn expand_file_glob(path: PathBuf) -> Vec<PathBuf> {
    let (Some(parent), Some(pattern)) = (path.parent(), path.file_name().and_then(|n| n.to_str()))
    else {
        return vec![path];
    };

    if !pattern.contains(['*', '?']) {
        return vec![path];
    }

    let Ok(entries) = fs::read_dir(parent) else {
        return Vec::new();
    };

    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry.file_name().to_str().is_some_and(|name| {
                // glob(3) does not let wildcards match a leading dot
                (!name.starts_with('.') || pattern.starts_with('.'))
                    && wildcard_match(pattern.as_bytes(), name.as_bytes())
            })
        })
        .map(|entry| entry.path())
        .collect();
    paths.sort();
    paths
}

fn wildcard_match(pattern: &[u8], name: &[u8]) -> bool {
    match (pattern.first(), name.first()) {
        (None, None) => true,
        (Some(b'*'), _) => {
            wildcard_match(&pattern[1..], name)
                || (!name.is_empty() && wildcard_match(pattern, &name[1..]))
        }
        (Some(b'?'), Some(_)) => wildcard_match(&pattern[1..], &name[1..]),
        (Some(p), Some(n)) if p == n => wildcard_match(&pattern[1..], &name[1..]),
        _ => false,
    }
}

#[derive(Debug, Clone)]
pub struct SshGroupItem {
    pub name: String,
    pub full_name: String,
    pub connection_count: i64,
    pub last_used: i64,
    pub host_config: HostConfig,
    pub comment: Option<String>,
}

#[derive(Debug)]
pub struct SshGroup {
    pub name: String,
    pub items: Vec<SshGroupItem>,
}

#[derive(Debug)]
pub struct SshConfigStore {
    pub groups: Vec<SshGroup>,
}

impl SshConfigStore {
    pub fn new(db: &FileDatabase) -> Result<SshConfigStore> {
        let home =
            dirs::home_dir().ok_or_else(|| format_err!("Could not determine home directory"))?;
        let config_path = home.join(".ssh").join("config");

        let contents = fs::read_to_string(&config_path)
            .with_context(|| format!("Could not read SSH config at {}", config_path.display()))?;

        let mut parser = ConfigParser::new(&home);
        parser.parse(&contents, 0);

        let groups = create_ssh_groups(db, parser.hosts);

        if groups.is_empty() {
            return Err(format_err!("Your configuration file contains no entries (or only wildcards) ! Please add at least one."));
        }

        Ok(SshConfigStore { groups })
    }
}

fn create_ssh_groups(db: &FileDatabase, hosts: Vec<ParsedHost>) -> Vec<SshGroup> {
    let mut groups: Vec<SshGroup> = vec![SshGroup {
        name: OTHERS_GROUP.to_string(),
        items: Vec::new(),
    }];
    let mut group_indices: HashMap<String, usize> = HashMap::new();
    group_indices.insert(OTHERS_GROUP.to_string(), 0);

    for host in hosts {
        let host_entry = db.get_host_values(&host.name).unwrap_or_else(|e| {
            eprintln!(
                "Warning: Failed to get database entry for '{}': {}",
                host.name, e
            );
            HostDatabaseEntry::default()
        });

        let (group_name, item_name) = match host.name.split_once('/') {
            Some((group_name, item_name)) => (group_name, item_name),
            None => (OTHERS_GROUP, host.name.as_str()),
        };

        let group_index = match group_indices.get(group_name) {
            Some(&index) => index,
            None => {
                let index = groups.len();
                group_indices.insert(group_name.to_string(), index);
                groups.push(SshGroup {
                    name: group_name.to_string(),
                    items: Vec::new(),
                });
                index
            }
        };

        let group_item = SshGroupItem {
            name: item_name.to_string(),
            full_name: host.name.clone(),
            connection_count: host_entry.connection_count,
            last_used: host_entry.last_used_date,
            host_config: host.config,
            comment: host.comment,
        };
        groups[group_index].items.push(group_item);
    }

    let mut groups: Vec<SshGroup> = groups.into_iter().filter(|g| !g.items.is_empty()).collect();
    groups.sort_by_cached_key(|a| a.name.to_lowercase());
    for group in &mut groups {
        group.items.sort_by_cached_key(|a| a.name.to_lowercase());
    }

    // Create "Recents" group from used items
    let mut all_used_items: Vec<SshGroupItem> = groups
        .iter()
        .flat_map(|g| g.items.iter().filter(|i| i.last_used > 0).cloned())
        .collect();

    if !all_used_items.is_empty() {
        all_used_items.sort_unstable_by_key(|b| Reverse(b.last_used));
        all_used_items.truncate(RECENTS_LIMIT);

        groups.insert(
            0,
            SshGroup {
                name: RECENTS_GROUP.to_string(),
                items: all_used_items,
            },
        );
    }

    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn parse(contents: &str) -> Vec<ParsedHost> {
        let mut parser = ConfigParser::new(Path::new("/nonexistent"));
        parser.parse(contents, 0);
        parser.hosts
    }

    fn names(hosts: &[ParsedHost]) -> Vec<&str> {
        hosts.iter().map(|h| h.name.as_str()).collect()
    }

    fn temp_home() -> PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let home = std::env::temp_dir().join(format!(
            "fast_ssh_home_{}_{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(home.join(".ssh").join("config.d")).unwrap();
        home
    }

    #[test]
    fn test_split_directive() {
        assert_eq!(split_directive("Host alpha"), Some(("Host", "alpha")));
        assert_eq!(split_directive("host=alpha"), Some(("host", "alpha")));
        assert_eq!(split_directive("Host = alpha"), Some(("Host", "alpha")));
        assert_eq!(
            split_directive("HostName\t192.0.2.1"),
            Some(("HostName", "192.0.2.1"))
        );
        assert_eq!(
            split_directive("Host alpha # trailing"),
            Some(("Host", "alpha"))
        );
        assert_eq!(
            split_directive("ProxyCommand ssh -o Foo=bar jump -W %h:%p"),
            Some(("ProxyCommand", "ssh -o Foo=bar jump -W %h:%p"))
        );
        assert_eq!(split_directive("Host"), Some(("Host", "")));
        assert_eq!(split_directive(""), None);
        assert_eq!(split_directive("# only a comment"), None);
    }

    #[test]
    fn test_parse_tolerates_global_options_and_unknown_keys() {
        let hosts = parse(
            "\
AddKeysToAgent yes
SomeFutureOption on
Host alpha
\tHostName\t192.0.2.1
    ProxyCommand ssh -o Foo=bar jump -W %h:%p
",
        );

        assert_eq!(names(&hosts), vec!["alpha"]);
        let config = &hosts[0].config;
        assert_eq!(config.get("hostname"), Some("192.0.2.1"));
        assert_eq!(
            config.get("ProxyCommand"),
            Some("ssh -o Foo=bar jump -W %h:%p")
        );
        assert_eq!(config.get("AddKeysToAgent"), None);
    }

    #[test]
    fn test_parse_splits_patterns_and_skips_wildcards() {
        let hosts = parse(
            "\
Host gamma delta web-? *.example.com !blocked
    User admin
",
        );

        assert_eq!(names(&hosts), vec!["gamma", "delta"]);
        assert!(hosts.iter().all(|h| h.config.get("user") == Some("admin")));
    }

    #[test]
    fn test_parse_ignores_match_blocks() {
        let hosts = parse(
            "\
Host alpha
    User first
Match host alpha
    User second
",
        );

        assert_eq!(hosts[0].config.get("User"), Some("first"));
        assert_eq!(hosts[0].config.iter().count(), 1);
    }

    #[test]
    fn test_parse_repeated_host_first_value_wins() {
        let hosts = parse(
            "\
Host alpha
    User first
Host alpha
    User second
    Port 2222
",
        );

        assert_eq!(names(&hosts), vec!["alpha"]);
        assert_eq!(hosts[0].config.get("User"), Some("first"));
        assert_eq!(hosts[0].config.get("Port"), Some("2222"));
    }

    #[test]
    fn test_parse_comments() {
        let hosts = parse(
            "\
# first host
# second line
Host alpha
    HostName 192.0.2.1

# lowercase directive
host beta

# multi pattern
Host gamma delta

# stale comment
Include does-not-exist
Host epsilon
",
        );

        let comment = |name: &str| {
            hosts
                .iter()
                .find(|h| h.name == name)
                .and_then(|h| h.comment.as_deref())
        };
        assert_eq!(comment("alpha"), Some("first host\nsecond line"));
        assert_eq!(comment("beta"), Some("lowercase directive"));
        assert_eq!(comment("gamma"), Some("multi pattern"));
        assert_eq!(comment("delta"), Some("multi pattern"));
        assert_eq!(comment("epsilon"), None);
    }

    #[test]
    fn test_parse_follows_includes() {
        let home = temp_home();
        let ssh_dir = home.join(".ssh");
        fs::write(ssh_dir.join("config.d").join("b.conf"), "Host bravo\n").unwrap();
        fs::write(ssh_dir.join("config.d").join("a.conf"), "Host alpha\n").unwrap();
        fs::write(ssh_dir.join("config.d").join(".hidden"), "Host hidden\n").unwrap();
        fs::write(ssh_dir.join("extra"), "Host extra\n").unwrap();
        // Self-include must not recurse forever
        fs::write(ssh_dir.join("loop"), "Include loop\nHost looped\n").unwrap();

        let mut parser = ConfigParser::new(&home);
        parser.parse(
            "Include config.d/*\nInclude ~/.ssh/extra missing\nInclude loop\nHost main\n",
            0,
        );

        assert_eq!(
            names(&parser.hosts),
            vec!["alpha", "bravo", "extra", "looped", "main"]
        );

        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn test_wildcard_match() {
        assert!(wildcard_match(b"*", b"anything"));
        assert!(wildcard_match(b"*.conf", b"a.conf"));
        assert!(wildcard_match(b"a?c", b"abc"));
        assert!(!wildcard_match(b"*.conf", b"a.cfg"));
        assert!(!wildcard_match(b"a?c", b"ac"));
    }
}
