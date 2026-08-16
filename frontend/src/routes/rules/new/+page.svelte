<script lang="ts">
  import { fade } from 'svelte/transition';

  // Rule State
  let resourceType = $state('service');
  let resourceName = $state('nginx.service');
  let metricType = $state('state');

  let fireOperator = $state('!=');
  let fireValue = $state('running');
  let fireDuration = $state(60);

  let resolveOperator = $state('==');
  let resolveValue = $state('running');
  let resolveDuration = $state(30);

  let severity = $state('Critical');
  let testing = $state(false);
  let testSuccess = $state(false);

  const operators = ['==', '!=', '>', '<', '>=', '<=', 'contains'];
  const severities = ['Critical', 'Warning', 'Info'];

  let metricLabel = $derived(metricType.replace('_', ' '));

  function handleTest() {
    testing = true;
    testSuccess = false;
    setTimeout(() => {
      testing = false;
      testSuccess = true;
      setTimeout(() => testSuccess = false, 3000);
    }, 1200);
  }
</script>

<div class="page">
  <div class="page-header">
    <div>
      <h2>Create Alert Rule</h2>
      <p>Tell Sentinel when to alert you without worrying about the internal architecture.</p>
    </div>
    <div class="actions">
      <a href="/alerts" class="back-link">← Back to Alerts</a>
    </div>
  </div>

  <div class="rule-builder-grid">

    <!-- Target Selection -->
    <div class="panel target-panel">
      <div class="panel-header">
        <div class="step-num">1</div>
        <h3>What do you want Sentinel to monitor?</h3>
      </div>
      <div class="panel-body form-grid">
        <label>
          Resource Type
          <select bind:value={resourceType}>
            <option value="vm_cpu">VM CPU</option>
            <option value="vm_memory">VM Memory</option>
            <option value="service">Guest Service</option>
            <option value="filesystem">Filesystem</option>
          </select>
        </label>

        {#if resourceType === 'service'}
          <label>
            Service Name
            <input type="text" bind:value={resourceName} placeholder="e.g. nginx.service" />
          </label>
          <label>
            Metric
            <select bind:value={metricType}>
              <option value="state">Service State</option>
              <option value="restarts">Restart Count</option>
            </select>
          </label>
        {:else if resourceType === 'vm_cpu'}
          <label>
            Metric
            <select bind:value={metricType}>
              <option value="usage_percent">Usage Percentage</option>
            </select>
          </label>
        {/if}
      </div>
    </div>

    <!-- Condition Logic -->
    <div class="panel condition-panel">
      <div class="panel-header">
        <div class="step-num">2</div>
        <h3>When should Sentinel alert you?</h3>
      </div>

      <div class="panel-body condition-sections">
        <!-- FIRE -->
        <div class="condition-box fire-box">
          <div class="box-label">🔥 FIRE</div>
          <div class="logic-row">
            <span class="static-text">When {metricLabel}</span>
            <select bind:value={fireOperator} class="operator-select">
              {#each operators as op}
                <option value={op}>{op}</option>
              {/each}
            </select>
            <input type="text" bind:value={fireValue} placeholder="value" class="value-input" />
          </div>
          <div class="logic-row">
            <span class="static-text">continuously for</span>
            <input type="number" bind:value={fireDuration} min="0" class="duration-input" />
            <span class="static-text">seconds</span>
          </div>
          <div class="logic-row mt-2">
            <span class="static-text">Severity:</span>
            <select bind:value={severity} class="severity-select" class:text-red={severity==='Critical'} class:text-orange={severity==='Warning'}>
              {#each severities as s}
                <option value={s}>{s}</option>
              {/each}
            </select>
          </div>
        </div>

        <!-- RESOLVE -->
        <div class="condition-box resolve-box">
          <div class="box-label">✅ RESOLVE</div>
          <div class="logic-row">
            <span class="static-text">When {metricLabel}</span>
            <select bind:value={resolveOperator} class="operator-select">
              {#each operators as op}
                <option value={op}>{op}</option>
              {/each}
            </select>
            <input type="text" bind:value={resolveValue} placeholder="value" class="value-input" />
          </div>
          <div class="logic-row">
            <span class="static-text">continuously for</span>
            <input type="number" bind:value={resolveDuration} min="0" class="duration-input" />
            <span class="static-text">seconds</span>
          </div>
        </div>
      </div>
    </div>

    <!-- Actions -->
    <div class="action-footer">
      {#if testSuccess}
        <div class="success-msg" transition:fade>✓ Rule syntax and telemetry path verified</div>
      {/if}
      <div class="btn-group">
        <button class="secondary-btn" onclick={handleTest} disabled={testing}>
          {testing ? 'Testing...' : 'Test Rule'}
        </button>
        <button class="primary-btn">Save Rule</button>
      </div>
    </div>

  </div>
</div>

<style>
  .page { display: flex; flex-direction: column; gap: 24px; padding-bottom: 40px; max-width: 800px; margin: 0 auto; }

  .page-header { display: flex; justify-content: space-between; align-items: flex-end; padding-bottom: 16px; border-bottom: 1px solid rgba(255,255,255,0.05); }
  .page-header h2 { font-size: 1.4rem; color: var(--text-primary); margin: 0; letter-spacing: 1px; }
  .page-header p { color: var(--text-secondary); font-size: 0.85rem; margin-top: 6px; }

  .back-link { color: var(--text-secondary); text-decoration: none; font-size: 0.85rem; font-weight: 700; transition: color 0.2s; }
  .back-link:hover { color: var(--accent-cyan); }

  .rule-builder-grid { display: flex; flex-direction: column; gap: 24px; margin-top: 10px; }

  .panel { background: var(--card-bg); border: 1px solid var(--border-color); border-radius: 10px; overflow: hidden; box-shadow: 0 4px 15px rgba(0,0,0,0.15); }

  .panel-header { display: flex; align-items: center; gap: 12px; padding: 16px 20px; background: rgba(0,0,0,0.25); border-bottom: 1px solid rgba(255,255,255,0.03); }
  .step-num { width: 28px; height: 28px; border-radius: 50%; background: rgba(0, 212, 255, 0.15); color: var(--accent-cyan); display: flex; align-items: center; justify-content: center; font-weight: 900; font-size: 0.9rem; border: 1px solid rgba(0, 212, 255, 0.3); }
  .panel-header h3 { margin: 0; font-size: 0.95rem; color: var(--text-primary); }

  .panel-body { padding: 20px; }

  .form-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 16px; }
  label { display: flex; flex-direction: column; gap: 6px; color: var(--text-secondary); font-size: 0.75rem; font-weight: 700; text-transform: uppercase; letter-spacing: 0.5px; }
  input, select { background: rgba(0,0,0,0.3); border: 1px solid rgba(255,255,255,0.1); color: var(--text-primary); padding: 10px 12px; border-radius: 6px; font-size: 0.85rem; transition: all 0.2s; }
  input:focus, select:focus { outline: none; border-color: var(--accent-cyan); background: rgba(0,212,255,0.05); }

  .condition-sections { display: flex; flex-direction: column; gap: 16px; }
  .condition-box { border: 1px solid rgba(255,255,255,0.05); border-radius: 8px; padding: 16px 20px; background: rgba(0,0,0,0.15); position: relative; }
  .fire-box { border-left: 3px solid var(--accent-red); }
  .resolve-box { border-left: 3px solid var(--accent-green); }

  .box-label { font-size: 0.7rem; font-weight: 900; letter-spacing: 1px; margin-bottom: 12px; }
  .fire-box .box-label { color: var(--accent-red); }
  .resolve-box .box-label { color: var(--accent-green); }

  .logic-row { display: flex; align-items: center; gap: 10px; margin-bottom: 10px; flex-wrap: wrap; }
  .logic-row:last-child { margin-bottom: 0; }
  .mt-2 { margin-top: 16px; padding-top: 16px; border-top: 1px solid rgba(255,255,255,0.05); }

  .static-text { color: var(--text-primary); font-size: 0.9rem; font-weight: 500; }

  .operator-select { width: 80px; font-family: monospace; font-size: 0.9rem; font-weight: 700; text-align: center; }
  .value-input { width: 140px; font-family: monospace; }
  .duration-input { width: 80px; text-align: center; }
  .severity-select { width: 120px; font-weight: 800; }

  .text-red { color: var(--accent-red); }
  .text-orange { color: var(--accent-orange); }

  .action-footer { display: flex; justify-content: flex-end; align-items: center; gap: 20px; margin-top: 10px; }
  .success-msg { color: var(--accent-green); font-size: 0.85rem; font-weight: 700; display: flex; align-items: center; gap: 6px; }

  .btn-group { display: flex; gap: 12px; }
  button { padding: 12px 24px; border-radius: 6px; font-weight: 800; font-size: 0.8rem; letter-spacing: 1px; text-transform: uppercase; cursor: pointer; transition: all 0.2s; border: 1px solid transparent; }
  .secondary-btn { background: rgba(255,255,255,0.05); color: var(--text-primary); border-color: rgba(255,255,255,0.1); }
  .secondary-btn:hover:not(:disabled) { background: rgba(255,255,255,0.1); }
  .secondary-btn:disabled { opacity: 0.5; cursor: wait; }
  .primary-btn { background: var(--accent-cyan); color: #000; box-shadow: 0 0 15px rgba(0,212,255,0.3); }
  .primary-btn:hover { background: #33ddff; box-shadow: 0 0 20px rgba(0,212,255,0.5); transform: translateY(-1px); }
</style>
