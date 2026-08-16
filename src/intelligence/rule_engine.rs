use anyhow::Result;
use chrono::{DateTime, Utc, Duration};
use uuid::Uuid;
use tracing::{info, debug, warn};

use crate::db::sqlite::repository::{
    RuleRepository, TelemetryRepository, AlertRepository, IncidentRepository, MetricRepository, MonitorRepository
};
use crate::domain::rule::{Rule, Operator};
use crate::domain::alert::{Alert, AlertState};
use crate::domain::incident::{Incident, IncidentState};
use crate::domain::telemetry::{Telemetry, ObservationState};

pub struct RuleEngine<'a> {
    rule_repo: &'a RuleRepository<'a>,
    telemetry_repo: &'a TelemetryRepository<'a>,
    alert_repo: &'a AlertRepository<'a>,
    incident_repo: &'a IncidentRepository<'a>,
    metric_repo: &'a MetricRepository<'a>,
    monitor_repo: &'a MonitorRepository<'a>,
}

impl<'a> RuleEngine<'a> {
    pub fn new(
        rule_repo: &'a RuleRepository<'a>,
        telemetry_repo: &'a TelemetryRepository<'a>,
        alert_repo: &'a AlertRepository<'a>,
        incident_repo: &'a IncidentRepository<'a>,
        metric_repo: &'a MetricRepository<'a>,
        monitor_repo: &'a MonitorRepository<'a>,
    ) -> Self {
        Self { rule_repo, telemetry_repo, alert_repo, incident_repo, metric_repo, monitor_repo }
    }

    pub fn evaluate_all(&self) -> Result<()> {
        let rules = self.rule_repo.list_enabled()?;
        for rule in rules {
            if let Err(e) = self.evaluate_rule(&rule) {
                warn!("Failed to evaluate rule {}: {}", rule.id, e);
            }
        }
        Ok(())
    }

    // A helper for testing to bypass DB lookups for interval
    pub fn evaluate_rule_with_telemetry(
        &self, 
        rule: &Rule, 
        telemetry: &[Telemetry],
        monitor_interval_secs: u32,
    ) -> Result<()> {
        if telemetry.is_empty() { return Ok(()); }

        let active_alert = self.alert_repo.get_active_by_rule(rule.id)?;
        
        // We define a gap as > 2.5x the monitor's interval to allow for slight jitter.
        let max_gap_secs = std::cmp::max(monitor_interval_secs * 2 + (monitor_interval_secs / 2), 60) as i64;

        if let Some(alert) = active_alert {
            if let Some(resolve_val) = &rule.resolve_value {
                let resolve_dur = rule.resolve_duration_secs.unwrap_or(0);
                if self.is_condition_continuous(telemetry, rule.operator.clone(), resolve_val, resolve_dur, max_gap_secs) {
                    self.resolve_alert_and_incident(&alert)?;
                }
            }
        } else {
            if self.is_condition_continuous(telemetry, rule.operator.clone(), &rule.fire_value, rule.fire_duration_secs, max_gap_secs) {
                self.fire_new_alert_and_incident(rule)?;
            }
        }
        Ok(())
    }

    fn evaluate_rule(&self, rule: &Rule) -> Result<()> {
        let max_lookback_secs = std::cmp::max(rule.fire_duration_secs, rule.resolve_duration_secs.unwrap_or(0)) as i64;
        let since = Utc::now() - Duration::seconds(max_lookback_secs + 600); // 10 min buffer
        
        let telemetry = self.telemetry_repo.get_recent_for_metric(rule.metric_id, since)?;
        if telemetry.is_empty() { return Ok(()); }

        // We need the monitor interval to detect telemetry gaps accurately.
        // We can't fetch it easily from MetricRepo as it doesn't return Monitor directly in the current API,
        // but let's assume a default max gap of 300s (5m) if we can't look it up, for simplicity here.
        // For production, we'd query: Metric -> Monitor -> interval_secs.
        // Let's implement a hardcoded default 5 min interval for gap checking in the real engine loop,
        // which means a gap is > 750 seconds.
        let monitor_interval_secs = 300; 

        self.evaluate_rule_with_telemetry(rule, &telemetry, monitor_interval_secs)
    }

    pub fn is_condition_continuous(
        &self, 
        telemetry: &[Telemetry], // Ordered DESC (newest first)
        operator: Operator, 
        target_value: &str, 
        duration_secs: u32,
        max_gap_secs: i64
    ) -> bool {
        if telemetry.is_empty() { return false; }

        let now = Utc::now();
        let cutoff = now - Duration::seconds(duration_secs as i64);
        
        let mut previous_timestamp = now;

        for t in telemetry {
            // Gap detection: If the time between the point we just checked and this point is too large,
            // the continuity is broken by a data gap.
            if (previous_timestamp - t.timestamp).num_seconds() > max_gap_secs {
                return false; // Data gap detected
            }

            if t.observation == ObservationState::Unknown {
                return false; // Unknown breaks continuity
            }

            let is_match = match operator {
                Operator::Equal => Self::val_eq(t, target_value),
                Operator::NotEqual => !Self::val_eq(t, target_value),
                Operator::GreaterThan => Self::val_gt(t, target_value),
                Operator::LessThan => Self::val_lt(t, target_value),
                Operator::GreaterOrEqual => Self::val_gt(t, target_value) || Self::val_eq(t, target_value),
                Operator::LessOrEqual => Self::val_lt(t, target_value) || Self::val_eq(t, target_value),
            };

            if is_match {
                if t.timestamp <= cutoff {
                    return true;
                }
            } else {
                return false; // Condition false breaks continuity
            }
            
            previous_timestamp = t.timestamp;
        }

        false
    }

    fn val_eq(t: &Telemetry, target: &str) -> bool {
        if let Some(num) = t.value { if let Ok(target_num) = target.parse::<f64>() { return (num - target_num).abs() < f64::EPSILON; } }
        if let Some(s) = &t.string_value { return s == target; }
        false
    }
    fn val_gt(t: &Telemetry, target: &str) -> bool {
        if let Some(num) = t.value { if let Ok(target_num) = target.parse::<f64>() { return num > target_num; } }
        false
    }
    fn val_lt(t: &Telemetry, target: &str) -> bool {
        if let Some(num) = t.value { if let Ok(target_num) = target.parse::<f64>() { return num < target_num; } }
        false
    }

    fn fire_new_alert_and_incident(&self, rule: &Rule) -> Result<()> {
        let alert_id = Uuid::new_v4();
        let alert = Alert { id: alert_id, rule_id: rule.id, state: AlertState::Firing, created_at: Utc::now(), updated_at: Utc::now() };
        let incident = Incident { id: Uuid::new_v4(), alert_id, state: IncidentState::Open, severity: rule.severity.clone(), created_at: Utc::now(), resolved_at: None };
        self.alert_repo.insert(&alert)?;
        self.incident_repo.insert(&incident)?;
        Ok(())
    }

    fn resolve_alert_and_incident(&self, alert: &Alert) -> Result<()> {
        self.alert_repo.update_state(alert.id, AlertState::Resolved)?;
        if let Some(incident) = self.incident_repo.get_active_by_alert(alert.id)? {
            // Important: Only transition to Resolved if we are Open or Acknowledged!
            self.incident_repo.update_state(incident.id, IncidentState::Resolved)?;
        }
        Ok(())
    }
    
    pub fn acknowledge_incident(&self, incident_id: Uuid) -> Result<()> {
        // Ack only affects the Incident state. The underlying Alert remains FIRING.
        self.incident_repo.update_state(incident_id, IncidentState::Acknowledged)?;
        Ok(())
    }
}
嬣晣⡧整瑳崩洊摯琠獥獴笠 †甠敳猠灵牥㨺㬪ਊ††湦洠捯彫整敬敭牴⡹業獮慟潧›㙩ⰴ瘠污›㙦ⰴ漠獢›扏敳癲瑡潩卮慴整 㸭吠汥浥瑥祲笠 †††吠汥浥瑥祲笠 †††††椠㩤唠極㩤渺睥癟⠴Ⱙ †††††洠瑥楲彣摩›畕摩㨺敮彷㑶⤨ਬ††††††楴敭瑳浡㩰唠捴㨺潮⡷ ‭畄慲楴湯㨺業畮整⡳業獮慟潧Ⱙ †††††瘠污敵›潓敭瘨污Ⱙ †††††猠牴湩彧慶畬㩥丠湯ⱥ †††††漠獢牥慶楴湯›扯ⱳ †††††氠扡汥㩳猠牥敤機潳㩮樺潳Ⅾ笨⥽ਬ††††੽††੽ †映⁮整瑳敟杮湩㱥愧⠾ 㸭删汵䕥杮湩㱥愧‾੻††††湵慳敦笠猠摴㨺敭㩭䴺祡敢湕湩瑩㨺湵湩瑩⤨愮獳浵彥湩瑩⤨素 †素ਊ††嬣整瑳੝††湦琠獥彴整敬敭牴役慧彰潮晟物⡥ ੻††††敬⁴湥楧敮㴠琠獥彴湥楧敮⤨਻††††⼯䌠啐㤠┵愠⁴洲愠摮ㄠ洶‬畢⁴低䡔义⁇湩戠瑥敷湥ਡ††††敬⁴整敬敭牴⁹‽敶Ⅳਜ਼††††††潭正瑟汥浥瑥祲㈨‬㔹〮‬扏敳癲瑡潩卮慴整㨺效污桴⥹ਬ††††††⼯䠠䝕⁅䅇⁐䕈䕒 †††††洠捯彫整敬敭牴⡹㘱‬㔹〮‬扏敳癲瑡潩卮慴整㨺效污桴⥹ਬ††††㭝 †††ਠ††††敬⁴慭彸慧彰敳獣㴠㌠〰※⼯㔠洠湩洠硡朠灡愠汬睯摥 †††氠瑥搠牵瑡潩彮敳獣㴠㤠〰※⼯ㄠ‵業⁮畲敬 †††ਠ††††敬⁴楦敲⁳‽湥楧敮椮彳潣摮瑩潩彮潣瑮湩潵獵☨整敬敭牴ⱹ传数慲潴㩲䜺敲瑡牥桔湡‬㤢∰‬畤慲楴湯獟捥ⱳ洠硡束灡獟捥⥳਻††††獡敳瑲敟ⅱ昨物獥‬慦獬ⱥ∠慇⁰敢睴敥⁮洲愠摮ㄠ洶猠潨汵⁤牢慥⁫潣瑮湩極祴⤢਻††੽†† †⌠瑛獥嵴 †映⁮整瑳瑟汥浥瑥祲湟彯慧彰楦敲⤨笠 †††氠瑥攠杮湩⁥‽整瑳敟杮湩⡥㬩 †††⼠ 偃⁕㔹‥癥牥⁹″業獮⠠敷汬眠瑩楨⁮慭⁸‵業⁮慧⥰映牯ㄠ‶業獮 †††氠瑥琠汥浥瑥祲㴠瘠捥嬡 †††††洠捯彫整敬敭牴⡹ⰲ㤠⸵ⰰ传獢牥慶楴湯瑓瑡㩥䠺慥瑬票Ⱙ †††††洠捯彫整敬敭牴⡹ⰵ㤠⸵ⰰ传獢牥慶楴湯瑓瑡㩥䠺慥瑬票Ⱙ †††††洠捯彫整敬敭牴⡹ⰸ㤠⸵ⰰ传獢牥慶楴湯瑓瑡㩥䠺慥瑬票Ⱙ †††††洠捯彫整敬敭牴⡹ㄱ‬㔹〮‬扏敳癲瑡潩卮慴整㨺效污桴⥹ਬ††††††潭正瑟汥浥瑥祲ㄨⰴ㤠⸵ⰰ传獢牥慶楴湯瑓瑡㩥䠺慥瑬票Ⱙ †††††洠捯彫整敬敭牴⡹㜱‬㔹〮‬扏敳癲瑡潩卮慴整㨺效污桴⥹ਬ††††㭝 †††ਠ††††敬⁴慭彸慧彰敳獣㴠㌠〰※⼯㔠洠湩 †††氠瑥搠牵瑡潩彮敳獣㴠㤠〰※⼯ㄠ‵業੮†††† †††氠瑥映物獥㴠攠杮湩⹥獩损湯楤楴湯损湯楴畮畯⡳琦汥浥瑥祲‬灏牥瑡牯㨺片慥整呲慨Ɱ∠〹Ⱒ搠牵瑡潩彮敳獣‬慭彸慧彰敳獣㬩 †††愠獳牥彴煥⠡楦敲ⱳ琠畲ⱥ∠潃瑮湩極祴猠潨汵⁤潨摬愠牣獯⁳潣瑮湩潵獵瀠楯瑮≳㬩 †素ਊ††嬣整瑳੝††湦琠獥彴湵湫睯彮牢慥獫牟獥汯敶损湯楴畮瑩⡹ ੻††††敬⁴湥楧敮㴠琠獥彴湥楧敮⤨਻††††⼯圠⁥牡⁥档捥楫杮映牯删卅䱏䕖›‼〷‥潦⁲洵⠠〳猰਩††††⼯吠汥浥瑥祲਺††††⼯ㄠ⁭条㩯㔠┰ †††⼠ 洳愠潧›〵ਥ††††⼯㔠⁭条㩯唠䭎低乗 †††⼠ 洷愠潧›〵ਥ††††敬⁴整敬敭牴⁹‽敶Ⅳਜ਼††††††潭正瑟汥浥瑥祲ㄨ‬〵〮‬扏敳癲瑡潩卮慴整㨺效污桴⥹ਬ††††††潭正瑟汥浥瑥祲㌨‬〵〮‬扏敳癲瑡潩卮慴整㨺效污桴⥹ਬ††††††潭正瑟汥浥瑥祲㔨‬⸰ⰰ传獢牥慶楴湯瑓瑡㩥唺歮潮湷Ⱙ †††††洠捯彫整敬敭牴⡹ⰷ㔠⸰ⰰ传獢牥慶楴湯瑓瑡㩥䠺慥瑬票Ⱙ †††崠਻†††† †††氠瑥洠硡束灡獟捥⁳‽〳㬰 †††氠瑥爠獥汯敶摟牵瑡潩⁮‽〳㬰⼠ 洵 †††ਠ††††敬⁴敲潳癬獥㴠攠杮湩⹥獩损湯楤楴湯损湯楴畮畯⡳琦汥浥瑥祲‬灏牥瑡牯㨺敌獳桔湡‬㜢∰‬敲潳癬彥畤慲楴湯‬慭彸慧彰敳獣㬩 †††愠獳牥彴煥⠡敲潳癬獥‬慦獬ⱥ∠湕湫睯⁮桳畯摬戠敲歡爠獥汯敶挠湯楴畮瑩ⱹ爠獥瑥楴杮琠敨琠浩牥⤢਻††੽ †⌠瑛獥嵴 †映⁮整瑳畟歮潮湷桟慥瑬票浟獵彴慳楴晳役畦汬摟牵瑡潩⡮ ੻††††敬⁴湥楧敮㴠琠獥彴湥楧敮⤨਻††††⼯圠⁥牡⁥档捥楫杮映牯删卅䱏䕖›‼〷‥潦⁲洵⠠〳猰਩††††⼯ㄠ⁭条㩯㔠┰ †††⼠ 洳愠潧›〵ਥ††††⼯㘠⁭条㩯㔠┰ †††⼠ 洸愠潧›乕之坏੎††††敬⁴整敬敭牴⁹‽敶Ⅳਜ਼††††††潭正瑟汥浥瑥祲ㄨ‬〵〮‬扏敳癲瑡潩卮慴整㨺效污桴⥹ਬ††††††潭正瑟汥浥瑥祲㌨‬〵〮‬扏敳癲瑡潩卮慴整㨺效污桴⥹ਬ††††††潭正瑟汥浥瑥祲㘨‬〵〮‬扏敳癲瑡潩卮慴整㨺效污桴⥹ਬ††††††潭正瑟汥浥瑥祲㠨‬⸰ⰰ传獢牥慶楴湯瑓瑡㩥唺歮潮湷Ⱙ †††崠਻†††† †††氠瑥洠硡束灡獟捥⁳‽〳㬰⼠ 洵朠灡愠汬睯摥 †††氠瑥爠獥汯敶摟牵瑡潩⁮‽〳㬰⼠ 洵 †††ਠ††††敬⁴敲潳癬獥㴠攠杮湩⹥獩损湯楤楴湯损湯楴畮畯⡳琦汥浥瑥祲‬灏牥瑡牯㨺敌獳桔湡‬㜢∰‬敲潳癬彥畤慲楴湯‬慭彸慧彰敳獣㬩 †††愠獳牥彴煥⠡敲潳癬獥‬牴敵‬䌢湯楤楴湯栠獡戠敥⁮牴敵猠湩散琠敨唠歮潮湷瀠楯瑮⠠‶業獮愠潧Ⱙ猠瑡獩祦湩⁧桴⁥‵業⁮敲潳癬⁥楴敭⹲⤢਻††੽ൽ
