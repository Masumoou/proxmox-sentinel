use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum DiscoveryEventType {
    Discovered,
    Changed,
    Disappeared,
    Reappeared,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscoveryEvent {
    pub id: Uuid,
    pub vm_id: Uuid,
    pub resource_id: Option<Uuid>,
    pub event_type: DiscoveryEventType,
    pub discovered_at: DateTime<Utc>,
    pub summary: String,
}

