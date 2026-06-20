use std::collections::{HashMap, HashSet};

use regex::Regex;
use serde_json::{json, Map, Value};

use crate::models::nodes::{configs_to_nodes, ProxyNode};
use crate::models::rules_schema::{BalancerRule, OutputFormat, TaggingRule, TransformRules};
use crate::models::subscription::SubscriptionPayload;
use crate::services::matching::node_matches;
use crate::services::transformer::SubscriptionTransformer;

const FAILED_SUFFIX: &str = " - Failed";

pub struct RulesTransformer {
    rules: TransformRules,
}

impl RulesTransformer {
    pub fn new(rules: TransformRules) -> Self {
        Self { rules }
    }
}

impl SubscriptionTransformer for RulesTransformer {
    fn transform(&self, payload: &SubscriptionPayload) -> SubscriptionPayload {
        if self.rules.output.format == OutputFormat::Passthrough {
            return payload.clone();
        }

        let configs = as_config_list(payload);
        if configs.is_empty() {
            return payload.clone();
        }

        let mut nodes = configs_to_nodes(&configs);
        nodes = filter_nodes(&self.rules, nodes);
        nodes = apply_tagging(&self.rules, nodes);

        if nodes.is_empty() {
            return payload.clone();
        }

        match self.rules.output.format {
            OutputFormat::Grouped => Value::Array(build_grouped_output(&configs, &nodes, &self.rules)),
            OutputFormat::Array => Value::Array(
                nodes
                    .iter()
                    .map(|node| {
                        build_balancer_config(
                            &configs[node.source_index],
                            std::slice::from_ref(node),
                            self.rules.balancers.first().expect("balancer required"),
                            None,
                        )
                    })
                    .collect(),
            ),
            OutputFormat::Single | OutputFormat::Passthrough => {
                build_single_merged_config(&configs[0], &nodes, &self.rules)
            }
        }
    }
}

fn as_config_list(payload: &SubscriptionPayload) -> Vec<Value> {
    match payload {
        Value::Array(items) => items.clone(),
        other => vec![other.clone()],
    }
}

fn apply_tagging(rules: &TransformRules, nodes: Vec<ProxyNode>) -> Vec<ProxyNode> {
    let mut tagged = Vec::new();
    let mut used_tags = HashSet::new();
    for node in nodes {
        let tag = resolve_tag(
            &rules.tagging.rules,
            &node,
            &used_tags,
            &rules.tagging.default_template,
        );
        used_tags.insert(tag.clone());
        tagged.push(node.with_tag(&tag));
    }
    tagged
}

fn resolve_tag(
    tagging_rules: &[TaggingRule],
    node: &ProxyNode,
    used_tags: &HashSet<String>,
    default_template: &str,
) -> String {
    for rule in tagging_rules {
        if node_matches(node, &rule.r#match) {
            return ensure_unique(&rule.tag, used_tags);
        }
    }

    if !node.fingerprint.is_empty() {
        return ensure_unique(&node.fingerprint, used_tags);
    }

    let base = default_template
        .replace("{index}", &node.source_index.to_string())
        .replace("{remarks}", &node.remarks)
        .replace("{protocol}", &node.protocol)
        .replace("{network}", if node.network.is_empty() { "default" } else { &node.network })
        .replace("{slug}", &slugify(&node.remarks));
    ensure_unique(&base, used_tags)
}

fn ensure_unique(tag: &str, used_tags: &HashSet<String>) -> String {
    if !used_tags.contains(tag) {
        return tag.to_string();
    }
    let mut counter = 2;
    while used_tags.contains(&format!("{tag}-{counter}")) {
        counter += 1;
    }
    format!("{tag}-{counter}")
}

fn slugify(value: &str) -> String {
    let re_non_word = Regex::new(r"[^\w\s-]").expect("valid regex");
    let re_spaces = Regex::new(r"[\s_]+").expect("valid regex");
    let stripped = re_non_word.replace_all(value.trim(), "");
    let cleaned = re_spaces.replace_all(&stripped, "-");
    let cleaned = cleaned.trim_matches('-').to_lowercase();
    if cleaned.is_empty() {
        "node".to_string()
    } else {
        cleaned
    }
}

fn filter_nodes(rules: &TransformRules, nodes: Vec<ProxyNode>) -> Vec<ProxyNode> {
    if rules.filters.exclude.is_empty() {
        return nodes;
    }
    nodes
        .into_iter()
        .filter(|node| {
            !rules
                .filters
                .exclude
                .iter()
                .any(|exclude_rule| node_matches(node, exclude_rule))
        })
        .collect()
}

fn resolve_balancer_members(balancer: &BalancerRule, nodes: &[ProxyNode]) -> Vec<String> {
    let mut tags = Vec::new();
    let mut tag_set = HashSet::new();
    let nodes_by_tag: HashMap<&str, &ProxyNode> =
        nodes.iter().map(|node| (node.tag.as_str(), node)).collect();
    let nodes_by_fingerprint: HashMap<&str, &ProxyNode> = nodes
        .iter()
        .filter(|node| !node.fingerprint.is_empty())
        .map(|node| (node.fingerprint.as_str(), node))
        .collect();

    for member in &balancer.members {
        for inbound_id in &member.inbound_ids {
            if let Some(node) = nodes_by_fingerprint.get(inbound_id.as_str()) {
                if tag_set.insert(node.tag.clone()) {
                    tags.push(node.tag.clone());
                }
            }
        }

        for explicit_tag in &member.tags {
            if nodes_by_tag.contains_key(explicit_tag.as_str()) && tag_set.insert(explicit_tag.clone()) {
                tags.push(explicit_tag.clone());
            }
        }

        if let Some(rule) = &member.r#match {
            for node in nodes {
                if node_matches(node, rule) && tag_set.insert(node.tag.clone()) {
                    tags.push(node.tag.clone());
                }
            }
        }
    }

    tags
}

fn expected_balancer_fingerprints(balancer: &BalancerRule) -> Vec<String> {
    let mut fingerprints = Vec::new();
    let mut seen = HashSet::new();
    for member in &balancer.members {
        for inbound_id in &member.inbound_ids {
            if seen.insert(inbound_id.clone()) {
                fingerprints.push(inbound_id.clone());
            }
        }
    }
    fingerprints
}

fn balancer_members_incomplete(balancer: &BalancerRule, nodes: &[ProxyNode]) -> bool {
    let expected = expected_balancer_fingerprints(balancer);
    if expected.is_empty() {
        return false;
    }
    let nodes_by_fingerprint: HashSet<&str> = nodes
        .iter()
        .filter(|node| !node.fingerprint.is_empty())
        .map(|node| node.fingerprint.as_str())
        .collect();
    expected
        .iter()
        .any(|fingerprint| !nodes_by_fingerprint.contains(fingerprint.as_str()))
}

fn balancer_display_remarks(balancer: &BalancerRule, all_nodes: &[ProxyNode]) -> String {
    let mut remarks = if balancer.remarks.is_empty() {
        balancer.tag.clone()
    } else {
        balancer.remarks.clone()
    };
    if balancer_members_incomplete(balancer, all_nodes) && !remarks.ends_with(FAILED_SUFFIX) {
        remarks.push_str(FAILED_SUFFIX);
    }
    remarks
}

fn system_outbounds(template_config: &Value) -> Vec<Value> {
    let Some(outbounds) = template_config.get("outbounds").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    outbounds
        .iter()
        .filter_map(|outbound| {
            let protocol = outbound
                .get("protocol")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let tag = outbound.get("tag").and_then(|v| v.as_str()).unwrap_or("");
            if protocol == "freedom" || protocol == "blackhole" || tag == "direct" || tag == "block" {
                Some(outbound.clone())
            } else {
                None
            }
        })
        .collect()
}

fn build_balancer_config(
    template_config: &Value,
    nodes: &[ProxyNode],
    balancer: &BalancerRule,
    all_nodes: Option<&[ProxyNode]>,
) -> Value {
    let mut config = template_config.clone();
    let proxy_outbounds: Vec<Value> = nodes.iter().map(|node| node.outbound.clone()).collect();
    let mut outbounds = proxy_outbounds;
    outbounds.extend(system_outbounds(template_config));

    if let Value::Object(ref mut map) = config {
        map.insert("outbounds".to_string(), Value::Array(outbounds));
        map.insert(
            "remarks".to_string(),
            Value::String(balancer_display_remarks(
                balancer,
                all_nodes.unwrap_or(nodes),
            )),
        );
        let selector: Vec<Value> = nodes
            .iter()
            .map(|node| Value::String(node.tag.clone()))
            .collect();
        map.insert(
            "routing".to_string(),
            json!({
                "balancers": [{
                    "tag": balancer.tag,
                    "selector": selector,
                    "strategy": { "type": balancer.strategy },
                }],
                "domainStrategy": "AsIs",
                "rules": [{
                    "type": "field",
                    "network": "tcp,udp",
                    "balancerTag": balancer.tag,
                }],
            }),
        );
    }
    config
}

fn balancer_pool_nodes(balancer: &BalancerRule, nodes: &[ProxyNode]) -> Vec<ProxyNode> {
    let member_tags: HashSet<String> = resolve_balancer_members(balancer, nodes).into_iter().collect();
    if member_tags.is_empty() {
        return Vec::new();
    }
    nodes
        .iter()
        .filter(|node| member_tags.contains(&node.tag))
        .cloned()
        .collect()
}

fn node_in_any_balancer(node: &ProxyNode, rules: &TransformRules, nodes: &[ProxyNode]) -> bool {
    rules.balancers.iter().any(|balancer| {
        resolve_balancer_members(balancer, nodes).contains(&node.tag)
    })
}

fn build_grouped_output(
    configs: &[Value],
    nodes: &[ProxyNode],
    rules: &TransformRules,
) -> Vec<Value> {
    let nodes_by_index: HashMap<usize, &ProxyNode> =
        nodes.iter().map(|node| (node.source_index, node)).collect();
    let mut emitted_balancers = HashSet::new();
    let mut result = Vec::new();

    for (index, config) in configs.iter().enumerate() {
        let Some(node) = nodes_by_index.get(&index) else {
            result.push(config.clone());
            continue;
        };

        let mut emitted_here = false;
        for balancer in &rules.balancers {
            if emitted_balancers.contains(&balancer.tag) {
                continue;
            }
            let pool_nodes = balancer_pool_nodes(balancer, nodes);
            let pool_tags: HashSet<String> = pool_nodes.iter().map(|item| item.tag.clone()).collect();
            if pool_nodes.is_empty() || !pool_tags.contains(&node.tag) {
                continue;
            }
            let template = &configs[pool_nodes[0].source_index];
            result.push(build_balancer_config(
                template,
                &pool_nodes,
                balancer,
                Some(nodes),
            ));
            emitted_balancers.insert(balancer.tag.clone());
            emitted_here = true;
        }

        if !emitted_here && !node_in_any_balancer(node, rules, nodes) {
            result.push(config.clone());
        }
    }

    result
}

fn build_single_merged_config(
    template_config: &Value,
    nodes: &[ProxyNode],
    rules: &TransformRules,
) -> Value {
    let mut config = template_config.clone();
    let proxy_outbounds: Vec<Value> = nodes.iter().map(|node| node.outbound.clone()).collect();
    let mut outbounds = proxy_outbounds;
    outbounds.extend(system_outbounds(template_config));

    let mut balancers_json = Vec::new();
    for balancer in &rules.balancers {
        let selector = resolve_balancer_members(balancer, nodes);
        if selector.is_empty() {
            continue;
        }
        balancers_json.push(json!({
            "tag": balancer.tag,
            "selector": selector,
            "strategy": { "type": balancer.strategy },
        }));
    }

    let default_balancer = if !rules.output.default_balancer.is_empty() {
        rules.output.default_balancer.clone()
    } else {
        balancers_json
            .first()
            .and_then(|item| item.get("tag"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    };

    if let Value::Object(ref mut map) = config {
        map.insert("outbounds".to_string(), Value::Array(outbounds));
        map.insert(
            "remarks".to_string(),
            Value::String(rules.output.remarks.clone()),
        );
        let mut routing = Map::new();
        routing.insert("domainStrategy".to_string(), Value::String("AsIs".to_string()));
        routing.insert("balancers".to_string(), Value::Array(balancers_json));
        if !default_balancer.is_empty() {
            routing.insert(
                "rules".to_string(),
                json!([{
                    "type": "field",
                    "network": "tcp,udp",
                    "balancerTag": default_balancer,
                }]),
            );
        }
        map.insert("routing".to_string(), Value::Object(routing));
    }

    config
}