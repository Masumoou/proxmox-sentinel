<script lang="ts">
  let nodeName = $state('CYBER-CORE-01');
  let listenPort = $state(9101);
  let apiInterval = $state(15);
  let cgroupInterval = $state(10);
  let cpuAlert = $state(90);
  let ramAlert = $state(85);
  let diskAlert = $state(90);
</script>

<div class="page">
  <h2 class="page-title">SETTINGS</h2>

  <div class="settings-grid">
    <div class="neon-card section">
      <div class="section-title">GENERAL</div>
      <div class="field">
        <label for="node-name">Node Display Name</label>
        <input id="node-name" type="text" bind:value={nodeName} class="input" />
      </div>
      <div class="field">
        <label for="metrics-port">Metrics Port</label>
        <input id="metrics-port" type="number" bind:value={listenPort} class="input" />
      </div>
    </div>

    <div class="neon-card section">
      <div class="section-title">COLLECTION INTERVALS</div>
      <div class="field">
        <label for="api-interval">API Poll Interval (s)</label>
        <input id="api-interval" type="number" bind:value={apiInterval} class="input" />
      </div>
      <div class="field">
        <label for="cgroup-interval">cgroup Poll Interval (s)</label>
        <input id="cgroup-interval" type="number" bind:value={cgroupInterval} class="input" />
      </div>
    </div>

    <div class="neon-card section">
      <div class="section-title">ALERT THRESHOLDS</div>
      <div class="field">
        <label for="cpu-alert">CPU Alert (%)</label>
        <input id="cpu-alert" type="number" bind:value={cpuAlert} class="input" />
      </div>
      <div class="field">
        <label for="ram-alert">RAM Alert (%)</label>
        <input id="ram-alert" type="number" bind:value={ramAlert} class="input" />
      </div>
      <div class="field">
        <label for="disk-alert">Disk Alert (%)</label>
        <input id="disk-alert" type="number" bind:value={diskAlert} class="input" />
      </div>
    </div>

    <div class="neon-card section">
      <div class="section-title">ABOUT</div>
      <div class="about-row"><span class="about-label">Version</span><span class="about-val text-cyan">0.2.3</span></div>
      <div class="about-row"><span class="about-label">Backend</span><span class="about-val">Rust + Axum + Tokio</span></div>
      <div class="about-row"><span class="about-label">Frontend</span><span class="about-val">SvelteKit 5 + uPlot</span></div>
      <div class="about-row"><span class="about-label">License</span><span class="about-val">MIT</span></div>
    </div>
  </div>
</div>

<style>
  .page { display: flex; flex-direction: column; gap: 20px; }
  .page-title { font-size: 0.85rem; letter-spacing: 3px; color: var(--text-secondary); font-weight: 600; }
  .settings-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(320px, 1fr)); gap: 14px; }
  .section { padding: 20px; display: flex; flex-direction: column; gap: 14px; }
  .section-title { font-size: 0.6rem; color: var(--accent-cyan); letter-spacing: 3px; font-weight: 700; padding-bottom: 8px; border-bottom: 1px solid var(--border-color); }
  .field { display: flex; flex-direction: column; gap: 4px; }
  .field label { font-size: 0.68rem; color: var(--text-secondary); letter-spacing: 1px; }
  .input { background: rgba(255,255,255,0.04); border: 1px solid var(--border-color); border-radius: 6px; padding: 8px 12px; color: var(--text-primary); font-size: 0.8rem; font-family: inherit; outline: none; transition: border-color 0.2s; }
  .input:focus { border-color: var(--accent-cyan); box-shadow: 0 0 8px rgba(0,212,255,0.15); }
  .about-row { display: flex; justify-content: space-between; padding: 6px 0; border-bottom: 1px solid rgba(255,255,255,0.03); font-size: 0.75rem; }
  .about-label { color: var(--text-secondary); }
  .about-val { color: var(--text-primary); font-weight: 600; }
</style>
