<script lang="ts">
  import { logs } from '$lib/store';

  let taskLogs = $derived($logs.filter((log) => /task|backup|snapshot|migrat|clone|restore/i.test(`${log.source} ${log.message}`)).slice(0, 80));
</script>

<div class="page">
  <div class="page-header">
    <h2>Proxmox Tasks</h2>
    <p>Failed and long-running task monitoring will use the Proxmox task API. Live task-like log events are shown here for now.</p>
  </div>

  <section class="panel">
    <div class="panel-title">Task Events</div>
    {#if taskLogs.length === 0}
      <div class="empty">No task events seen yet.</div>
    {:else}
      {#each taskLogs as log}
        <div class="row">
          <span>{log.time}</span>
          <b>{log.source}</b>
          <p>{log.message}</p>
        </div>
      {/each}
    {/if}
  </section>
</div>

<style>
  .page { display: flex; flex-direction: column; gap: 18px; }
  .page-header h2 { font-size: 0.9rem; letter-spacing: 3px; color: var(--text-secondary); text-transform: uppercase; }
  .page-header p, .empty { color: var(--text-secondary); font-size: 0.75rem; margin-top: 6px; }
  .panel { border: 1px solid var(--border-color); background: var(--card-bg); border-radius: 8px; padding: 16px; }
  .panel-title { color: var(--text-primary); font-weight: 800; letter-spacing: 1.5px; text-transform: uppercase; font-size: 0.72rem; margin-bottom: 12px; }
  .row { display: grid; grid-template-columns: 90px 140px minmax(0, 1fr); gap: 12px; padding: 9px 0; border-bottom: 1px solid rgba(255,255,255,0.05); font-size: 0.75rem; }
  .row span { color: var(--text-secondary); }
  .row b { color: var(--accent-cyan); }
  .row p { min-width: 0; overflow-wrap: anywhere; }
</style>
