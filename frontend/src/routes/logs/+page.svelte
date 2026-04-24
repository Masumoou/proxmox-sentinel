<script lang="ts">
  import { clearLogs, logs, wsConnected } from '$lib/store';

  let level = $state('all');
  let query = $state('');
  let filtered = $derived(
    $logs.filter((log) => {
      const levelOk = level === 'all' || log.level.toLowerCase() === level;
      const q = query.toLowerCase();
      const queryOk = !q || `${log.source} ${log.message}`.toLowerCase().includes(q);
      return levelOk && queryOk;
    }).slice(-500)
  );
</script>

<div class="logs-page">
  <div class="page-header">
    <div>
      <h2 class="page-title">LIVE LOG STREAM</h2>
      <p>Cluster, VM collector, LXC collector, and watched log events share one stream.</p>
    </div>
    <div class="controls">
      <input bind:value={query} placeholder="Search logs" />
      <select bind:value={level}>
        <option value="all">All</option>
        <option value="info">Info</option>
        <option value="warn">Warn</option>
        <option value="warning">Warning</option>
        <option value="error">Error</option>
        <option value="critical">Critical</option>
      </select>
      <button onclick={clearLogs}>CLEAR</button>
    </div>
  </div>

  <div class="log-panel">
    {#if filtered.length === 0}
      <div class="empty">{$wsConnected ? 'WAITING FOR EVENTS...' : 'CONNECTING...'}</div>
    {:else}
      {#each filtered as log}
        <div class="log-line">
          <span class="time">{log.time}</span>
          <span class="level" class:warn={['WARN','WARNING'].includes(log.level)} class:error={['ERROR','CRITICAL'].includes(log.level)}>{log.level}</span>
          <span class="source">[{log.source}]</span>
          <span class="message">{log.message}</span>
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .logs-page { display: flex; flex-direction: column; gap: 16px; height: 100%; }
  .page-header { display: flex; justify-content: space-between; align-items: flex-end; gap: 16px; flex-wrap: wrap; }
  .page-title { font-size: 0.85rem; letter-spacing: 3px; color: var(--text-secondary); font-weight: 800; }
  p { color: var(--text-dim); font-size: 0.74rem; margin-top: 4px; }
  .controls { display: flex; gap: 8px; align-items: center; }
  input, select, button { background: var(--panel-bg); border: 1px solid var(--border-color); color: var(--text-primary); border-radius: 6px; padding: 8px 10px; font-size: 0.72rem; }
  button { color: var(--accent-cyan); font-weight: 800; letter-spacing: 1px; cursor: pointer; }
  .log-panel { flex: 1; min-height: 520px; overflow: auto; background: var(--card-bg); border: 1px solid var(--border-color); border-radius: 8px; padding: 12px; font-family: 'Courier New', monospace; }
  .log-line { min-width: 920px; display: grid; grid-template-columns: 82px 78px 130px 1fr; gap: 12px; padding: 5px 8px; border-bottom: 1px solid rgba(255,255,255,0.04); font-size: 0.74rem; line-height: 1.45; }
  .log-line:hover { background: rgba(0,212,255,0.04); }
  .time { color: var(--text-secondary); font-variant-numeric: tabular-nums; }
  .level { color: var(--accent-cyan); font-weight: 800; }
  .level.warn { color: var(--accent-yellow); }
  .level.error { color: var(--accent-red); }
  .source { color: var(--accent-purple); }
  .message { color: var(--text-primary); overflow-wrap: anywhere; }
  .empty { height: 100%; min-height: 360px; display: grid; place-items: center; color: var(--text-secondary); letter-spacing: 2px; }
</style>
