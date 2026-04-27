use crate::config::Config;

pub(super) fn service_is_healthy(state: &str, sub_state: &str) -> bool {
    matches!(state, "active" | "started") && matches!(sub_state, "running" | "started" | "active")
}

pub(super) fn is_public_bind_without_auth(cfg: &Config) -> bool {
    let auth_empty = cfg
        .metrics
        .auth
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .is_empty();
    let public_bind = matches!(
        cfg.metrics.listen_addr.as_str(),
        "0.0.0.0" | "::" | "[::]" | ""
    );
    auth_empty && public_bind
}
