use tracing::{info, warn};

use crate::config::Settings;
use crate::db::{CatalogRepository, Database};
use crate::models::subscription::SubscriptionPayload;
use crate::services::profile_builder::build_balancer_rules;
use crate::services::transformer::{PassthroughTransformer, SubscriptionTransformer};
use crate::transform::rules_engine::RulesTransformer;

pub struct TransformService {
    settings: Settings,
    passthrough: PassthroughTransformer,
    repository: CatalogRepository,
}

impl TransformService {
    pub fn new(settings: Settings) -> anyhow::Result<Self> {
        let repository = CatalogRepository::new(Database::new(&settings.db_path))?;
        Ok(Self {
            settings,
            passthrough: PassthroughTransformer,
            repository,
        })
    }

    pub fn transform(&self, sub_id: &str, payload: &SubscriptionPayload) -> anyhow::Result<SubscriptionPayload> {
        let mode = self.settings.transform_mode.trim().to_lowercase();
        if mode != "rules" {
            info!(
                "transform skipped for sub_id={sub_id}: TRANSFORM_MODE={} (need rules)",
                self.settings.transform_mode
            );
            return Ok(self.passthrough.transform(payload));
        }

        let balancer_tags = self.repository.get_balancer_tags_for_sub_id(sub_id)?;
        if balancer_tags.is_empty() {
            let group_name = self.repository.get_group_for_sub_id(sub_id)?;
            info!(
                "transform skipped for sub_id={sub_id}: no balancers (client group={})",
                group_name.as_deref().unwrap_or("not in index")
            );
            return Ok(self.passthrough.transform(payload));
        }

        let db_rules = build_balancer_rules(&self.repository, &balancer_tags)?;
        let Some(db_rules) = db_rules else {
            warn!(
                "balancers {:?} are missing or empty, passthrough for sub_id={sub_id}",
                balancer_tags
            );
            return Ok(self.passthrough.transform(payload));
        };

        info!(
            "transform rules for sub_id={sub_id}: balancers={balancer_tags:?}"
        );
        Ok(RulesTransformer::new(db_rules).transform(payload))
    }
}