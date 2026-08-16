<script lang="ts">
  import { page } from '$app/stores';
  import { slide, fade } from 'svelte/transition';

  let vmid = $derived($page.params.vmid);
  let vmName = $state("web-01");
  let healthScore = $state(92);

  // New Discovery Notification Simulation
  let newResourceDetected = $state(true);
  let newResourceName = $state("redis.service");

  function handleNewResource(action: 'monitored' | 'ignored' | 'ignored_until_changed') {
    newResourceDetected = false;
    if (action === 'monitored') {
      monitoredResources = [...monitoredResources, { kind: 'service', identifier: newResourceName, state: 'Pending Data' }];
    } else {
      ignoredResources = [...ignoredResources, { kind: 'service', identifier: newResourceName }];
    }
  }

  // Segmented Data
  let monitoredResources = $state([
    { kind: 'cpu', identifier: 'CPU', state: '43%' },
    { kind: 'memory', identifier: 'Memory', state: '61%' },
    { kind: 'service', identifier: 'nginx.service', state: 'Running' },
    { kind: 'service', identifier: 'postgresql.service', state: 'Running' },
    { kind: 'filesystem', identifier: '/var', state: '68% Full' },
    { kind: 'network', identifier: 'eth0', state: 'Healthy' }
  ]);

  let pendingResources = $state([
    { kind: 'service', identifier: 'docker.service' }
  ]);

  let ignoredResources = $state<{kind: string; identifier: string}[]>([
    { kind: 'service', identifier: 'cups.service' },
    { kind: 'service', identifier: 'bluetooth.service' }
  ]);

  function promoteToMonitored(resource: any, fromList: 'pending' | 'ignored') {
    if (fromList === 'pending') {
      pendingResources = pendingResources.filter(r => r !== resource);
    } else {
      ignoredResources = ignoredResources.filter(r => r !== resource);
    }
    monitoredResources = [...monitoredResources, { ...resource, state: 'Pending Data' }];
  }

  function demoteToIgnored(resource: any) {
    pendingResources = pendingResources.filter(r => r !== resource);
    ignoredResources = [...ignoredResources, { kind: resource.kind, identifier: resource.identifier }];
  }
</script>

<div class="page">
  <!-- New Resource Notification -->
  {#if newResourceDetected}
    <div class="notification-banner" transition:slide>
      <div class="notification-content">
        <div class="notification-icon">🟡</div>
        <div class="notification-text">
          <strong>New resource detected</strong>
          <span>{newResourceName} was discovered on this VM.</span>
        </div>
      </div>
      <div class="notification-actions">
        <button onclick={() => handleNewResource('monitored')} class="btn-sm primary">Monitor</button>
        <button onclick={() => handleNewResource('ignored')} class="btn-sm">Ignore</button>
        <button onclick={() => handleNewResource('ignored_until_changed')} class="btn-sm text-btn">Ignore Until Changed</button>
      </div>
    </div>
  {/if}

  <div class="page-header">
    <div>
      <h2>VM: <span class="vm-name">{vmName}</span></h2>
      <p>ID: {vmid} &bull; Proxmox Sentinel Observability</p>
    </div>
    <div class="health-badge">
      <div class="health-score">{healthScore}<span class="max-score">/100</span></div>
      <div class="health-label">System Health</div>
    </div>
  </div>

  <div class="dashboard-grid">

    <!-- MONITORED SECTION -->
    <div class="panel monitored-panel">
      <div class="panel-header">
        <div class="status-indicator active"></div>
        <h3>Monitored</h3>
      </div>
      <div class="resource-table">
        {#each monitoredResources as res}
          <div class="resource-row">
            <span class="res-kind badge-{res.kind}">{res.kind}</span>
            <span class="res-ident">{res.identifier}</span>
            <span class="res-state" class:problem={res.state !== 'Running' && !res.state.includes('%') && res.state !== 'Healthy'}>{res.state}</span>
          </div>
        {/each}
        {#if monitoredResources.length === 0}
          <div class="empty">No resources are actively monitored.</div>
        {/if}
      </div>
    </div>

    <!-- PENDING USER SECTION -->
    <div class="panel pending-panel">
      <div class="panel-header">
        <div class="status-indicator pending"></div>
        <h3>Pending User</h3>
      </div>
      <div class="resource-table">
        {#each pendingResources as res}
          <div class="resource-row">
            <span class="res-kind badge-{res.kind}">{res.kind}</span>
            <span class="res-ident">{res.identifier}</span>
            <div class="row-actions">
              <button onclick={() => promoteToMonitored(res, 'pending')} class="action-btn text-cyan">Monitor</button>
              <button onclick={() => demoteToIgnored(res)} class="action-btn text-muted">Ignore</button>
            </div>
          </div>
        {/each}
        {#if pendingResources.length === 0}
          <div class="empty">All resources classified.</div>
        {/if}
      </div>
    </div>

    <!-- IGNORED SECTION -->
    <div class="panel ignored-panel">
      <div class="panel-header">
        <div class="status-indicator ignored"></div>
        <h3>Ignored</h3>
      </div>
      <div class="resource-table">
        {#each ignoredResources as res}
          <div class="resource-row muted-row">
            <span class="res-kind badge-{res.kind}">{res.kind}</span>
            <span class="res-ident">{res.identifier}</span>
            <div class="row-actions">
              <button onclick={() => promoteToMonitored(res, 'ignored')} class="action-btn text-cyan">Monitor</button>
            </div>
          </div>
        {/each}
        {#if ignoredResources.length === 0}
          <div class="empty">No ignored resources.</div>
        {/if}
      </div>
    </div>

  </div>
</div>

<style>
  .page { display: flex; flex-direction: column; gap: 24px; padding-bottom: 40px; }

  .notification-banner { background: rgba(255, 208, 0, 0.1); border-left: 4px solid var(--accent-yellow); border-radius: 6px; padding: 14px 20px; display: flex; justify-content: space-between; align-items: center; box-shadow: 0 4px 15px rgba(0,0,0,0.2); }
  .notification-content { display: flex; align-items: center; gap: 14px; }
  .notification-icon { font-size: 1.4rem; }
  .notification-text { display: flex; flex-direction: column; gap: 2px; }
  .notification-text strong { color: var(--accent-yellow); font-size: 0.9rem; }
  .notification-text span { color: var(--text-primary); font-size: 0.85rem; }
  .notification-actions { display: flex; gap: 10px; align-items: center; }

  .btn-sm { padding: 6px 12px; font-size: 0.75rem; border-radius: 4px; font-weight: 700; cursor: pointer; border: 1px solid var(--border-color); background: rgba(255,255,255,0.05); color: var(--text-primary); }
  .btn-sm.primary { background: rgba(0,212,255,0.15); color: var(--accent-cyan); border-color: rgba(0,212,255,0.4); }
  .btn-sm.primary:hover { background: rgba(0,212,255,0.25); }
  .text-btn { background: none; border: none; padding: 0; color: var(--text-secondary); }
  .text-btn:hover { color: var(--text-primary); text-decoration: underline; }

  .page-header { display: flex; justify-content: space-between; align-items: center; padding-bottom: 10px; border-bottom: 1px solid rgba(255,255,255,0.05); }
  .page-header h2 { font-size: 1.4rem; color: var(--text-primary); margin: 0; letter-spacing: 1px; }
  .vm-name { color: var(--accent-cyan); }
  .page-header p { color: var(--text-secondary); font-size: 0.85rem; margin-top: 4px; }

  .health-badge { display: flex; flex-direction: column; align-items: flex-end; }
  .health-score { font-size: 2rem; font-weight: 900; color: var(--accent-green); line-height: 1; text-shadow: 0 0 10px rgba(0,255,136,0.2); }
  .max-score { font-size: 1rem; color: var(--text-secondary); font-weight: 700; }
  .health-label { font-size: 0.7rem; color: var(--text-secondary); text-transform: uppercase; letter-spacing: 1.5px; margin-top: 4px; font-weight: 700; }

  .dashboard-grid { display: flex; flex-direction: column; gap: 20px; }

  .panel { background: var(--card-bg); border: 1px solid var(--border-color); border-radius: 8px; overflow: hidden; }
  .panel-header { display: flex; align-items: center; gap: 10px; padding: 14px 20px; background: rgba(0,0,0,0.2); border-bottom: 1px solid rgba(255,255,255,0.03); }
  .panel-header h3 { margin: 0; font-size: 0.85rem; letter-spacing: 1.5px; text-transform: uppercase; color: var(--text-primary); }

  .status-indicator { width: 10px; height: 10px; border-radius: 50%; }
  .status-indicator.active { background: var(--accent-cyan); box-shadow: 0 0 8px var(--accent-cyan); }
  .status-indicator.pending { background: var(--accent-orange); }
  .status-indicator.ignored { background: var(--text-secondary); }

  .monitored-panel { border-color: rgba(0,212,255,0.3); }

  .resource-table { display: flex; flex-direction: column; }
  .resource-row { display: grid; grid-template-columns: 100px 1fr auto; align-items: center; padding: 12px 20px; border-bottom: 1px solid rgba(255,255,255,0.03); gap: 16px; }
  .resource-row:last-child { border-bottom: none; }
  .resource-row:hover { background: rgba(255,255,255,0.02); }

  .res-kind { font-size: 0.7rem; text-transform: uppercase; font-weight: 800; padding: 3px 8px; border-radius: 4px; background: rgba(255,255,255,0.05); color: var(--text-secondary); text-align: center; }
  .res-ident { font-size: 0.85rem; color: var(--text-primary); font-family: monospace; }
  .res-state { font-size: 0.85rem; font-weight: 700; color: var(--accent-green); text-align: right; }
  .res-state.problem { color: var(--accent-orange); }

  .muted-row .res-ident { color: var(--text-secondary); }

  .row-actions { display: flex; gap: 12px; }
  .action-btn { background: none; border: none; font-size: 0.75rem; font-weight: 700; cursor: pointer; text-transform: uppercase; padding: 4px; }
  .action-btn:hover { text-decoration: underline; }
  .text-cyan { color: var(--accent-cyan); }
  .text-muted { color: var(--text-secondary); }
  .text-muted:hover { color: var(--text-primary); }

  .empty { padding: 20px; color: var(--text-secondary); font-size: 0.85rem; font-style: italic; }
</style>
