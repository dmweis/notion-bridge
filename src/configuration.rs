use anyhow::{Context, Result};
use config::Config;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const PROJECT_QUALIFIER: &str = "com";
const PROJECT_ORGANIZATION: &str = "dmweis";
const PROJECT_APPLICATION_NAME: &str = "notion_bridge";

const CONFIG_FILE_NAME: &str = "config";
const CONFIG_FILE_EXTENSION: &str = "yaml";

pub fn get_project_dirs() -> Result<ProjectDirs> {
    ProjectDirs::from(
        PROJECT_QUALIFIER,
        PROJECT_ORGANIZATION,
        PROJECT_APPLICATION_NAME,
    )
    .context("failed to establish project dirs")
}

fn get_config_file_path() -> Result<PathBuf> {
    let proj_dirs = get_project_dirs()?;
    let config_dir_path = proj_dirs.config_dir();
    Ok(config_dir_path.join(CONFIG_FILE_NAME))
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct AppConfig {
    pub notion_api_key: String,
}

impl AppConfig {
    pub fn new(notion_api_key: String) -> Self {
        Self { notion_api_key }
    }

    pub fn load_user_config() -> anyhow::Result<Self> {
        let config_file_path = get_config_file_path()?;
        let settings = Config::builder()
            .add_source(config::File::from(config_file_path))
            .add_source(config::Environment::with_prefix("CHATTY"))
            .build()?;

        Ok(settings.try_deserialize::<AppConfig>()?)
    }

    pub fn save_user_config(&self) -> anyhow::Result<()> {
        let config_file_path = get_config_file_path()?.with_extension(CONFIG_FILE_EXTENSION);

        std::fs::create_dir_all(
            config_file_path
                .parent()
                .context("failed to get config file parent directory")?,
        )?;

        let file = std::fs::File::create(config_file_path)?;
        serde_yaml::to_writer(file, self)?;
        Ok(())
    }
}
