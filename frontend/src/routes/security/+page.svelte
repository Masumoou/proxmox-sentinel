<script lang="ts">
  import { enrichedGuests, nodes } from '$lib/store';

  let noAgent = $derived($enrichedGuests.filter((guest) => guest.type === 'QEMU' && guest.status === 'running' && !guest.agent && !guest.ssh));
</script>

<div class="page">
  <div class="page-header">
    <h2>Security Checks</h2>
    <p>Read-only posture checks will report risks without auto-fixing anything.</p>
  </div>

  <section class="check-grid">
    <div class="check">
      <span>Cluster visibility</span>
      <strong>{$nodes.length} nodes</strong>
      <small>API token can list monitored nodes</small>
    </div>
    <div class="check" class:warn={noAgent.length > 0}>
      <span>Guest visibility</span>
      <strong>{noAgent.length}</strong>
      <small>running QEMU guests without agent/SSH visibility</small>
    </div>
    <div class="check planned">
      <span>API token posture</span>
      <strong>Planned</strong>
      <small>expiration and privilege checks</small>
    </div>
    <div class="check planned">
      <span>Firewall posture</span>
      <strong>Planned</strong>
      <small>node and guest firewall state</small>
    </div>
  </section>
</div>

<style>
  .page { display: flex; flex-direction: column; gap: 18px; }
  .page-header h2 { font-size: 0.9rem; letter-spacing: 3px; color: var(--text-secondary); text-transform: uppercase; }
  .page-header p { color: var(--text-secondary); font-size: 0.75rem; margin-top: 6px; }
  .check-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(240px, 1fr)); gap: 12px; }
  .check { border: 1px solid var(--border-color); background: var(--card-bg); border-radius: 8px; min-height: 116px; padding: 16px; display: flex; flex-direction: column; justify-content: space-between; }
  .check.warn { border-color: rgba(255,140,0,0.45); }
  .check.planned { opacity: 0.72; }
  .check span { color: var(--text-secondary); font-size: 0.65rem; letter-spacing: 1.8px; text-transform: uppercase; }
  .check strong { color: var(--text-primary); font-size: 1.35rem; }
  .check small { color: var(--text-dim); line-height: 1.45; }
</style>
