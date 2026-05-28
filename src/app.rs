use anyhow::{format_err, Context, Result};
use std::fs;
use tui::widgets::TableState;

use crate::{
    database::FileDatabase,
    searcher::Searcher,
    ssh_config_store::{SshConfigStore, SshGroup, SshGroupItem},
};

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
    pub hosts_area_height: u16,
    pub db: FileDatabase,
    pub show_help: bool,
    pub pending_g: bool,
}

impl App {
    pub async fn new() -> Result<App> {
        let db = App::create_or_get_db_file()?;
        let scs = SshConfigStore::new(&db).await?;

        Ok(App {
            state: AppState::Normal,
            selected_group: 0,
            config_paragraph_offset: 0,
            hosts_area_height: 0,
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
            .filter(|group| group.name != crate::ssh_config_store::RECENTS_GROUP)
            .flat_map(|group| &group.items)
            .collect()
    }

    pub fn get_items_based_on_mode(&self) -> Vec<&SshGroupItem> {
        match self.state {
            AppState::Normal => {
                let Some(selected_group) = self.get_selected_group() else {
                    return Vec::new();
                };
                selected_group.items.iter().collect()
            }
            AppState::Searching => self.searcher.get_filtered_items(self),
        }
    }

    pub fn clamp_host_selection(&mut self) {
        let items_len = self.get_items_based_on_mode().len();
        if items_len > 0 && self.host_state.selected().unwrap_or(0) >= items_len {
            self.host_state.select(Some(0));
        }
    }

    #[inline]
    pub fn reset_config_scroll(&mut self) {
        self.config_paragraph_offset = 0;
    }

    #[inline]
    pub fn change_selected_group(&mut self, rot_right: bool) {
        let items_len = self.scs.groups.len();
        
        // Guard against empty groups (should never happen in practice due to validation in new())
        if items_len == 0 {
            return;
        }

        let actual_idx = self.selected_group;
        self.selected_group = match rot_right {
            true => (actual_idx + 1) % items_len,
            false => (actual_idx + items_len - 1) % items_len,
        };
        self.reset_config_scroll();
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
        self.reset_config_scroll();
    }

    #[inline]
    pub fn select_recents_group(&mut self) {
        if let Some(first_group) = self.scs.groups.first() {
            if first_group.name == "Recents" {
                self.selected_group = 0;
                self.host_state.select(Some(0));
                self.reset_config_scroll();
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
            self.reset_config_scroll();
        }
    }

    #[inline]
    pub fn jump_to_last_item(&mut self) {
        let items_len = self.get_items_based_on_mode().len();
        if items_len > 0 {
            self.host_state.select(Some(items_len - 1));
            self.reset_config_scroll();
        }
    }

    #[inline]
    pub fn scroll_half_page(&mut self, down: bool) {
        let items_len = self.get_items_based_on_mode().len();

        if items_len == 0 {
            return;
        }

        // Derive half-page from actual visible rows: each row takes 2 terminal lines,
        // with 4 lines overhead (block border + header). Fall back to DEFAULT_HALF_PAGE_SIZE.
        let half_page = if self.hosts_area_height > 4 {
            ((self.hosts_area_height as usize - 4) / 2 / 2).max(1)
        } else {
            DEFAULT_HALF_PAGE_SIZE
        }
        .min(items_len);

        let current = self.host_state.selected().unwrap_or(0);
        let new_pos = if down {
            (current + half_page).min(items_len - 1)
        } else {
            current.saturating_sub(half_page)
        };

        self.host_state.select(Some(new_pos));
        self.reset_config_scroll();
    }
}

