<script lang="ts">
  import {
    enrichedGuests,
    diskSummaryLabel,
    formatBytes,
    isApplicationService,
    isServiceFailed,
    isServiceRunning,
    pct,
    previewServices,
    serviceClassification,
    serviceDisplayStatus,
    sortedServices,
    wsConnected,
    type ServiceData,
  } from '$lib/store';

  let filter = $state('all');
  let query = $state('');
  let selectedVmid = $state<number | null>(null);
  let serviceFilter = $state('all');
  let serviceQuery = $state('');

  let filteredGuests = $derived(
    $enrichedGuests.filter((guest) => {
      const matchesFilter =
        filter === 'all' ||
        (filter === 'running' && guest.status === 'running') ||
        (filter === 'stopped' && guest.status !== 'running') ||
        (filter === 'lxc' && guest.type === 'LXC') ||
        (filter === 'qemu' && guest.type === 'QEMU');
      const serviceText = (guest.services || []).map((service: ServiceData) => service.name).join(' ');
      const q = query.toLowerCase();
      const matchesQuery =
        !q ||
        `${guest.vmid} ${guest.name} ${guest.node} ${guest.ip || ''} ${guest.os_name || ''} ${guest.os_version || ''} ${serviceText}`
          .toLowerCase()
          .includes(q);
      return matchesFilter && matchesQuery;
    })
  );

  let selectedGuest = $derived($enrichedGuests.find((guest) => guest.vmid === selectedVmid) || null);
  let selectedServices = $derived(
    filterServiceRows(sortedServices(selectedGuest?.services || []), serviceFilter, serviceQuery)
  );

  function filterServiceRows(services: ServiceData[], activeFilter: string, q: string) {
    const needle = q.trim().toLowerCase();
    return services.filter((service) => {
      const matchesFilter =
        activeFilter === 'all' ||
        (activeFilter === 'running' && isServiceRunning(service)) ||
        (activeFilter === 'failed' && isServiceFailed(service)) ||
        (activeFilter === 'application' && isApplicationService(service)) ||
        (activeFilter === 'system' && serviceClassification(service) === 'system') ||
        (activeFilter === 'inactive' && !isServiceRunning(service) && !isServiceFailed(service));
      const matchesQuery =
        !needle ||
        `${service.name} ${service.description || ''} ${serviceClassification(service)} ${(service.ports || []).join(' ')}`
          .toLowerCase()
          .includes(needle);
      return matchesFilter && matchesQuery;
    });
  }

  function serviceCount(guest: any) {
    const running = guest.service_running ?? guest.services.filter(isServiceRunning).length;
    const total = guest.service_total ?? guest.services.length;
    const failed = guest.service_failed ?? guest.services.filter(isServiceFailed).length;
    return failed > 0 ? `${running}/${total} · ${failed} failed` : `${running}/${total}`;
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

  function portsLabel(service: ServiceData) {
    return (service.ports || []).length ? (service.ports || []).join(', ') : '-';
  }

  function toggleServices(vmid: number) {
    selectedVmid = selectedVmid === vmid ? null : vmid;
    serviceFilter = 'all';
    serviceQuery = '';
  }
</script>

<div class="guests-page">
  <div class="page-header">
    <div>
      <h2 class="page-title">GUEST MONITOR</h2>
      <p class="page-subtitle">VMs and containers with full service inventory when guest visibility is available</p>
    </div>
    <div class="controls">
      <input bind:value={query} placeholder="Search ID, name, node, IP, OS, service" />
      <div class="filter-bar">
        {#each ['all', 'running', 'stopped', 'lxc', 'qemu'] as f}
          <button class:active={filter === f} onclick={() => (filter = f)}>{f.toUpperCase()}</button>
        {/each}
      </div>
    </div>
  </div>

  {#if filteredGuests.length === 0}
    <div class="empty">{$wsConnected ? 'NO GUESTS MATCH FILTER' : 'CONNECTING...'}</div>
  {:else}
    <div class="table">
      <div class="table-head">
        <span>ID</span><span>Name</span><span>Type</span><span>OS</span><span>Node</span><span>IP</span><span>Status</span><span>CPU</span><span>RAM</span><span>Storage</span><span>Services</span><span>Visibility</span><span>Action</span>
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
          <span class:bad={(guest.disk_summary?.max_mount_pct || 0) > 90}>{guest.disk_summary?.available ? diskSummaryLabel(guest.disk_summary) : 'unavailable'}</span>
          <span class:bad={(guest.service_failed || 0) > 0}>{serviceCount(guest)}</span>
          <span class:bad={visibility(guest) === 'none'}>{visibility(guest)}</span>
          <span>
            <button class="link-button" disabled={guest.services.length === 0} onclick={() => toggleServices(guest.vmid)}>
              {selectedVmid === guest.vmid ? 'Hide' : 'View all'}
            </button>
          </span>
        </div>
        {#if guest.services.length > 0}
          <div class="service-row">
            {#each previewServices(guest.services, 18) as service}
              <span
                class="service-chip"
                class:down={isServiceFailed(service)}
                class:muted={!isServiceRunning(service) && !isServiceFailed(service)}
                title={`${service.name} · ${serviceDisplayStatus(service)} · ${serviceClassification(service)}`}
              >
                {service.name}{(service.ports || []).length ? ` :${(service.ports || []).join(',')}` : ''}
              </span>
            {/each}
          </div>
        {:else if guest.type === 'QEMU' && guest.status === 'running'}
          <div class="hint-row">Install/enable QEMU Guest Agent or configure SSH to show services, disks, and reliable IPs for this VM.</div>
        {/if}

        {#if selectedGuest?.vmid === guest.vmid}
          <div class="service-detail">
            <div class="service-toolbar">
              <div>
                <strong>{selectedGuest.name}</strong>
                <span>{selectedGuest.service_running || 0} running / {selectedGuest.service_total || selectedGuest.services.length} discovered · {selectedGuest.service_failed || 0} failed</span>
              </div>
              <input bind:value={serviceQuery} placeholder="Filter service, description, port" />
              <div class="filter-bar">
                {#each ['all', 'running', 'failed', 'application', 'system', 'inactive'] as f}
                  <button class:active={serviceFilter === f} onclick={() => (serviceFilter = f)}>{f.toUpperCase()}</button>
                {/each}
              </div>
            </div>
            <div class="service-table">
              <div class="service-head">
                <span>Name</span><span>Load</span><span>Active</span><span>Sub</span><span>Class</span><span>Ports</span><span>Description</span>
              </div>
              {#each selectedServices as service (service.name)}
                <div class="service-line" class:failed={isServiceFailed(service)}>
                  <span class="name">{service.name}</span>
                  <span>{service.load || '-'}</span>
                  <span class:ok={isServiceRunning(service)} class:bad={isServiceFailed(service)}>{service.state || service.status || '-'}</span>
                  <span>{service.sub_state || service.status || '-'}</span>
                  <span class="class-pill">{serviceClassification(service)}</span>
                  <span class="mono">{portsLabel(service)}</span>
                  <span>{service.description || '-'}</span>
                </div>
              {:else}
                <div class="service-empty">No services match this filter.</div>
              {/each}
            </div>
          </div>
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
  input { width: 280px; background: var(--panel-bg); border: 1px solid var(--border-color); color: var(--text-primary); border-radius: 6px; padding: 8px 10px; }
  .filter-bar { display: flex; gap: 6px; flex-wrap: wrap; }
  button { padding: 7px 12px; border-radius: 6px; border: 1px solid var(--border-color); background: transparent; color: var(--text-secondary); font-size: 0.62rem; font-weight: 800; letter-spacing: 1.4px; cursor: pointer; }
  button.active, button:hover:not(:disabled) { color: var(--accent-cyan); background: rgba(0,212,255,0.1); }
  button:disabled { opacity: 0.45; cursor: not-allowed; }
  .link-button { min-width: 76px; padding: 6px 8px; }
  .table { overflow-x: auto; border: 1px solid var(--border-color); border-radius: 8px; background: var(--card-bg); }
  .table-head, .row { min-width: 1580px; display: grid; grid-template-columns: 70px 1.5fr 80px 180px 120px 150px 100px 90px 130px 180px 130px 100px 90px; gap: 12px; align-items: center; padding: 10px 14px; }
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
  .service-row, .hint-row, .service-detail { min-width: 1580px; padding: 8px 14px 10px 84px; border-bottom: 1px solid rgba(255,255,255,0.04); }
  .service-row { display: flex; flex-wrap: wrap; gap: 6px; }
  .service-chip { font-size: 0.65rem; color: var(--accent-green); border: 1px solid rgba(0,255,136,0.18); background: rgba(0,255,136,0.08); border-radius: 4px; padding: 3px 7px; }
  .service-chip.down { color: var(--accent-red); border-color: rgba(255,51,85,0.22); background: rgba(255,51,85,0.08); }
  .service-chip.muted { color: var(--text-secondary); border-color: rgba(255,255,255,0.08); background: rgba(255,255,255,0.03); }
  .hint-row { color: var(--text-secondary); font-size: 0.72rem; }
  .service-detail { background: rgba(0,212,255,0.035); }
  .service-toolbar { display: flex; align-items: center; justify-content: space-between; gap: 12px; flex-wrap: wrap; margin-bottom: 10px; }
  .service-toolbar strong { display: block; }
  .service-toolbar span { color: var(--text-secondary); font-size: 0.7rem; }
  .service-table { border: 1px solid rgba(255,255,255,0.06); border-radius: 6px; overflow: hidden; }
  .service-head, .service-line { display: grid; grid-template-columns: 220px 90px 90px 100px 100px 120px minmax(280px, 1fr); gap: 10px; padding: 8px 10px; align-items: center; }
  .service-head { color: var(--text-secondary); background: rgba(255,255,255,0.025); font-size: 0.58rem; letter-spacing: 1.6px; text-transform: uppercase; }
  .service-line { min-height: 38px; color: var(--text-secondary); border-top: 1px solid rgba(255,255,255,0.035); font-size: 0.7rem; }
  .service-line.failed { background: rgba(255,51,85,0.05); }
  .class-pill { width: fit-content; color: var(--accent-cyan); border: 1px solid rgba(0,212,255,0.16); border-radius: 4px; padding: 2px 6px; }
  .service-empty { padding: 18px; color: var(--text-secondary); }
  .empty { min-height: 360px; display: grid; place-items: center; color: var(--text-secondary); letter-spacing: 2px; border: 1px solid var(--border-color); border-radius: 8px; background: var(--card-bg); }
</style>
