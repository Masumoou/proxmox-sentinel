<script lang="ts">
  import { onMount } from 'svelte';

  let guests = $state<any[]>([]);
  let wsConnected = $state(false);
  let detailMap: Record<number, any> = {};

  function formatBytes(bytes: number, decimals = 1) {
    if (!+bytes) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${parseFloat((bytes / Math.pow(k, i)).toFixed(decimals))} ${sizes[i]}`;
  }

  let lxcGuests = $derived(guests.filter(g => g.type === 'LXC'));

  onMount(() => {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const ws = new WebSocket(`${protocol}//${window.location.host}/ws`);
    ws.onopen = () => { wsConnected = true; };
    ws.onclose = () => { wsConnected = false; };
    ws.onmessage = (e) => {
      try {
        const p = JSON.parse(e.data);
        if (p.type === 'cluster_update') {
          guests = (p.guests || []).filter((g: any) => g.type === 'lxc').map((g: any) => {
            const d = detailMap[g.vmid] || {};
            return { id: g.vmid, name: g.name, node: g.node, type: 'LXC', status: g.status, cpu: Math.round(g.cpu * 100), ram: formatBytes(g.mem), services: d.services || [], disk_mounts: d.disk_mounts || [] };
          });
        }
        if (p.type === 'lxc_detail') {
          for (const lxc of p.lxc || []) detailMap[lxc.vmid] = { services: lxc.services || [], disk_mounts: lxc.disk_mounts || [] };
          guests = guests.map(g => ({ ...g, ...(detailMap[g.id] || {}) }));
        }
      } catch {}
    };
    return () => ws.close();
  });
</script>

<div class="page">
  <h2 class="page-title">CONTAINERS (LXC)</h2>

  {#if lxcGuests.length === 0}
    <div class="neon-card empty"><div class="pulse-ring"></div><p>{wsConnected ? 'NO LXC CONTAINERS FOUND' : 'CONNECTING...'}</p></div>
  {:else}
    <div class="grid">
      {#each lxcGuests as ct}
        <div class="neon-card neon-card-purple card">
          <div class="card-head">
            <div><div class="ct-name">{ct.name}</div><div class="ct-id">CT {ct.id} • {ct.node || '—'}</div></div>
            <span class="status-badge" class:up={ct.status === 'running'} class:down={ct.status !== 'running'}>● {ct.status.toUpperCase()}</span>
          </div>
          <div class="metrics">
            <div><span class="label">CPU</span><span class="val text-cyan">{ct.cpu}%</span></div>
            <div><span class="label">MEM</span><span class="val text-cyan">{ct.ram}</span></div>
          </div>
          {#if ct.services && ct.services.length > 0}
            <div class="svcs">
              {#each ct.services as svc}
                <div class="svc-row">
                  <span class="svc-dot" style="background: {svc.status === 'running' ? 'var(--accent-green)' : 'var(--accent-red)'}"></span>
                  <span class="svc-name">{svc.name}</span>
                  <span class={svc.status === 'running' ? 'badge-active' : 'badge-inactive'}>{svc.status === 'running' ? 'ACTIVE' : 'DOWN'}</span>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .page { display: flex; flex-direction: column; gap: 20px; }
  .page-title { font-size: 0.85rem; letter-spacing: 3px; color: var(--text-secondary); font-weight: 600; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(320px, 1fr)); gap: 14px; }
  .card { padding: 18px; display: flex; flex-direction: column; gap: 12px; }
  .card-head { display: flex; justify-content: space-between; align-items: flex-start; }
  .ct-name { font-size: 1.05rem; font-weight: 700; letter-spacing: 1px; }
  .ct-id { font-size: 0.6rem; color: var(--text-secondary); letter-spacing: 1.5px; margin-top: 2px; }
  .status-badge { font-size: 0.65rem; font-weight: 600; letter-spacing: 1px; }
  .up { color: var(--accent-green); }
  .down { color: var(--accent-red); }
  .metrics { display: flex; gap: 24px; padding: 8px 0; border-bottom: 1px solid rgba(255,255,255,0.04); }
  .label { font-size: 0.55rem; color: var(--text-secondary); letter-spacing: 2px; margin-right: 8px; }
  .val { font-size: 1.1rem; font-weight: 700; }
  .svcs { display: flex; flex-direction: column; gap: 4px; }
  .svc-row { display: flex; align-items: center; gap: 8px; padding: 3px 0; }
  .svc-dot { width: 7px; height: 7px; border-radius: 50%; }
  .svc-name { font-size: 0.72rem; font-weight: 600; letter-spacing: 1px; flex: 1; }
  .empty { padding: 60px; text-align: center; color: var(--text-secondary); letter-spacing: 2px; font-size: 0.85rem; }
  .pulse-ring { width: 40px; height: 40px; border: 2px solid var(--accent-purple); border-radius: 50%; margin: 0 auto 20px; animation: pr 2s ease-in-out infinite; }
  @keyframes pr { 0%,100% { transform: scale(0.8); opacity: 0.3; } 50% { transform: scale(1.1); opacity: 1; } }
</style>
