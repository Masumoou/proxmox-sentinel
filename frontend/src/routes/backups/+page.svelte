<script lang="ts">
  import { formatBytes, platformHealth } from '$lib/store';

  let backups = $derived($platformHealth.backups || []);
  let critical = $derived(backups.filter((b: any) => b.status === 'critical').length);
  let warning = $derived(backups.filter((b: any) => b.status === 'warning').length);

  function ageLabel(row: any) {
    if (row.age_hours === null || row.age_hours === undefined) return 'never';
    if (row.age_hours < 24) return `${row.age_hours}h ago`;
    return `${Math.floor(row.age_hours / 24)}d ${row.age_hours % 24}h ago`;
  }
</script>

<div class="page">
  <div class="page-header">
    <div>
      <h2>Backup Health</h2>
      <p>vzdump/PBS task history, backup age, failed jobs, and guests without backups.</p>
    </div>
    <div class="summary">
      <span class:bad={critical > 0}>{critical} critical</span>
      <span class:warn={warning > 0}>{warning} warning</span>
    </div>
  </div>

  <section class="panel">
    {#if backups.length === 0}
      <div class="empty">Waiting for Proxmox backup history...</div>
    {:else}
      <div class="table">
        <div class="head"><span>VMID</span><span>Name</span><span>Node</span><span>Last backup</span><span>Task</span><span>Status</span></div>
        {#each backups as row (`${row.vmid}-${row.node}`)}
          <div class="row" class:bad={row.status === 'critical'} class:warn={row.status === 'warning'}>
            <span>{row.vmid}</span>
            <b>{row.name}</b>
            <span>{row.node}</span>
            <span>{ageLabel(row)}</span>
            <span>{row.task_status}{row.size_bytes ? ` · ${formatBytes(row.size_bytes)}` : ''}</span>
            <strong>{row.status.toUpperCase()}</strong>
          </div>
        {/each}
      </div>
    {/if}
  </section>
</div>

<style>
  .page { display: flex; flex-direction: column; gap: 16px; }
  .page-header { display: flex; justify-content: space-between; gap: 16px; align-items: flex-start; }
  h2 { font-size: 0.9rem; letter-spacing: 3px; color: var(--text-secondary); text-transform: uppercase; }
  p, .empty { color: var(--text-secondary); font-size: 0.75rem; margin-top: 6px; }
  .summary { display: flex; gap: 8px; flex-wrap: wrap; justify-content: flex-end; }
  .summary span { border: 1px solid var(--border-color); border-radius: 6px; padding: 7px 10px; color: var(--text-secondary); font-size: 0.68rem; text-transform: uppercase; letter-spacing: 1.2px; }
  .panel { background: var(--card-bg); border: 1px solid var(--border-color); border-radius: 8px; padding: 16px; }
  .table { overflow-x: auto; }
  .head, .row { min-width: 920px; display: grid; grid-template-columns: 80px 1.6fr 1fr 1fr 1.4fr 0.9fr; gap: 12px; align-items: center; padding: 10px 12px; }
  .head { color: var(--text-secondary); font-size: 0.58rem; letter-spacing: 2px; text-transform: uppercase; border-bottom: 1px solid var(--border-color); }
  .row { border-bottom: 1px solid rgba(255,255,255,0.05); font-size: 0.75rem; }
  .row b { color: var(--text-primary); }
  .row span { color: var(--text-secondary); }
  .row strong { color: var(--accent-green); font-size: 0.68rem; }
  .bad, .row.bad strong { color: var(--accent-red) !important; }
  .warn, .row.warn strong { color: var(--accent-orange) !important; }
  .empty { min-height: 180px; display: grid; place-items: center; letter-spacing: 1px; }
</style>
