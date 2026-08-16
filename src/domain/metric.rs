use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum MetricValueType {
    Number,
    String,
    State,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Metric {
    pub id: Uuid,
    pub monitor_id: Uuid,
    pub name: String,
    pub value_type: MetricValueType,
    pub unit: Option<String>,
}
