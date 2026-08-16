<script lang="ts">
  // Mock Data mimicking the backend state
  // Notice we don't calculate Health here, we just consume it.
  let fleet = $state([
    {
      vmid: '101',
      name: 'web-server-prod',
      node: 'pve-01',
      state: 'RUNNING',
      agent: 'ACTIVE',
      health_score: 72, // Consumed from backend
      health_status: 'Degraded',
      resources: { monitored: 12, pending: 3, ignored: 5 },
      incidents: { open: 2, acknowledged: 1 },
      maintenance: 'NONE',
      notification: { inhibited: 1, active: 1 }
    },
    {
      vmid: '102',
      name: 'db-postgres-prod',
      node: 'pve-02',
      state: 'RUNNING',
      agent: 'ACTIVE',
      health_score: 98,
      health_status: 'Healthy',
      resources: { monitored: 18, pending: 0, ignored: 0 },
      incidents: { open: 0, acknowledged: 0 },
      maintenance: 'NONE',
      notification: { inhibited: 0, active: 0 }
    },
    {
      vmid: '103',
      name: 'legacy-app-server',
      node: 'pve-01',
      state: 'RUNNING',
      agent: 'UNKNOWN',
      health_score: null,
      health_status: 'Unknown',
      resources: { monitored: 8, pending: 1, ignored: 2 },
      incidents: { open: 1, acknowledged: 0 },
      maintenance: 'NONE',
      notification: { inhibited: 0, active: 1 }
    },
    {
      vmid: '104',
      name: 'redis-cache',
      node: 'pve-03',
      state: 'STOPPED',
      agent: 'UNREACHABLE',
      health_score: 0,
      health_status: 'Critical',
      resources: { monitored: 5, pending: 0, ignored: 0 },
      incidents: { open: 3, acknowledged: 0 },
      maintenance: 'ACTIVE', // Under maintenance
      notification: { inhibited: 3, active: 0 }
    }
  ]);

  // Aggregate stats derived from the fleet list
  let totalVMs = $derived(fleet.length);
  let healthyCount = $derived(fleet.filter(f => f.health_status === 'Healthy').length);
  let degradedCount = $derived(fleet.filter(f => f.health_status === 'Degraded').length);
  let criticalCount = $derived(fleet.filter(f => f.health_status === 'Critical').length);
</script>

<div class="page">
  <div class="header">
    <div>
      <h2>Sentinel Fleet</h2>
      <p>Operational overview of monitored resources and their alert states.</p>
    </div>
  </div>

  <div class="summary-cards">
    <div class="summary-card total">
      <div class="value">{totalVMs}</div>
      <div class="label">Total VMs</div>
    </div>
    <div class="summary-card healthy">
      <div class="value">{healthyCount}</div>
      <div class="label">Healthy</div>
    </div>
    <div class="summary-card degraded">
      <div class="value">{degradedCount}</div>
      <div class="label">Degraded</div>
    </div>
    <div class="summary-card critical">
      <div class="value">{criticalCount}</div>
      <div class="label">Critical</div>
    </div>
  </div>

  <div class="panel">
    <table class="fleet-table">
      <thead>
        <tr>
          <th>VM</th>
          <th>Health</th>
          <th>Agent</th>
          <th>Resources</th>
          <th>Incidents</th>
          <th>Maintenance</th>
          <th>Notifications</th>
        </tr>
      </thead>
      <tbody>
        {#each fleet as vm}
          <tr class="vm-row" onclick={() => window.location.href = `/guests/${vm.vmid}`}>
            <td>
              <div class="vm-info">
                <span class="vm-name">{vm.name}</span>
                <div class="vm-meta">
                  <span class="vm-id">#{vm.vmid}</span>
                  <span class="vm-node">on {vm.node}</span>
                  <span class="vm-state {vm.state.toLowerCase()}">{vm.state}</span>
                </div>
              </div>
            </td>
            
            <td>
              <div class="health-block">
                {#if vm.health_score !== null}
                  <span class="health-score {vm.health_status.toLowerCase()}">{vm.health_score}</span>
                  <span class="health-max">/100</span>
                {:else}
                  <span class="health-score unknown">N/A</span>
                {/if}
              </div>
            </td>

            <td>
              <div class="agent-status {vm.agent.toLowerCase()}" title={vm.agent}>
                {#if vm.agent === 'ACTIVE'}
                  ●
                {:else if vm.agent === 'UNKNOWN'}
                  ?
                {:else}
                  ×
                {/if}
              </div>
            </td>

            <td>
              <div class="resource-block">
                <div class="res-primary">{vm.resources.monitored} <span class="dim">monitored</span></div>
                <div class="res-secondary">
                  {#if vm.resources.pending > 0}
                    <span class="res-pending" title="Pending Onboarding">{vm.resources.pending} pend</span>
                  {/if}
                  {#if vm.resources.ignored > 0}
                    <span class="res-ignored" title="Permanently Ignored">{vm.resources.ignored} ign</span>
                  {/if}
                </div>
              </div>
            </td>

            <td>
              <div class="incident-block">
                {#if vm.incidents.open > 0}
                  <div class="inc-open">{vm.incidents.open} OPEN</div>
                {:else}
                  <div class="inc-none">0</div>
                {/if}
                {#if vm.incidents.acknowledged > 0}
                  <div class="inc-ack">{vm.incidents.acknowledged} ACK</div>
                {/if}
              </div>
            </td>

            <td>
              <div class="maintenance-block">
                {#if vm.maintenance === 'ACTIVE'}
                  <span class="maint-active">ACTIVE</span>
                {:else}
                  <span class="maint-none">NONE</span>
                {/if}
              </div>
            </td>

            <td>
              <div class="notification-block">
                {#if vm.notification.inhibited > 0}
                  <div class="notif-inhibited">🔇 {vm.notification.inhibited} INHIBITED</div>
                {/if}
                {#if vm.notification.active > 0}
                  <div class="notif-active">🔔 {vm.notification.active} ACTIVE</div>
                {/if}
                {#if vm.notification.inhibited === 0 && vm.notification.active === 0}
                  <div class="notif-none">--</div>
                {/if}
              </div>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</div>

<style>
  .page { padding-bottom: 60px; max-width: 1300px; margin: 0 auto; display: flex; flex-direction: column; gap: 20px; }
  .header { display: flex; justify-content: space-between; align-items: flex-end; margin-bottom: 10px; }
  .header h2 { margin: 0 0 8px 0; font-size: 1.8rem; color: var(--text-primary); }
  .header p { margin: 0; color: var(--text-secondary); font-size: 0.95rem; }

  /* Summary Cards */
  .summary-cards { display: grid; grid-template-columns: repeat(4, 1fr); gap: 16px; margin-bottom: 10px; }
  .summary-card { background: var(--card-bg); border: 1px solid var(--border-color); border-radius: 8px; padding: 20px; display: flex; flex-direction: column; align-items: center; justify-content: center; box-shadow: 0 4px 15px rgba(0,0,0,0.1); }
  .summary-card .value { font-size: 2.2rem; font-weight: 900; line-height: 1; margin-bottom: 8px; }
  .summary-card .label { font-size: 0.8rem; text-transform: uppercase; letter-spacing: 1px; font-weight: 700; color: var(--text-secondary); }
  
  .summary-card.total .value { color: var(--text-primary); }
  .summary-card.healthy { border-bottom: 3px solid var(--accent-green); }
  .summary-card.healthy .value { color: var(--accent-green); }
  .summary-card.degraded { border-bottom: 3px solid var(--accent-orange); }
  .summary-card.degraded .value { color: var(--accent-orange); }
  .summary-card.critical { border-bottom: 3px solid var(--accent-red); }
  .summary-card.critical .value { color: var(--accent-red); }

  /* Panel & Table */
  .panel { background: var(--card-bg); border: 1px solid var(--border-color); border-radius: 10px; overflow: hidden; box-shadow: 0 4px 20px rgba(0,0,0,0.15); }
  
  .fleet-table { width: 100%; border-collapse: collapse; text-align: left; }
  .fleet-table th { padding: 16px 20px; background: rgba(0,0,0,0.3); color: var(--text-dim); font-size: 0.75rem; text-transform: uppercase; letter-spacing: 1px; font-weight: 800; border-bottom: 1px solid rgba(255,255,255,0.05); }
  
  .vm-row { border-bottom: 1px solid rgba(255,255,255,0.03); transition: background 0.2s; cursor: pointer; }
  .vm-row:hover { background: rgba(255,255,255,0.03); }
  .vm-row:last-child { border-bottom: none; }
  .fleet-table td { padding: 16px 20px; vertical-align: top; }

  /* VM Info */
  .vm-info { display: flex; flex-direction: column; gap: 4px; }
  .vm-name { font-weight: 700; font-size: 1.05rem; color: var(--text-primary); }
  .vm-meta { display: flex; gap: 8px; align-items: center; font-size: 0.75rem; color: var(--text-dim); font-family: monospace; }
  .vm-state { padding: 2px 6px; border-radius: 4px; font-weight: 800; font-size: 0.7rem; background: rgba(255,255,255,0.05); }
  .vm-state.running { color: var(--accent-green); background: rgba(0,255,136,0.1); }
  .vm-state.stopped { color: var(--accent-red); background: rgba(255,51,85,0.1); }

  /* Health Block */
  .health-block { display: flex; align-items: baseline; }
  .health-score { font-size: 1.4rem; font-weight: 900; font-family: monospace; }
  .health-score.healthy { color: var(--accent-green); }
  .health-score.degraded { color: var(--accent-orange); }
  .health-score.critical { color: var(--accent-red); }
  .health-score.unknown { color: var(--text-dim); font-size: 1rem; }
  .health-max { font-size: 0.8rem; color: var(--text-dim); margin-left: 2px; }

  /* Agent Block */
  .agent-status { font-size: 1.2rem; display: flex; align-items: center; justify-content: center; width: 24px; height: 24px; border-radius: 50%; }
  .agent-status.active { color: var(--accent-green); text-shadow: 0 0 10px rgba(0,255,136,0.5); }
  .agent-status.unknown { color: var(--text-dim); font-size: 1rem; font-weight: bold; background: rgba(255,255,255,0.05); }
  .agent-status.unreachable { color: var(--accent-red); font-size: 1.1rem; font-weight: bold; }

  /* Resource Block */
  .resource-block { display: flex; flex-direction: column; gap: 4px; }
  .res-primary { font-size: 1rem; font-weight: 800; color: var(--text-primary); }
  .res-primary .dim { font-size: 0.75rem; color: var(--text-dim); font-weight: 600; text-transform: uppercase; letter-spacing: 0.5px; }
  .res-secondary { display: flex; gap: 8px; font-size: 0.75rem; font-family: monospace; font-weight: 600; }
  .res-pending { color: var(--accent-orange); }
  .res-ignored { color: var(--text-dim); }

  /* Incident Block */
  .incident-block { display: flex; flex-direction: column; gap: 6px; }
  .inc-open { font-size: 0.8rem; font-weight: 800; color: var(--accent-red); background: rgba(255,51,85,0.1); border: 1px solid rgba(255,51,85,0.2); padding: 2px 8px; border-radius: 4px; display: inline-block; width: max-content; }
  .inc-ack { font-size: 0.8rem; font-weight: 800; color: var(--text-secondary); background: rgba(255,255,255,0.05); padding: 2px 8px; border-radius: 4px; display: inline-block; width: max-content; }
  .inc-none { font-size: 0.9rem; color: var(--text-dim); font-weight: 800; }

  /* Maintenance Block */
  .maintenance-block { padding-top: 4px; }
  .maint-active { font-size: 0.75rem; font-weight: 800; color: var(--accent-orange); letter-spacing: 1px; border: 1px solid var(--accent-orange); padding: 2px 6px; border-radius: 4px; }
  .maint-none { font-size: 0.75rem; font-weight: 700; color: var(--text-dim); }

  /* Notification Block */
  .notification-block { display: flex; flex-direction: column; gap: 4px; font-size: 0.8rem; font-weight: 700; }
  .notif-inhibited { color: var(--text-dim); }
  .notif-active { color: var(--accent-cyan); }
  .notif-none { color: var(--text-dim); font-weight: normal; }
</style>
