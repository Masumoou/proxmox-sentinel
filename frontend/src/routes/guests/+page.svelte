<script lang="ts">
  import { onMount } from 'svelte';

  let guests = $state<any[]>([]);
  let detailMap: Record<number, any> = {};
  let wsConnected = $state(false);
  let filter = $state('all'); // all | running | stopped | lxc | qemu

  function formatBytes(bytes: number, decimals = 1) {
    if (!+bytes) return '0 B';
    const k = 1024;
    const dm = decimals < 0 ? 0 : decimals;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${parseFloat((bytes / Math.pow(k, i)).toFixed(dm))} ${sizes[i]}`;
  }

  let filteredGuests = $derived(
    guests.filter(g => {
      if (filter === 'running') return g.status === 'running';
      if (filter === 'stopped') return g.status !== 'running';
      if (filter === 'lxc') return g.type === 'LXC';
      if (filter === 'qemu') return g.type === 'QEMU';
      return true;
    })
  );

  onMount(() => {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const ws = new WebSocket(`${protocol}//${window.location.host}/ws`);
    ws.onopen = () => { wsConnected = true; };
    ws.onclose = () => { wsConnected = false; };

    ws.onmessage = (e) => {
      try {
        const p = JSON.parse(e.data);

        if (p.type === 'cluster_update') {
          const guestList = p.guests || [];
          guests = guestList.map((g: any) => {
            const detail = detailMap[g.vmid] || {};
            return {
              id: g.vmid, name: g.name, node: g.node,
              type: g.type === 'lxc' ? 'LXC' : 'QEMU',
              status: g.status,
              cpu: (g.cpu * 100).toFixed(1),
              ram: formatBytes(g.mem),
              maxram: formatBytes(g.maxmem),
              services: detail.services || [],
              disk_mounts: detail.disk_mounts || [],
            };
          });
        }

        if (p.type === 'lxc_detail') {
          for (const lxc of p.lxc || []) {
            detailMap[lxc.vmid] = { services: lxc.services || [], disk_mounts: lxc.disk_mounts || [] };
          }
          refreshDetails();
        }

        if (p.type === 'vm_detail') {
          for (const vm of p.vms || []) {
            detailMap[vm.vmid] = { services: vm.services || [], disk_mounts: vm.disk_mounts || [], agent: vm.agent, ssh: vm.ssh, ip: vm.ip };
          }
          refreshDetails();
        }
      } catch {}
    };

    return () => ws.close();
  });

  function refreshDetails() {
    guests = guests.map(g => {
      const detail = detailMap[g.id] || {};
      return { ...g, services: detail.services || g.services, disk_mounts: detail.disk_mounts || g.disk_mounts };
    });
  }
</script>

<div class="guests-page">
  <div class="page-header">
    <h2 class="page-title">GUEST MONITOR</h2>
    <div class="filter-bar">
      {#each ['all', 'running', 'stopped', 'lxc', 'qemu'] as f}
        <button class="filter-btn" class:active={filter === f} onclick={() => filter = f}>
          {f.toUpperCase()}
        </button>
      {/each}
    </div>
  </div>

  {#if filteredGuests.length === 0}
    <div class="glass-panel empty-state">
      <div class="pulse-dot"></div>
      <p>{wsConnected ? 'NO GUESTS MATCH FILTER' : 'CONNECTING...'}</p>
    </div>
  {:else}
    <div class="guest-table">
      <div class="table-header">
        <span class="col-id">ID</span>
        <span class="col-name">NAME</span>
        <span class="col-type">TYPE</span>
        <span class="col-node">NODE</span>
        <span class="col-status">STATUS</span>
        <span class="col-cpu">CPU</span>
        <span class="col-mem">MEMORY</span>
        <span class="col-svc">SERVICES</span>
      </div>

      {#each filteredGuests as guest}
        <div class="table-row" class:row-running={guest.status === 'running'} class:row-stopped={guest.status !== 'running'}>
          <span class="col-id">{guest.id}</span>
          <span class="col-name">{guest.name}</span>
          <span class="col-type"><span class="type-badge">{guest.type}</span></span>
          <span class="col-node">{guest.node || '—'}</span>
          <span class="col-status">
            <span class="status-indicator" class:si-up={guest.status === 'running'} class:si-down={guest.status !== 'running'}>
              ● {guest.status.toUpperCase()}
            </span>
          </span>
          <span class="col-cpu text-neon-blue">{guest.cpu}%</span>
          <span class="col-mem">{guest.ram}</span>
          <span class="col-svc">
            <span style="color: var(--accent-green);">{guest.services.filter((s: any) => s.status === 'running').length}</span>
            /
            <span>{guest.services.length}</span>
          </span>
        </div>

        <!-- Expandable service row -->
        {#if guest.services.length > 0}
          <div class="service-row">
            {#each guest.services as svc}
              <div class="service-tag">
                <span class="status-dot {svc.status === 'running' ? 'status-running' : 'status-stopped'}"></span>
                {svc.name}
              </div>
            {/each}
          </div>
        {/if}
      {/each}
    </div>
  {/if}
</div>

<style>
  .guests-page { display: flex; flex-direction: column; gap: 20px; }

  .page-header { display: flex; justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 12px; }

  .page-title {
    font-size: 0.85rem; letter-spacing: 3px;
    color: var(--text-secondary); font-weight: 600;
  }

  .filter-bar { display: flex; gap: 6px; }

  .filter-btn {
    padding: 5px 14px; border-radius: 6px;
    border: 1px solid var(--border-color);
    background: transparent; color: var(--text-secondary);
    font-size: 0.65rem; font-weight: 600; letter-spacing: 1.5px;
    cursor: pointer; transition: all 0.2s;
  }

  .filter-btn:hover { background: rgba(0, 210, 255, 0.06); color: var(--text-primary); }

  .filter-btn.active {
    background: rgba(0, 210, 255, 0.12);
    color: var(--accent-blue);
    border-color: rgba(0, 210, 255, 0.3);
  }

  /* ── Table ──────────────────────────────────────────────── */
  .guest-table { display: flex; flex-direction: column; gap: 2px; }

  .table-header, .table-row {
    display: grid;
    grid-template-columns: 60px 1.5fr 70px 1fr 100px 80px 100px 80px;
    align-items: center;
    padding: 10px 16px;
    border-radius: 6px;
    font-size: 0.78rem;
  }

  .table-header {
    font-size: 0.6rem;
    color: var(--text-secondary);
    letter-spacing: 2px;
    font-weight: 600;
    padding-bottom: 8px;
    border-bottom: 1px solid var(--border-color);
  }

  .table-row {
    background: var(--panel-bg);
    border: 1px solid var(--border-color);
    transition: all 0.15s;
  }

  .table-row:hover {
    background: rgba(0, 210, 255, 0.04);
    border-color: rgba(0, 210, 255, 0.15);
  }

  .row-stopped { opacity: 0.6; }

  .type-badge {
    font-size: 0.6rem; padding: 2px 8px; border-radius: 3px;
    background: rgba(0, 210, 255, 0.12); color: var(--accent-blue);
    letter-spacing: 1px;
  }

  .status-indicator { font-size: 0.7rem; font-weight: 600; letter-spacing: 1px; }
  .si-up { color: var(--accent-green); }
  .si-down { color: var(--accent-red); }

  .service-row {
    display: flex; flex-wrap: wrap; gap: 6px;
    padding: 6px 16px 10px 76px;
    margin-top: -2px;
  }

  .empty-state {
    padding: 60px; text-align: center;
    color: var(--text-secondary); letter-spacing: 2px; font-size: 0.85rem;
  }

  .pulse-dot {
    width: 12px; height: 12px; border-radius: 50%;
    background: var(--accent-blue); margin: 0 auto 20px;
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 0.3; transform: scale(0.8); }
    50% { opacity: 1; transform: scale(1.2); }
  }
</style>
