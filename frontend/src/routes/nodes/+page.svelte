<script lang="ts">
  import { formatBytes, nodes, pct, storagePools, wsConnected } from '$lib/store';
  import NodeSparkline from './NodeSparkline.svelte';

  function nodeStorage(node: string) {
    return $storagePools.filter((pool) => pool.node === node);
  }
</script>

<div class="nodes-page">
  <h2 class="page-title">CLUSTER NODES</h2>

  {#if $nodes.length === 0}
    <div class="empty">{$wsConnected ? 'WAITING FOR NODE DATA...' : 'CONNECTING...'}</div>
  {:else}
    <div class="node-grid">
      {#each $nodes as node (node.node)}
        <article class="node-card">
          <div class="node-header">
            <div>
              <h3>{node.node}</h3>
              <span class="status" class:online={node.status === 'online'}>● {(node.status || 'online').toUpperCase()}</span>
            </div>
            <div class="node-ip">PVE</div>
          </div>

          <div class="metrics">
            <div class="metric-row"><span>CPU</span><strong>{(node.cpu * 100).toFixed(1)}%</strong></div>
            <div class="bar"><div style="width:{Math.round(node.cpu * 100)}%"></div></div>

            <div class="metric-row"><span>MEMORY</span><strong>{Math.round(pct(node.mem_used, node.mem_total))}%</strong></div>
            <div class="bar pink"><div style="width:{Math.round(pct(node.mem_used, node.mem_total))}%"></div></div>
            <small>{formatBytes(node.mem_used)} / {formatBytes(node.mem_total)}</small>
          </div>

          <div class="storage-list">
            <div class="section-label">Storage Pools</div>
            {#each nodeStorage(node.node).slice(0, 4) as pool}
              <div class="pool-row">
                <span>{pool.storage}</span>
                <b>{Math.round(pct(pool.used, pool.total))}%</b>
              </div>
            {:else}
              <div class="muted">No storage data yet</div>
            {/each}
          </div>

          <div class="sparkline">
            <div class="section-label">24H Performance</div>
            <NodeSparkline nodeName={node.node} />
          </div>
        </article>
      {/each}
    </div>
  {/if}
</div>

<style>
  .nodes-page { display: flex; flex-direction: column; gap: 20px; }
  .page-title { font-size: 0.85rem; letter-spacing: 3px; color: var(--text-secondary); font-weight: 800; }
  .node-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(340px, 1fr)); gap: 14px; align-items: stretch; }
  .node-card { min-height: 430px; background: var(--card-bg); border: 1px solid var(--border-color); border-left: 3px solid var(--accent-cyan); border-radius: 8px; padding: 18px; display: flex; flex-direction: column; gap: 16px; }
  .node-header { display: flex; justify-content: space-between; gap: 16px; align-items: flex-start; min-height: 58px; }
  h3 { font-size: 1.2rem; letter-spacing: 1px; }
  .status { display: block; margin-top: 6px; color: var(--accent-red); font-size: 0.65rem; font-weight: 800; letter-spacing: 1.4px; }
  .status.online { color: var(--accent-green); }
  .node-ip { color: var(--accent-cyan); border: 1px solid rgba(0,212,255,0.2); border-radius: 4px; padding: 3px 8px; font-size: 0.6rem; font-weight: 800; }
  .metrics { display: flex; flex-direction: column; gap: 7px; }
  .metric-row, .pool-row { display: flex; justify-content: space-between; align-items: center; gap: 12px; }
  .metric-row span, .section-label { color: var(--text-secondary); font-size: 0.62rem; letter-spacing: 2px; text-transform: uppercase; }
  .metric-row strong { color: var(--accent-cyan); font-size: 1rem; }
  small, .muted { color: var(--text-dim); text-align: right; font-size: 0.68rem; }
  .bar { height: 6px; background: rgba(255,255,255,0.06); border-radius: 999px; overflow: hidden; }
  .bar div { height: 100%; background: var(--accent-cyan); border-radius: inherit; }
  .bar.pink div { background: var(--accent-pink); }
  .storage-list { display: flex; flex-direction: column; gap: 8px; min-height: 116px; }
  .pool-row { border: 1px solid rgba(255,255,255,0.06); border-radius: 5px; padding: 7px 9px; color: var(--text-primary); font-size: 0.72rem; }
  .pool-row b { color: var(--accent-green); }
  .sparkline { margin-top: auto; min-height: 145px; border-top: 1px solid rgba(255,255,255,0.05); padding-top: 12px; }
  .empty { min-height: 360px; display: grid; place-items: center; color: var(--text-secondary); background: var(--card-bg); border: 1px solid var(--border-color); border-radius: 8px; letter-spacing: 2px; }
</style>
