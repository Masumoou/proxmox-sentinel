<script lang="ts">
  import { onMount, tick } from 'svelte';
  import NodeSparkline from './NodeSparkline.svelte';

  let nodes = $state<any[]>([]);
  let wsConnected = $state(false);

  function formatBytes(bytes: number, decimals = 1) {
    if (!+bytes) return '0 B';
    const k = 1024;
    const dm = decimals < 0 ? 0 : decimals;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${parseFloat((bytes / Math.pow(k, i)).toFixed(dm))} ${sizes[i]}`;
  }

  onMount(() => {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const ws = new WebSocket(`${protocol}//${window.location.host}/ws`);
    ws.onopen = () => { wsConnected = true; };
    ws.onclose = () => { wsConnected = false; };

    ws.onmessage = (e) => {
      try {
        const p = JSON.parse(e.data);
        if (p.type === 'cluster_update' && p.nodes) {
          nodes = p.nodes.map((n: any) => ({
            ...n,
            cpu_pct: (n.cpu * 100).toFixed(1),
            mem_pct: n.mem_total > 0 ? ((n.mem_used / n.mem_total) * 100).toFixed(1) : '0.0',
            mem_used_fmt: formatBytes(n.mem_used),
            mem_total_fmt: formatBytes(n.mem_total),
          }));
        }
      } catch {}
    };

    return () => ws.close();
  });
</script>

<div class="nodes-page">
  <h2 class="page-title">CLUSTER NODES</h2>

  {#if nodes.length === 0}
    <div class="glass-panel empty-state">
      <div class="pulse-dot"></div>
      <p>{wsConnected ? 'WAITING FOR NODE DATA...' : 'CONNECTING...'}</p>
    </div>
  {:else}
    <div class="node-list">
      {#each nodes as node (node.node)}
        <div class="glass-panel node-card">
          <div class="node-header">
            <div>
              <h3 class="node-name">{node.node}</h3>
              <span class="node-status-badge" class:online={node.status === 'online'}>
                ● {(node.status || 'online').toUpperCase()}
              </span>
            </div>
          </div>

          <div class="node-metrics">
            <div class="metric-row">
              <span class="metric-name">CPU</span>
              <span class="metric-val text-neon-blue">{node.cpu_pct}%</span>
            </div>
            <div class="bar"><div class="bar-fill" style="width: {node.cpu_pct}%; background: var(--accent-blue);"></div></div>

            <div class="metric-row" style="margin-top: 12px;">
              <span class="metric-name">MEMORY</span>
              <span class="metric-val text-neon-pink">{node.mem_pct}%</span>
            </div>
            <div class="bar"><div class="bar-fill" style="width: {node.mem_pct}%; background: var(--accent-pink);"></div></div>
            <div class="metric-detail">{node.mem_used_fmt} / {node.mem_total_fmt}</div>

            <!-- Sparkline Chart -->
            <div class="sparkline-container mt-6 pt-4 border-t border-white/5">
              <div class="flex items-center justify-between mb-2">
                <span class="text-[9px] font-bold text-zinc-500 tracking-widest uppercase">24H PERFORMANCE</span>
                <div class="flex gap-3">
                   <span class="flex items-center gap-1 text-[8px] font-bold text-cyan-400">
                     <span class="w-1.5 h-1.5 rounded-full bg-cyan-400"></span> CPU
                   </span>
                   <span class="flex items-center gap-1 text-[8px] font-bold text-pink-400">
                     <span class="w-1.5 h-1.5 rounded-full bg-pink-400"></span> RAM
                   </span>
                </div>
              </div>
              <NodeSparkline nodeName={node.node} />
            </div>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .nodes-page { display: flex; flex-direction: column; gap: 24px; }

  .page-title {
    font-size: 0.85rem;
    letter-spacing: 3px;
    color: var(--text-secondary);
    font-weight: 600;
  }

  .node-list {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(380px, 1fr));
    gap: 20px;
  }

  .node-card { padding: 24px; border-left: 3px solid var(--accent-blue); }
  .node-card:hover { box-shadow: 0 8px 32px rgba(0, 210, 255, 0.1); }

  .node-header { display: flex; justify-content: space-between; margin-bottom: 20px; }

  .node-name {
    font-size: 1.3rem;
    font-weight: 700;
    letter-spacing: 2px;
    margin: 0 0 6px 0;
  }

  .node-status-badge {
    font-size: 0.65rem;
    letter-spacing: 1px;
    color: var(--accent-red);
  }

  .node-status-badge.online { color: var(--accent-green); }

  .node-metrics { display: flex; flex-direction: column; gap: 4px; }

  .metric-row {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }

  .metric-name {
    font-size: 0.65rem;
    color: var(--text-secondary);
    letter-spacing: 2px;
  }

  .metric-val { font-size: 1.1rem; font-weight: 700; }

  .bar {
    height: 5px;
    background: rgba(255,255,255,0.06);
    border-radius: 3px;
    overflow: hidden;
  }

  .bar-fill {
    height: 100%;
    border-radius: 3px;
    transition: width 0.5s ease;
  }

  .metric-detail {
    font-size: 0.7rem;
    color: var(--text-secondary);
    margin-top: 4px;
    text-align: right;
  }

  .text-neon-pink { color: #ff6ec7; text-shadow: 0 0 8px rgba(255,110,199,0.4); }
  .text-neon-blue { color: #00d2ff; text-shadow: 0 0 8px rgba(0,210,255,0.4); }

  .empty-state {
    padding: 60px;
    text-align: center;
    color: var(--text-secondary);
    letter-spacing: 2px;
    font-size: 0.85rem;
  }

  .pulse-dot {
    width: 12px; height: 12px; border-radius: 50%;
    background: var(--accent-blue);
    margin: 0 auto 20px;
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 0.3; transform: scale(0.8); }
    50% { opacity: 1; transform: scale(1.2); }
  }

  .mt-6 { margin-top: 24px; }
  .pt-4 { padding-top: 16px; }
  .border-t { border-top-width: 1px; }
  .border-white\/5 { border-color: rgba(255, 255, 255, 0.05); }
  .flex { display: flex; }
  .items-center { align-items: center; }
  .justify-between { justify-content: space-between; }
  .mb-2 { margin-bottom: 8px; }
  .gap-1 { gap: 4px; }
  .gap-3 { gap: 12px; }
  .rounded-full { border-radius: 9999px; }
  .w-1\.5 { width: 6px; }
  .h-1\.5 { height: 6px; }
  .bg-cyan-400 { background-color: #22d3ee; }
  .bg-pink-400 { background-color: #f472b6; }
</style>
