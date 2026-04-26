use super::*;
use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
use openssl::x509::X509;
use std::net::TcpStream;
use std::time::Duration as StdDuration;

pub(super) async fn collect_certs(
    cfg: &CertificateConfig,
    alerts: &mut Vec<Alert>,
) -> Vec<CertCheck> {
    let mut targets = vec![(
        "proxmox-local".to_string(),
        "local-pveproxy-cert".to_string(),
    )];
    targets.extend(cfg.targets.iter().map(|t| (t.name.clone(), t.url.clone())));

    let mut rows = Vec::new();
    for (name, url) in targets {
        let check = if url == "local-pveproxy-cert" {
            check_local_cert(&name, cfg).await
        } else {
            check_remote_cert(&name, &url, cfg).await
        };
        if matches!(check.status.as_str(), "warning" | "critical") {
            alerts.push(platform_alert(
                format!("cert:{}", check.name),
                &check.status,
                format!("Certificate {}: {}", check.name, check.detail),
            ));
        }
        rows.push(check);
    }
    rows
}

async fn check_local_cert(name: &str, cfg: &CertificateConfig) -> CertCheck {
    match tokio::fs::read("/etc/pve/local/pveproxy-ssl.pem").await {
        Ok(bytes) => match not_after_from_pem(&bytes) {
            Ok(out) => cert_from_not_after(name, "local-pveproxy-cert", &out, cfg),
            Err(e) => CertCheck {
                name: name.into(),
                url: "local-pveproxy-cert".into(),
                status: "unknown".into(),
                days_remaining: None,
                expires_at: None,
                detail: format!("local certificate parse failed: {e}"),
            },
        },
        Err(e) => CertCheck {
            name: name.into(),
            url: "local-pveproxy-cert".into(),
            status: "unknown".into(),
            days_remaining: None,
            expires_at: None,
            detail: format!("local certificate unreadable: {e}"),
        },
    }
}

async fn check_remote_cert(name: &str, url: &str, cfg: &CertificateConfig) -> CertCheck {
    let parsed = Url::parse(url);
    let Ok(parsed) = parsed else {
        return CertCheck {
            name: name.into(),
            url: url.into(),
            status: "critical".into(),
            days_remaining: None,
            expires_at: None,
            detail: "invalid URL".into(),
        };
    };
    if parsed.scheme() != "https" {
        return CertCheck {
            name: name.into(),
            url: url.into(),
            status: "unknown".into(),
            days_remaining: None,
            expires_at: None,
            detail: "certificate checks require https URL".into(),
        };
    }
    let Some(host) = parsed.host_str() else {
        return CertCheck {
            name: name.into(),
            url: url.into(),
            status: "critical".into(),
            days_remaining: None,
            expires_at: None,
            detail: "missing host".into(),
        };
    };
    let port = parsed.port_or_known_default().unwrap_or(443);
    match fetch_remote_cert_enddate(host, port).await {
        Ok(out) => cert_from_not_after(name, url, &out, cfg),
        Err(e) => CertCheck {
            name: name.into(),
            url: url.into(),
            status: "critical".into(),
            days_remaining: None,
            expires_at: None,
            detail: format!("certificate probe failed: {e}"),
        },
    }
}

fn not_after_from_pem(bytes: &[u8]) -> Result<String> {
    let cert = X509::from_pem(bytes)?;
    Ok(format!("notAfter={}", cert.not_after()))
}

fn cert_from_not_after(name: &str, url: &str, out: &str, cfg: &CertificateConfig) -> CertCheck {
    let raw = out.trim().strip_prefix("notAfter=").unwrap_or("").trim();
    if raw.is_empty() {
        return CertCheck {
            name: name.into(),
            url: url.into(),
            status: "unknown".into(),
            days_remaining: None,
            expires_at: None,
            detail: "certificate expiry unavailable".into(),
        };
    }
    let parsed_ts = chrono::DateTime::parse_from_str(raw, "%b %e %H:%M:%S %Y %Z")
        .map(|dt| dt.timestamp())
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(raw, "%b %e %H:%M:%S %Y GMT")
                .map(|dt| dt.and_utc().timestamp())
        })
        .ok();
    let days = parsed_ts.map(|ts| (ts - chrono::Utc::now().timestamp()) / 86400);
    let status = match days {
        Some(d) if d < 0 => "critical",
        Some(d) if d <= cfg.critical_days as i64 => "critical",
        Some(d) if d <= cfg.warn_days as i64 => "warning",
        Some(_) => "ok",
        None => "unknown",
    }
    .to_string();
    CertCheck {
        name: name.into(),
        url: url.into(),
        status,
        days_remaining: days,
        expires_at: parsed_ts.and_then(|ts| {
            chrono::DateTime::<chrono::Utc>::from_timestamp(ts, 0).map(|d| d.to_rfc3339())
        }),
        detail: raw.into(),
    }
}

async fn fetch_remote_cert_enddate(host: &str, port: u16) -> Result<String> {
    let host = host.to_string();
    tokio::task::spawn_blocking(move || fetch_remote_cert_enddate_blocking(&host, port)).await?
}

fn fetch_remote_cert_enddate_blocking(host: &str, port: u16) -> Result<String> {
    let mut builder = SslConnector::builder(SslMethod::tls())?;
    // We are monitoring certificate metadata, including self-signed or expired certs,
    // so verification is intentionally disabled for this probe only.
    builder.set_verify(SslVerifyMode::NONE);
    let connector = builder.build();
    let stream = TcpStream::connect((host, port))?;
    stream.set_read_timeout(Some(StdDuration::from_secs(10)))?;
    stream.set_write_timeout(Some(StdDuration::from_secs(10)))?;
    let stream = connector
        .connect(host, stream)
        .map_err(|e| anyhow::anyhow!("TLS handshake failed: {e}"))?;
    let cert = stream
        .ssl()
        .peer_certificate()
        .ok_or_else(|| anyhow::anyhow!("peer did not present a certificate"))?;
    Ok(format!("notAfter={}", cert.not_after()))
}

#[cfg(test)]
mod tests {
    use super::cert_from_not_after;
    use crate::config::CertificateConfig;

    #[test]
    fn parses_certificate_expiry_as_ok_for_future_date() {
        let cfg = CertificateConfig::default();
        let check = cert_from_not_after(
            "test",
            "https://example.com",
            "notAfter=Apr 25 12:00:00 2099 GMT",
            &cfg,
        );
        assert_eq!(check.status, "ok");
        assert!(check.days_remaining.unwrap_or_default() > 0);
    }
}
