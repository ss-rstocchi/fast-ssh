use anyhow::{format_err, Context, Result};
use std::fs;
use tui::widgets::TableState;

use crate::{
    config::resolve_config,
    database::FileDatabase,
    searcher::Searcher,
    ssh_config_store::{SshConfigStore, SshGroup, SshGroupItem},
};

pub enum ConfigDisplayMode {
    Global,
    Selected,
}

#[derive(PartialEq)]
pub enum AppState {
    Searching,
    Normal,
    Editing,
}

pub struct FilteredGroup {
    pub original_index: usize,
    pub name: String,
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
    pub should_save_config: bool,

    pub config_paragraph_offset: u16,
    pub db: FileDatabase,
    pub show_help: bool,
}

impl App {
    pub async fn new() -> Result<App> {
        let db = App::create_or_get_db_file()?;
        let scs = SshConfigStore::new(&db).await?;
        resolve_config();

        let initial_group = if scs.groups.len() > 1 && scs.groups[0].name == "Recents" {
            1
        } else {
            0
        };

        Ok(App {
            state: AppState::Normal,
            selected_group: initial_group,
            config_paragraph_offset: 0,
            scs,
            host_state: TableState::default(),
            should_quit: false,
            should_spawn_ssh: false,
            should_copy_ssh_key: false,
            should_copy_files: false,
            should_save_config: false,
            config_display_mode: ConfigDisplayMode::Selected,
            db,
            searcher: Searcher::new(),
            show_help: false,
        })
    }

    pub fn create_or_get_db_file() -> Result<FileDatabase> {
        let config_dir =
            dirs::config_dir().ok_or_else(|| format_err!("Could not get config directory"))?;

        let conf_path = config_dir.join("FastSSH");
        let db_path = conf_path.join("db.ron");

        fs::create_dir_all(&conf_path)
            .with_context(|| format_err!("Could not create the config directory"))?;

        FileDatabase::new(db_path.to_str().unwrap())
    }

    pub fn get_selected_group(&self) -> &SshGroup {
        &self.scs.groups[self.selected_group]
    }
    
    pub fn get_filtered_groups(&self) -> Vec<FilteredGroup> {
        self.scs.groups.iter().enumerate()
            .filter(|(_, group)| {
                if matches!(self.state, AppState::Editing) {
                    group.name != "Recents"
                } else {
                    true
                }
            })
            .map(|(idx, group)| FilteredGroup {
                original_index: idx,
                name: group.name.clone(),
            })
            .collect()
    }

    pub fn get_visible_group_index(&self, original_index: usize) -> Option<usize> {
        self.get_filtered_groups().iter().position(|fg| fg.original_index == original_index)
    }

    pub fn get_original_group_index(&self, visible_index: usize) -> Option<usize> {
        self.get_filtered_groups().get(visible_index).map(|fg| fg.original_index)
    }

    pub fn get_selected_item(&self) -> Option<&SshGroupItem> {
        if let Some(host_state) = self.host_state.selected() {
            let items_len = self.get_items_based_on_mode().len();
            if host_state < items_len {
                Some(self.get_items_based_on_mode()[host_state])
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn get_all_items(&self) -> Vec<&SshGroupItem> {
        self.scs
            .groups
            .iter()
            .flat_map(|group| &group.items)
            .collect::<Vec<&SshGroupItem>>()
    }

    pub fn get_items_based_on_mode(&self) -> Vec<&SshGroupItem> {
        match self.state {
            AppState::Normal => self
                .get_selected_group()
                .items
                .iter()
                .collect::<Vec<&SshGroupItem>>(),
            AppState::Searching => self.searcher.get_filtered_items(self),
            AppState::Editing => self
                .get_selected_group()
                .items
                .iter()
                .collect::<Vec<&SshGroupItem>>(),
        }
    }

    pub fn change_selected_group(&mut self, rot_right: bool) {
        let actual_idx = self.selected_group;
        let items_len = self.scs.groups.len();

        self.selected_group = match rot_right {
            true => (actual_idx + 1) % items_len,
            false => (actual_idx + items_len - 1) % items_len,
        };
    }

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

    pub fn select_recents_group(&mut self) {
        if !self.scs.groups.is_empty() && self.scs.groups[0].name == "Recents" {
            self.selected_group = 0;
            self.host_state.select(Some(0));
        }
    }

    pub fn scroll_config_paragraph(&mut self, offset: i64) {
        self.config_paragraph_offset = (self.config_paragraph_offset as i64 + offset).max(0) as u16;
    }

    pub fn toggle_config_display_mode(&mut self) {
        self.config_display_mode = match self.config_display_mode {
            ConfigDisplayMode::Global => ConfigDisplayMode::Selected,
            ConfigDisplayMode::Selected => ConfigDisplayMode::Global,
        };
    }
}
