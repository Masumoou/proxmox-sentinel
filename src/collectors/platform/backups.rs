use super::*;
use super::tasks::is_backup_task;

pub(super) async fn collect_backups(
    cfg: &PlatformConfig,
    policy: &BackupPolicyConfig,
    guests: &[crate::proxmox_api::GuestStatus],
    tasks: &[TaskHealth],
    artifacts: &[BackupArtifact],
    alerts: &mut Vec<Alert>,
) -> Vec<BackupHealth> {
    let now = chrono::Utc::now().timestamp();
    let mut latest_artifact: HashMap<u32, &BackupArtifact> = HashMap::new();
    let mut latest_task: HashMap<u32, &TaskHealth> = HashMap::new();

    for artifact in artifacts {
        let replace = latest_artifact
            .get(&artifact.vmid)
            .map(|old| artifact.ctime > old.ctime)
            .unwrap_or(true);
        if replace {
            latest_artifact.insert(artifact.vmid, artifact);
        }
    }

    for task in tasks {
        if !is_backup_task(&task.worker_type) {
            continue;
        }
        if let Some(vmid) = task.vmid {
            let replace = latest_task
                .get(&vmid)
                .map(|old| task.start_time > old.start_time)
                .unwrap_or(true);
            if replace {
                latest_task.insert(vmid, task);
            }
        }
    }

    let mut rows = Vec::new();
    for guest in guests {
        let requirement = backup_requirement(cfg, policy, guest);
        let artifact = latest_artifact.get(&guest.vmid).copied();
        let task = latest_task.get(&guest.vmid).copied();
        let last_backup_ts = artifact.map(|a| a.ctime);
        let age_hours = last_backup_ts.map(|ts| (now.saturating_sub(ts)) / 3600);
        let latest_task_status = task.map(|t| t.status.clone()).unwrap_or_else(|| "none".to_string());
        let status = if !requirement.required {
            "ignored".to_string()
        } else {
            match age_hours {
                Some(age) if age >= requirement.critical_hours as i64 => "critical",
                Some(age) if age >= requirement.warn_hours as i64 => "warning",
                Some(_) => "ok",
                None => "critical",
            }.to_string()
        };

        if requirement.required && status != "ok" {
            let summary = if let Some(age) = age_hours {
                format!("Guest {} ({}) latest backup artifact is {age}h old", guest.name, guest.vmid)
            } else {
                format!("Guest {} ({}) has no backup artifact found", guest.name, guest.vmid)
            };
            alerts.push(platform_alert(format!("backup:{}:{}", guest.vmid, status), &status, summary));
        }

        rows.push(BackupHealth {
            vmid: guest.vmid,
            name: guest.name.clone(),
            node: guest.node.clone(),
            kind: match guest.kind { GuestKind::Vm => "qemu", GuestKind::Lxc => "lxc" }.to_string(),
            last_backup_ts,
            age_hours,
            status,
            task_status: latest_task_status,
            size_bytes: artifact.and_then(|a| a.size_bytes),
            source: artifact
                .map(|a| format!("{}:{} ({})", a.node, a.storage, a.volid))
                .unwrap_or_else(|| "none".to_string()),
        });
    }
    rows
}

struct BackupRequirement {
    required: bool,
    warn_hours: u64,
    critical_hours: u64,
}

fn backup_requirement(
    cfg: &PlatformConfig,
    policy: &BackupPolicyConfig,
    guest: &crate::proxmox_api::GuestStatus,
) -> BackupRequirement {
    if !policy.enabled
        || cfg.exclude_backup_vmids.contains(&guest.vmid)
        || policy.exclude_vmids.contains(&guest.vmid)
        || (cfg.ignore_stopped_guests_for_backup && guest.status != "running")
        || (policy.ignore_stopped_guests && guest.status != "running")
        || (cfg.ignore_templates && guest.template)
        || (policy.ignore_templates && guest.template)
        || tag_matches(&guest.tags, &policy.exclude_tags)
    {
        return BackupRequirement {
            required: false,
            warn_hours: policy.warn_hours,
            critical_hours: policy.critical_hours,
        };
    }

    if let Some(rule) = policy.tag_rules.iter().find(|rule| tag_matches(&guest.tags, std::slice::from_ref(&rule.tag))) {
        return BackupRequirement {
            required: rule.required,
            warn_hours: rule.warn_hours,
            critical_hours: rule.critical_hours,
        };
    }

    let included_by_tag = policy.include_tags.is_empty() || tag_matches(&guest.tags, &policy.include_tags);
    BackupRequirement {
        required: policy.default_required && included_by_tag,
        warn_hours: policy.warn_hours,
        critical_hours: policy.critical_hours,
    }
}

fn tag_matches(guest_tags: &[String], policy_tags: &[String]) -> bool {
    guest_tags.iter().any(|guest_tag| {
        policy_tags.iter().any(|policy_tag| guest_tag.eq_ignore_ascii_case(policy_tag))
    })
}

pub(super) async fn collect_backup_artifacts(
    client: Arc<ProxmoxClient>,
    nodes: Arc<Vec<String>>,
) -> Vec<BackupArtifact> {
    let mut artifacts = Vec::new();

    for node in nodes.iter() {
        let storages = match client.storage_status(node).await {
            Ok(storages) => storages,
            Err(e) => {
                debug!("backup storage list {node}: {e}");
                continue;
            }
        };

        for storage in storages
            .iter()
            .filter(|s| s.enabled && s.active && s.content.split(',').any(|c| c.trim() == "backup"))
        {
            match client.storage_content(node, &storage.storage, "backup").await {
                Ok(rows) => {
                    artifacts.extend(rows.iter().filter_map(|row| parse_backup_artifact(row, node, &storage.storage)));
                }
                Err(e) => debug!("backup content {node}/{}: {e}", storage.storage),
            }
        }
    }

    artifacts.extend(scan_local_backup_artifacts().await);

    let mut dedup: HashMap<String, BackupArtifact> = HashMap::new();
    for artifact in artifacts {
        let key = if artifact.volid.is_empty() {
            format!("{}:{}:{}", artifact.node, artifact.vmid, artifact.ctime)
        } else {
            artifact.volid.clone()
        };
        dedup.entry(key).or_insert(artifact);
    }
    dedup.into_values().collect()
}


pub(super) fn parse_backup_artifact(row: &Value, node: &str, storage: &str) -> Option<BackupArtifact> {
    let volid = str_field(row, "volid").or_else(|| str_field(row, "volume")).unwrap_or_default();
    let vmid = int_field(row, "vmid")
        .and_then(|v| u32::try_from(v).ok())
        .or_else(|| parse_vmid_from_backup_name(&volid))?;
    let ctime = int_field(row, "ctime").or_else(|| parse_backup_timestamp(&volid)).unwrap_or(0);
    Some(BackupArtifact {
        vmid,
        node: node.to_string(),
        storage: storage.to_string(),
        volid,
        ctime,
        size_bytes: int_field(row, "size").and_then(|v| u64::try_from(v).ok()),
    })
}

async fn scan_local_backup_artifacts() -> Vec<BackupArtifact> {
    let mut artifacts = Vec::new();
    for dir in ["/var/lib/vz/dump", "/mnt/pve"] {
        scan_backup_dir(dir, &mut artifacts).await;
    }
    artifacts
}

async fn scan_backup_dir(path: &str, artifacts: &mut Vec<BackupArtifact>) {
    let mut stack = vec![std::path::PathBuf::from(path)];
    while let Some(dir) = stack.pop() {
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let file_type = match entry.file_type().await {
                Ok(file_type) => file_type,
                Err(_) => continue,
            };
            let path = entry.path();
            if file_type.is_dir() {
                if path.file_name().and_then(|n| n.to_str()) == Some("dump")
                    || dir.as_path() == std::path::Path::new("/mnt/pve")
                {
                    stack.push(path);
                }
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.starts_with("vzdump-") {
                continue;
            }
            let Some(vmid) = parse_vmid_from_backup_name(name) else {
                continue;
            };
            let metadata = entry.metadata().await.ok();
            artifacts.push(BackupArtifact {
                vmid,
                node: "local".to_string(),
                storage: "local-scan".to_string(),
                volid: path.display().to_string(),
                ctime: parse_backup_timestamp(name)
                    .or_else(|| metadata.as_ref().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64))
                    .unwrap_or(0),
                size_bytes: metadata.map(|m| m.len()),
            });
        }
    }
}

fn parse_vmid_from_backup_name(name: &str) -> Option<u32> {
    let file = name.rsplit('/').next().unwrap_or(name);
    let parts: Vec<&str> = file.split('-').collect();
    if parts.len() < 3 || parts[0] != "vzdump" {
        return None;
    }
    parts[2].parse().ok()
}

fn parse_backup_timestamp(name: &str) -> Option<i64> {
    let file = name.rsplit('/').next().unwrap_or(name);
    let parts: Vec<&str> = file.split('-').collect();
    if parts.len() < 5 || parts[0] != "vzdump" {
        return None;
    }
    let raw = format!("{}-{}", parts[3], parts[4]);
    let trimmed = raw
        .trim_end_matches(".vma.zst")
        .trim_end_matches(".vma.lzo")
        .trim_end_matches(".vma.gz")
        .trim_end_matches(".tar.zst")
        .trim_end_matches(".tar.lzo")
        .trim_end_matches(".tar.gz");
    chrono::NaiveDateTime::parse_from_str(trimmed, "%Y_%m_%d-%H_%M_%S")
        .ok()
        .map(|dt| dt.and_utc().timestamp())
}
