<script lang="ts">
  import { onMount } from 'svelte';

  let storages = $state<any[]>([]);
  let wsConnected = $state(false);

  function formatBytes(bytes: number) {
    if (!+bytes) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
  }

  onMount(() => {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const ws = new WebSocket(`${protocol}//${window.location.host}/ws`);
    ws.onopen = () => { wsConnected = true; };
    ws.onclose = () => { wsConnected = false; };
    ws.onmessage = (e) => {
      try {
        const p = JSON.parse(e.data);
        if (p.type === 'lxc_detail' || p.type === 'vm_detail') {
          const items = p.lxc || p.vms || [];
          const allDisks: any[] = [];
          for (const item of items) {
            for (const d of item.disk_mounts || []) {
              allDisks.push({ guest: item.name, vmid: item.vmid, ...d });
            }
          }
          if (allDisks.length > 0) storages = allDisks;
        }
      } catch {}
    };
    return () => ws.close();
  });
</script>

<div class="page">
  <h2 class="page-title">STORAGE OVERVIEW</h2>

  {#if storages.length === 0}
    <div class="neon-card empty"><div class="pulse-ring"></div><p>{wsConnected ? 'WAITING FOR STORAGE DATA...' : 'CONNECTING...'}</p></div>
  {:else}
    <div class="table-wrap neon-card">
      <div class="table-header">
        <span class="col-guest">GUEST</span>
        <span class="col-mount">MOUNT</span>
        <span class="col-used">USED</span>
        <span class="col-total">TOTAL</span>
        <span class="col-pct">USAGE</span>
        <span class="col-bar">BAR</span>
      </div>
      {#each storages as s}
        <div class="table-row">
          <span class="col-guest">{s.guest} <span class="dim">({s.vmid})</span></span>
          <span class="col-mount mono">{s.mountpoint}</span>
          <span class="col-used">{formatBytes(s.used)}</span>
          <span class="col-total">{formatBytes(s.total)}</span>
          <span class="col-pct" class:warn={s.use_pct > 80} class:crit={s.use_pct > 95}>{s.use_pct.toFixed(0)}%</span>
          <span class="col-bar">
            <div class="bar"><div class="bar-fill" style="width:{s.use_pct}%; background:{s.use_pct > 90 ? 'var(--accent-red)' : s.use_pct > 70 ? 'var(--accent-yellow)' : 'var(--accent-green)'}"></div></div>
          </span>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .page { display: flex; flex-direction: column; gap: 20px; }
  .page-title { font-size: 0.85rem; letter-spacing: 3px; color: var(--text-secondary); font-weight: 600; }
  .table-wrap { padding: 16px; }
  .table-header, .table-row { display: grid; grid-template-columns: 1.2fr 1.5fr 0.8fr 0.8fr 0.6fr 1fr; align-items: center; padding: 8px 0; font-size: 0.75rem; }
  .table-header { font-size: 0.6rem; color: var(--text-secondary); letter-spacing: 2px; font-weight: 600; border-bottom: 1px solid var(--border-color); }
  .table-row { border-bottom: 1px solid rgba(255,255,255,0.02); }
  .table-row:hover { background: rgba(0,212,255,0.03); }
  .mono { font-family: 'Courier New', monospace; font-size: 0.7rem; color: var(--text-secondary); }
  .dim { font-size: 0.6rem; color: var(--text-dim); }
  .warn { color: var(--accent-yellow) !important; }
  .crit { color: var(--accent-red) !important; }
  .bar { height: 5px; background: rgba(255,255,255,0.06); border-radius: 3px; overflow: hidden; }
  .bar-fill { height: 100%; border-radius: 3px; transition: width 0.5s; }
  .empty { padding: 60px; text-align: center; color: var(--text-secondary); letter-spacing: 2px; font-size: 0.85rem; }
  .pulse-ring { width: 40px; height: 40px; border: 2px solid var(--accent-cyan); border-radius: 50%; margin: 0 auto 20px; animation: pr 2s ease-in-out infinite; }
  @keyframes pr { 0%,100% { transform: scale(0.8); opacity: 0.3; } 50% { transform: scale(1.1); opacity: 1; } }
</style>
