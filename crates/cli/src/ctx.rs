use anyhow::Result;
use middlewarejson_core::config::Settings;
use middlewarejson_core::db::{CatalogRepository, Database};
use middlewarejson_core::services::panel_api::{
    resolve_panel_base_url, resolve_panel_token, resolve_panel_web_base_path,
    PANEL_API_BASE_URL_KEY, PANEL_API_TOKEN_KEY, PANEL_WEB_BASE_PATH_KEY,
};

pub struct AppCtx {
    pub settings: Settings,
}

impl AppCtx {
    pub fn new() -> Self {
        Self {
            settings: Settings::from_env(),
        }
    }

    pub fn repo(&self) -> Result<CatalogRepository> {
        CatalogRepository::new(Database::new(&self.settings.db_path))
    }

    pub fn resolved_panel_settings(
        &self,
        repo: &CatalogRepository,
    ) -> Result<(String, String, String)> {
        let base_url = resolve_panel_base_url(
            &self.settings,
            repo.get_setting(PANEL_API_BASE_URL_KEY)?.as_deref(),
        );
        let web_path = resolve_panel_web_base_path(
            &self.settings,
            repo.get_setting(PANEL_WEB_BASE_PATH_KEY)?.as_deref(),
        );
        let token = resolve_panel_token(
            &self.settings,
            repo.get_setting(PANEL_API_TOKEN_KEY)?.as_deref(),
        );
        Ok((base_url, web_path, token))
    }
}