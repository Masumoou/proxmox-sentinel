<script lang="ts">
  import { page } from '$app/stores';

  let incidentId = $derived($page.params.id);

  // Mock Incident Data from backend
  let incident = $state({
    status: 'OPEN', // OPEN or RESOLVED
    vmId: '101',
    vmName: 'web-server',
    resourceName: 'nginx.service',
    description: 'nginx.service is not running',
    rule: 'nginx != running for 60s',
    severity: 'Critical',
    startedAt: '16:42:18',
    acknowledged: false,
    inhibited: true, // Correlated / notification suppressed
    parentIncidentId: 'inc-993',
    parentDescription: 'VM 101 unreachable'
  });

  // Mock Timeline Data
  let timeline = $state([
    { time: '16:40:02', event: 'VM became unreachable', detail: 'Rule: VM reachability failed', type: 'critical' },
    { time: '16:40:05', event: 'QEMU Guest Agent unavailable', detail: null, type: 'warning' },
    { time: '16:40:08', event: 'nginx.service telemetry became UNKNOWN', detail: null, type: 'warning' },
    { time: '16:41:10', event: 'nginx.service incident opened', detail: null, type: 'critical' },
    { time: '16:41:12', event: 'Correlated with VM reachability incident', detail: null, type: 'link' },
    { time: '16:41:12', event: 'nginx notification inhibited', detail: null, type: 'muted' }
  ]);

  function handleAcknowledge() {
    incident.acknowledged = true;
  }
</script>

<div class="page">
  <div class="header-nav">
    <a href="/alerts" class="back-link">← Back to Alerts</a>
  </div>

  <div class="incident-dashboard">
    <!-- Left Column: Core Incident Info -->
    <div class="left-col">
      <!-- Incident Header Card -->
      <div class="panel incident-card">
        <div class="status-badge" class:open={incident.status === 'OPEN'}>
          <span class="pulse"></span>
          {incident.status}
        </div>

        <h2 class="title">VM {incident.vmId} &middot; {incident.resourceName}</h2>
        <p class="description">{incident.description}</p>
        
        <div class="meta-grid">
          <div class="meta-item">
            <span class="label">Rule:</span>
            <span class="value code">{incident.rule}</span>
          </div>
          <div class="meta-item">
            <span class="label">Severity:</span>
            <span class="value severity-text">{incident.severity}</span>
          </div>
          <div class="meta-item">
            <span class="label">Started:</span>
            <span class="value">{incident.startedAt}</span>
          </div>
          <div class="meta-item">
            <span class="label">Acknowledgement:</span>
            {#if incident.acknowledged}
              <span class="value success-text">Acknowledged</span>
            {:else}
              <span class="value text-muted">Not acknowledged</span>
            {/if}
          </div>
        </div>

        {#if !incident.acknowledged && incident.status === 'OPEN'}
          <div class="action-row">
            <button class="primary-btn" onclick={handleAcknowledge}>Acknowledge Incident</button>
          </div>
        {/if}
      </div>

      <!-- State Distinction Box -->
      <div class="panel state-distinction">
        <div class="state-row">
          <span class="label">Incident:</span>
          <span class="value">{incident.acknowledged ? 'ACKNOWLEDGED' : (incident.status === 'OPEN' ? 'OPEN' : 'RESOLVED')}</span>
        </div>
        <div class="state-row">
          <span class="label">Alert:</span>
          <span class="value open-text">{incident.status === 'OPEN' ? 'FIRING' : 'RESOLVED'}</span>
        </div>
        <div class="state-row">
          <span class="label">Rule:</span>
          <span class="value success-text">ENABLED</span>
        </div>
        <div class="state-row">
          <span class="label">Monitoring:</span>
          <span class="value success-text">ACTIVE</span>
        </div>
      </div>

      <!-- Notification Inhibition Box -->
      <div class="panel notification-panel">
        <div class="panel-title">NOTIFICATION</div>
        {#if incident.inhibited}
          <div class="inhibited-state">
            <span class="inhibit-badge">🔇 INHIBITED</span>
          </div>
          <div class="inhibit-reason">
            <div class="label">Reason:</div>
            <p>VM {incident.vmId} has an active reachability incident.</p>
            <div class="label mt-1">Parent incident:</div>
            <a href="/incidents/{incident.parentIncidentId}" class="parent-link">🔴 {incident.parentDescription}</a>
          </div>
        {:else}
          <div class="sent-state">
            <span class="sent-badge">SENT</span>
          </div>
        {/if}
      </div>
    </div>

    <!-- Right Column: Timeline & Correlation -->
    <div class="right-col">
      <div class="panel root-cause-panel">
        <div class="panel-title">ROOT CAUSE</div>
        <h3 class="rc-title">{incident.parentDescription}</h3>
        <div class="confidence">Confidence: <strong>94%</strong></div>
        
        <div class="rc-details">
          <span class="rc-label">Why Sentinel correlated this:</span>
          <ul>
            <li>Same VM</li>
            <li>Parent incident started first</li>
            <li>nginx incident started afterward</li>
            <li>VM reachability failure explains loss of guest/service visibility</li>
          </ul>
        </div>
        
        <div class="visual-tree">
          <div class="tree-node parent">🔴 VM UNREACHABLE</div>
          <div class="tree-line">│</div>
          <div class="tree-branch">
            <div class="branch-top">┌──────────┼──────────┐</div>
            <div class="branch-arrows">↓&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;↓&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;↓</div>
          </div>
          <div class="tree-children">
            <div class="child-node">
              <span class="name">nginx</span>
              <span class="icon">🔇</span>
              <span class="status">inhibited</span>
            </div>
            <div class="child-node">
              <span class="name">PostgreSQL</span>
              <span class="icon">🔇</span>
              <span class="status">inhibited</span>
            </div>
            <div class="child-node">
              <span class="name">HTTP</span>
              <span class="icon">🔇</span>
              <span class="status">inhibited</span>
            </div>
          </div>
        </div>
      </div>

      <div class="panel timeline-panel">
        <div class="panel-title">VM {incident.vmId} &middot; Incident Timeline</div>
        <div class="timeline">
          {#each timeline as item}
            <div class="timeline-item">
              <div class="time">{item.time}</div>
              <div class="marker {item.type}">
                {#if item.type === 'critical'}🔴
                {:else if item.type === 'warning'}🟠
                {:else if item.type === 'link'}🔗
                {:else if item.type === 'muted'}🔇
                {/if}
              </div>
              <div class="content" class:muted={item.type === 'muted'}>
                <div class="event">{item.event}</div>
                {#if item.detail}
                  <div class="detail">{item.detail}</div>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      </div>
    </div>
  </div>
</div>

<style>
  .page { padding-bottom: 60px; max-width: 1200px; margin: 0 auto; display: flex; flex-direction: column; gap: 20px; }
  
  .header-nav { display: flex; justify-content: flex-start; padding-bottom: 10px; }
  .back-link { color: var(--text-secondary); text-decoration: none; font-size: 0.85rem; font-weight: 700; transition: color 0.2s; }
  .back-link:hover { color: var(--accent-cyan); }

  .incident-dashboard { display: grid; grid-template-columns: 1fr 1.3fr; gap: 24px; align-items: start; }
  
  .left-col { display: flex; flex-direction: column; gap: 20px; }
  .right-col { display: flex; flex-direction: column; gap: 20px; }

  .panel { background: var(--card-bg); border: 1px solid var(--border-color); border-radius: 10px; padding: 24px; box-shadow: 0 4px 20px rgba(0,0,0,0.15); }
  .panel-title { font-size: 0.8rem; letter-spacing: 2px; text-transform: uppercase; color: var(--text-secondary); font-weight: 800; margin-bottom: 16px; border-bottom: 1px solid rgba(255,255,255,0.05); padding-bottom: 10px; }

  /* Incident Card */
  .incident-card { border-top: 4px solid var(--accent-red); position: relative; }
  .status-badge { position: absolute; top: 24px; right: 24px; display: flex; align-items: center; gap: 8px; font-weight: 900; font-size: 0.85rem; letter-spacing: 1px; color: var(--text-secondary); }
  .status-badge.open { color: var(--accent-red); }
  .pulse { width: 10px; height: 10px; background: var(--accent-red); border-radius: 50%; box-shadow: 0 0 10px var(--accent-red); animation: pulse 1.5s infinite; }
  @keyframes pulse { 0% { opacity: 1; box-shadow: 0 0 10px var(--accent-red); } 50% { opacity: 0.4; box-shadow: 0 0 2px var(--accent-red); } 100% { opacity: 1; box-shadow: 0 0 10px var(--accent-red); } }
  
  .title { font-size: 1.2rem; color: var(--text-primary); margin: 0 0 8px 0; padding-right: 80px; }
  .description { font-size: 1rem; color: var(--text-secondary); margin: 0 0 24px 0; }
  
  .meta-grid { display: flex; flex-direction: column; gap: 12px; margin-bottom: 24px; }
  .meta-item { display: grid; grid-template-columns: 140px 1fr; align-items: center; font-size: 0.85rem; }
  .label { color: var(--text-dim); font-weight: 700; text-transform: uppercase; letter-spacing: 1px; font-size: 0.75rem; }
  .value { color: var(--text-primary); font-weight: 600; }
  .code { font-family: monospace; background: rgba(255,255,255,0.05); padding: 2px 6px; border-radius: 4px; font-size: 0.8rem; }
  .severity-text { color: var(--accent-red); font-weight: 800; }
  
  .action-row { margin-top: 10px; }
  .primary-btn { background: rgba(0,212,255,0.15); border: 1px solid var(--accent-cyan); color: var(--accent-cyan); padding: 10px 16px; border-radius: 6px; font-weight: 800; font-size: 0.75rem; letter-spacing: 1px; text-transform: uppercase; cursor: pointer; transition: all 0.2s; width: 100%; }
  .primary-btn:hover { background: rgba(0,212,255,0.25); box-shadow: 0 0 15px rgba(0,212,255,0.2); }

  /* State Distinction Box */
  .state-distinction { display: flex; flex-direction: column; gap: 12px; background: rgba(0,0,0,0.2); border-color: rgba(255,255,255,0.05); }
  .state-row { display: flex; justify-content: space-between; align-items: center; border-bottom: 1px dashed rgba(255,255,255,0.05); padding-bottom: 8px; }
  .state-row:last-child { border-bottom: none; padding-bottom: 0; }
  .success-text { color: var(--accent-green); font-weight: 800; }
  .text-muted { color: var(--text-secondary); }
  .open-text { color: var(--accent-red); font-weight: 800; }

  /* Notification Panel */
  .notification-panel { border-left: 3px solid var(--text-dim); }
  .inhibited-state { margin-bottom: 16px; }
  .inhibit-badge { background: rgba(255,255,255,0.1); color: var(--text-secondary); padding: 4px 8px; border-radius: 4px; font-weight: 800; font-size: 0.8rem; letter-spacing: 1px; }
  .inhibit-reason p { color: var(--text-primary); font-size: 0.9rem; margin: 4px 0 12px 0; }
  .mt-1 { margin-top: 12px; }
  .parent-link { color: var(--accent-red); text-decoration: none; font-weight: 700; font-size: 0.85rem; display: inline-block; margin-top: 4px; padding: 6px 10px; background: rgba(255,51,85,0.1); border-radius: 6px; border: 1px solid rgba(255,51,85,0.2); transition: all 0.2s; }
  .parent-link:hover { background: rgba(255,51,85,0.15); border-color: var(--accent-red); }

  /* Root Cause Panel */
  .root-cause-panel { background: linear-gradient(180deg, rgba(14,22,42,0.95) 0%, rgba(10,16,32,0.9) 100%); border-top: 3px solid var(--accent-orange); }
  .rc-title { font-size: 1.1rem; color: var(--text-primary); margin: 0 0 6px 0; }
  .confidence { font-size: 0.8rem; color: var(--text-secondary); margin-bottom: 20px; }
  .confidence strong { color: var(--accent-green); }
  
  .rc-details { margin-bottom: 24px; }
  .rc-label { font-size: 0.8rem; font-weight: 700; color: var(--text-secondary); text-transform: uppercase; letter-spacing: 1px; }
  .rc-details ul { margin: 8px 0 0 0; padding-left: 20px; color: var(--text-primary); font-size: 0.85rem; }
  .rc-details li { margin-bottom: 4px; }

  /* Visual Tree */
  .visual-tree { display: flex; flex-direction: column; align-items: center; font-family: monospace; font-size: 0.85rem; background: rgba(0,0,0,0.3); padding: 20px; border-radius: 8px; border: 1px solid rgba(255,255,255,0.05); }
  .tree-node.parent { color: var(--accent-red); font-weight: bold; }
  .tree-line { color: var(--text-dim); line-height: 1; }
  .branch-top { color: var(--text-dim); line-height: 1; letter-spacing: -0.5px; }
  .branch-arrows { color: var(--text-dim); line-height: 1; letter-spacing: -0.5px; }
  .tree-children { display: flex; justify-content: center; gap: 20px; margin-top: 4px; }
  .child-node { display: flex; flex-direction: column; align-items: center; gap: 2px; }
  .child-node .name { color: var(--text-primary); }
  .child-node .icon { font-size: 1.1rem; }
  .child-node .status { color: var(--text-dim); font-size: 0.75rem; }

  /* Timeline */
  .timeline { display: flex; flex-direction: column; margin-top: 10px; }
  .timeline-item { display: grid; grid-template-columns: 70px 30px 1fr; gap: 10px; position: relative; padding-bottom: 20px; }
  .timeline-item:last-child { padding-bottom: 0; }
  .timeline-item:not(:last-child)::after { content: ''; position: absolute; left: 84px; top: 24px; bottom: 0; width: 2px; background: rgba(255,255,255,0.1); }
  
  .time { font-family: monospace; font-size: 0.75rem; color: var(--text-dim); padding-top: 4px; text-align: right; }
  .marker { display: flex; justify-content: center; align-items: flex-start; padding-top: 2px; font-size: 0.9rem; z-index: 2; background: var(--card-bg); }
  .content { display: flex; flex-direction: column; gap: 4px; padding-top: 2px; }
  .event { color: var(--text-primary); font-size: 0.9rem; font-weight: 600; }
  .detail { color: var(--text-secondary); font-size: 0.8rem; font-family: monospace; }
  .content.muted .event { color: var(--text-dim); font-style: italic; }
</style>
