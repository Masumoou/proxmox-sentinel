<script lang="ts">
  import { logs } from '$lib/store';

  let alertLogs = $derived($logs.filter((log) => ['WARN', 'ERROR', 'CRITICAL'].includes(log.level)).slice(0, 100));
</script>

<div class="page">
  <div class="page-header">
    <h2>Alerts</h2>
    <p>Default and manual alert rules will live here. Current warning/error telemetry is shown below.</p>
  </div>

  <section class="rule-grid">
    <div class="rule">Node CPU &gt; 90%</div>
    <div class="rule">Storage &gt; 85%</div>
    <div class="rule">VM stopped unexpectedly</div>
    <div class="rule">Guest agent not responding</div>
    <div class="rule">No backup in 48h</div>
    <div class="rule">ZFS degraded</div>
  </section>

  <section class="panel">
    <div class="panel-title">Recent Alert Stream</div>
    {#if alertLogs.length === 0}
      <div class="empty">No warning or error events yet.</div>
    {:else}
      {#each alertLogs as log}
        <div class="row">
          <span>{log.time}</span>
          <b class:bad={log.level !== 'WARN'}>{log.level}</b>
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
  .rule-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(210px, 1fr)); gap: 10px; }
  .rule, .panel { border: 1px solid var(--border-color); background: var(--card-bg); border-radius: 8px; }
  .rule { min-height: 64px; padding: 14px; display: grid; place-items: center; color: var(--text-primary); font-weight: 800; font-size: 0.75rem; text-align: center; }
  .panel { padding: 16px; }
  .panel-title { color: var(--text-primary); font-weight: 800; letter-spacing: 1.5px; text-transform: uppercase; font-size: 0.72rem; margin-bottom: 12px; }
  .row { display: grid; grid-template-columns: 90px 90px minmax(0, 1fr); gap: 12px; padding: 9px 0; border-bottom: 1px solid rgba(255,255,255,0.05); font-size: 0.75rem; }
  .row span { color: var(--text-secondary); }
  .row b { color: var(--accent-orange); }
  .row b.bad { color: var(--accent-red); }
  .row p { min-width: 0; overflow-wrap: anywhere; }
</style>
