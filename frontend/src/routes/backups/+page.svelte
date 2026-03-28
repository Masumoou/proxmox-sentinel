<script lang="ts">
</script>

<div class="page">
  <h2 class="page-title">BACKUPS</h2>

  <div class="neon-card info-panel">
    <div class="info-icon">⊘</div>
    <div class="info-text">
      <h3>Backup Management</h3>
      <p>Backup scheduling and restore operations will be available in a future update. Proxmox vzdump and PBS integration is planned for V0.2.0.</p>
    </div>
  </div>

  <div class="feature-grid">
    <div class="neon-card feature">
      <div class="feature-icon text-cyan">⏱</div>
      <div class="feature-title">Scheduled Backups</div>
      <div class="feature-desc">Configure automated vzdump schedules for VMs and LXCs</div>
      <span class="coming-badge">PLANNED</span>
    </div>
    <div class="neon-card feature">
      <div class="feature-icon text-green">↻</div>
      <div class="feature-title">Restore</div>
      <div class="feature-desc">One-click restore from any backup point</div>
      <span class="coming-badge">PLANNED</span>
    </div>
    <div class="neon-card feature">
      <div class="feature-icon text-purple">▤</div>
      <div class="feature-title">PBS Integration</div>
      <div class="feature-desc">Proxmox Backup Server status and management</div>
      <span class="coming-badge">PLANNED</span>
    </div>
  </div>
</div>

<style>
  .page { display: flex; flex-direction: column; gap: 20px; }
  .page-title { font-size: 0.85rem; letter-spacing: 3px; color: var(--text-secondary); font-weight: 600; }
  .info-panel { padding: 24px; display: flex; gap: 16px; align-items: center; border-color: rgba(0,212,255,0.2); }
  .info-icon { font-size: 2rem; color: var(--accent-cyan); }
  .info-text h3 { font-size: 0.9rem; letter-spacing: 1px; margin-bottom: 6px; }
  .info-text p { font-size: 0.75rem; color: var(--text-secondary); line-height: 1.6; }
  .feature-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 14px; }
  .feature { padding: 20px; text-align: center; display: flex; flex-direction: column; align-items: center; gap: 10px; }
  .feature-icon { font-size: 1.8rem; }
  .feature-title { font-size: 0.8rem; font-weight: 700; letter-spacing: 1px; }
  .feature-desc { font-size: 0.68rem; color: var(--text-secondary); line-height: 1.5; }
  .coming-badge { font-size: 0.55rem; padding: 3px 10px; border-radius: 3px; background: rgba(182,68,224,0.12); color: var(--accent-purple); border: 1px solid rgba(182,68,224,0.2); letter-spacing: 2px; font-weight: 700; }
</style>
