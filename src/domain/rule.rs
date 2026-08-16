use crate::domain::monitor::ConfigState;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Rule {
    pub id: Uuid,
    pub metric_id: Uuid,
    pub template_id: Option<Uuid>,
    pub state: ConfigState,
    pub severity: String,

    pub fire_operator: String,
    pub fire_value_type: String,
    pub fire_value: String,
    pub fire_duration_secs: i64,

    pub resolve_operator: String,
    pub resolve_value_type: String,
    pub resolve_value: String,
    pub resolve_duration_secs: i64,

    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}
