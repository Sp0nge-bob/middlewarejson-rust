use regex::Regex;

use crate::db::repository::CatalogRepository;
use crate::models::rules_schema::{
    BalancerMember, BalancerRule, OutputConfig, OutputFormat, TransformRules,
};

pub fn build_balancer_rules(
    repository: &CatalogRepository,
    balancer_tags: &[String],
) -> anyhow::Result<Option<TransformRules>> {
    let mut balancers = Vec::new();

    for balancer_tag in balancer_tags {
        let Some(balancer) = repository.get_balancer_by_tag(balancer_tag)? else {
            continue;
        };
        if balancer.member_fingerprints.is_empty() {
            continue;
        }
        balancers.push(BalancerRule {
            tag: balancer.tag.clone(),
            remarks: if balancer.remarks.is_empty() {
                balancer.tag.clone()
            } else {
                balancer.remarks.clone()
            },
            strategy: balancer.strategy.clone(),
            members: vec![BalancerMember {
                tags: Vec::new(),
                inbound_ids: balancer.member_fingerprints.clone(),
                r#match: None,
            }],
        });
    }

    if balancers.is_empty() {
        return Ok(None);
    }

    Ok(Some(TransformRules {
        output: OutputConfig {
            format: OutputFormat::Grouped,
            remarks: "ExamplePool".to_string(),
            default_balancer: String::new(),
        },
        filters: Default::default(),
        tagging: Default::default(),
        balancers,
    }))
}

pub fn default_balancer_tag(name: &str) -> String {
    slugify_tag(name)
}

fn slugify_tag(value: &str) -> String {
    let re_non_word = Regex::new(r"[^\w\s-]").expect("valid regex");
    let re_spaces = Regex::new(r"[\s_]+").expect("valid regex");
    let stripped = re_non_word.replace_all(value.trim(), "");
    let cleaned = re_spaces.replace_all(&stripped, "-");
    let cleaned = cleaned.trim_matches('-').to_lowercase();
    if cleaned.is_empty() {
        "balancer".to_string()
    } else {
        cleaned
    }
}