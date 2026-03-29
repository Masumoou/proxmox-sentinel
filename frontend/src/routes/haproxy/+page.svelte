<script lang="ts">
  import { onMount } from 'svelte';

  let stats = $state<any>(null);
  let wsConnected = $state(false);

  onMount(() => {
    let ws: WebSocket;
    let reconnectTimer: any;
    let reconnectAttempts = 0;
    
    function connect() {
      const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
      ws = new WebSocket(`${protocol}//${window.location.host}/ws`);
      
      ws.onopen = () => { 
        wsConnected = true; 
        reconnectAttempts = 0;
      };
      
      ws.onclose = () => { 
        wsConnected = false;
        reconnectAttempts++;
        let backoff = Math.min(3 * Math.pow(2, reconnectAttempts - 1), 30);
        clearTimeout(reconnectTimer);
        reconnectTimer = setTimeout(connect, backoff * 1000);
      };

      ws.onmessage = (e) => {
        try {
          const p = JSON.parse(e.data);
          if (p.type === 'haproxy_update') {
            stats = p;
          }
        } catch {}
      };
    }
    
    connect();

    return () => { clearTimeout(reconnectTimer); if(ws) ws.close(); };
  });

  function formatBytes(bytes: number) {
    if (!+bytes) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
  }
</script>

<div class="page">
  <h2 class="page-title">HAPROXY LOAD BALANCER</h2>

  {#if !wsConnected && !stats}
    <div class="neon-card empty"><div class="pulse-ring"></div><p>CONNECTING...</p></div>
  {:else if stats === null || (!stats.proxies && stats.total_servers === undefined)}
    <div class="neon-card empty"><p>HAPROXY NOT CONFIGURED OR NO DATA AVAILABLE</p></div>
  {:else}
    <div class="summary-cards">
      <div class="neon-card card-stat">
        <div class="label">SERVERS UP</div>
        <div class="val text-green">{stats.servers_up}</div>
      </div>
      <div class="neon-card card-stat">
        <div class="label">SERVERS DOWN</div>
        <div class="val text-red">{stats.servers_down}</div>
      </div>
      <div class="neon-card card-stat">
        <div class="label">TOTAL SERVERS</div>
        <div class="val text-cyan">{stats.total_servers}</div>
      </div>
    </div>

    <div class="grid">
      {#each stats.proxies as proxy}
        <div class="neon-card card">
          <div class="card-head">
            <div><div class="proxy-name">{proxy.name}</div></div>
            <span class="status-badge" class:up={proxy.frontend_status === 'OPEN'} class:down={proxy.frontend_status !== 'OPEN'}>● {proxy.frontend_status}</span>
          </div>
          <div class="servers">
            {#each proxy.servers as server}
              <div class="server-row">
                <span class="svc-dot" class:dot-up={server.status.startsWith('UP')} class:dot-down={!server.status.startsWith('UP')}></span>
                <span class="svc-name">{server.name}</span>
                <span class="metrics-sm">
                  <span class="metrics-val">Sess: {server.sessions}</span>
                  <span class="metrics-val">Err: {server.http_5xx}</span>
                  <span class="metrics-val">In: {formatBytes(server.bytes_in)}</span>
                  <span class="metrics-val">Out: {formatBytes(server.bytes_out)}</span>
                </span>
                <span class={server.status.startsWith('UP') ? 'badge-active' : 'badge-inactive'}>{server.status}</span>
              </div>
            {/each}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .page { display: flex; flex-direction: column; gap: 20px; }
  .page-title { font-size: 0.85rem; letter-spacing: 3px; color: var(--text-secondary); font-weight: 600; }
  .summary-cards { display: flex; gap: 20px; flex-wrap: wrap; }
  .card-stat { flex: 1; padding: 20px; min-width: 200px; text-align: center; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(400px, 1fr)); gap: 14px; }
  .card { padding: 18px; display: flex; flex-direction: column; gap: 12px; }
  .card-head { display: flex; justify-content: space-between; align-items: flex-start; margin-bottom: 10px; border-bottom: 1px solid rgba(255,255,255,0.05); padding-bottom: 10px; }
  .proxy-name { font-size: 1.1rem; font-weight: 700; letter-spacing: 1px; color: var(--accent-magenta); }
  .status-badge { font-size: 0.65rem; font-weight: 600; letter-spacing: 1px; }
  .up { color: var(--accent-green); }
  .down { color: var(--accent-red); }
  .label { font-size: 0.6rem; color: var(--text-secondary); letter-spacing: 2px; margin-bottom: 8px; }
  .val { font-size: 2rem; font-weight: 700; }
  .text-green { color: var(--accent-green); }
  .text-red { color: var(--accent-red); }
  .text-cyan { color: var(--accent-cyan); }

  .servers { display: flex; flex-direction: column; gap: 6px; }
  .server-row { display: flex; align-items: center; gap: 8px; padding: 6px 0; border-bottom: 1px dashed rgba(255,255,255,0.05); }
  .server-row:last-child { border-bottom: none; }
  .svc-dot { width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0; }
  .dot-up { background: var(--accent-green); }
  .dot-down { background: var(--accent-red); }
  .svc-name { font-size: 0.8rem; font-weight: 600; letter-spacing: 0.5px; flex: 1; color: var(--text-primary); }
  
  .metrics-sm { display: flex; gap: 10px; font-size: 0.65rem; color: var(--text-secondary); letter-spacing: 0.5px; }
  .metrics-val { background: rgba(255,255,255,0.05); padding: 2px 6px; border-radius: 4px; }
  
  .empty { padding: 60px; text-align: center; color: var(--text-secondary); letter-spacing: 2px; font-size: 0.85rem; }
  .pulse-ring { width: 40px; height: 40px; border: 2px solid var(--accent-cyan); border-radius: 50%; margin: 0 auto 20px; animation: pr 2s ease-in-out infinite; }
  @keyframes pr { 0%,100% { transform: scale(0.8); opacity: 0.3; } 50% { transform: scale(1.1); opacity: 1; } }
</style>
