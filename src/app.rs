use anyhow::{format_err, Context, Result};
use std::fs;
use tui::widgets::TableState;

use crate::{
    database::FileDatabase,
    searcher::Searcher,
    ssh_config_store::{SshConfigStore, SshGroup, SshGroupItem},
};

// Default number of items to scroll when using half-page navigation
const DEFAULT_HALF_PAGE_SIZE: usize = 10;

pub enum ConfigDisplayMode {
    Global,
    Selected,
}

pub enum AppState {
    Searching,
    Normal,
}

pub struct App {
    pub state: AppState,
    pub searcher: Searcher,
    pub selected_group: usize,
    pub host_state: TableState,
    pub scs: SshConfigStore,
    pub config_display_mode: ConfigDisplayMode,
    pub should_quit: bool,
    pub should_spawn_ssh: bool,
    pub should_copy_ssh_key: bool,
    pub should_copy_files: bool,

    pub config_paragraph_offset: u16,
    pub db: FileDatabase,
    pub show_help: bool,
    pub pending_g: bool, // Track if 'g' was just pressed for 'gg' detection
}

impl App {
    pub async fn new() -> Result<App> {
        let db = App::create_or_get_db_file()?;
        let scs = SshConfigStore::new(&db).await?;

        Ok(App {
            state: AppState::Normal,
            selected_group: 0,
            config_paragraph_offset: 0,
            scs,
            host_state: TableState::default(),
            should_quit: false,
            should_spawn_ssh: false,
            should_copy_ssh_key: false,
            should_copy_files: false,
            config_display_mode: ConfigDisplayMode::Selected,
            db,
            searcher: Searcher::new(),
            show_help: false,
            pending_g: false,
        })
    }

    pub fn create_or_get_db_file() -> Result<FileDatabase> {
        let config_dir =
            dirs::config_dir().ok_or_else(|| format_err!("Could not get config directory"))?;

        let conf_path = config_dir.join("FastSSH");
        let db_path = conf_path.join("db.ron");

        fs::create_dir_all(&conf_path)
            .with_context(|| format_err!("Could not create the config directory"))?;

        let db_path_str = db_path
            .to_str()
            .ok_or_else(|| format_err!("Database path contains invalid UTF-8"))?;

        FileDatabase::new(db_path_str)
    }

    #[inline]
    pub fn get_selected_group(&self) -> Option<&SshGroup> {
        self.scs.groups.get(self.selected_group)
    }

    #[inline]
    pub fn get_selected_item(&self) -> Option<&SshGroupItem> {
        let items = self.get_items_based_on_mode();
        self.host_state.selected().and_then(|idx| items.get(idx).copied())
    }

    #[inline]
    pub fn get_all_items(&self) -> Vec<&SshGroupItem> {
        self.scs
            .groups
            .iter()
            .flat_map(|group| &group.items)
            .collect::<Vec<&SshGroupItem>>()
    }

    #[inline]
    pub fn get_all_items_except_recents(&self) -> Vec<&SshGroupItem> {
        self.scs
            .groups
            .iter()
            .filter(|group| group.name != "Recents")
            .flat_map(|group| &group.items)
            .collect::<Vec<&SshGroupItem>>()
    }

    pub fn get_items_based_on_mode(&self) -> Vec<&SshGroupItem> {
        let items: Vec<&SshGroupItem> = match self.state {
            AppState::Normal => {
                // Safely get selected group, return empty if out of bounds
                let Some(selected_group) = self.get_selected_group() else {
                    return Vec::new();
                };

                let mut group_items = selected_group
                    .items
                    .iter()
                    .collect::<Vec<&SshGroupItem>>();

                if selected_group.name != "Recents" {
                    group_items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                }

                group_items
            }
            AppState::Searching => self.searcher.get_filtered_items(self),
        };

        items
    }

    #[inline]
    pub fn change_selected_group(&mut self, rot_right: bool) {
        let actual_idx = self.selected_group;
        let items_len = self.scs.groups.len();

        self.selected_group = match rot_right {
            true => (actual_idx + 1) % items_len,
            false => (actual_idx + items_len - 1) % items_len,
        };
    }

    #[inline]
    pub fn change_selected_item(&mut self, rot_right: bool) {
        let items_len = self.get_items_based_on_mode().len();

        if items_len == 0 {
            return;
        }

        let i = match self.host_state.selected() {
            Some(i) => {
                if rot_right {
                    (i + 1) % items_len
                } else {
                    (i + items_len - 1) % items_len
                }
            }
            None => 0,
        };
        self.host_state.select(Some(i));
    }

    #[inline]
    pub fn select_recents_group(&mut self) {
        if let Some(first_group) = self.scs.groups.first() {
            if first_group.name == "Recents" {
                self.selected_group = 0;
                self.host_state.select(Some(0));
            }
        }
    }

    #[inline]
    pub fn scroll_config_paragraph(&mut self, offset: i64) {
        let new_offset = (self.config_paragraph_offset as i64 + offset).max(0);
        self.config_paragraph_offset = new_offset.min(u16::MAX as i64) as u16;
    }

    #[inline]
    pub fn toggle_config_display_mode(&mut self) {
        self.config_display_mode = match self.config_display_mode {
            ConfigDisplayMode::Global => ConfigDisplayMode::Selected,
            ConfigDisplayMode::Selected => ConfigDisplayMode::Global,
        };
    }

    #[inline]
    pub fn jump_to_first_item(&mut self) {
        let items_len = self.get_items_based_on_mode().len();
        if items_len > 0 {
            self.host_state.select(Some(0));
        }
    }

    #[inline]
    pub fn jump_to_last_item(&mut self) {
        let items_len = self.get_items_based_on_mode().len();
        if items_len > 0 {
            self.host_state.select(Some(items_len - 1));
        }
    }

    #[inline]
    pub fn scroll_half_page(&mut self, down: bool) {
        let items_len = self.get_items_based_on_mode().len();
        if items_len == 0 {
            return;
        }

        // Use a reasonable half-page size (DEFAULT_HALF_PAGE_SIZE items)
        let half_page = DEFAULT_HALF_PAGE_SIZE.min(items_len / 2).max(1);
        
        let current = self.host_state.selected().unwrap_or(0);
        let new_pos = if down {
            (current + half_page).min(items_len - 1)
        } else {
            current.saturating_sub(half_page)
        };
        
        self.host_state.select(Some(new_pos));
    }
}
