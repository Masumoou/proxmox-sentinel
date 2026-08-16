use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum MaintenanceScopeType {
    Global,
    Vm,
    Resource,
    Rule,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MaintenanceWindow {
    pub id: Uuid,
    pub scope_type: MaintenanceScopeType,
    pub scope_id: Option<Uuid>,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub created_by: String,
}

