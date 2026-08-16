use rusqlite::{Connection, Result, params, OptionalExtension};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use std::str::FromStr;
use serde_json::Value;

use crate::domain::resource::{Resource, ResourceState};
use crate::domain::monitor::{Monitor, ConfigState};
use crate::domain::metric::{Metric, MetricValueType};
use crate::domain::telemetry::{Telemetry, ObservationState};
use crate::domain::discovery::{DiscoveryEvent, DiscoveryEventType};
use crate::domain::rule::{Rule, Operator};
use crate::domain::alert::{Alert, AlertState};
use crate::domain::incident::{Incident, IncidentState};

fn uuid_to_string(id: Uuid) -> String { id.to_string() }
fn string_to_uuid(s: String) -> Uuid { Uuid::from_str(&s).unwrap_or_default() }
fn dt_to_string(dt: DateTime<Utc>) -> String { dt.to_rfc3339() }
fn string_to_dt(s: String) -> DateTime<Utc> { DateTime::parse_from_rfc3339(&s).map(|dt| dt.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now()) }
fn opt_dt_to_string(dt: Option<DateTime<Utc>>) -> Option<String> { dt.map(dt_to_string) }
fn opt_string_to_dt(s: Option<String>) -> Option<DateTime<Utc>> { s.map(string_to_dt) }

pub fn resource_state_to_str(state: ResourceState) -> &'static str { match state { ResourceState::Discovered => "DISCOVERED", ResourceState::PendingUser => "PENDING_USER", ResourceState::Monitored => "MONITORED", ResourceState::Ignored => "IGNORED", ResourceState::Removed => "REMOVED" } }
pub fn str_to_resource_state(s: &str) -> ResourceState { match s { "DISCOVERED" => ResourceState::Discovered, "PENDING_USER" => ResourceState::PendingUser, "MONITORED" => ResourceState::Monitored, "IGNORED" => ResourceState::Ignored, "REMOVED" => ResourceState::Removed, _ => ResourceState::Discovered } }

pub fn config_state_to_str(state: ConfigState) -> &'static str { match state { ConfigState::Enabled => "ENABLED", ConfigState::Disabled => "DISABLED" } }
pub fn str_to_config_state(s: &str) -> ConfigState { match s { "ENABLED" => ConfigState::Enabled, "DISABLED" => ConfigState::Disabled, _ => ConfigState::Disabled } }

pub fn event_type_to_str(state: DiscoveryEventType) -> &'static str { match state { DiscoveryEventType::Discovered => "DISCOVERED", DiscoveryEventType::Changed => "CHANGED", DiscoveryEventType::Disappeared => "DISAPPEARED", DiscoveryEventType::Reappeared => "REAPPEARED" } }

pub fn str_to_value_type(s: &str) -> MetricValueType { match s { "number" => MetricValueType::Number, "string" => MetricValueType::String, "state" => MetricValueType::State, _ => MetricValueType::Number } }
pub fn value_type_to_str(vt: MetricValueType) -> &'static str { match vt { MetricValueType::Number => "number", MetricValueType::String => "string", MetricValueType::State => "state" } }

pub fn observation_to_str(obs: ObservationState) -> &'static str { match obs { ObservationState::Healthy => "HEALTHY", ObservationState::Problem => "PROBLEM", ObservationState::Unknown => "UNKNOWN" } }
pub fn str_to_observation(s: &str) -> ObservationState { match s { "HEALTHY" => ObservationState::Healthy, "PROBLEM" => ObservationState::Problem, "UNKNOWN" => ObservationState::Unknown, _ => ObservationState::Unknown } }

pub fn operator_to_str(op: Operator) -> &'static str { match op { Operator::Equal => "EQUAL", Operator::NotEqual => "NOT_EQUAL", Operator::GreaterThan => "GREATER_THAN", Operator::LessThan => "LESS_THAN", Operator::GreaterOrEqual => "GREATER_OR_EQUAL", Operator::LessOrEqual => "LESS_OR_EQUAL" } }
pub fn str_to_operator(s: &str) -> Operator { match s { "EQUAL" => Operator::Equal, "NOT_EQUAL" => Operator::NotEqual, "GREATER_THAN" => Operator::GreaterThan, "LESS_THAN" => Operator::LessThan, "GREATER_OR_EQUAL" => Operator::GreaterOrEqual, "LESS_OR_EQUAL" => Operator::LessOrEqual, _ => Operator::Equal } }

pub fn alert_state_to_str(s: AlertState) -> &'static str { match s { AlertState::Inactive => "INACTIVE", AlertState::Firing => "FIRING", AlertState::Resolved => "RESOLVED" } }
pub fn str_to_alert_state(s: &str) -> AlertState { match s { "INACTIVE" => AlertState::Inactive, "FIRING" => AlertState::Firing, "RESOLVED" => AlertState::Resolved, _ => AlertState::Inactive } }

pub fn incident_state_to_str(s: IncidentState) -> &'static str { match s { IncidentState::Open => "OPEN", IncidentState::Acknowledged => "ACKNOWLEDGED", IncidentState::Resolved => "RESOLVED" } }
pub fn str_to_incident_state(s: &str) -> IncidentState { match s { "OPEN" => IncidentState::Open, "ACKNOWLEDGED" => IncidentState::Acknowledged, "RESOLVED" => IncidentState::Resolved, _ => IncidentState::Open } }

pub struct ResourceRepository<'a> { pub conn: &'a Connection }
impl<'a> ResourceRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self { Self { conn } }
    pub fn insert(&self, resource: &Resource) -> Result<()> {
        self.conn.execute("INSERT INTO resources (id, vm_id, kind, identifier, state, version, created_at, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)", params![uuid_to_string(resource.id), uuid_to_string(resource.vm_id), resource.kind, resource.identifier, resource_state_to_str(resource.state), resource.version, dt_to_string(resource.created_at), dt_to_string(resource.updated_at), opt_dt_to_string(resource.deleted_at)])?; Ok(())
    }
    pub fn get_by_id(&self, id: Uuid) -> Result<Option<Resource>> {
        self.conn.query_row("SELECT id, vm_id, kind, identifier, state, version, created_at, updated_at, deleted_at FROM resources WHERE id = ?1", params![uuid_to_string(id)], |row| Ok(Resource { id: string_to_uuid(row.get(0)?), vm_id: string_to_uuid(row.get(1)?), kind: row.get(2)?, identifier: row.get(3)?, state: str_to_resource_state(&row.get::<_, String>(4)?), version: row.get(5)?, created_at: string_to_dt(row.get(6)?), updated_at: string_to_dt(row.get(7)?), deleted_at: opt_string_to_dt(row.get(8)?) })).optional()
    }
    pub fn list_by_vm_and_kind(&self, vm_id: Uuid, kind: &str) -> Result<Vec<Resource>> {
        let mut stmt = self.conn.prepare("SELECT id, vm_id, kind, identifier, state, version, created_at, updated_at, deleted_at FROM resources WHERE vm_id = ?1 AND kind = ?2 AND deleted_at IS NULL")?;
        let rows = stmt.query_map(params![uuid_to_string(vm_id), kind], |row| Ok(Resource { id: string_to_uuid(row.get(0)?), vm_id: string_to_uuid(row.get(1)?), kind: row.get(2)?, identifier: row.get(3)?, state: str_to_resource_state(&row.get::<_, String>(4)?), version: row.get(5)?, created_at: string_to_dt(row.get(6)?), updated_at: string_to_dt(row.get(7)?), deleted_at: opt_string_to_dt(row.get(8)?) }))?;
        let mut results = Vec::new();
        for row in rows { results.push(row?); }
        Ok(results)
    }
    pub fn update_state(&self, id: Uuid, state: ResourceState) -> Result<()> {
        self.conn.execute("UPDATE resources SET state = ?1, updated_at = ?2, version = version + 1 WHERE id = ?3", params![resource_state_to_str(state), dt_to_string(Utc::now()), uuid_to_string(id)])?; Ok(())
    }
}

pub struct MonitorRepository<'a> { pub conn: &'a Connection }
impl<'a> MonitorRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self { Self { conn } }
    pub fn get_by_resource_id(&self, resource_id: Uuid) -> Result<Vec<Monitor>> {
        let mut stmt = self.conn.prepare("SELECT id, resource_id, state, interval_secs, collection_type, version, created_at, updated_at, deleted_at FROM monitors WHERE resource_id = ?1 AND deleted_at IS NULL")?;
        let rows = stmt.query_map(params![uuid_to_string(resource_id)], |row| Ok(Monitor { id: string_to_uuid(row.get(0)?), resource_id: string_to_uuid(row.get(1)?), state: str_to_config_state(&row.get::<_, String>(2)?), interval_secs: row.get(3)?, collection_type: row.get(4)?, version: row.get(5)?, created_at: string_to_dt(row.get(6)?), updated_at: string_to_dt(row.get(7)?), deleted_at: opt_string_to_dt(row.get(8)?) }))?;
        let mut results = Vec::new(); for row in rows { results.push(row?); } Ok(results)
    }
}

pub struct MetricRepository<'a> { pub conn: &'a Connection }
impl<'a> MetricRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self { Self { conn } }
    pub fn get_by_monitor_and_name(&self, monitor_id: Uuid, name: &str) -> Result<Option<Metric>> {
        self.conn.query_row("SELECT id, monitor_id, name, value_type, unit FROM metrics WHERE monitor_id = ?1 AND name = ?2", params![uuid_to_string(monitor_id), name], |row| Ok(Metric { id: string_to_uuid(row.get(0)?), monitor_id: string_to_uuid(row.get(1)?), name: row.get(2)?, value_type: str_to_value_type(&row.get::<_, String>(3)?), unit: row.get(4)? })).optional()
    }
}

pub struct TelemetryRepository<'a> { pub conn: &'a Connection }
impl<'a> TelemetryRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self { Self { conn } }
    pub fn insert(&self, telemetry: &Telemetry) -> Result<()> {
        self.conn.execute("INSERT INTO telemetry (id, metric_id, timestamp, value, string_value, observation, labels) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)", params![uuid_to_string(telemetry.id), uuid_to_string(telemetry.metric_id), dt_to_string(telemetry.timestamp), telemetry.value, telemetry.string_value, observation_to_str(telemetry.observation), telemetry.labels])?; Ok(())
    }
    
    // In reality, this would query a time-window of telemetry data for the rule engine evaluator
    pub fn get_recent_for_metric(&self, metric_id: Uuid, since: DateTime<Utc>) -> Result<Vec<Telemetry>> {
        let mut stmt = self.conn.prepare("SELECT id, metric_id, timestamp, value, string_value, observation, labels FROM telemetry WHERE metric_id = ?1 AND timestamp >= ?2 ORDER BY timestamp DESC")?;
        let rows = stmt.query_map(params![uuid_to_string(metric_id), dt_to_string(since)], |row| {
            Ok(Telemetry {
                id: string_to_uuid(row.get(0)?),
                metric_id: string_to_uuid(row.get(1)?),
                timestamp: string_to_dt(row.get(2)?),
                value: row.get(3)?,
                string_value: row.get(4)?,
                observation: str_to_observation(&row.get::<_, String>(5)?),
                labels: row.get(6)?
            })
        })?;
        let mut results = Vec::new(); for row in rows { results.push(row?); } Ok(results)
    }
}

pub struct DiscoveryEventRepository<'a> { pub conn: &'a Connection }
impl<'a> DiscoveryEventRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self { Self { conn } }
    pub fn insert(&self, event: &DiscoveryEvent) -> Result<()> { self.conn.execute("INSERT INTO discovery_events (id, vm_id, resource_id, event_type, discovered_at, summary) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![uuid_to_string(event.id), uuid_to_string(event.vm_id), event.resource_id.map(uuid_to_string), event_type_to_str(event.event_type), dt_to_string(event.discovered_at), event.summary])?; Ok(()) }
}

pub struct RuleRepository<'a> { pub conn: &'a Connection }
impl<'a> RuleRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self { Self { conn } }
    pub fn list_enabled(&self) -> Result<Vec<Rule>> {
        let mut stmt = self.conn.prepare("SELECT id, metric_id, state, operator, fire_value, fire_duration_secs, resolve_value, resolve_duration_secs, severity, version, created_at, updated_at, deleted_at FROM rules WHERE state = 'ENABLED' AND deleted_at IS NULL")?;
        let rows = stmt.query_map([], |row| Ok(Rule { id: string_to_uuid(row.get(0)?), metric_id: string_to_uuid(row.get(1)?), state: str_to_config_state(&row.get::<_, String>(2)?), operator: str_to_operator(&row.get::<_, String>(3)?), fire_value: row.get(4)?, fire_duration_secs: row.get(5)?, resolve_value: row.get(6)?, resolve_duration_secs: row.get(7)?, severity: row.get(8)?, version: row.get(9)?, created_at: string_to_dt(row.get(10)?), updated_at: string_to_dt(row.get(11)?), deleted_at: opt_string_to_dt(row.get(12)?) }))?;
        let mut results = Vec::new(); for row in rows { results.push(row?); } Ok(results)
    }
}

pub struct AlertRepository<'a> { pub conn: &'a Connection }
impl<'a> AlertRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self { Self { conn } }
    pub fn insert(&self, alert: &Alert) -> Result<()> {
        self.conn.execute("INSERT INTO alerts (id, rule_id, state, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)", params![uuid_to_string(alert.id), uuid_to_string(alert.rule_id), alert_state_to_str(alert.state), dt_to_string(alert.created_at), dt_to_string(alert.updated_at)])?; Ok(())
    }
    pub fn update_state(&self, id: Uuid, state: AlertState) -> Result<()> {
        self.conn.execute("UPDATE alerts SET state = ?1, updated_at = ?2 WHERE id = ?3", params![alert_state_to_str(state), dt_to_string(Utc::now()), uuid_to_string(id)])?; Ok(())
    }
    pub fn get_active_by_rule(&self, rule_id: Uuid) -> Result<Option<Alert>> {
        self.conn.query_row("SELECT id, rule_id, state, created_at, updated_at FROM alerts WHERE rule_id = ?1 AND state = 'FIRING'", params![uuid_to_string(rule_id)], |row| Ok(Alert { id: string_to_uuid(row.get(0)?), rule_id: string_to_uuid(row.get(1)?), state: str_to_alert_state(&row.get::<_, String>(2)?), created_at: string_to_dt(row.get(3)?), updated_at: string_to_dt(row.get(4)?) })).optional()
    }
}

pub struct IncidentRepository<'a> { pub conn: &'a Connection }
impl<'a> IncidentRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self { Self { conn } }
    pub fn insert(&self, incident: &Incident) -> Result<()> {
        self.conn.execute("INSERT INTO incidents (id, alert_id, state, severity, created_at, resolved_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![uuid_to_string(incident.id), uuid_to_string(incident.alert_id), incident_state_to_str(incident.state), incident.severity, dt_to_string(incident.created_at), opt_dt_to_string(incident.resolved_at)])?; Ok(())
    }
    pub fn update_state(&self, id: Uuid, state: IncidentState) -> Result<()> {
        let resolved_at = if state == IncidentState::Resolved { Some(dt_to_string(Utc::now())) } else { None };
        self.conn.execute("UPDATE incidents SET state = ?1, resolved_at = COALESCE(?2, resolved_at) WHERE id = ?3", params![incident_state_to_str(state), resolved_at, uuid_to_string(id)])?; Ok(())
    }
    pub fn get_active_by_alert(&self, alert_id: Uuid) -> Result<Option<Incident>> {
        self.conn.query_row("SELECT id, alert_id, state, severity, created_at, resolved_at FROM incidents WHERE alert_id = ?1 AND state != 'RESOLVED'", params![uuid_to_string(alert_id)], |row| Ok(Incident { id: string_to_uuid(row.get(0)?), alert_id: string_to_uuid(row.get(1)?), state: str_to_incident_state(&row.get::<_, String>(2)?), severity: row.get(3)?, created_at: string_to_dt(row.get(4)?), resolved_at: opt_string_to_dt(row.get(5)?) })).optional()
    }
}
獵⁥牣瑡㩥携浯楡㩮洺楡瑮湥湡散㨺䵻楡瑮湥湡散楗摮睯‬慍湩整慮据卥潣数祔数㭽ਊ異⁢湦猠潣数瑟灹彥潴獟牴猨›慍湩整慮据卥潣数祔数 㸭☠猧慴楴⁣瑳⁲੻††慭捴⁨⁳੻††††慍湩整慮据卥潣数祔数㨺汇扯污㴠‾䜢佌䅂≌ਬ††††慍湩整慮据卥潣数祔数㨺浖㴠‾嘢≍ਬ††††慍湩整慮据卥潣数祔数㨺敒潳牵散㴠‾刢卅問䍒≅ਬ††††慍湩整慮据卥潣数祔数㨺畒敬㴠‾刢䱕≅ਬ††੽੽異⁢湦猠牴瑟彯捳灯彥祴数猨›猦牴 㸭䴠楡瑮湥湡散捓灯呥灹⁥੻††慭捴⁨⁳੻††††䜢佌䅂≌㴠‾慍湩整慮据卥潣数祔数㨺汇扯污ਬ††††嘢≍㴠‾慍湩整慮据卥潣数祔数㨺浖ਬ††††刢卅問䍒≅㴠‾慍湩整慮据卥潣数祔数㨺敒潳牵散ਬ††††刢䱕≅㴠‾慍湩整慮据卥潣数祔数㨺畒敬ਬ†††† 㸽䴠楡瑮湥湡散捓灯呥灹㩥䜺潬慢ⱬ †素紊ਊ異⁢瑳畲瑣䴠楡瑮湥湡散楗摮睯敒潰楳潴祲✼㹡笠瀠扵挠湯㩮☠愧䌠湯敮瑣潩⁮੽浩汰✼㹡䴠楡瑮湥湡散楗摮睯敒潰楳潴祲✼㹡笠 †瀠扵映⁮敮⡷潣湮›✦⁡潃湮捥楴湯 㸭匠汥⁦⁻敓晬笠挠湯⁮⁽੽†† †瀠扵映⁮湩敳瑲☨敳晬‬楷摮睯›䴦楡瑮湥湡散楗摮睯 㸭删獥汵㱴⤨‾੻††††敳晬挮湯⹮硥捥瑵⡥䤢华剅⁔义佔洠楡瑮湥湡散睟湩潤獷⠠摩‬捳灯彥祴数‬捳灯彥摩‬瑳牡彴楴敭‬湥彤楴敭‬牣慥整彤祢 䅖啌卅⠠ㄿ‬㈿‬㌿‬㐿‬㔿‬㘿∩‬慰慲獭嬡畵摩瑟彯瑳楲杮眨湩潤⹷摩Ⱙ猠潣数瑟灹彥潴獟牴眨湩潤⹷捳灯彥祴数Ⱙ眠湩潤⹷捳灯彥摩洮灡用極彤潴獟牴湩⥧‬瑤瑟彯瑳楲杮眨湩潤⹷瑳牡彴楴敭Ⱙ搠彴潴獟牴湩⡧楷摮睯攮摮瑟浩⥥‬楷摮睯挮敲瑡摥扟嵹㼩※歏⠨⤩ †素ਊ††異⁢湦朠瑥慟瑣癩⡥猦汥⥦ⴠ‾敒畳瑬嘼捥䴼楡瑮湥湡散楗摮睯㸾笠 †††氠瑥渠睯㴠搠彴潴獟牴湩⡧瑕㩣渺睯⤨㬩 †††氠瑥洠瑵猠浴⁴‽敳晬挮湯⹮牰灥牡⡥匢䱅䍅⁔摩‬捳灯彥祴数‬捳灯彥摩‬瑳牡彴楴敭‬湥彤楴敭‬牣慥整彤祢䘠佒⁍慭湩整慮据彥楷摮睯⁳䡗剅⁅瑳牡彴楴敭㰠‽ㄿ䄠䑎攠摮瑟浩⁥㴾㼠∱㼩਻††††敬⁴潲獷㴠猠浴⹴畱牥役慭⡰慰慲獭嬡潮嵷‬牼睯⁼੻††††††歏䴨楡瑮湥湡散楗摮睯笠 †††††††椠㩤猠牴湩彧潴畟極⡤潲⹷敧⡴⤰⤿ਬ††††††††捳灯彥祴数›瑳彲潴獟潣数瑟灹⡥爦睯朮瑥㨺弼‬瑓楲杮⠾⤱⤿ਬ††††††††捳灯彥摩›灯彴瑳楲杮瑟彯畵摩爨睯朮瑥㈨㼩Ⱙ †††††††猠慴瑲瑟浩㩥猠牴湩彧潴摟⡴潲⹷敧⡴⤳⤿ਬ††††††††湥彤楴敭›瑳楲杮瑟彯瑤爨睯朮瑥㐨㼩Ⱙ †††††††挠敲瑡摥扟㩹爠睯朮瑥㔨㼩ਬ††††††⥽ †††素㼩਻††††敬⁴畭⁴敲畳瑬⁳‽敖㩣渺睥⤨※潦⁲潲⁷湩爠睯⁳⁻敲畳瑬⹳異桳爨睯⤿※⁽歏爨獥汵獴਩††੽੽昊⁮灯彴瑳楲杮瑟彯畵摩猨›灏楴湯匼牴湩㹧 㸭传瑰潩㱮畕摩‾⁻⹳慭⡰瑳楲杮瑟彯畵摩 ੽⼊ 摁楤楴湯污栠汥数⁲畱牥敩⁳潦⁲牴捡湩⁧敲慬楴湯桳灩ੳ浩汰✼㹡䄠敬瑲敒潰楳潴祲✼㹡笠 †瀠扵映⁮敧彴祢楟⡤猦汥ⱦ椠㩤唠極⥤ⴠ‾敒畳瑬似瑰潩㱮汁牥㹴‾੻††††敳晬挮湯⹮畱牥役潲⡷匢䱅䍅⁔摩‬畲敬楟Ɽ猠慴整‬牣慥整彤瑡‬灵慤整彤瑡䘠佒⁍污牥獴圠䕈䕒椠⁤‽ㄿⰢ瀠牡浡ⅳ畛極彤潴獟牴湩⡧摩崩‬牼睯⁼歏䄨敬瑲笠椠㩤猠牴湩彧潴畟極⡤潲⹷敧⡴⤰⤿‬畲敬楟㩤猠牴湩彧潴畟極⡤潲⹷敧⡴⤱⤿‬瑳瑡㩥猠牴瑟彯污牥彴瑳瑡⡥爦睯朮瑥㨺弼‬瑓楲杮⠾⤲⤿‬牣慥整彤瑡›瑳楲杮瑟彯瑤爨睯朮瑥㌨㼩Ⱙ甠摰瑡摥慟㩴猠牴湩彧潴摟⡴潲⹷敧⡴⤴⤿素⤩漮瑰潩慮⡬਩††੽੽椊灭㱬愧‾畒敬敒潰楳潴祲✼㹡笠 †瀠扵映⁮敧彴祢楟⡤猦汥ⱦ椠㩤唠極⥤ⴠ‾敒畳瑬似瑰潩㱮畒敬㸾笠 †††猠汥⹦潣湮焮敵祲牟睯∨䕓䕌呃椠Ɽ洠瑥楲彣摩‬瑳瑡ⱥ漠数慲潴Ⱳ映物彥慶畬ⱥ映物彥畤慲楴湯獟捥ⱳ爠獥汯敶癟污敵‬敲潳癬彥畤慲楴湯獟捥ⱳ猠癥牥瑩ⱹ瘠牥楳湯‬牣慥整彤瑡‬灵慤整彤瑡‬敤敬整彤瑡䘠佒⁍畲敬⁳䡗剅⁅摩㴠㼠∱‬慰慲獭嬡畵摩瑟彯瑳楲杮椨⥤ⱝ簠潲籷传⡫畒敬笠椠㩤猠牴湩彧潴畟極⡤潲⹷敧⡴⤰⤿‬敭牴捩楟㩤猠牴湩彧潴畟極⡤潲⹷敧⡴⤱⤿‬瑳瑡㩥猠牴瑟彯潣普杩獟慴整☨潲⹷敧㩴㰺ⱟ匠牴湩㹧㈨㼩Ⱙ漠数慲潴㩲猠牴瑟彯灯牥瑡牯☨潲⹷敧㩴㰺ⱟ匠牴湩㹧㌨㼩Ⱙ映物彥慶畬㩥爠睯朮瑥㐨㼩‬楦敲摟牵瑡潩彮敳獣›潲⹷敧⡴⤵ⰿ爠獥汯敶癟污敵›潲⹷敧⡴⤶ⰿ爠獥汯敶摟牵瑡潩彮敳獣›潲⹷敧⡴⤷ⰿ猠癥牥瑩㩹爠睯朮瑥㠨㼩‬敶獲潩㩮爠睯朮瑥㤨㼩‬牣慥整彤瑡›瑳楲杮瑟彯瑤爨睯朮瑥ㄨ⤰⤿‬灵慤整彤瑡›瑳楲杮瑟彯瑤爨睯朮瑥ㄨ⤱⤿‬敤敬整彤瑡›灯彴瑳楲杮瑟彯瑤爨睯朮瑥ㄨ⤲⤿素⤩漮瑰潩慮⡬਩††੽੽椊灭㱬愧‾敍牴捩敒潰楳潴祲✼㹡笠 †瀠扵映⁮敧彴祢楟⡤猦汥ⱦ椠㩤唠極⥤ⴠ‾敒畳瑬似瑰潩㱮敍牴捩㸾笠 †††猠汥⹦潣湮焮敵祲牟睯∨䕓䕌呃椠Ɽ洠湯瑩牯楟Ɽ渠浡ⱥ瘠污敵瑟灹ⱥ甠楮⁴剆䵏洠瑥楲獣圠䕈䕒椠⁤‽ㄿⰢ瀠牡浡ⅳ畛極彤潴獟牴湩⡧摩崩‬牼睯⁼歏䴨瑥楲⁣⁻摩›瑳楲杮瑟彯畵摩爨睯朮瑥〨㼩Ⱙ洠湯瑩牯楟㩤猠牴湩彧潴畟極⡤潲⹷敧⡴⤱⤿‬慮敭›潲⹷敧⡴⤲ⰿ瘠污敵瑟灹㩥猠牴瑟彯慶畬彥祴数☨潲⹷敧㩴㰺ⱟ匠牴湩㹧㌨㼩Ⱙ甠楮㩴爠睯朮瑥㐨㼩素⤩漮瑰潩慮⡬਩††੽੽椊灭㱬愧‾潍楮潴割灥獯瑩牯㱹愧‾੻††異⁢湦朠瑥扟役摩☨敳晬‬摩›畕摩 㸭删獥汵㱴灏楴湯䴼湯瑩牯㸾笠 †††猠汥⹦潣湮焮敵祲牟睯∨䕓䕌呃椠Ɽ爠獥畯捲彥摩‬瑳瑡ⱥ椠瑮牥慶彬敳獣‬潣汬捥楴湯瑟灹ⱥ瘠牥楳湯‬牣慥整彤瑡‬灵慤整彤瑡‬敤敬整彤瑡䘠佒⁍潭楮潴獲圠䕈䕒椠⁤‽ㄿⰢ瀠牡浡ⅳ畛極彤潴獟牴湩⡧摩崩‬牼睯⁼歏䴨湯瑩牯笠椠㩤猠牴湩彧潴畟極⡤潲⹷敧⡴⤰⤿‬敲潳牵散楟㩤猠牴湩彧潴畟極⡤潲⹷敧⡴⤱⤿‬瑳瑡㩥猠牴瑟彯潣普杩獟慴整☨潲⹷敧㩴㰺ⱟ匠牴湩㹧㈨㼩Ⱙ椠瑮牥慶彬敳獣›潲⹷敧⡴⤳ⰿ挠汯敬瑣潩彮祴数›潲⹷敧⡴⤴ⰿ瘠牥楳湯›潲⹷敧⡴⤵ⰿ挠敲瑡摥慟㩴猠牴湩彧潴摟⡴潲⹷敧⡴⤶⤿‬灵慤整彤瑡›瑳楲杮瑟彯瑤爨睯朮瑥㜨㼩Ⱙ搠汥瑥摥慟㩴漠瑰獟牴湩彧潴摟⡴潲⹷敧⡴⤸⤿素⤩漮瑰潩慮⡬਩††੽੽਍獵⁥牣瑡㩥携浯楡㩮渺瑯晩捩瑡潩㩮为瑯晩捩瑡潩㭮ਊ異⁢瑳畲瑣丠瑯晩捩瑡潩剮灥獯瑩牯㱹愧‾⁻異⁢潣湮›✦⁡潃湮捥楴湯素椊灭㱬愧‾潎楴楦慣楴湯敒潰楳潴祲✼㹡笠 †瀠扵映⁮敮⡷潣湮›✦⁡潃湮捥楴湯 㸭匠汥⁦⁻敓晬笠挠湯⁮⁽੽†† †瀠扵映⁮湩敳瑲☨敳晬‬潮楴㩦☠潎楴楦慣楴湯 㸭删獥汵㱴⤨‾੻††††敳晬挮湯⹮硥捥瑵⡥䤢华剅⁔义佔渠瑯晩捩瑡潩獮⠠摩‬湩楣敤瑮楟Ɽ挠慨湮汥楟Ɽ猠湥彴瑡‬畳捣獥ⱳ攠牲牯浟獥慳敧 䅖啌卅⠠ㄿ‬㈿‬㌿‬㐿‬㔿‬㘿∩‬慰慲獭嬡畵摩瑟彯瑳楲杮渨瑯晩椮⥤‬畵摩瑟彯瑳楲杮渨瑯晩椮据摩湥彴摩Ⱙ甠極彤潴獟牴湩⡧潮楴⹦档湡敮彬摩Ⱙ搠彴潴獟牴湩⡧潮楴⹦敳瑮慟⥴‬潮楴⹦畳捣獥ⱳ渠瑯晩攮牲牯浟獥慳敧⥝㬿传⡫⤨਩††੽ൽ甊敳爠獵汱瑩㩥刺獥汵㭴甊敳猠摴㨺潣汬捥楴湯㩳䠺獡䵨灡਻椊灭㱬愧‾敔敬敭牴剹灥獯瑩牯㱹愧‾੻††異⁢湦朠瑥江瑡獥彴潦彲敭牴捩☨敳晬‬敭牴捩楟㩤唠極⥤ⴠ‾敒畳瑬似瑰潩㱮敔敬敭牴㹹‾੻††††敳晬挮湯⹮畱牥役潲⡷ †††††∠䕓䕌呃椠Ɽ洠瑥楲彣摩‬楴敭瑳浡Ɒ瘠污敵‬瑳楲杮癟污敵‬扯敳癲瑡潩Ɱ氠扡汥⁳ ††††††剆䵏琠汥浥瑥祲圠䕈䕒洠瑥楲彣摩㴠㼠‱剏䕄⁒奂琠浩獥慴灭䐠卅⁃䥌䥍⁔∱ਬ††††††慰慲獭嬡畵摩瑟彯瑳楲杮洨瑥楲彣摩崩ਬ††††††牼睯⁼歏吨汥浥瑥祲笠 †††††††椠㩤猠牴湩彧潴畟極⡤潲⹷敧⡴⤰⤿ਬ††††††††敭牴捩楟㩤猠牴湩彧潴畟極⡤潲⹷敧⡴⤱⤿ਬ††††††††楴敭瑳浡㩰猠牴湩彧潴摟⡴潲⹷敧⡴⤲⤿ਬ††††††††慶畬㩥爠睯朮瑥㌨㼩ਬ††††††††瑳楲杮癟污敵›潲⹷敧⡴⤴ⰿ †††††††漠獢牥慶楴湯›瑳彲潴潟獢牥慶楴湯☨潲⹷敧㩴㰺ⱟ匠牴湩㹧㔨㼩Ⱙ †††††††氠扡汥㩳爠睯朮瑥㘨㼩 †††††素਩††††⸩灯楴湯污⤨ †素紊਍異⁢瑳畲瑣䔠灸牯整兲敵楲獥✼㹡笠瀠扵挠湯㩮☠愧䌠湯敮瑣潩⁮੽浩汰✼㹡䔠灸牯整兲敵楲獥✼㹡笠 †瀠扵映⁮敮⡷潣湮›✦⁡潃湮捥楴湯 㸭匠汥⁦⁻敓晬笠挠湯⁮⁽੽ †瀠扵映⁮敧彴潭楮潴敲彤敭牴捩⡳猦汥⥦ⴠ‾敒畳瑬嘼捥⠼畕摩‬畕摩‬瑓楲杮‬瑓楲杮‬瑓楲杮㸩‾੻††††⼯删瑥牵獮›敭牴捩楟Ɽ瘠彭摩‬敲潳牵散歟湩Ɽ爠獥畯捲彥摩湥楴楦牥‬敭牴捩湟浡੥††††敬⁴畭⁴瑳瑭㴠猠汥⹦潣湮瀮敲慰敲∨ †††††匠䱅䍅⁔⹭摩‬⹲浶楟Ɽ爠欮湩Ɽ爠椮敤瑮晩敩Ⱳ洠渮浡⁥ †††††䘠佒⁍敭牴捩⁳੭††††††佊义洠湯瑩牯⁳潭传⁎⹭潭楮潴彲摩㴠洠⹯摩 †††††䨠䥏⁎敲潳牵散⁳⁲乏洠⹯敲潳牵散楟⁤‽⹲摩 †††††圠䕈䕒爠献慴整㴠✠位䥎佔䕒❄䄠䑎洠⹯瑳瑡⁥‽䔧䅎䱂䑅‧乁⁄潭搮汥瑥摥慟⁴卉丠䱕⁌乁⁄⹲敤敬整彤瑡䤠⁓啎䱌 †††∠㼩਻††††敬⁴潲獷㴠猠浴⹴畱牥役慭⡰嵛‬牼睯⁼੻††††††歏⠨ †††††††猠牴湩彧潴畟極⡤潲⹷敧⡴⤰⤿ਬ††††††††瑳楲杮瑟彯畵摩爨睯朮瑥ㄨ㼩Ⱙ †††††††爠睯朮瑥㈨㼩ਬ††††††††潲⹷敧⡴⤳ⰿ †††††††爠睯朮瑥㐨㼩ਬ††††††⤩ †††素㼩਻††††敬⁴畭⁴敲畳瑬⁳‽敖㩣渺睥⤨※潦⁲潲⁷湩爠睯⁳⁻敲畳瑬⹳異桳爨睯⤿※⁽歏爨獥汵獴਩††੽ൽ椊灭㱬愧‾硅潰瑲牥畑牥敩㱳愧‾੻††異⁢湦朠瑥浟湯瑩牯摥浟瑥楲獣睟瑩彨浶☨敳晬 㸭删獥汵㱴敖㱣唨極Ɽ甠㈳‬瑓楲杮‬瑓楲杮‬瑓楲杮‬瑓楲杮㸩‾੻††††⼯删瑥牵獮›敭牴捩楟Ɽ瀠潲浸硯癟業Ɽ瘠彭慮敭‬敲潳牵散歟湩Ɽ爠獥畯捲彥摩湥楴楦牥‬敭牴捩湟浡੥††††敬⁴畭⁴瑳瑭㴠猠汥⹦潣湮瀮敲慰敲∨ †††††匠䱅䍅⁔⹭摩‬⹶牰硯潭彸浶摩‬⹶慮敭‬⹲楫摮‬⹲摩湥楴楦牥‬⹭慮敭ਠ††††††剆䵏洠瑥楲獣洠 †††††䨠䥏⁎潭楮潴獲洠⁯乏洠洮湯瑩牯楟⁤‽潭椮੤††††††佊义爠獥畯捲獥爠传⁎潭爮獥畯捲彥摩㴠爠椮੤††††††佊义瘠獭瘠传⁎⹲浶楟⁤‽⹶摩 †††††圠䕈䕒爠献慴整㴠✠位䥎佔䕒❄䄠䑎洠⹯瑳瑡⁥‽䔧䅎䱂䑅‧乁⁄潭搮汥瑥摥慟⁴卉丠䱕⁌乁⁄⹲敤敬整彤瑡䤠⁓啎䱌 †††∠㼩਻††††敬⁴潲獷㴠猠浴⹴畱牥役慭⡰嵛‬牼睯⁼੻††††††歏⠨ †††††††猠牴湩彧潴畟極⡤潲⹷敧⡴⤰⤿ਬ††††††††潲⹷敧⡴⤱ⰿ †††††††爠睯朮瑥㈨㼩ਬ††††††††潲⹷敧⡴⤳ⰿ †††††††爠睯朮瑥㐨㼩ਬ††††††††潲⹷敧⡴⤵ⰿ †††††⤠਩††††⥽㬿 †††氠瑥洠瑵爠獥汵獴㴠嘠捥㨺敮⡷㬩映牯爠睯椠⁮潲獷笠爠獥汵獴瀮獵⡨潲㽷㬩素传⡫敲畳瑬⥳ †素紊਍
use crate::domain::incident::{IncidentCorrelation, CorrelationType};

pub fn str_to_correlation_type(s: &str) -> CorrelationType {
    match s {
        "NODE_TO_VM" => CorrelationType::NodeToVm,
        "VM_TO_RESOURCE" => CorrelationType::VmToResource,
        "NETWORK_TO_RESOURCE" => CorrelationType::NetworkToResource,
        "GUEST_AGENT_TO_RESOURCE" => CorrelationType::GuestAgentToResource,
        "TEMPORAL" => CorrelationType::Temporal,
        _ => CorrelationType::Temporal,
    }
}

pub fn correlation_type_to_str(c: &CorrelationType) -> &'static str {
    match c {
        CorrelationType::NodeToVm => "NODE_TO_VM",
        CorrelationType::VmToResource => "VM_TO_RESOURCE",
        CorrelationType::NetworkToResource => "NETWORK_TO_RESOURCE",
        CorrelationType::GuestAgentToResource => "GUEST_AGENT_TO_RESOURCE",
        CorrelationType::Temporal => "TEMPORAL",
    }
}

pub struct IncidentCorrelationRepository<'a> { pub conn: &'a Connection }
impl<'a> IncidentCorrelationRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self { Self { conn } }
    
    pub fn insert(&self, corr: &IncidentCorrelation) -> Result<()> {
        self.conn.execute("INSERT INTO incident_correlations (id, parent_incident_id, child_incident_id, correlation_type, confidence_score, reason, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![uuid_to_string(corr.id), uuid_to_string(corr.parent_incident_id), uuid_to_string(corr.child_incident_id), correlation_type_to_str(&corr.correlation_type), corr.confidence_score, corr.reason, dt_to_string(corr.created_at)])?; Ok(())
    }

    pub fn get_by_child_id(&self, child_id: Uuid) -> Result<Option<IncidentCorrelation>> {
        self.conn.query_row("SELECT id, parent_incident_id, child_incident_id, correlation_type, confidence_score, reason, created_at FROM incident_correlations WHERE child_incident_id = ?1 LIMIT 1",
            params![uuid_to_string(child_id)],
            |row| Ok(IncidentCorrelation {
                id: string_to_uuid(row.get(0)?),
                parent_incident_id: string_to_uuid(row.get(1)?),
                child_incident_id: string_to_uuid(row.get(2)?),
                correlation_type: str_to_correlation_type(&row.get::<_, String>(3)?),
                confidence_score: row.get(4)?,
                reason: row.get(5)?,
                created_at: string_to_dt(row.get(6)?)
            })).optional()
    }
}

impl<'a> IncidentRepository<'a> {
    pub fn list_open_incidents(&self) -> Result<Vec<Incident>> {
        let mut stmt = self.conn.prepare("SELECT id, alert_id, state, severity, created_at, resolved_at FROM incidents WHERE state = 'OPEN' OR state = 'ACKNOWLEDGED'")?;
        let rows = stmt.query_map([], |row| {
            Ok(Incident {
                id: string_to_uuid(row.get(0)?),
                alert_id: string_to_uuid(row.get(1)?),
                state: str_to_incident_state(&row.get::<_, String>(2)?),
                severity: row.get(3)?,
                created_at: string_to_dt(row.get(4)?),
                resolved_at: opt_string_to_dt(row.get(5)?)
            })
        })?;
        let mut results = Vec::new(); for row in rows { results.push(row?); } Ok(results)
    }
}

impl<'a> ResourceRepository<'a> {
    pub fn get_by_id(&self, id: Uuid) -> Result<Option<crate::domain::resource::Resource>> {
        self.conn.query_row("SELECT id, vm_id, kind, identifier, state, created_at, updated_at, deleted_at FROM resources WHERE id = ?1", params![uuid_to_string(id)], |row| {
            Ok(crate::domain::resource::Resource {
                id: string_to_uuid(row.get(0)?),
                vm_id: string_to_uuid(row.get(1)?),
                kind: row.get(2)?,
                identifier: row.get(3)?,
                state: str_to_resource_state(&row.get::<_, String>(4)?),
                created_at: string_to_dt(row.get(5)?),
                updated_at: string_to_dt(row.get(6)?),
                deleted_at: opt_string_to_dt(row.get(7)?)
            })
        }).optional()
    }
}

pub struct NotificationRouteRepository<'a> { pub conn: &'a Connection }
impl<'a> NotificationRouteRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self { Self { conn } }
    
    pub fn insert(&self, route: &crate::domain::notification::NotificationRoute) -> rusqlite::Result<()> {
        self.conn.execute("INSERT INTO notification_routes (id, name, rule_id, severity, scope_type, scope_id, priority, template_id, channel_id, state, version, created_at, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            rusqlite::params![uuid_to_string(route.id), route.name, route.rule_id.map(uuid_to_string), route.severity, route.scope_type, route.scope_id.map(uuid_to_string), route.priority, uuid_to_string(route.template_id), uuid_to_string(route.channel_id), config_state_to_str(route.state), route.version, dt_to_string(route.created_at), dt_to_string(route.updated_at), opt_dt_to_string(route.deleted_at)])?;
        Ok(())
    }

    pub fn list_active(&self) -> rusqlite::Result<Vec<crate::domain::notification::NotificationRoute>> {
        let mut stmt = self.conn.prepare("SELECT id, name, rule_id, severity, scope_type, scope_id, priority, template_id, channel_id, state, version, created_at, updated_at, deleted_at FROM notification_routes WHERE state = 'ENABLED' AND deleted_at IS NULL ORDER BY priority DESC")?;
        let rows = stmt.query_map([], |row| {
            Ok(crate::domain::notification::NotificationRoute {
                id: string_to_uuid(row.get(0)?),
                name: row.get(1)?,
                rule_id: row.get::<_, Option<String>>(2)?.map(string_to_uuid),
                severity: row.get(3)?,
                scope_type: row.get(4)?,
                scope_id: row.get::<_, Option<String>>(5)?.map(string_to_uuid),
                priority: row.get(6)?,
                template_id: string_to_uuid(row.get(7)?),
                channel_id: string_to_uuid(row.get(8)?),
                state: str_to_config_state(&row.get::<_, String>(9)?),
                version: row.get(10)?,
                created_at: string_to_dt(row.get(11)?),
                updated_at: string_to_dt(row.get(12)?),
                deleted_at: opt_string_to_dt(row.get(13)?)
            })
        })?;
        let mut results = Vec::new();
        for row in rows { results.push(row?); }
        Ok(results)
    }
}

