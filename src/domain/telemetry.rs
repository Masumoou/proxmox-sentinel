use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ObservationState {
    Healthy,
    Problem,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Telemetry {
    pub id: Uuid,
    pub metric_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub value: Option<f64>,
    pub string_value: Option<String>,
    pub observation: ObservationState,
    pub labels: Option<String>, // Stored as JSON string in SQLite
}

