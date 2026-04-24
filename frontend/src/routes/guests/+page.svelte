<script lang="ts">
  import { enrichedGuests, formatBytes, pct, wsConnected } from '$lib/store';

  let filter = $state('all');
  let query = $state('');

  let filteredGuests = $derived(
    $enrichedGuests.filter((guest) => {
      const matchesFilter =
        filter === 'all' ||
        (filter === 'running' && guest.status === 'running') ||
        (filter === 'stopped' && guest.status !== 'running') ||
        (filter === 'lxc' && guest.type === 'LXC') ||
        (filter === 'qemu' && guest.type === 'QEMU');
      const q = query.toLowerCase();
      const matchesQuery = !q || `${guest.vmid} ${guest.name} ${guest.node} ${guest.ip || ''}`.toLowerCase().includes(q);
      return matchesFilter && matchesQuery;
    })
  );

  function serviceCount(guest: any) {
    const up = guest.services.filter((service: any) => service.status === 'running').length;
    return `${up}/${guest.services.length}`;
  }

  function visibility(guest: any) {
    if (guest.type === 'LXC') return 'host';
    if (guest.agent) return 'agent';
    if (guest.ssh) return 'ssh';
    return 'none';
  }

  function osLabel(guest: any) {
    if (guest.os_name && guest.os_version) return `${guest.os_name} ${guest.os_version}`;
    if (guest.os_name) return guest.os_name;
    return 'unknown';
  }
</script>

<div class="guests-page">
  <div class="page-header">
    <div>
      <h2 class="page-title">GUEST MONITOR</h2>
      <p class="page-subtitle">VMs and containers with cached live state</p>
    </div>
    <div class="controls">
      <input bind:value={query} placeholder="Search ID, name, node, IP" />
      <div class="filter-bar">
        {#each ['all', 'running', 'stopped', 'lxc', 'qemu'] as f}
          <button class:active={filter === f} onclick={() => filter = f}>{f.toUpperCase()}</button>
        {/each}
      </div>
    </div>
  </div>

  {#if filteredGuests.length === 0}
    <div class="empty">{ $wsConnected ? 'NO GUESTS MATCH FILTER' : 'CONNECTING...' }</div>
  {:else}
    <div class="table">
      <div class="table-head">
        <span>ID</span><span>Name</span><span>Type</span><span>OS</span><span>Node</span><span>IP</span><span>Status</span><span>CPU</span><span>RAM</span><span>Services</span><span>Visibility</span>
      </div>
      {#each filteredGuests as guest (guest.vmid)}
        <div class="row" class:stopped={guest.status !== 'running'}>
          <span>{guest.vmid}</span>
          <span class="name">{guest.name}</span>
          <span><b>{guest.type}</b></span>
          <span class="mono">{osLabel(guest)}</span>
          <span>{guest.node}</span>
          <span class="mono">{guest.ip || 'unknown'}</span>
          <span class:ok={guest.status === 'running'} class:bad={guest.status !== 'running'}>{guest.status}</span>
          <span>{(guest.cpu * 100).toFixed(1)}% <small>{guest.maxcpu ? `${guest.maxcpu} vCPU` : ''}</small></span>
          <span>{formatBytes(guest.mem)} <small>{Math.round(pct(guest.mem, guest.maxmem))}%</small></span>
          <span>{serviceCount(guest)}</span>
          <span class:bad={visibility(guest) === 'none'}>{visibility(guest)}</span>
        </div>
        {#if guest.services.length > 0}
          <div class="service-row">
            {#each guest.services.slice(0, 16) as service}
              <span class="service-chip" class:down={service.status !== 'running'}>{service.name}</span>
            {/each}
          </div>
        {:else if guest.type === 'QEMU' && guest.status === 'running'}
          <div class="hint-row">Install/enable QEMU Guest Agent or configure SSH to show services, disks, and reliable IPs for this VM.</div>
        {/if}
      {/each}
    </div>
  {/if}
</div>

<style>
  .guests-page { display: flex; flex-direction: column; gap: 18px; min-width: 0; }
  .page-header { display: flex; justify-content: space-between; align-items: flex-end; gap: 16px; flex-wrap: wrap; }
  .page-title { font-size: 0.85rem; letter-spacing: 3px; color: var(--text-secondary); font-weight: 800; }
  .page-subtitle { margin-top: 4px; color: var(--text-dim); font-size: 0.75rem; }
  .controls { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }
  input { width: 260px; background: var(--panel-bg); border: 1px solid var(--border-color); color: var(--text-primary); border-radius: 6px; padding: 8px 10px; }
  .filter-bar { display: flex; gap: 6px; }
  button { padding: 7px 12px; border-radius: 6px; border: 1px solid var(--border-color); background: transparent; color: var(--text-secondary); font-size: 0.62rem; font-weight: 800; letter-spacing: 1.4px; cursor: pointer; }
  button.active { color: var(--accent-cyan); background: rgba(0,212,255,0.1); }
  .table { overflow-x: auto; border: 1px solid var(--border-color); border-radius: 8px; background: var(--card-bg); }
  .table-head, .row { min-width: 1320px; display: grid; grid-template-columns: 70px 1.6fr 80px 170px 120px 150px 100px 90px 130px 90px 100px; gap: 12px; align-items: center; padding: 10px 14px; }
  .table-head { color: var(--text-secondary); font-size: 0.6rem; letter-spacing: 2px; text-transform: uppercase; border-bottom: 1px solid var(--border-color); }
  .row { min-height: 48px; border-bottom: 1px solid rgba(255,255,255,0.04); font-size: 0.78rem; }
  .row:hover { background: rgba(0,212,255,0.04); }
  .row.stopped { opacity: 0.58; }
  .name { color: var(--text-primary); font-weight: 800; overflow-wrap: anywhere; }
  .mono { font-family: 'Courier New', monospace; color: var(--text-secondary); }
  small { color: var(--text-dim); margin-left: 4px; }
  b { color: var(--accent-cyan); font-size: 0.65rem; background: rgba(0,212,255,0.1); padding: 2px 6px; border-radius: 4px; }
  .ok { color: var(--accent-green); }
  .bad { color: var(--accent-red); }
  .service-row, .hint-row { min-width: 1320px; padding: 8px 14px 10px 84px; border-bottom: 1px solid rgba(255,255,255,0.04); }
  .service-row { display: flex; flex-wrap: wrap; gap: 6px; }
  .service-chip { font-size: 0.65rem; color: var(--accent-green); border: 1px solid rgba(0,255,136,0.18); background: rgba(0,255,136,0.08); border-radius: 4px; padding: 3px 7px; }
  .service-chip.down { color: var(--accent-red); border-color: rgba(255,51,85,0.22); background: rgba(255,51,85,0.08); }
  .hint-row { color: var(--text-secondary); font-size: 0.72rem; }
  .empty { min-height: 360px; display: grid; place-items: center; color: var(--text-secondary); letter-spacing: 2px; border: 1px solid var(--border-color); border-radius: 8px; background: var(--card-bg); }
</style>
