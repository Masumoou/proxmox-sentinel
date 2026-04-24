<script lang="ts">
  import { appLogEvents, appLogStats, appMetrics, wsConnected } from '$lib/store';

  let appEntries = $derived(Object.entries($appMetrics));
</script>

<div class="page">
  <div class="header">
    <div>
      <h2>APP OVERVIEW</h2>
      <p>Application metrics and parsed app logs</p>
    </div>
    <span class:ok={$wsConnected}>{$wsConnected ? 'REAL-TIME CONNECTED' : 'DISCONNECTED'}</span>
  </div>

  {#if appEntries.length === 0}
    <div class="empty">
      <strong>No applications configured</strong>
      <span>Add `[[app_metrics]]` or `[[app_logs]]` entries for sites like dev32.disk.bg, Nextcloud, APIs, or custom apps.</span>
    </div>
  {:else}
    <div class="app-grid">
      {#each appEntries as [appName, metrics]}
        <article class="app-card">
          <div class="app-head">
            <h3>{appName}</h3>
            {#if $appLogStats[appName]}
              <span>{$appLogStats[appName].requests_per_min} req/min · {$appLogStats[appName].errors_per_min} errors</span>
            {/if}
          </div>
          <div class="metric-grid">
            {#each Object.entries(metrics as Record<string, any>) as [key, metric]}
              <div class="metric">
                <span>{metric.label || key}</span>
                <strong>{metric.value}</strong>
                <small>{metric.unit}</small>
              </div>
            {/each}
          </div>
        </article>
      {/each}
    </div>
  {/if}

  <section class="log-card">
    <div class="section-head">Application Log Events</div>
    {#each $appLogEvents.slice(0, 100) as log (log.timestamp + log.line)}
      <div class="log-line">
        <span>{log.timestamp?.split('T')[1]?.split('.')[0]}</span>
        <b>{log.app}</b>
        <small>{log.level >= 3 ? 'ERROR' : log.level >= 2 ? 'WARN' : 'INFO'}</small>
        <p>{log.line}</p>
      </div>
    {:else}
      <div class="muted">No app log events yet.</div>
    {/each}
  </section>
</div>

<style>
  .page { display: flex; flex-direction: column; gap: 16px; }
  .header { display: flex; justify-content: space-between; align-items: flex-start; gap: 16px; }
  h2 { font-size: 1.5rem; letter-spacing: 2px; }
  p, .muted, .empty span { color: var(--text-secondary); }
  .header span { color: var(--accent-red); font-size: 0.68rem; font-weight: 800; letter-spacing: 1.5px; }
  .header span.ok { color: var(--accent-green); }
  .app-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(360px, 1fr)); gap: 12px; }
  .app-card, .log-card, .empty { background: var(--card-bg); border: 1px solid var(--border-color); border-radius: 8px; }
  .app-card { padding: 16px; }
  .app-head { display: flex; justify-content: space-between; gap: 12px; padding-bottom: 12px; border-bottom: 1px solid rgba(255,255,255,0.06); }
  h3 { color: var(--accent-cyan); text-transform: uppercase; letter-spacing: 1.4px; }
  .app-head span { color: var(--text-secondary); font-size: 0.68rem; }
  .metric-grid { display: grid; grid-template-columns: repeat(2, 1fr); gap: 10px; margin-top: 12px; }
  .metric { border: 1px solid rgba(255,255,255,0.06); border-radius: 6px; padding: 12px; min-height: 88px; display: flex; flex-direction: column; justify-content: space-between; }
  .metric span { color: var(--text-secondary); font-size: 0.62rem; letter-spacing: 1.5px; text-transform: uppercase; }
  .metric strong { font-size: 1.4rem; }
  .metric small { color: var(--text-dim); }
  .log-card { padding: 14px; min-height: 260px; }
  .section-head { color: var(--text-secondary); font-size: 0.68rem; letter-spacing: 2px; text-transform: uppercase; font-weight: 800; margin-bottom: 10px; }
  .log-line { display: grid; grid-template-columns: 82px 120px 70px 1fr; gap: 12px; padding: 7px 4px; border-bottom: 1px solid rgba(255,255,255,0.04); font-family: 'Courier New', monospace; font-size: 0.72rem; }
  .log-line span, .log-line small { color: var(--text-secondary); }
  .log-line b { color: var(--accent-purple); }
  .empty { min-height: 260px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; text-align: center; padding: 20px; }
  .empty strong { font-size: 1.1rem; }
</style>
