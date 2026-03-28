<script lang="ts">
  import { onMount } from 'svelte';

  let logs = $state<any[]>([]);
  let wsConnected = $state(false);
  let autoScroll = $state(true);
  let logContainer: HTMLDivElement;

  onMount(() => {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const ws = new WebSocket(`${protocol}//${window.location.host}/ws`);
    ws.onopen = () => { wsConnected = true; };
    ws.onclose = () => { wsConnected = false; };

    ws.onmessage = (e) => {
      try {
        const p = JSON.parse(e.data);

        // Log each incoming event as a log entry
        const time = new Date().toLocaleTimeString('en-US', { hour12: false });
        
        if (p.type === 'cluster_update') {
          const nodeCount = (p.nodes || []).length;
          const guestCount = (p.guests || []).length;
          const running = (p.guests || []).filter((g: any) => g.status === 'running').length;
          addLog(time, 'INFO', 'CLUSTER', `Polled ${nodeCount} nodes, ${guestCount} guests (${running} running)`);
        }

        if (p.type === 'lxc_detail') {
          const count = (p.lxc || []).length;
          for (const lxc of (p.lxc || [])) {
            const svcCount = (lxc.services || []).length;
            const diskCount = (lxc.disk_mounts || []).length;
            addLog(time, 'INFO', `LXC-${lxc.vmid}`, `${lxc.name}: ${svcCount} services, ${diskCount} disks`);
          }
        }

        if (p.type === 'vm_detail') {
          for (const vm of (p.vms || [])) {
            const agent = vm.agent ? 'AGENT' : vm.ssh ? 'SSH' : 'NONE';
            addLog(time, 'INFO', `VM-${vm.vmid}`, `${vm.name}: via ${agent}, ${(vm.services || []).length} services`);
          }
        }
      } catch {}
    };

    return () => ws.close();
  });

  function addLog(time: string, level: string, source: string, message: string) {
    logs = [...logs.slice(-200), { time, level, source, message }];
    if (autoScroll && logContainer) {
      requestAnimationFrame(() => {
        logContainer.scrollTop = logContainer.scrollHeight;
      });
    }
  }
</script>

<div class="logs-page">
  <div class="page-header">
    <h2 class="page-title">LIVE LOG STREAM</h2>
    <div class="log-controls">
      <span class="log-count">{logs.length} ENTRIES</span>
      <button class="filter-btn" class:active={autoScroll} onclick={() => autoScroll = !autoScroll}>
        {autoScroll ? '⇩ AUTO-SCROLL ON' : '⇩ AUTO-SCROLL OFF'}
      </button>
      <button class="filter-btn" onclick={() => logs = []}>CLEAR</button>
    </div>
  </div>

  <div class="glass-panel log-panel" bind:this={logContainer}>
    {#if logs.length === 0}
      <div class="empty-log">
        <div class="pulse-dot"></div>
        <p>{wsConnected ? 'WAITING FOR EVENTS...' : 'CONNECTING...'}</p>
      </div>
    {:else}
      {#each logs as log}
        <div class="log-line">
          <span class="log-time">{log.time}</span>
          <span class="log-level" class:log-info={log.level === 'INFO'} class:log-warn={log.level === 'WARN'} class:log-error={log.level === 'ERROR'}>{log.level}</span>
          <span class="log-source">[{log.source}]</span>
          <span class="log-msg">{log.message}</span>
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .logs-page { display: flex; flex-direction: column; gap: 16px; height: 100%; }

  .page-header { display: flex; justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 12px; }

  .page-title {
    font-size: 0.85rem; letter-spacing: 3px;
    color: var(--text-secondary); font-weight: 600;
  }

  .log-controls { display: flex; gap: 8px; align-items: center; }

  .log-count {
    font-size: 0.65rem; color: var(--text-secondary);
    letter-spacing: 1.5px;
  }

  .filter-btn {
    padding: 5px 14px; border-radius: 6px;
    border: 1px solid var(--border-color);
    background: transparent; color: var(--text-secondary);
    font-size: 0.65rem; font-weight: 600; letter-spacing: 1.5px;
    cursor: pointer; transition: all 0.2s;
  }

  .filter-btn:hover { background: rgba(0, 210, 255, 0.06); color: var(--text-primary); }
  .filter-btn.active { background: rgba(0, 210, 255, 0.12); color: var(--accent-blue); border-color: rgba(0, 210, 255, 0.3); }

  /* ── Log Panel ──────────────────────────────────────────── */
  .log-panel {
    flex: 1;
    overflow-y: auto;
    padding: 16px;
    font-family: 'Courier New', 'Fira Code', monospace;
    font-size: 0.75rem;
    line-height: 1.8;
    min-height: 400px;
  }

  .log-line {
    display: flex;
    gap: 10px;
    padding: 2px 0;
    border-bottom: 1px solid rgba(255,255,255,0.02);
    transition: background 0.15s;
  }

  .log-line:hover { background: rgba(0, 210, 255, 0.03); }

  .log-time {
    color: var(--text-secondary);
    min-width: 75px;
    font-variant-numeric: tabular-nums;
  }

  .log-level {
    min-width: 45px;
    font-weight: 700;
    letter-spacing: 1px;
  }

  .log-info { color: var(--accent-blue); }
  .log-warn { color: var(--accent-yellow); }
  .log-error { color: var(--accent-red); }

  .log-source {
    color: var(--accent-purple);
    min-width: 90px;
    letter-spacing: 0.5px;
  }

  .log-msg { color: var(--text-primary); }

  .empty-log {
    display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    height: 100%; color: var(--text-secondary);
    letter-spacing: 2px; font-size: 0.85rem;
  }

  .pulse-dot {
    width: 12px; height: 12px; border-radius: 50%;
    background: var(--accent-blue); margin-bottom: 20px;
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 0.3; transform: scale(0.8); }
    50% { opacity: 1; transform: scale(1.2); }
  }
</style>
