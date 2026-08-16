<script lang="ts">
  // State for the creation form
  let scope = $state('VM'); // GLOBAL, VM, RESOURCE, RULE
  let targetId = $state(''); // The VM ID, Resource ID, or Rule ID
  let startTime = $state('');
  let endTime = $state('');
  let createdBy = $state('admin');

  // State for the list
  let windows = $state([
    {
      id: 'mw-1',
      scope: 'VM',
      target: '101',
      startTime: '2026-08-16T12:00',
      endTime: '2026-08-16T18:00',
      createdBy: 'admin',
      status: 'ACTIVE'
    },
    {
      id: 'mw-2',
      scope: 'GLOBAL',
      target: 'ALL',
      startTime: '2026-08-20T00:00',
      endTime: '2026-08-20T04:00',
      createdBy: 'system',
      status: 'SCHEDULED'
    },
    {
      id: 'mw-3',
      scope: 'RESOURCE',
      target: 'nginx.service (VM 102)',
      startTime: '2026-08-15T10:00',
      endTime: '2026-08-15T11:00',
      createdBy: 'admin',
      status: 'EXPIRED'
    }
  ]);

  function handleCreate() {
    // In a real app, this would POST to the backend
    if (!startTime || !endTime) return;
    
    windows = [{
      id: `mw-${Date.now()}`,
      scope,
      target: scope === 'GLOBAL' ? 'ALL' : targetId,
      startTime,
      endTime,
      createdBy,
      status: 'SCHEDULED'
    }, ...windows];

    targetId = '';
    startTime = '';
    endTime = '';
  }
</script>

<div class="page">
  <div class="header">
    <div>
      <h2>Maintenance Windows</h2>
      <p>Suppress notifications during planned downtime without stopping telemetry or monitoring.</p>
    </div>
  </div>

  <div class="dashboard-grid">
    <!-- Creation Form -->
    <div class="panel creation-panel">
      <div class="panel-title">Schedule Maintenance</div>
      
      <div class="form-group">
        <label for="scope">Scope</label>
        <select id="scope" bind:value={scope} class="input">
          <option value="GLOBAL">Global (All Alerts)</option>
          <option value="VM">Specific VM</option>
          <option value="RESOURCE">Specific Resource</option>
          <option value="RULE">Specific Rule</option>
        </select>
      </div>

      {#if scope !== 'GLOBAL'}
        <div class="form-group">
          <label for="targetId">Target ID (VM ID / Resource / Rule)</label>
          <input type="text" id="targetId" bind:value={targetId} class="input" placeholder="e.g. 101 or nginx.service" />
        </div>
      {/if}

      <div class="form-row">
        <div class="form-group">
          <label for="startTime">Start Time</label>
          <input type="datetime-local" id="startTime" bind:value={startTime} class="input" />
        </div>
        <div class="form-group">
          <label for="endTime">End Time</label>
          <input type="datetime-local" id="endTime" bind:value={endTime} class="input" />
        </div>
      </div>

      <div class="form-group">
        <label for="createdBy">Created By</label>
        <input type="text" id="createdBy" bind:value={createdBy} class="input" readonly />
      </div>

      <button class="primary-btn mt-4" onclick={handleCreate} disabled={!startTime || !endTime || (scope !== 'GLOBAL' && !targetId)}>
        Schedule Window
      </button>

      <!-- Educational callout -->
      <div class="architecture-note mt-6">
        <div class="note-title">How this affects Sentinel:</div>
        <div class="note-grid">
          <div class="note-item">
            <span class="icon success">●</span>
            <span class="label">Telemetry:</span>
            <span class="value">ACTIVE</span>
          </div>
          <div class="note-item">
            <span class="icon success">●</span>
            <span class="label">Monitoring:</span>
            <span class="value">ACTIVE</span>
          </div>
          <div class="note-item">
            <span class="icon success">●</span>
            <span class="label">Incident:</span>
            <span class="value">OPEN</span>
          </div>
          <div class="note-item">
            <span class="icon muted">🔇</span>
            <span class="label">Notification:</span>
            <span class="value muted-text">SUPPRESSED</span>
          </div>
        </div>
      </div>
    </div>

    <!-- Maintenance List -->
    <div class="panel list-panel">
      <div class="panel-title">Maintenance Windows</div>
      
      <div class="window-list">
        {#each windows as win}
          <div class="window-card {win.status.toLowerCase()}">
            <div class="status-badge {win.status.toLowerCase()}">
              {#if win.status === 'ACTIVE'}
                <span class="pulse"></span> MAINTENANCE ACTIVE
              {:else}
                {win.status}
              {/if}
            </div>
            
            <div class="window-header">
              <span class="scope-tag">{win.scope}</span>
              <h3 class="target-name">{win.target}</h3>
            </div>
            
            <div class="window-details">
              <div class="detail-row">
                <span class="detail-label">Start:</span>
                <span class="detail-value">{new Date(win.startTime).toLocaleString()}</span>
              </div>
              <div class="detail-row">
                <span class="detail-label">End:</span>
                <span class="detail-value">{new Date(win.endTime).toLocaleString()}</span>
              </div>
              <div class="detail-row">
                <span class="detail-label">Created by:</span>
                <span class="detail-value">{win.createdBy}</span>
              </div>
            </div>

            {#if win.status === 'ACTIVE'}
              <div class="active-effect">
                <div class="effect-label">Effect:</div>
                <div class="effect-value">🔇 Notifications Suppressed</div>
              </div>
            {/if}
          </div>
        {/each}
      </div>
    </div>
  </div>
</div>

<style>
  .page { padding-bottom: 60px; max-width: 1200px; margin: 0 auto; display: flex; flex-direction: column; gap: 20px; }
  .header { display: flex; justify-content: space-between; align-items: flex-end; margin-bottom: 10px; }
  .header h2 { margin: 0 0 8px 0; font-size: 1.8rem; color: var(--text-primary); }
  .header p { margin: 0; color: var(--text-secondary); font-size: 0.95rem; }

  .dashboard-grid { display: grid; grid-template-columns: 1fr 1.5fr; gap: 24px; align-items: start; }

  .panel { background: var(--card-bg); border: 1px solid var(--border-color); border-radius: 10px; padding: 24px; box-shadow: 0 4px 20px rgba(0,0,0,0.15); }
  .panel-title { font-size: 0.8rem; letter-spacing: 2px; text-transform: uppercase; color: var(--text-secondary); font-weight: 800; margin-bottom: 20px; border-bottom: 1px solid rgba(255,255,255,0.05); padding-bottom: 10px; }

  /* Form Styles */
  .form-group { display: flex; flex-direction: column; gap: 6px; margin-bottom: 16px; }
  .form-row { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; }
  label { font-size: 0.8rem; font-weight: 700; color: var(--text-secondary); text-transform: uppercase; letter-spacing: 0.5px; }
  .input { background: rgba(0,0,0,0.2); border: 1px solid rgba(255,255,255,0.1); border-radius: 6px; padding: 10px 12px; color: var(--text-primary); font-size: 0.95rem; transition: border-color 0.2s; font-family: inherit; }
  .input:focus { outline: none; border-color: var(--accent-cyan); }
  .input:disabled { opacity: 0.5; cursor: not-allowed; }
  select.input { appearance: none; background-image: url("data:image/svg+xml;charset=US-ASCII,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20width%3D%22292.4%22%20height%3D%22292.4%22%3E%3Cpath%20fill%3D%22%23FFFFFF%22%20d%3D%22M287%2069.4a17.6%2017.6%200%200%200-13-5.4H18.4c-5%200-9.3%201.8-12.9%205.4A17.6%2017.6%200%200%200%200%2082.2c0%205%201.8%209.3%205.4%2012.9l128%20127.9c3.6%203.6%207.8%205.4%2012.8%205.4s9.2-1.8%2012.8-5.4L287%2095c3.5-3.5%205.4-7.8%205.4-12.8%200-5-1.9-9.2-5.5-12.8z%22%2F%3E%3C%2Fsvg%3E"); background-repeat: no-repeat; background-position: right 12px top 50%; background-size: 10px auto; padding-right: 30px; }
  .mt-4 { margin-top: 16px; }
  .mt-6 { margin-top: 24px; }
  
  .primary-btn { background: rgba(0,212,255,0.15); border: 1px solid var(--accent-cyan); color: var(--accent-cyan); padding: 12px 20px; border-radius: 6px; font-weight: 800; font-size: 0.85rem; letter-spacing: 1px; text-transform: uppercase; cursor: pointer; transition: all 0.2s; width: 100%; }
  .primary-btn:hover:not(:disabled) { background: rgba(0,212,255,0.25); box-shadow: 0 0 15px rgba(0,212,255,0.2); }
  .primary-btn:disabled { opacity: 0.5; border-color: rgba(255,255,255,0.1); color: var(--text-dim); cursor: not-allowed; }

  /* Architecture Note */
  .architecture-note { background: rgba(0,0,0,0.3); border: 1px dashed rgba(255,255,255,0.1); border-radius: 8px; padding: 16px; }
  .note-title { font-size: 0.8rem; font-weight: 700; color: var(--text-secondary); margin-bottom: 12px; }
  .note-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .note-item { display: flex; align-items: center; gap: 6px; font-size: 0.85rem; }
  .icon.success { color: var(--accent-green); font-size: 0.7rem; }
  .icon.muted { font-size: 0.9rem; }
  .note-item .label { color: var(--text-primary); font-weight: 600; font-size: 0.85rem; text-transform: none; letter-spacing: normal; }
  .note-item .value { font-weight: 800; color: var(--accent-green); }
  .note-item .muted-text { color: var(--text-dim); }

  /* Window List */
  .window-list { display: flex; flex-direction: column; gap: 16px; }
  .window-card { background: rgba(255,255,255,0.03); border: 1px solid rgba(255,255,255,0.05); border-radius: 8px; padding: 20px; position: relative; display: flex; flex-direction: column; gap: 12px; }
  .window-card.active { border-color: rgba(255, 170, 0, 0.4); background: linear-gradient(135deg, rgba(255, 170, 0, 0.05) 0%, rgba(255, 170, 0, 0.01) 100%); }
  .window-card.expired { opacity: 0.6; }

  .status-badge { position: absolute; top: 20px; right: 20px; font-size: 0.75rem; font-weight: 800; letter-spacing: 1px; padding: 4px 8px; border-radius: 4px; display: flex; align-items: center; gap: 6px; }
  .status-badge.active { color: var(--accent-orange); background: rgba(255, 170, 0, 0.1); }
  .status-badge.scheduled { color: var(--accent-cyan); background: rgba(0, 212, 255, 0.1); }
  .status-badge.expired { color: var(--text-dim); background: rgba(255, 255, 255, 0.05); }

  .pulse { width: 8px; height: 8px; background: var(--accent-orange); border-radius: 50%; box-shadow: 0 0 8px var(--accent-orange); animation: pulse-orange 1.5s infinite; }
  @keyframes pulse-orange { 0% { opacity: 1; box-shadow: 0 0 8px var(--accent-orange); } 50% { opacity: 0.5; box-shadow: 0 0 2px var(--accent-orange); } 100% { opacity: 1; box-shadow: 0 0 8px var(--accent-orange); } }

  .window-header { display: flex; flex-direction: column; gap: 4px; padding-right: 120px; }
  .scope-tag { font-size: 0.7rem; font-weight: 800; color: var(--text-dim); letter-spacing: 1px; }
  .target-name { font-size: 1.1rem; margin: 0; color: var(--text-primary); }

  .window-details { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 12px; margin-top: 4px; }
  .detail-row { display: flex; flex-direction: column; gap: 2px; }
  .detail-label { font-size: 0.7rem; color: var(--text-dim); text-transform: uppercase; font-weight: 700; letter-spacing: 1px; }
  .detail-value { font-size: 0.9rem; color: var(--text-secondary); font-family: monospace; }

  .active-effect { margin-top: 8px; padding-top: 12px; border-top: 1px dashed rgba(255, 170, 0, 0.2); display: flex; gap: 8px; align-items: center; }
  .effect-label { font-size: 0.75rem; color: var(--accent-orange); font-weight: 700; text-transform: uppercase; letter-spacing: 1px; }
  .effect-value { font-size: 0.85rem; color: var(--text-primary); font-weight: 600; }
</style>
