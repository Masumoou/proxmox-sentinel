<script lang="ts">
  import { enrichedGuests, formatBytes, haproxyStats, nodes, pct, platformHealth, reconnectAttempts, storagePools, wsConnected } from '$lib/store';

  let runningGuests = $derived($enrichedGuests.filter((guest) => guest.status === 'running'));
  let stoppedGuests = $derived($enrichedGuests.filter((guest) => guest.status !== 'running'));
  let visibleGuests = $derived($enrichedGuests.slice(0, 18));
  let clusterCpu = $derived($nodes.length ? Math.round(($nodes.reduce((sum, n) => sum + n.cpu, 0) / $nodes.length) * 100) : 0);
  let clusterMemUsed = $derived($nodes.reduce((sum, n) => sum + n.mem_used, 0));
  let clusterMemTotal = $derived($nodes.reduce((sum, n) => sum + n.mem_total, 0));
  let clusterStorageUsed = $derived($storagePools.reduce((sum, s) => sum + (s.used || 0), 0));
  let clusterStorageTotal = $derived($storagePools.reduce((sum, s) => sum + (s.total || 0), 0));
  let platformIssues = $derived(countPlatformIssues($platformHealth));
  let backupIssues = $derived(countBackupIssues($platformHealth));
  let serviceIssues = $derived($enrichedGuests.reduce((sum, guest) => sum + guest.services.filter((service: any) => service.status !== 'running').length, 0));
  let showWebhook = $state(false);
  let webhookTestUrl = $state('');
  let webhookStatus = $state<'idle' | 'sending' | 'ok' | 'error'>('idle');
  let webhookMessage = $state('');

  function gaugeStyle(percent: number, color: string) {
    const clamped = Math.min(100, Math.max(0, percent || 0));
    return `background: conic-gradient(${color} ${clamped * 3.6}deg, rgba(255,255,255,0.06) 0deg);`;
  }

  function serviceSummary(guest: any) {
    if (guest.services.length > 0) {
      const up = guest.services.filter((service: any) => service.status === 'running').length;
      return `${up}/${guest.services.length} services`;
    }
    if (guest.type === 'QEMU') return 'Agent/SSH unavailable';
    return 'No services discovered';
  }

  function primaryIp(guest: any) {
    return guest.ip || 'IP unknown';
  }

  function osLabel(guest: any) {
    if (guest.os_name && guest.os_version) return `${guest.os_name} ${guest.os_version}`;
    if (guest.os_name) return guest.os_name;
    return 'OS unknown';
  }

  function countPlatformIssues(health: any) {
    const arrays = ['zfs', 'backups', 'tasks', 'thin_pools', 'snapshots', 'security', 'certificates', 'guest_agents'];
    let count = 0;
    for (const key of arrays) {
      for (const item of health?.[key] || []) {
        const status = String(item.status || item.state || item.severity || '').toLowerCase();
        if (['warning', 'critical', 'degraded', 'faulted', 'failed'].includes(status)) count += 1;
      }
    }
    for (const item of [health?.cluster, health?.ceph]) {
      const status = String(item?.quorum || item?.health || '').toLowerCase();
      if (['warning', 'critical', 'degraded', 'faulted', 'health_warn', 'health_err'].includes(status)) count += 1;
    }
    return count;
  }

  function countBackupIssues(health: any) {
    return (health?.backups || []).filter((backup: any) => ['warning', 'critical'].includes(String(backup.status || '').toLowerCase())).length;
  }

  async function sendWebhookTest() {
    if (!webhookTestUrl.trim()) {
      webhookStatus = 'error';
      webhookMessage = 'Enter a webhook URL first.';
      return;
    }

    webhookStatus = 'sending';
    webhookMessage = 'Sending test alert...';

    try {
      const res = await fetch('/api/v1/alerts/test', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ webhook_url: webhookTestUrl.trim() }),
      });
      webhookStatus = res.ok ? 'ok' : 'error';
      webhookMessage = res.ok ? 'Test alert sent.' : 'Webhook test failed.';
    } catch {
      webhookStatus = 'error';
      webhookMessage = 'Could not reach Sentinel API.';
    }
  }
</script>

<div class="dashboard-page">
  <header class="dash-header">
    <div>
      <div class="eyebrow">PROXMOX SENTINEL v0.2.12</div>
      <h1>Cluster Overview</h1>
    </div>
    <div class="header-actions">
      <div class="live-pill" class:online={$wsConnected}>
        <span></span>
        {$wsConnected ? 'LIVE TELEMETRY' : `RECONNECTING ${$reconnectAttempts ? `(${ $reconnectAttempts })` : ''}`}
      </div>
      <button class="webhook-button" onclick={() => showWebhook = true}>Webhook Integration</button>
    </div>
  </header>

  <section class="summary-grid">
    <div class="summary-card">
      <span>Nodes</span>
      <strong>{$nodes.length}</strong>
      <small>{$nodes.filter((n) => n.status === 'online').length} online</small>
    </div>
    <div class="summary-card">
      <span>Guests</span>
      <strong>{runningGuests.length}</strong>
      <small>{stoppedGuests.length} stopped/templates</small>
    </div>
    <div class="summary-card">
      <span>Cluster CPU</span>
      <strong>{clusterCpu}%</strong>
      <small>average across nodes</small>
    </div>
    <div class="summary-card">
      <span>Cluster RAM</span>
      <strong>{Math.round(pct(clusterMemUsed, clusterMemTotal))}%</strong>
      <small>{formatBytes(clusterMemUsed)} / {formatBytes(clusterMemTotal)}</small>
    </div>
    <div class="summary-card">
      <span>Storage</span>
      <strong>{Math.round(pct(clusterStorageUsed, clusterStorageTotal))}%</strong>
      <small>{$storagePools.length} pools</small>
    </div>
    <div class="summary-card">
      <span>Platform Health</span>
      <strong>{platformIssues}</strong>
      <small>{platformIssues === 0 ? 'no issues reported' : 'items need review'}</small>
    </div>
    <div class="summary-card">
      <span>Backups</span>
      <strong>{backupIssues}</strong>
      <small>{backupIssues === 0 ? 'policy OK' : 'stale/missing backups'}</small>
    </div>
    <div class="summary-card">
      <span>Services</span>
      <strong>{serviceIssues}</strong>
      <small>{serviceIssues === 0 ? 'no down services seen' : 'not running'}</small>
    </div>
    <div class="summary-card">
      <span>HAProxy</span>
      <strong>{$haproxyStats?.servers_up ?? 0}</strong>
      <small>{($haproxyStats?.servers_down ?? 0)} down</small>
    </div>
  </section>

  <section class="workspace-grid">
    <div class="panel guests-panel">
      <div class="panel-title">
        <span>Guest Health</span>
        <small>{visibleGuests.length} shown</small>
      </div>

      {#if visibleGuests.length === 0}
        <div class="empty">Waiting for cluster data...</div>
      {:else}
        <div class="guest-grid">
          {#each visibleGuests as guest (guest.vmid)}
            <article class="guest-card" class:stopped={guest.status !== 'running'}>
              <div class="guest-head">
                <div>
                  <h2>{guest.name}</h2>
                  <p>{guest.vmid} · {guest.node} · {primaryIp(guest)}</p>
                </div>
                <span class="type-badge" title={guest.type}>{osLabel(guest)}</span>
              </div>

              <div class="metric-strip">
                <div class="mini-gauge" style={gaugeStyle(Math.round(guest.cpu * 100), 'var(--accent-magenta)')}>
                  <div><strong>{Math.round(guest.cpu * 100)}%</strong><span>{guest.maxcpu ? `${guest.maxcpu} vCPU` : 'CPU'}</span></div>
                </div>
                <div class="mini-gauge" style={gaugeStyle(pct(guest.mem, guest.maxmem), 'var(--accent-cyan)')}>
                  <div><strong>{Math.round(pct(guest.mem, guest.maxmem))}%</strong><span>{formatBytes(guest.mem)}</span></div>
                </div>
              </div>

              <div class="guest-footer">
                <span class:ok={guest.status === 'running'} class:bad={guest.status !== 'running'}>{guest.status.toUpperCase()}</span>
                <span>{serviceSummary(guest)}</span>
              </div>
            </article>
          {/each}
        </div>
      {/if}
    </div>

    <aside class="panel side-panel">
      <div class="panel-title"><span>Operational Notes</span></div>
      <div class="note-list">
        {#if $enrichedGuests.some((g) => g.type === 'QEMU' && g.status === 'running' && !g.agent && !g.ssh)}
          <div class="note warn">Some VMs have no QEMU Guest Agent or SSH visibility, so IPs, disks, and services are limited.</div>
        {/if}
        {#if !$haproxyStats}
          <div class="note">HAProxy needs `[haproxy] enabled = true` and a reachable stats CSV URL.</div>
        {:else}
          <div class="note ok">HAProxy telemetry is live: {$haproxyStats.servers_up} up, {$haproxyStats.servers_down} down.</div>
        {/if}
        {#if platformIssues > 0}
          <div class="note warn">Platform health reports {platformIssues} issue{platformIssues === 1 ? '' : 's'} across backups, storage, snapshots, security, certificates, or guest agents.</div>
        {:else}
          <div class="note ok">Platform health is clean from the latest collector run.</div>
        {/if}
        <div class="note">Storage pools now come from Proxmox API. Guest mount data still requires guest agent/SSH.</div>
      </div>
    </aside>
  </section>
</div>

{#if showWebhook}
  <div class="modal-backdrop" role="button" tabindex="0" onclick={() => showWebhook = false} onkeydown={(event) => event.key === 'Escape' && (showWebhook = false)}>
    <div class="modal-panel" role="dialog" aria-modal="true" aria-labelledby="webhook-title" tabindex="-1" onclick={(event) => event.stopPropagation()} onkeydown={(event) => event.stopPropagation()}>
      <div class="modal-head">
        <div>
          <span>Alerts</span>
          <h2 id="webhook-title">Webhook Integration</h2>
        </div>
        <button class="close-button" aria-label="Close webhook integration" onclick={() => showWebhook = false}>×</button>
      </div>

      <label for="webhook-url">Webhook URL</label>
      <div class="webhook-row">
        <input id="webhook-url" bind:value={webhookTestUrl} placeholder="https://discord.com/api/webhooks/..." />
        <button onclick={sendWebhookTest} disabled={webhookStatus === 'sending'}>{webhookStatus === 'sending' ? 'Sending' : 'Send Test'}</button>
      </div>

      {#if webhookMessage}
        <p class:ok={webhookStatus === 'ok'} class:bad={webhookStatus === 'error'}>{webhookMessage}</p>
      {/if}
      <small>For persistent alerts, set `[alerts].webhook_url` in `config.toml`.</small>
    </div>
  </div>
{/if}

<style>
  .dashboard-page { display: flex; flex-direction: column; gap: 18px; min-width: 0; }
  .dash-header { display: flex; align-items: center; justify-content: space-between; gap: 16px; }
  .header-actions { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; justify-content: flex-end; }
  .eyebrow { color: var(--accent-cyan); letter-spacing: 3px; font-size: 0.72rem; font-weight: 800; }
  h1 { font-size: 1.6rem; letter-spacing: 1px; margin-top: 4px; }
  .live-pill { display: inline-flex; align-items: center; gap: 8px; color: var(--accent-red); font-size: 0.68rem; font-weight: 800; letter-spacing: 1.5px; }
  .live-pill span { width: 8px; height: 8px; border-radius: 50%; background: var(--accent-red); box-shadow: 0 0 10px var(--accent-red); }
  .live-pill.online { color: var(--accent-green); }
  .live-pill.online span { background: var(--accent-green); box-shadow: 0 0 10px var(--accent-green); }
  .webhook-button, .webhook-row button, .close-button { border: 1px solid var(--border-color); background: rgba(255,255,255,0.04); color: var(--text-primary); border-radius: 6px; cursor: pointer; font-weight: 800; }
  .webhook-button { padding: 8px 11px; font-size: 0.66rem; letter-spacing: 1px; text-transform: uppercase; }
  .webhook-button:hover, .webhook-row button:hover { border-color: var(--accent-cyan); color: var(--accent-cyan); }
  .summary-grid { display: grid; grid-template-columns: repeat(6, minmax(120px, 1fr)); gap: 10px; }
  .summary-card, .panel, .guest-card { background: var(--card-bg); border: 1px solid var(--border-color); border-radius: 8px; }
  .summary-card { min-height: 92px; padding: 14px; display: flex; flex-direction: column; justify-content: space-between; }
  .summary-card span, .panel-title small, .guest-head p, .guest-footer { color: var(--text-secondary); }
  .summary-card span { font-size: 0.62rem; letter-spacing: 2px; text-transform: uppercase; }
  .summary-card strong { font-size: 1.5rem; color: var(--text-primary); }
  .summary-card small { color: var(--text-dim); }
  .workspace-grid { display: grid; grid-template-columns: minmax(0, 1fr) 320px; gap: 14px; align-items: start; }
  .panel { padding: 16px; min-width: 0; }
  .panel-title { display: flex; justify-content: space-between; align-items: center; margin-bottom: 14px; color: var(--text-primary); font-weight: 800; letter-spacing: 1.5px; text-transform: uppercase; font-size: 0.72rem; }
  .guest-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 12px; }
  .guest-card { height: 210px; padding: 14px; display: flex; flex-direction: column; justify-content: space-between; overflow: hidden; }
  .guest-card.stopped { opacity: 0.62; }
  .guest-head { display: flex; justify-content: space-between; align-items: flex-start; gap: 12px; min-height: 58px; }
  .guest-head h2 { font-size: 1rem; line-height: 1.2; overflow-wrap: anywhere; }
  .guest-head p { font-size: 0.65rem; margin-top: 4px; }
  .type-badge { flex: 0 1 130px; max-width: 130px; color: var(--accent-cyan); background: rgba(0, 212, 255, 0.1); border: 1px solid rgba(0, 212, 255, 0.2); border-radius: 4px; padding: 3px 7px; font-size: 0.58rem; font-weight: 800; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; text-align: right; }
  .metric-strip { display: flex; gap: 14px; align-items: center; }
  .mini-gauge { width: 72px; height: 72px; border-radius: 50%; display: grid; place-items: center; }
  .mini-gauge > div { width: 54px; height: 54px; border-radius: 50%; background: var(--card-bg); display: grid; place-items: center; text-align: center; }
  .mini-gauge strong { font-size: 0.76rem; line-height: 1; max-width: 46px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .mini-gauge span { display: block; margin-top: 3px; color: var(--text-secondary); font-size: 0.52rem; letter-spacing: 1.5px; }
  .guest-footer { display: flex; justify-content: space-between; gap: 10px; font-size: 0.64rem; letter-spacing: 0.8px; border-top: 1px solid rgba(255,255,255,0.04); padding-top: 10px; }
  .ok { color: var(--accent-green) !important; }
  .bad, .warn { color: var(--accent-red) !important; }
  .note-list { display: flex; flex-direction: column; gap: 10px; }
  .note { border: 1px solid rgba(255,255,255,0.08); border-left: 3px solid var(--accent-cyan); border-radius: 6px; padding: 10px; color: var(--text-secondary); font-size: 0.72rem; line-height: 1.45; }
  .note.ok { border-left-color: var(--accent-green); color: var(--text-primary); }
  .note.warn { border-left-color: var(--accent-red); }
  .empty { min-height: 260px; display: grid; place-items: center; color: var(--text-secondary); letter-spacing: 2px; }
  .modal-backdrop { position: fixed; inset: 0; z-index: 50; display: grid; place-items: center; background: rgba(0,0,0,0.72); padding: 20px; }
  .modal-panel { width: min(560px, 100%); background: var(--card-bg); border: 1px solid var(--accent-cyan); border-radius: 8px; padding: 18px; box-shadow: 0 0 28px rgba(0,212,255,0.16); }
  .modal-head { display: flex; align-items: flex-start; justify-content: space-between; gap: 14px; margin-bottom: 18px; }
  .modal-head span, label, .modal-panel small { color: var(--text-secondary); font-size: 0.68rem; letter-spacing: 1.6px; text-transform: uppercase; }
  .modal-head h2 { margin-top: 4px; font-size: 1.25rem; }
  .close-button { width: 32px; height: 32px; font-size: 1.1rem; }
  .webhook-row { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 10px; margin: 8px 0 12px; }
  input { min-width: 0; border: 1px solid var(--border-color); background: var(--panel-bg); color: var(--text-primary); border-radius: 6px; padding: 10px; }
  .webhook-row button { padding: 0 14px; }
  .webhook-row button:disabled { opacity: 0.55; cursor: not-allowed; }
  .modal-panel p { margin-bottom: 10px; font-size: 0.78rem; }
  @media (max-width: 1200px) {
    .summary-grid { grid-template-columns: repeat(3, 1fr); }
    .workspace-grid { grid-template-columns: 1fr; }
  }
  @media (max-width: 640px) {
    .dash-header { align-items: flex-start; flex-direction: column; }
    .header-actions { justify-content: flex-start; }
    .webhook-row { grid-template-columns: 1fr; }
    .webhook-row button { min-height: 38px; }
  }
</style>
