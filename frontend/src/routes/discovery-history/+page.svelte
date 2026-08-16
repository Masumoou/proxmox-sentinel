<script lang="ts">
  // Filters State
  let filterVm = $state('');
  let filterResourceType = $state('');
  let filterEventType = $state('');
  let filterStartDate = $state('');
  let filterEndDate = $state('');

  // Mock Discovery History Events
  let rawEvents = [
    {
      id: 'evt-10',
      timestamp: '2026-08-16T16:42:31',
      vmId: '101',
      vmName: 'web-server-prod',
      resourceType: 'Service',
      resourceName: 'redis.service',
      eventType: 'DISCOVERED',
      summary: 'New systemd service detected'
    },
    {
      id: 'evt-9',
      timestamp: '2026-08-16T15:10:04',
      vmId: '101',
      vmName: 'web-server-prod',
      resourceType: 'Service',
      resourceName: 'nginx.service',
      eventType: 'CHANGED',
      summary: 'Service configuration/state changed'
    },
    {
      id: 'evt-8',
      timestamp: '2026-08-16T14:20:11',
      vmId: '102',
      vmName: 'db-postgres-prod',
      resourceType: 'Filesystem',
      resourceName: '/var/lib/postgresql',
      eventType: 'DISCOVERED',
      summary: 'New filesystem mount detected'
    },
    {
      id: 'evt-7',
      timestamp: '2026-08-16T12:05:22',
      vmId: '101',
      vmName: 'web-server-prod',
      resourceType: 'Service',
      resourceName: 'postgres.service',
      eventType: 'DISAPPEARED',
      summary: 'Service no longer reported by Guest Agent'
    },
    {
      id: 'evt-6',
      timestamp: '2026-08-16T09:21:18',
      vmId: '101',
      vmName: 'web-server-prod',
      resourceType: 'Service',
      resourceName: 'postgres.service',
      eventType: 'REAPPEARED',
      summary: 'Previously disappeared resource detected again'
    }
  ];

  // Apply filters via derived state
  let filteredEvents = $derived(rawEvents.filter(evt => {
    if (filterVm && !evt.vmName.toLowerCase().includes(filterVm.toLowerCase()) && evt.vmId !== filterVm) return false;
    if (filterResourceType && evt.resourceType !== filterResourceType) return false;
    if (filterEventType && evt.eventType !== filterEventType) return false;
    
    const evtDate = new Date(evt.timestamp);
    if (filterStartDate && evtDate < new Date(filterStartDate)) return false;
    if (filterEndDate && evtDate > new Date(filterEndDate + 'T23:59:59')) return false;

    return true;
  }));

  function resetFilters() {
    filterVm = '';
    filterResourceType = '';
    filterEventType = '';
    filterStartDate = '';
    filterEndDate = '';
  }
</script>

<div class="page">
  <div class="header">
    <div>
      <h2>Discovery & Change History</h2>
      <p>A chronological audit log of guest agent observations.</p>
    </div>
  </div>

  <div class="architecture-note">
    <div class="note-title">Architectural Boundary</div>
    <div class="boundary-flow">
      <div class="flow-step">
        <span class="step-icon">👁️</span>
        <span class="step-text">Discovery Event</span>
        <span class="step-sub">Historical fact</span>
      </div>
      <div class="flow-arrow">→</div>
      <div class="flow-boundary">
        <div class="boundary-label">Does NOT automatically mean</div>
        <div class="boundary-items">
          <div class="b-item">Monitoring enabled</div>
          <div class="b-arrow">→</div>
          <div class="b-item">Rule created</div>
          <div class="b-arrow">→</div>
          <div class="b-item">Alert generated</div>
        </div>
      </div>
    </div>
  </div>

  <div class="dashboard-grid">
    <!-- Left Sidebar: Filters -->
    <div class="filter-panel panel">
      <div class="panel-title">Filter Events</div>
      
      <div class="form-group">
        <label for="filterVm">VM (Name or ID)</label>
        <input type="text" id="filterVm" bind:value={filterVm} class="input" placeholder="e.g. 101 or web-server" />
      </div>

      <div class="form-group">
        <label for="filterResourceType">Resource Type</label>
        <select id="filterResourceType" bind:value={filterResourceType} class="input">
          <option value="">All Types</option>
          <option value="Service">Service</option>
          <option value="Filesystem">Filesystem</option>
          <option value="Network">Network</option>
        </select>
      </div>

      <div class="form-group">
        <label for="filterEventType">Event Type</label>
        <select id="filterEventType" bind:value={filterEventType} class="input">
          <option value="">All Events</option>
          <option value="DISCOVERED">DISCOVERED</option>
          <option value="CHANGED">CHANGED</option>
          <option value="DISAPPEARED">DISAPPEARED</option>
          <option value="REAPPEARED">REAPPEARED</option>
        </select>
      </div>

      <div class="form-group">
        <label for="filterStartDate">Start Date</label>
        <input type="date" id="filterStartDate" bind:value={filterStartDate} class="input" />
      </div>

      <div class="form-group">
        <label for="filterEndDate">End Date</label>
        <input type="date" id="filterEndDate" bind:value={filterEndDate} class="input" />
      </div>

      <button class="secondary-btn mt-4" onclick={resetFilters}>Reset Filters</button>
    </div>

    <!-- Main Content: History Timeline -->
    <div class="history-panel panel">
      <div class="panel-title">
        Audit Log
        <span class="badge">{filteredEvents.length} events</span>
      </div>

      {#if filteredEvents.length === 0}
        <div class="empty-state">
          No discovery events match the current filters.
        </div>
      {:else}
        <div class="history-list">
          {#each filteredEvents as evt}
            <div class="history-card">
              <div class="card-left">
                <div class="time">{new Date(evt.timestamp).toLocaleTimeString()}</div>
                <div class="date">{new Date(evt.timestamp).toLocaleDateString()}</div>
              </div>
              
              <div class="card-divider {evt.eventType.toLowerCase()}"></div>
              
              <div class="card-content">
                <div class="evt-header">
                  <span class="evt-type {evt.eventType.toLowerCase()}">{evt.eventType}</span>
                  <div class="evt-target">
                    <a href="/guests/{evt.vmId}" class="target-link">VM-{evt.vmId}</a>
                    <span class="dot">&middot;</span>
                    <span class="resource-type">{evt.resourceType}</span>
                    <span class="dot">&middot;</span>
                    <span class="resource-name">{evt.resourceName}</span>
                  </div>
                </div>
                <div class="evt-summary">{evt.summary}</div>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .page { padding-bottom: 60px; max-width: 1200px; margin: 0 auto; display: flex; flex-direction: column; gap: 20px; }
  .header { display: flex; justify-content: space-between; align-items: flex-end; margin-bottom: 10px; }
  .header h2 { margin: 0 0 8px 0; font-size: 1.8rem; color: var(--text-primary); }
  .header p { margin: 0; color: var(--text-secondary); font-size: 0.95rem; }

  /* Architectural Boundary Note */
  .architecture-note { background: rgba(0,0,0,0.3); border: 1px solid rgba(255,255,255,0.05); border-radius: 8px; padding: 20px; box-shadow: inset 0 0 20px rgba(0,212,255,0.02); }
  .note-title { font-size: 0.75rem; font-weight: 800; color: var(--accent-cyan); text-transform: uppercase; letter-spacing: 1px; margin-bottom: 12px; }
  
  .boundary-flow { display: flex; align-items: center; gap: 20px; flex-wrap: wrap; }
  
  .flow-step { display: flex; flex-direction: column; align-items: center; gap: 4px; background: rgba(255,255,255,0.03); padding: 12px 20px; border-radius: 6px; border: 1px solid rgba(255,255,255,0.05); }
  .step-icon { font-size: 1.2rem; }
  .step-text { font-size: 0.9rem; font-weight: 800; color: var(--text-primary); letter-spacing: 0.5px; }
  .step-sub { font-size: 0.75rem; color: var(--text-dim); text-transform: uppercase; letter-spacing: 0.5px; }
  
  .flow-arrow { font-size: 1.2rem; color: var(--text-dim); }
  
  .flow-boundary { display: flex; flex-direction: column; gap: 8px; background: rgba(255,51,85,0.05); border: 1px dashed rgba(255,51,85,0.3); padding: 12px 20px; border-radius: 6px; }
  .boundary-label { font-size: 0.75rem; font-weight: 800; color: var(--accent-red); text-transform: uppercase; letter-spacing: 1px; text-align: center; }
  .boundary-items { display: flex; align-items: center; gap: 12px; }
  .b-item { font-size: 0.85rem; color: var(--text-secondary); font-weight: 600; }
  .b-arrow { font-size: 0.8rem; color: rgba(255,51,85,0.5); }

  /* Dashboard Grid */
  .dashboard-grid { display: grid; grid-template-columns: 280px 1fr; gap: 24px; align-items: start; }
  .panel { background: var(--card-bg); border: 1px solid var(--border-color); border-radius: 10px; padding: 24px; box-shadow: 0 4px 20px rgba(0,0,0,0.15); }
  .panel-title { display: flex; justify-content: space-between; align-items: center; font-size: 0.8rem; letter-spacing: 2px; text-transform: uppercase; color: var(--text-secondary); font-weight: 800; margin-bottom: 20px; border-bottom: 1px solid rgba(255,255,255,0.05); padding-bottom: 10px; }
  .badge { background: rgba(255,255,255,0.05); padding: 2px 8px; border-radius: 10px; font-size: 0.7rem; color: var(--text-dim); letter-spacing: 0.5px; }

  /* Filters */
  .form-group { display: flex; flex-direction: column; gap: 6px; margin-bottom: 16px; }
  label { font-size: 0.75rem; font-weight: 700; color: var(--text-secondary); text-transform: uppercase; letter-spacing: 0.5px; }
  .input { background: rgba(0,0,0,0.2); border: 1px solid rgba(255,255,255,0.1); border-radius: 6px; padding: 8px 10px; color: var(--text-primary); font-size: 0.9rem; transition: border-color 0.2s; font-family: inherit; }
  .input:focus { outline: none; border-color: var(--accent-cyan); }
  select.input { appearance: none; background-image: url("data:image/svg+xml;charset=US-ASCII,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20width%3D%22292.4%22%20height%3D%22292.4%22%3E%3Cpath%20fill%3D%22%23FFFFFF%22%20d%3D%22M287%2069.4a17.6%2017.6%200%200%200-13-5.4H18.4c-5%200-9.3%201.8-12.9%205.4A17.6%2017.6%200%200%200%200%2082.2c0%205%201.8%209.3%205.4%2012.9l128%20127.9c3.6%203.6%207.8%205.4%2012.8%205.4s9.2-1.8%2012.8-5.4L287%2095c3.5-3.5%205.4-7.8%205.4-12.8%200-5-1.9-9.2-5.5-12.8z%22%2F%3E%3C%2Fsvg%3E"); background-repeat: no-repeat; background-position: right 10px top 50%; background-size: 8px auto; padding-right: 24px; }
  
  .mt-4 { margin-top: 16px; }
  .secondary-btn { background: rgba(255,255,255,0.05); border: 1px solid rgba(255,255,255,0.1); color: var(--text-primary); padding: 10px 16px; border-radius: 6px; font-weight: 800; font-size: 0.8rem; letter-spacing: 1px; text-transform: uppercase; cursor: pointer; transition: all 0.2s; width: 100%; }
  .secondary-btn:hover { background: rgba(255,255,255,0.1); border-color: rgba(255,255,255,0.3); }

  /* History Timeline */
  .empty-state { padding: 40px; text-align: center; color: var(--text-dim); font-size: 0.9rem; border: 1px dashed rgba(255,255,255,0.1); border-radius: 8px; }
  
  .history-list { display: flex; flex-direction: column; gap: 0; }
  .history-card { display: flex; position: relative; padding-bottom: 24px; }
  .history-card:last-child { padding-bottom: 0; }
  
  /* Timeline connecting line */
  .history-card:not(:last-child)::before { content: ''; position: absolute; left: 93px; top: 12px; bottom: 0; width: 2px; background: rgba(255,255,255,0.05); z-index: 1; }

  .card-left { width: 80px; display: flex; flex-direction: column; align-items: flex-end; padding-top: 2px; flex-shrink: 0; }
  .time { font-size: 0.85rem; font-family: monospace; color: var(--text-primary); font-weight: 600; }
  .date { font-size: 0.7rem; font-family: monospace; color: var(--text-dim); margin-top: 2px; }
  
  .card-divider { width: 12px; height: 12px; border-radius: 50%; margin: 6px 16px 0 8px; z-index: 2; flex-shrink: 0; }
  .card-divider.discovered { background: var(--accent-green); box-shadow: 0 0 10px rgba(0,255,136,0.3); }
  .card-divider.changed { background: var(--accent-cyan); box-shadow: 0 0 10px rgba(0,212,255,0.3); }
  .card-divider.disappeared { background: var(--text-dim); border: 2px solid rgba(255,255,255,0.2); }
  .card-divider.reappeared { background: var(--accent-orange); box-shadow: 0 0 10px rgba(255,170,0,0.3); }

  .card-content { flex: 1; background: rgba(255,255,255,0.02); border: 1px solid rgba(255,255,255,0.05); border-radius: 8px; padding: 16px; display: flex; flex-direction: column; gap: 8px; transition: background 0.2s; }
  .card-content:hover { background: rgba(255,255,255,0.04); }

  .evt-header { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
  .evt-type { font-size: 0.7rem; font-weight: 800; padding: 2px 8px; border-radius: 4px; letter-spacing: 1px; }
  .evt-type.discovered { color: var(--accent-green); background: rgba(0,255,136,0.1); border: 1px solid rgba(0,255,136,0.2); }
  .evt-type.changed { color: var(--accent-cyan); background: rgba(0,212,255,0.1); border: 1px solid rgba(0,212,255,0.2); }
  .evt-type.disappeared { color: var(--text-secondary); background: rgba(255,255,255,0.05); border: 1px solid rgba(255,255,255,0.1); text-decoration: line-through; }
  .evt-type.reappeared { color: var(--accent-orange); background: rgba(255,170,0,0.1); border: 1px solid rgba(255,170,0,0.2); }

  .evt-target { display: flex; align-items: center; gap: 6px; font-size: 0.85rem; color: var(--text-secondary); }
  .target-link { color: var(--text-primary); text-decoration: none; font-weight: 700; font-family: monospace; padding: 2px 6px; background: rgba(0,0,0,0.3); border-radius: 4px; transition: color 0.2s; }
  .target-link:hover { color: var(--accent-cyan); }
  .dot { color: var(--text-dim); }
  .resource-type { font-weight: 600; text-transform: uppercase; font-size: 0.75rem; letter-spacing: 0.5px; }
  .resource-name { font-family: monospace; color: var(--text-primary); }

  .evt-summary { font-size: 0.9rem; color: var(--text-secondary); line-height: 1.4; margin-top: 4px; }
</style>
