<script lang="ts">
  import { enrichedGuests, formatBytes, isServiceFailed, isServiceRunning, pct, previewServices, wsConnected } from '$lib/store';

  let containers = $derived($enrichedGuests.filter((guest) => guest.type === 'LXC'));
</script>

<div class="page">
  <h2 class="page-title">CONTAINERS (LXC)</h2>

  {#if containers.length === 0}
    <div class="empty">{$wsConnected ? 'NO LXC CONTAINERS FOUND IN CURRENT TELEMETRY' : 'CONNECTING...'}</div>
  {:else}
    <div class="grid">
      {#each containers as ct (ct.vmid)}
        <article class="card">
          <div class="head">
            <div><h3>{ct.name}</h3><p>CT {ct.vmid} · {ct.node}</p></div>
            <span class:ok={ct.status === 'running'} class:bad={ct.status !== 'running'}>{ct.status}</span>
          </div>
          <div class="metrics">
            <div><span>CPU</span><strong>{Math.round(ct.cpu * 100)}%</strong></div>
            <div><span>RAM</span><strong>{formatBytes(ct.mem)}</strong><small>{Math.round(pct(ct.mem, ct.maxmem))}%</small></div>
            <div><span>PIDS</span><strong>{ct.pids || '-'}</strong></div>
          </div>
          <div class="services">
            {#each previewServices(ct.services, 10) as service}
              <span class:down={isServiceFailed(service)} class:muted={!isServiceRunning(service) && !isServiceFailed(service)}>{service.name}</span>
            {:else}
              <em>No services discovered</em>
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
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 12px; }
  .card, .empty { background: var(--card-bg); border: 1px solid var(--border-color); border-radius: 8px; }
  .card { min-height: 220px; padding: 15px; display: flex; flex-direction: column; gap: 14px; }
  .head { display: flex; justify-content: space-between; gap: 12px; min-height: 54px; }
  h3 { font-size: 1rem; overflow-wrap: anywhere; }
  p, em, small { color: var(--text-secondary); font-size: 0.68rem; }
  .ok { color: var(--accent-green); }
  .bad { color: var(--accent-red); }
  .metrics { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; }
  .metrics div { border: 1px solid rgba(255,255,255,0.06); border-radius: 6px; padding: 9px; min-height: 70px; }
  .metrics span { display: block; color: var(--text-secondary); font-size: 0.58rem; letter-spacing: 1.5px; }
  .metrics strong { display: block; margin-top: 8px; }
  .services { display: flex; flex-wrap: wrap; gap: 6px; margin-top: auto; }
  .services span { color: var(--accent-green); border: 1px solid rgba(0,255,136,0.18); background: rgba(0,255,136,0.08); border-radius: 4px; padding: 3px 7px; font-size: 0.65rem; }
  .services span.down { color: var(--accent-red); border-color: rgba(255,51,85,0.22); background: rgba(255,51,85,0.08); }
  .services span.muted { color: var(--text-secondary); border-color: rgba(255,255,255,0.08); background: rgba(255,255,255,0.03); }
  .empty { min-height: 320px; display: grid; place-items: center; color: var(--text-secondary); letter-spacing: 2px; }
</style>
