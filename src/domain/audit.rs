use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditEvent {
    pub id: Uuid,
    pub actor: String,
    pub entity_type: String,
    pub entity_id: Uuid,
    pub action: String,
    pub timestamp: DateTime<Utc>,
    pub previous_state: Option<String>, // JSON
    pub new_state: Option<String>, // JSON
}

