use std::process::Command;

use anyhow::Result;
use serde::{Deserialize, Serialize};

pub const APPNAME: &str = "adoboards";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct BoardConfig {
    pub organization: String,
    pub project: String,
    pub team: String,
}

impl Default for BoardConfig {
    fn default() -> Self {
        BoardConfig {
            organization: "<organization>".to_string(),
            project: "<project>".to_string(),
            team: "<team>".to_string(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct IterationConfig {
    pub organization: String,
    pub project: String,
    pub team: String,
    pub iteration: String,
}

impl Default for IterationConfig {
    fn default() -> Self {
        IterationConfig {
            organization: "<organization>".to_string(),
            project: "<project>".to_string(),
            team: "<team>".to_string(),
            iteration: "<iteration path>".to_string(),
        }
    }
}

#[derive(Default, Clone, Debug, Deserialize, Serialize)]
pub struct CommonConfig {
    pub me: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct KeysConfig {
    pub quit: String,
    pub next: String,
    pub previous: String,
    pub hover: String,
    pub open: String,
    pub next_board: String,
    pub previous_board: String,
    pub search: String,
    pub assigned_to_me_filter: String,
    pub work_item_type_filter: String,
    pub jump_to_top: String,
    pub jump_to_end: String,
    pub refresh: String,
    pub edit_config: String,
    pub edit_item: String,
}

impl Default for KeysConfig {
    fn default() -> Self {
        KeysConfig {
            quit: "q".to_string(),
            next: "j".to_string(),
            previous: "k".to_string(),
            hover: "K".to_string(),
            open: "o".to_string(),
            next_board: ">".to_string(),
            previous_board: "<".to_string(),
            search: "/".to_string(),
            assigned_to_me_filter: "m".to_string(),
            work_item_type_filter: "t".to_string(),
            jump_to_top: "gg".to_string(),
            jump_to_end: "G".to_string(),
            refresh: "r".to_string(),
            edit_config: "c".to_string(),
            edit_item: "e".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AppConfig {
    #[serde(default)]
    pub common: CommonConfig,
    #[serde(default)]
    pub boards: Vec<BoardConfig>,
    #[serde(default)]
    pub iterations: Vec<IterationConfig>,
    #[serde(default)]
    pub keys: KeysConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            common: CommonConfig { me: "".to_string() },
            boards: vec![BoardConfig::default()],
            iterations: Vec::new(),
            keys: KeysConfig::default(),
        }
    }
}

pub fn open_config() -> Result<()> {
    let file_path = confy::get_configuration_file_path(APPNAME, None)?;
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| {
        if cfg!(target_os = "windows") {
            "notepad".to_string()
        } else {
            "vi".to_string()
        }
    });

    println!(
        "Opening configuration file in {}: {}",
        editor,
        file_path.display()
    );

    let status = Command::new(&editor).arg(file_path).status()?;
    if !status.success() {
        anyhow::bail!("Failed to open editor: {}", status);
    }
    Ok(())
}

pub fn load_config_or_prompt() -> (AppConfig, bool) {
    let cfg: AppConfig = match confy::load(APPNAME, None) {
        Ok(conf) => conf,
        Err(e) => {
            eprintln!("Error loading configuration: {}", e);
            AppConfig::default()
        }
    };

    let default_board = BoardConfig::default();
    let default_iteration = IterationConfig::default();

    let boards_ok = match cfg.boards.as_slice() {
        [] => false,
        [item] if item == &default_board => false,
        _ => true,
    };

    let iterations_ok = match cfg.iterations.as_slice() {
        [] => false,
        [item] if item == &default_iteration => false,
        _ => true,
    };

    let config_ok = boards_ok || iterations_ok;

    if !config_ok {
        let _ = open_config();
        eprintln!("Reopen {}", APPNAME);
    }

    (cfg, config_ok)
}
