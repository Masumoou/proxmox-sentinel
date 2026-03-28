<script lang="ts">
  import { onMount } from 'svelte';

  // ── State ────────────────────────────────────────────────────────
  let guests = $state<any[]>([]);
  let nodes = $state<any[]>([]);
  let clusterCpu = $state(0);
  let clusterRam = $state(0);
  let clusterNet = $state('0 Mb/s');
  let clusterStorage = $state(0);
  let wsConnected = $state(false);
  let detailMap: Record<number, any> = {};
  let alerts = $state<any[]>([]);

  function formatBytes(bytes: number, decimals = 1) {
    if (!+bytes) return '0 B';
    const k = 1024;
    const dm = decimals < 0 ? 0 : decimals;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${parseFloat((bytes / Math.pow(k, i)).toFixed(dm))} ${sizes[i]}`;
  }

  function gaugeStyle(pct: number, color: string) {
    return `background: conic-gradient(${color} ${pct * 3.6}deg, rgba(255,255,255,0.04) 0deg);`;
  }

  onMount(() => {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const ws = new WebSocket(`${protocol}//${window.location.host}/ws`);
    ws.onopen = () => { wsConnected = true; };
    ws.onclose = () => { wsConnected = false; };

    ws.onmessage = (e) => {
      try {
        const p = JSON.parse(e.data);

        if (p.type === 'cluster_update') {
          nodes = p.nodes || [];
          const guestList = p.guests || [];

          if (nodes.length > 0) {
            const avgCpu = nodes.reduce((a: number, n: any) => a + n.cpu, 0) / nodes.length;
            const usedMem = nodes.reduce((a: number, n: any) => a + n.mem_used, 0);
            const totalMem = nodes.reduce((a: number, n: any) => a + n.mem_total, 0);
            clusterCpu = Math.round(avgCpu * 100);
            clusterRam = totalMem > 0 ? Math.round((usedMem / totalMem) * 100) : 0;
          }

          guests = guestList.map((g: any) => {
            const detail = detailMap[g.vmid] || {};
            return {
              id: g.vmid,
              name: g.name,
              node: g.node,
              type: g.type === 'lxc' ? 'LXC' : 'VM',
              role: getRoleLabel(g.name),
              os: g.type === 'lxc' ? 'Debian' : 'Ubuntu',
              status: g.status,
              cpu: Math.round(g.cpu * 100),
              ram: formatBytes(g.mem),
              maxram: formatBytes(g.maxmem),
              bandwidth_up: '↑ 12Mb/s',
              bandwidth_val: formatBytes(g.mem > 0 ? g.mem * 0.01 : 0),
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

  function getRoleLabel(name: string): string {
    const n = name.toLowerCase();
    if (n.includes('web') || n.includes('nginx') || n.includes('apache')) return 'WEB-SERVER';
    if (n.includes('db') || n.includes('sql') || n.includes('maria') || n.includes('postgres')) return 'DATABASE';
    if (n.includes('app')) return 'APP-SERVER';
    if (n.includes('mail')) return 'MAIL-SERVER';
    if (n.includes('dns')) return 'DNS-SERVER';
    return 'SERVER';
  }

  function getServiceColor(name: string): string {
    const n = name.toLowerCase();
    if (n.includes('nginx')) return '#00ff88';
    if (n.includes('redis')) return '#ff4d4d';
    if (n.includes('php')) return '#7b7fb5';
    if (n.includes('postgres') || n.includes('mysql') || n.includes('maria')) return '#00b4d8';
    if (n.includes('ssh')) return '#ffd000';
    if (n.includes('docker')) return '#0db7ed';
    if (n.includes('node')) return '#68a063';
    if (n.includes('memcache')) return '#00c853';
    return '#b644e0';
  }
</script>

<div class="dashboard-page">

  <!-- ── VM / LXC Cards Grid ─────────────────────────────── -->
  <div class="guest-grid">
    {#each guests as guest}
      <div class="neon-card guest-card" class:neon-card-purple={guest.type === 'LXC'}>
        <!-- Card Header -->
        <div class="card-header">
          <div>
            <div class="card-name">{guest.name}</div>
            <div class="card-role">{guest.role}</div>
          </div>
          <div class="card-header-right">
            <span class="os-badge">{guest.os}</span>
            <span class="card-icons">⚙ ⛶</span>
          </div>
        </div>

        <!-- Gauges Row -->
        <div class="gauges-row">
          <div class="gauge-container">
            <div class="gauge" style={gaugeStyle(guest.cpu, 'var(--accent-magenta)')}>
              <div class="gauge-inner">
                <span class="gauge-value">{guest.cpu}%</span>
              </div>
            </div>
            <div class="gauge-sub-label">CPU</div>
          </div>

          <div class="gauge-container">
            <div class="gauge" style={gaugeStyle(65, 'var(--accent-cyan)')}>
              <div class="gauge-inner">
                <span class="gauge-value" style="font-size: 0.7rem;">{guest.ram}</span>
              </div>
            </div>
            <div class="gauge-sub-label">RAM</div>
          </div>

          <div class="gauge-container">
            <div class="bw-block">
              <div class="bw-label">Bandwidth</div>
              <div class="bw-value">{guest.bandwidth_up}</div>
              <div class="bw-sub">{guest.bandwidth_val}</div>
            </div>
          </div>
        </div>

        <!-- Services List -->
        {#if guest.services.length > 0}
          <div class="services-section">
            <div class="services-title">Services</div>
            {#each guest.services as svc}
              <div class="service-row">
                <div class="service-left">
                  <span class="svc-dot" style="background: {getServiceColor(svc.name)}; box-shadow: 0 0 6px {getServiceColor(svc.name)};"></span>
                  <span class="svc-name">{svc.name.toUpperCase()}</span>
                </div>
                {#if svc.status === 'running'}
                  <span class="badge-active">ACTIVE</span>
                {:else}
                  <span class="badge-inactive">DOWN</span>
                {/if}
              </div>
            {/each}
          </div>
        {:else}
          <div class="services-section">
            <div class="services-title">Services</div>
            <div class="no-services">No services detected</div>
          </div>
        {/if}
      </div>
    {/each}

    <!-- Empty State -->
    {#if guests.length === 0}
      <div class="neon-card empty-card">
        <div class="empty-state">
          <div class="pulse-ring"></div>
          <p>{wsConnected ? 'WAITING FOR GUEST DATA...' : 'CONNECTING TO SENTINEL...'}</p>
        </div>
      </div>
    {/if}
  </div>

  <!-- ── Bottom Bar: Node Overview + Alerts ──────────────── -->
  <div class="bottom-bar">
    <div class="neon-card node-overview">
      <div class="overview-title">NODE OVERVIEW</div>
      <div class="overview-gauges">
        <div class="overview-gauge-block">
          <div class="gauge gauge-lg" style={gaugeStyle(clusterCpu, 'var(--accent-magenta)')}>
            <div class="gauge-inner gauge-inner-lg">
              <span class="gauge-value">{clusterCpu}%</span>
            </div>
          </div>
          <div class="overview-gauge-label">CPU</div>
          <div class="sparkline">
            <svg viewBox="0 0 80 20" style="width: 80px; height: 20px;">
              <polyline fill="none" stroke="var(--accent-magenta)" stroke-width="1.5"
                points="0,15 10,12 20,14 30,8 40,10 50,6 60,9 70,7 80,10" />
            </svg>
          </div>
        </div>

        <div class="overview-gauge-block">
          <div class="gauge gauge-lg" style={gaugeStyle(clusterRam, 'var(--accent-cyan)')}>
            <div class="gauge-inner gauge-inner-lg">
              <span class="gauge-value">{clusterRam}%</span>
            </div>
          </div>
          <div class="overview-gauge-label">RAM</div>
          <div class="sparkline">
            <svg viewBox="0 0 80 20" style="width: 80px; height: 20px;">
              <polyline fill="none" stroke="var(--accent-cyan)" stroke-width="1.5"
                points="0,10 10,12 20,8 30,14 40,10 50,12 60,8 70,10 80,9" />
            </svg>
          </div>
        </div>

        <div class="overview-gauge-block">
          <div class="gauge gauge-lg" style={gaugeStyle(35, 'var(--accent-green)')}>
            <div class="gauge-inner gauge-inner-lg">
              <span class="gauge-value" style="font-size: 0.7rem;">24Gbps</span>
            </div>
          </div>
          <div class="overview-gauge-label">NET</div>
          <div class="sparkline">
            <svg viewBox="0 0 80 20" style="width: 80px; height: 20px;">
              <polyline fill="none" stroke="var(--accent-green)" stroke-width="1.5"
                points="0,18 10,10 20,14 30,6 40,12 50,4 60,8 70,6 80,10" />
            </svg>
          </div>
        </div>

        <div class="overview-gauge-block">
          <div class="gauge gauge-lg" style={gaugeStyle(72, 'var(--accent-purple)')}>
            <div class="gauge-inner gauge-inner-lg">
              <span class="gauge-value">72%</span>
            </div>
          </div>
          <div class="overview-gauge-label">STORAGE</div>
          <div class="sparkline">
            <svg viewBox="0 0 80 20" style="width: 80px; height: 20px;">
              <polyline fill="none" stroke="var(--accent-purple)" stroke-width="1.5"
                points="0,8 10,10 20,9 30,11 40,10 50,12 60,11 70,12 80,11" />
            </svg>
          </div>
        </div>
      </div>
    </div>

    <!-- Recent Alerts -->
    <div class="neon-card alerts-panel" style="border-color: rgba(255, 140, 0, 0.3);">
      <div class="alerts-title">RECENT ALERTS</div>
      {#if alerts.length === 0}
        <div class="alert-item">
          <span class="alert-icon">⚠</span>
          <div class="alert-content">
            <div class="alert-name">Service Card <span class="alert-time-badge">18:00</span></div>
            <div class="alert-desc">Service Card warnings info.</div>
          </div>
        </div>
        <div class="alert-item">
          <span class="alert-icon" style="color: var(--accent-red);">⚠</span>
          <div class="alert-content">
            <div class="alert-name">Service Card <span class="alert-time-badge" style="background: rgba(255,51,85,0.2); color: var(--accent-red);">NOW</span></div>
            <div class="alert-desc">Service Card warnings info.</div>
          </div>
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .dashboard-page {
    display: flex;
    flex-direction: column;
    gap: 16px;
    height: 100%;
  }

  /* ── Guest Grid ────────────────────────────────────────── */
  .guest-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: 14px;
    flex: 1;
    align-content: start;
  }

  .guest-card {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .empty-card {
    grid-column: 1 / -1;
    min-height: 300px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  /* ── Card Header ───────────────────────────────────────── */
  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
  }

  .card-name {
    font-size: 1.05rem;
    font-weight: 700;
    letter-spacing: 1px;
    color: var(--text-primary);
  }

  .card-role {
    font-size: 0.6rem;
    color: var(--accent-cyan);
    letter-spacing: 2px;
    margin-top: 2px;
  }

  .card-header-right {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .os-badge {
    font-size: 0.58rem;
    padding: 2px 8px;
    border-radius: 3px;
    background: rgba(182, 68, 224, 0.12);
    color: var(--accent-purple);
    border: 1px solid rgba(182, 68, 224, 0.2);
    letter-spacing: 1px;
    font-weight: 600;
  }

  .card-icons {
    font-size: 0.7rem;
    color: var(--text-dim);
    cursor: pointer;
  }

  /* ── Gauges Row ────────────────────────────────────────── */
  .gauges-row {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 8px 0;
    border-bottom: 1px solid rgba(255,255,255,0.04);
  }

  .gauge-container {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
  }

  .gauge-sub-label {
    font-size: 0.55rem;
    color: var(--text-secondary);
    letter-spacing: 2px;
    text-transform: uppercase;
  }

  .bw-block {
    text-align: center;
  }

  .bw-label {
    font-size: 0.55rem;
    color: var(--text-secondary);
    letter-spacing: 1px;
    margin-bottom: 4px;
  }

  .bw-value {
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--accent-cyan);
  }

  .bw-sub {
    font-size: 0.65rem;
    color: var(--text-dim);
  }

  /* ── Services Section ──────────────────────────────────── */
  .services-section {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .services-title {
    font-size: 0.6rem;
    color: var(--text-secondary);
    letter-spacing: 2px;
    text-transform: uppercase;
    margin-bottom: 2px;
  }

  .service-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 3px 0;
  }

  .service-left {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .svc-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }

  .svc-name {
    font-size: 0.72rem;
    font-weight: 600;
    letter-spacing: 1px;
    color: var(--text-primary);
  }

  .no-services {
    font-size: 0.7rem;
    color: var(--text-dim);
    font-style: italic;
  }

  /* ── Bottom Bar ────────────────────────────────────────── */
  .bottom-bar {
    display: grid;
    grid-template-columns: 1fr 280px;
    gap: 14px;
    flex-shrink: 0;
  }

  .node-overview {
    padding: 16px 24px;
  }

  .overview-title {
    font-size: 0.65rem;
    color: var(--text-secondary);
    letter-spacing: 3px;
    margin-bottom: 12px;
    font-weight: 600;
  }

  .overview-gauges {
    display: flex;
    justify-content: space-around;
    align-items: center;
  }

  .overview-gauge-block {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
  }

  .gauge-lg {
    width: 80px;
    height: 80px;
  }

  .gauge-inner-lg {
    width: 60px;
    height: 60px;
  }

  .overview-gauge-label {
    font-size: 0.6rem;
    color: var(--text-secondary);
    letter-spacing: 2px;
    font-weight: 600;
  }

  .sparkline {
    opacity: 0.7;
  }

  /* ── Alerts Panel ──────────────────────────────────────── */
  .alerts-panel {
    padding: 14px;
    background: rgba(255, 140, 0, 0.04);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .alerts-title {
    font-size: 0.65rem;
    color: var(--accent-orange);
    letter-spacing: 3px;
    font-weight: 700;
  }

  .alert-item {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 8px 0;
    border-bottom: 1px solid rgba(255,140,0,0.1);
  }

  .alert-icon {
    font-size: 1rem;
    color: var(--accent-orange);
  }

  .alert-content {
    flex: 1;
  }

  .alert-name {
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--text-primary);
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .alert-time-badge {
    font-size: 0.55rem;
    padding: 1px 6px;
    border-radius: 3px;
    background: rgba(255, 140, 0, 0.2);
    color: var(--accent-orange);
    font-weight: 700;
    letter-spacing: 1px;
  }

  .alert-desc {
    font-size: 0.65rem;
    color: var(--text-secondary);
    margin-top: 2px;
  }

  /* ── Empty State ───────────────────────────────────────── */
  .empty-state {
    text-align: center;
    color: var(--text-secondary);
    letter-spacing: 2px;
    font-size: 0.8rem;
  }

  .pulse-ring {
    width: 40px;
    height: 40px;
    border: 2px solid var(--accent-cyan);
    border-radius: 50%;
    margin: 0 auto 20px;
    animation: pulse-ring 2s ease-in-out infinite;
  }

  @keyframes pulse-ring {
    0% { transform: scale(0.8); opacity: 0.3; border-color: var(--accent-cyan); }
    50% { transform: scale(1.1); opacity: 1; border-color: var(--accent-purple); }
    100% { transform: scale(0.8); opacity: 0.3; border-color: var(--accent-cyan); }
  }
</style>
