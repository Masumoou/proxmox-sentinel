<script lang="ts">
  import { formatBytes, haproxyStats, wsConnected } from '$lib/store';
</script>

<div class="page">
  <h2 class="page-title">HAPROXY LOAD BALANCER</h2>

  {#if !$haproxyStats}
    <div class="empty">
      <strong>HAProxy telemetry is not configured</strong>
      <span>Set `[haproxy] enabled = true` and `stats_url = "http://host:8404/stats;csv"` in config.toml.</span>
      <small>{$wsConnected ? 'WebSocket is connected.' : 'Waiting for telemetry connection.'}</small>
    </div>
  {:else}
    <div class="summary">
      <div><span>Servers Up</span><strong class="ok">{$haproxyStats.servers_up}</strong></div>
      <div><span>Servers Down</span><strong class="bad">{$haproxyStats.servers_down}</strong></div>
      <div><span>Total Servers</span><strong>{$haproxyStats.total_servers}</strong></div>
    </div>

    <div class="proxy-grid">
      {#each $haproxyStats.proxies || [] as proxy}
        <article class="proxy-card">
          <div class="proxy-head">
            <h3>{proxy.name}</h3>
            <span>{proxy.backend_status}</span>
          </div>
          <div class="server-list">
            {#each proxy.servers || [] as server}
              <div class="server-row">
                <span class:ok={server.status?.startsWith('UP')} class:bad={!server.status?.startsWith('UP')}>●</span>
                <b>{server.name}</b>
                <small>{server.status}</small>
                <small>{server.sessions} sessions</small>
                <small>{formatBytes(server.bytes_in)} in</small>
                <small>{formatBytes(server.bytes_out)} out</small>
                <small>{server.http_5xx} 5xx</small>
              </div>
            {/each}
          </div>
        </article>
      {/each}
    </div>
  {/if}
</div>

<style>
  .page { display: flex; flex-direction: column; gap: 16px; }
  .page-title { font-size: 0.85rem; letter-spacing: 3px; color: var(--text-secondary); font-weight: 800; }
  .summary { display: grid; grid-template-columns: repeat(3, minmax(160px, 1fr)); gap: 12px; }
  .summary div, .proxy-card, .empty { background: var(--card-bg); border: 1px solid var(--border-color); border-radius: 8px; }
  .summary div { padding: 16px; display: flex; flex-direction: column; gap: 8px; }
  .summary span { color: var(--text-secondary); font-size: 0.62rem; letter-spacing: 2px; text-transform: uppercase; }
  .summary strong { font-size: 2rem; }
  .proxy-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(420px, 1fr)); gap: 12px; }
  .proxy-card { padding: 16px; }
  .proxy-head { display: flex; justify-content: space-between; align-items: center; gap: 12px; padding-bottom: 12px; border-bottom: 1px solid rgba(255,255,255,0.06); }
  h3 { color: var(--accent-cyan); font-size: 1rem; }
  .proxy-head span { color: var(--text-secondary); font-size: 0.65rem; }
  .server-list { display: flex; flex-direction: column; margin-top: 10px; }
  .server-row { display: grid; grid-template-columns: 18px 1fr 70px 86px 84px 84px 56px; gap: 8px; padding: 8px 0; border-bottom: 1px solid rgba(255,255,255,0.04); align-items: center; font-size: 0.72rem; }
  .server-row b { overflow-wrap: anywhere; }
  small { color: var(--text-secondary); }
  .ok { color: var(--accent-green); }
  .bad { color: var(--accent-red); }
  .empty { min-height: 260px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; text-align: center; color: var(--text-secondary); }
  .empty strong { color: var(--text-primary); }
  @media (max-width: 900px) {
    .summary { grid-template-columns: 1fr; }
    .proxy-grid { grid-template-columns: 1fr; }
  }
</style>
