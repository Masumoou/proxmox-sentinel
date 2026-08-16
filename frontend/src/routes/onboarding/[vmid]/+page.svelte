<script lang="ts">
  import { page } from '$app/stores';
  import { onMount } from 'svelte';

  let vmid = $derived($page.params.vmid);
  let vmName = $state("web-server");

  // Discovery Data Structure
  type DiscoveredResource = {
    id: string;
    identifier: string;
    kind: string;
    selected: boolean;
    status: 'pending' | 'monitored' | 'ignored';
  };

  let discoveredResources: DiscoveredResource[] = $state([]);
  let loading = $state(true);

  onMount(async () => {
    setTimeout(() => {
      discoveredResources = [
        { id: '1', identifier: 'nginx.service', kind: 'service', selected: false, status: 'pending' },
        { id: '2', identifier: 'postgresql.service', kind: 'service', selected: false, status: 'pending' },
        { id: '3', identifier: 'redis.service', kind: 'service', selected: false, status: 'pending' },
        { id: '4', identifier: 'ssh.service', kind: 'service', selected: false, status: 'pending' },
        { id: '5', identifier: 'docker.service', kind: 'service', selected: false, status: 'pending' },
        { id: '6', identifier: '/var', kind: 'filesystem', selected: false, status: 'pending' },
        { id: '7', identifier: '/etc', kind: 'filesystem', selected: false, status: 'pending' },
        { id: '8', identifier: '/', kind: 'filesystem', selected: false, status: 'pending' },
        { id: '9', identifier: 'eth0', kind: 'network', selected: false, status: 'pending' },
        { id: '10', identifier: 'eth1', kind: 'network', selected: false, status: 'pending' },
      ];
      loading = false;
    }, 800);
  });

  // Computed groupings
  let groupedResources = $derived(
    discoveredResources.reduce((acc, res) => {
      if (!acc[res.kind]) acc[res.kind] = [];
      acc[res.kind].push(res);
      return acc;
    }, {} as Record<string, DiscoveredResource[]>)
  );

  let summary = $derived(
    Object.entries(groupedResources).map(([kind, items]) => ({
      kind,
      count: items.length
    }))
  );

  let selectedCount = $derived(discoveredResources.filter(r => r.selected).length);

  function selectAll(kind: string) {
    const pending = discoveredResources.filter(r => r.kind === kind && r.status === 'pending');
    const allSelected = pending.every(r => r.selected);
    discoveredResources = discoveredResources.map(r => {
      if (r.kind === kind && r.status === 'pending') {
        return { ...r, selected: !allSelected };
      }
      return r;
    });
  }

  function applyAction(action: 'monitored' | 'ignored') {
    discoveredResources = discoveredResources.map(r => {
      if (r.selected && r.status === 'pending') {
        return { ...r, status: action, selected: false };
      }
      return r;
    });
  }

  function capitalize(s: string) {
    if (!s) return s;
    return s.charAt(0).toUpperCase() + s.slice(1);
  }
</script>

<div class="page">
  <div class="page-header">
    <div>
      <h2>VM {vmid} <span class="vm-name">{vmName}</span></h2>
      <p>Sentinel Discovery Engine has scanned this guest.</p>
    </div>
    <div class="actions">
      <a href="/nodes" class="back-link">← Back to Nodes</a>
    </div>
  </div>

  {#if loading}
    <div class="loading-state">
      <div class="pulse-ring"></div>
      <p>Interrogating QEMU Guest Agent...</p>
    </div>
  {:else}
    <!-- Discovery Summary Panel -->
    <div class="panel summary-panel">
      <div class="panel-title">Sentinel discovered:</div>
      <div class="summary-stats">
        {#each summary as { kind, count }}
          <div class="stat-box">
            <span class="stat-number">{count}</span>
            <span class="stat-label">{capitalize(kind)}s</span>
          </div>
        {/each}
      </div>
      <p class="summary-instruction">Choose what you want to monitor. Sentinel will ignore unselected resources until you say otherwise.</p>
    </div>

    <!-- Resource Selection Area -->
    <div class="discovery-grid">
      {#each Object.entries(groupedResources) as [kind, items]}
        <div class="panel">
          <div class="panel-title group-title">
            <span>{capitalize(kind)}s</span>
            <button class="toggle-btn" onclick={() => selectAll(kind)}>Toggle All</button>
          </div>

          <div class="resource-list">
            {#each items as resource (resource.id)}
              {#if resource.status === 'pending'}
                <label class="resource-item" class:selected={resource.selected}>
                  <input type="checkbox" bind:checked={resource.selected} />
                  <span class="identifier">{resource.identifier}</span>
                </label>
              {/if}
            {/each}

            {#if items.filter(r => r.status === 'pending').length === 0}
              <div class="empty-state">All {kind}s processed.</div>
            {/if}
          </div>
        </div>
      {/each}
    </div>

    <!-- Fixed Action Bar -->
    {#if selectedCount > 0}
      <div class="action-bar slide-up">
        <div class="action-bar-content">
          <div class="selection-count">
            <span class="badge">{selectedCount}</span> resources selected
          </div>
          <div class="action-buttons">
            <button class="ignore-btn" onclick={() => applyAction('ignored')}>Ignore Selected</button>
            <button class="primary monitor-btn" onclick={() => applyAction('monitored')}>Monitor Selected</button>
          </div>
        </div>
      </div>
    {/if}
  {/if}
</div>

<style>
  .page { display: flex; flex-direction: column; gap: 24px; padding-bottom: 80px; }
  .page-header { display: flex; justify-content: space-between; gap: 16px; align-items: center; }
  .page-header h2 { font-size: 1.1rem; letter-spacing: 2px; color: var(--text-primary); margin: 0; }
  .vm-name { color: var(--accent-cyan); font-weight: 800; }
  .page-header p { color: var(--text-secondary); font-size: 0.85rem; margin-top: 6px; }

  .back-link { color: var(--text-secondary); text-decoration: none; font-size: 0.82rem; font-weight: 700; transition: color 0.2s; }
  .back-link:hover { color: var(--accent-cyan); }

  .panel { border: 1px solid var(--border-color); background: var(--card-bg); border-radius: 8px; padding: 20px; box-shadow: 0 4px 12px rgba(0,0,0,0.2); }
  .panel-title { color: var(--text-primary); font-weight: 800; letter-spacing: 1.5px; text-transform: uppercase; font-size: 0.8rem; margin-bottom: 16px; }

  .summary-panel { background: linear-gradient(180deg, rgba(14,22,42,0.95) 0%, rgba(10,16,32,0.9) 100%); border-top: 2px solid var(--accent-cyan); }
  .summary-stats { display: flex; gap: 24px; flex-wrap: wrap; margin-bottom: 16px; }
  .stat-box { display: flex; flex-direction: column; align-items: flex-start; gap: 4px; }
  .stat-number { font-size: 1.8rem; font-weight: 900; color: var(--accent-cyan); text-shadow: 0 0 10px rgba(0, 212, 255, 0.3); }
  .stat-label { font-size: 0.75rem; color: var(--text-secondary); text-transform: uppercase; letter-spacing: 1px; font-weight: 700; }
  .summary-instruction { color: var(--text-secondary); font-size: 0.85rem; border-top: 1px solid rgba(255,255,255,0.05); padding-top: 16px; margin: 0; }

  .discovery-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 20px; }

  .group-title { display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px; }
  .toggle-btn { background: none; border: none; color: var(--accent-blue); font-size: 0.7rem; cursor: pointer; text-transform: uppercase; font-weight: 700; padding: 0; }
  .toggle-btn:hover { color: var(--accent-cyan); text-decoration: underline; }

  .resource-list { display: flex; flex-direction: column; gap: 8px; }
  .resource-item { display: flex; align-items: center; gap: 12px; padding: 10px 14px; background: rgba(0,0,0,0.2); border: 1px solid rgba(255,255,255,0.05); border-radius: 6px; cursor: pointer; transition: all 0.2s ease; }
  .resource-item:hover { background: rgba(255,255,255,0.03); border-color: rgba(255,255,255,0.1); }
  .resource-item.selected { background: rgba(0, 212, 255, 0.08); border-color: rgba(0, 212, 255, 0.3); }
  .resource-item input { margin: 0; cursor: pointer; accent-color: var(--accent-cyan); width: 16px; height: 16px; }
  .identifier { color: var(--text-primary); font-size: 0.85rem; font-family: monospace; }

  .empty-state { color: var(--text-secondary); font-size: 0.8rem; font-style: italic; padding: 12px 0; }

  .action-bar { position: fixed; bottom: 20px; left: 50%; transform: translateX(-50%); background: var(--panel-bg); border: 1px solid var(--accent-cyan); box-shadow: 0 10px 30px rgba(0,0,0,0.5), 0 0 15px rgba(0,212,255,0.15); border-radius: 12px; padding: 16px 24px; z-index: 100; backdrop-filter: blur(10px); }
  .action-bar-content { display: flex; align-items: center; gap: 32px; }
  .selection-count { color: var(--text-primary); font-size: 0.9rem; font-weight: 700; display: flex; align-items: center; gap: 8px; }
  .badge { background: var(--accent-cyan); color: #000; padding: 2px 8px; border-radius: 12px; font-weight: 900; font-size: 0.8rem; }

  .action-buttons { display: flex; gap: 12px; }
  button { border: 1px solid var(--border-color); background: rgba(255,255,255,0.04); color: var(--text-primary); border-radius: 6px; padding: 10px 16px; cursor: pointer; font-weight: 800; font-size: 0.75rem; letter-spacing: 1px; text-transform: uppercase; transition: all 0.2s ease; }
  .ignore-btn:hover { background: rgba(255,51,85,0.1); border-color: var(--accent-red); color: var(--accent-red); }
  .monitor-btn { background: rgba(0,212,255,0.15); color: var(--accent-cyan); border-color: rgba(0,212,255,0.4); }
  .monitor-btn:hover { background: rgba(0,212,255,0.25); border-color: var(--accent-cyan); box-shadow: 0 0 10px rgba(0,212,255,0.2); }

  .loading-state { display: flex; flex-direction: column; align-items: center; justify-content: center; padding: 60px 0; gap: 20px; }
  .pulse-ring { width: 40px; height: 40px; border-radius: 50%; border: 2px solid var(--accent-cyan); border-top-color: transparent; animation: spin 1s linear infinite; }

  @keyframes spin { 100% { transform: rotate(360deg); } }
  @keyframes slideUp { from { transform: translate(-50%, 20px); opacity: 0; } to { transform: translate(-50%, 0); opacity: 1; } }
  .slide-up { animation: slideUp 0.3s cubic-bezier(0.175, 0.885, 0.32, 1.275) forwards; }
</style>
