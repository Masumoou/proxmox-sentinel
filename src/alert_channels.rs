use anyhow::Result;
use reqwest::Client;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct AlertNotification {
    pub key: String,
    pub severity: String,
    pub summary: String,
}

#[derive(Debug, Clone)]
pub enum AlertChannel {
    Webhook(WebhookChannel),
}

impl AlertChannel {
    // Keep provider-specific code behind this enum. Telegram, Discord, Slack,
    // SMTP, Gotify, ntfy.sh, Alertmanager, Grafana OnCall, PagerDuty, Opsgenie,
    // and Microsoft Teams can be added here without bloating AlertDispatcher.
    pub fn webhook(url: String) -> Self {
        Self::Webhook(WebhookChannel { url })
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Webhook(_) => "webhook",
        }
    }

    pub async fn send(&self, client: &Client, notification: &AlertNotification) -> Result<()> {
        match self {
            Self::Webhook(channel) => channel.send(client, notification).await,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WebhookChannel {
    url: String,
}

impl WebhookChannel {
    async fn send(&self, client: &Client, notification: &AlertNotification) -> Result<()> {
        let mut labels = HashMap::new();
        labels.insert("alertname".into(), notification.key.clone());
        labels.insert("severity".into(), notification.severity.clone());

        let mut annotations = HashMap::new();
        annotations.insert("summary".into(), notification.summary.clone());

        let payload = WebhookPayload {
            alerts: vec![WebhookAlert {
                status: "firing",
                labels,
                annotations,
                generator_url: String::new(),
            }],
        };

        client.post(&self.url).json(&payload).send().await?;
        Ok(())
    }
}

#[derive(Serialize)]
struct WebhookPayload {
    alerts: Vec<WebhookAlert>,
}

#[derive(Serialize)]
struct WebhookAlert {
    status: &'static str,
    labels: HashMap<String, String>,
    annotations: HashMap<String, String>,
    #[serde(rename = "generatorURL")]
    generator_url: String,
}
