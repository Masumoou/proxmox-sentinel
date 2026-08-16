use crate::domain::monitor::ConfigState;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NotificationChannel {
    pub id: Uuid,
    pub name: String,
    pub channel_type: String,
    pub config: String, // JSON payload
    pub state: ConfigState,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NotificationRoute {
    pub id: Uuid,
    pub name: String,
    pub rule_id: Option<Uuid>,
    pub severity: Option<String>,
    pub scope_type: Option<String>,
    pub scope_id: Option<Uuid>,
    pub priority: i32,
    pub template_id: Uuid,
    pub channel_id: Uuid,
    pub state: ConfigState,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Notification {
    pub id: Uuid,
    pub incident_id: Uuid,
    pub route_id: Uuid,
    pub channel_id: Uuid,
    pub sent_at: DateTime<Utc>,
    pub success: bool,
    pub error_message: Option<String>,
}
