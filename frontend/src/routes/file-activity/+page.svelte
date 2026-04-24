<script lang="ts">
  import { clearSecurityEvents, formatBytes, securityEvents, wsConnected } from '$lib/store';

  let ipFilter = $state('');
  let pathFilter = $state('');

  let filteredEvents = $derived(
    $securityEvents.filter((event) =>
      (event.matches?.ip || '').includes(ipFilter) &&
      (event.matches?.path || '').toLowerCase().includes(pathFilter.toLowerCase())
    )
  );

  let failedRequests = $derived($securityEvents.filter((event) => Number(event.matches?.status || 0) >= 400).length);
  let largestFile = $derived(Math.max(0, ...$securityEvents.map((event) => Number(event.matches?.size || 0))));
  let mostActiveIp = $derived.by(() => {
    const counts: Record<string, number> = {};
    for (const event of $securityEvents) counts[event.matches?.ip || 'unknown'] = (counts[event.matches?.ip || 'unknown'] || 0) + 1;
    return Object.entries(counts).sort((a, b) => b[1] - a[1])[0]?.[0] || 'N/A';
  });
</script>

<div class="page">
  <div class="header">
    <div>
      <h2>FILE ACTIVITY</h2>
      <p>Access log and security event stream</p>
    </div>
    <span class:ok={$wsConnected}>{$wsConnected ? 'LIVE STREAM' : 'DISCONNECTED'}</span>
  </div>

  <div class="stats">
    <div><span>Total Requests</span><strong>{$securityEvents.length}</strong></div>
    <div><span>Failed 4xx/5xx</span><strong class="bad">{failedRequests}</strong></div>
    <div><span>Largest File</span><strong>{formatBytes(largestFile)}</strong></div>
    <div><span>Most Active IP</span><strong>{mostActiveIp}</strong></div>
  </div>

  <div class="filters">
    <input bind:value={ipFilter} placeholder="Filter by IP" />
    <input bind:value={pathFilter} placeholder="Search path" />
    <button onclick={clearSecurityEvents}>CLEAR</button>
  </div>

  <div class="table">
    <div class="table-head"><span>Time</span><span>Source</span><span>IP</span><span>User</span><span>Method</span><span>Path</span><span>Size</span><span>Status</span></div>
    {#each filteredEvents as event (event.timestamp + event.line)}
      <div class="row">
        <span>{event.timestamp?.split('T')[1]?.split('.')[0] || '-'}</span>
        <span>{event.file?.split('/').pop() || '-'}</span>
        <span>{event.matches?.ip || '-'}</span>
        <span>{event.matches?.user || '-'}</span>
        <span>{event.matches?.method || '-'}</span>
        <span class="path">{event.matches?.path || '-'}</span>
        <span>{formatBytes(Number(event.matches?.size || 0))}</span>
        <span class:bad={Number(event.matches?.status || 0) >= 400}>{event.matches?.status || '-'}</span>
      </div>
    {:else}
      <div class="empty">No file activity yet. Configure `[file_activity] watch_paths` with access logs to populate this page.</div>
    {/each}
  </div>
</div>

<style>
  .page { display: flex; flex-direction: column; gap: 16px; }
  .header { display: flex; justify-content: space-between; align-items: flex-start; gap: 16px; }
  h2 { font-size: 1.5rem; letter-spacing: 2px; }
  p { color: var(--text-secondary); margin-top: 4px; }
  .header span { color: var(--accent-red); font-size: 0.68rem; font-weight: 800; letter-spacing: 1.5px; }
  .header span.ok { color: var(--accent-green); }
  .stats { display: grid; grid-template-columns: repeat(4, minmax(160px, 1fr)); gap: 12px; }
  .stats div, .filters, .table { background: var(--card-bg); border: 1px solid var(--border-color); border-radius: 8px; }
  .stats div { padding: 14px; display: flex; flex-direction: column; gap: 8px; min-height: 92px; }
  .stats span { color: var(--text-secondary); font-size: 0.62rem; letter-spacing: 2px; text-transform: uppercase; }
  .stats strong { font-size: 1.35rem; overflow-wrap: anywhere; }
  .bad { color: var(--accent-red) !important; }
  .filters { display: flex; gap: 10px; padding: 12px; flex-wrap: wrap; }
  input, button { background: var(--panel-bg); border: 1px solid var(--border-color); color: var(--text-primary); border-radius: 6px; padding: 8px 10px; }
  button { color: var(--accent-cyan); font-weight: 800; letter-spacing: 1px; cursor: pointer; }
  .table { overflow-x: auto; }
  .table-head, .row { min-width: 1100px; display: grid; grid-template-columns: 90px 150px 130px 100px 80px 1fr 90px 80px; gap: 10px; padding: 9px 12px; align-items: center; }
  .table-head { color: var(--text-secondary); font-size: 0.58rem; letter-spacing: 2px; text-transform: uppercase; border-bottom: 1px solid var(--border-color); }
  .row { border-bottom: 1px solid rgba(255,255,255,0.04); font-family: 'Courier New', monospace; font-size: 0.72rem; }
  .path { overflow-wrap: anywhere; color: var(--text-primary); }
  .empty { min-height: 220px; display: grid; place-items: center; color: var(--text-secondary); text-align: center; padding: 24px; }
  @media (max-width: 1100px) { .stats { grid-template-columns: repeat(2, 1fr); } }
</style>
