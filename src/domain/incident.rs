use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum IncidentState {
    Open,
    Acknowledged,
    Resolved,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Incident {
    pub id: Uuid,
    pub alert_id: Uuid,
    pub vm_id: Uuid,
    pub state: IncidentState,
    pub started_at: DateTime<Utc>,
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub acknowledged_by: Option<String>,
    pub root_cause_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CorrelationType {
    NodeToVm,
    VmToResource,
    NetworkToResource,
    GuestAgentToResource,
    Temporal, // Correlated just by happening at the exact same time on the same VM
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IncidentCorrelation {
    pub id: Uuid,
    pub parent_incident_id: Uuid,
    pub child_incident_id: Uuid,
    pub correlation_type: CorrelationType,
    pub confidence_score: u8, // 0-100
    pub reason: String,
    pub created_at: DateTime<Utc>,
}
