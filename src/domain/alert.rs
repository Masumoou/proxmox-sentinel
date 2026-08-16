use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum AlertState {
    Inactive,
    Firing,
    Resolved,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Alert {
    pub id: Uuid,
    pub rule_id: Uuid,
    pub state: AlertState,
    pub fired_at: Option<DateTime<Utc>>,
    pub resolved_at: Option<DateTime<Utc>>,
}
