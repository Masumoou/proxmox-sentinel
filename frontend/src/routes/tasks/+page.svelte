<script lang="ts">
  import { platformHealth } from '$lib/store';

  let tasks = $derived(($platformHealth.tasks || []).slice(0, 120));
  let failed = $derived(tasks.filter((t: any) => /fail|error/i.test(t.status)).length);
  let running = $derived(tasks.filter((t: any) => !t.end_time).length);

  function time(ts: number) {
    if (!ts) return '--';
    return new Date(ts * 1000).toLocaleString();
  }
</script>

<div class="page">
  <div class="page-header">
    <div>
      <h2>Proxmox Tasks</h2>
      <p>Failed, backup, migration, clone, restore, snapshot, and long-running task visibility.</p>
    </div>
    <div class="summary">
      <span class:bad={failed > 0}>{failed} failed</span>
      <span>{running} running</span>
    </div>
  </div>

  <section class="panel">
    {#if tasks.length === 0}
      <div class="empty">Waiting for Proxmox task API...</div>
    {:else}
      <div class="table">
        <div class="head"><span>Type</span><span>Node</span><span>VMID</span><span>User</span><span>Started</span><span>Duration</span><span>Status</span></div>
        {#each tasks as task (task.upid)}
          <div class="row" class:bad={/fail|error/i.test(task.status)} class:active={!task.end_time}>
            <b>{task.worker_type || 'task'}</b>
            <span>{task.node}</span>
            <span>{task.vmid || '--'}</span>
            <span>{task.user || '--'}</span>
            <span>{time(task.start_time)}</span>
            <span>{Math.round((task.duration_secs || 0) / 60)}m</span>
            <strong>{task.status || 'running'}</strong>
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
  .panel { border: 1px solid var(--border-color); background: var(--card-bg); border-radius: 8px; padding: 16px; }
  .table { overflow-x: auto; }
  .head, .row { min-width: 1040px; display: grid; grid-template-columns: 1fr 1fr 90px 1fr 1.5fr 0.8fr 1fr; gap: 12px; padding: 10px 12px; align-items: center; }
  .head { color: var(--text-secondary); font-size: 0.58rem; letter-spacing: 2px; text-transform: uppercase; border-bottom: 1px solid var(--border-color); }
  .row { border-bottom: 1px solid rgba(255,255,255,0.05); font-size: 0.75rem; }
  .row b { color: var(--accent-cyan); }
  .row span { color: var(--text-secondary); }
  .row strong { color: var(--accent-green); }
  .row.active strong { color: var(--accent-orange); }
  .bad, .row.bad strong { color: var(--accent-red) !important; }
  .empty { min-height: 180px; display: grid; place-items: center; letter-spacing: 1px; }
</style>
