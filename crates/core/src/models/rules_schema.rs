use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MatchRule {
    #[serde(default)]
    pub remarks_equals: Vec<String>,
    #[serde(default)]
    pub remarks_contains: Vec<String>,
    #[serde(default)]
    pub flag: String,
    #[serde(default)]
    pub network_in: Vec<String>,
    #[serde(default)]
    pub protocol: String,
    #[serde(default)]
    pub network: String,
    #[serde(default)]
    pub address_equals: Vec<String>,
    #[serde(default)]
    pub path_equals: Vec<String>,
    #[serde(default)]
    pub security: String,
    #[serde(default)]
    pub port: Option<i64>,
    #[serde(default)]
    pub fingerprint_equals: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaggingRule {
    pub r#match: MatchRule,
    pub tag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Single,
    Grouped,
    Array,
    Passthrough,
}

impl Default for OutputFormat {
    fn default() -> Self {
        Self::Grouped
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct OutputConfig {
    #[serde(default)]
    pub format: OutputFormat,
    #[serde(default = "default_remarks")]
    pub remarks: String,
    #[serde(default)]
    pub default_balancer: String,
}

fn default_remarks() -> String {
    "ExamplePool".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TaggingConfig {
    #[serde(default)]
    pub rules: Vec<TaggingRule>,
    #[serde(default = "default_template")]
    pub default_template: String,
}

fn default_template() -> String {
    "node-{index}".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct FiltersConfig {
    #[serde(default)]
    pub exclude: Vec<MatchRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BalancerMember {
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub inbound_ids: Vec<String>,
    #[serde(default)]
    pub r#match: Option<MatchRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BalancerRule {
    pub tag: String,
    #[serde(default)]
    pub remarks: String,
    #[serde(default = "default_strategy")]
    pub strategy: String,
    #[serde(default)]
    pub members: Vec<BalancerMember>,
}

fn default_strategy() -> String {
    "roundRobin".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransformRules {
    #[serde(default)]
    pub output: OutputConfig,
    #[serde(default)]
    pub filters: FiltersConfig,
    #[serde(default)]
    pub tagging: TaggingConfig,
    #[serde(default)]
    pub balancers: Vec<BalancerRule>,
}

impl TransformRules {
    pub fn from_value(data: &Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(data.clone())
    }

    pub fn from_json_str(data: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(data)
    }
}