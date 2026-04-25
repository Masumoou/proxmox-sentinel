<script lang="ts">
  import { platformHealth } from '$lib/store';

  let checks = $derived($platformHealth.security || []);
  let certs = $derived($platformHealth.certificates || []);
  let bad = $derived([...checks, ...certs].filter((x: any) => x.severity === 'critical' || x.status === 'critical').length);
  let warn = $derived([...checks, ...certs].filter((x: any) => x.severity === 'warning' || x.status === 'warning').length);
</script>

<div class="page">
  <div class="page-header">
    <div>
      <h2>Security Checks</h2>
      <p>Read-only posture, certificate expiry, repo, firewall, root-login, and visibility checks. Sentinel does not auto-fix.</p>
    </div>
    <div class="summary">
      <span class:bad={bad > 0}>{bad} critical</span>
      <span class:warn={warn > 0}>{warn} warning</span>
    </div>
  </div>

  <section class="grid">
    {#if checks.length === 0}
      <div class="panel empty">Waiting for security collector...</div>
    {:else}
      {#each checks as check (check.key)}
        <article class="panel" class:bad={check.severity === 'critical'} class:warn={check.severity === 'warning'}>
          <span>{check.label}</span>
          <strong>{check.status}</strong>
          <small>{check.detail}</small>
        </article>
      {/each}
    {/if}
  </section>

  <section class="panel">
    <div class="section-head"><span>Certificates</span><small>{certs.length} targets</small></div>
    {#if certs.length === 0}
      <div class="hint">No certificate data yet.</div>
    {:else}
      {#each certs as cert (cert.name)}
        <div class="cert-row" class:bad={cert.status === 'critical'} class:warn={cert.status === 'warning'}>
          <b>{cert.name}</b>
          <span>{cert.days_remaining ?? '--'} days</span>
          <span>{cert.status}</span>
          <small>{cert.url} · {cert.detail}</small>
        </div>
      {/each}
    {/if}
  </section>
</div>

<style>
  .page { display: flex; flex-direction: column; gap: 16px; }
  .page-header { display: flex; justify-content: space-between; gap: 16px; align-items: flex-start; }
  h2 { font-size: 0.9rem; letter-spacing: 3px; color: var(--text-secondary); text-transform: uppercase; }
  p, .hint, .empty { color: var(--text-secondary); font-size: 0.75rem; margin-top: 6px; }
  .summary { display: flex; gap: 8px; flex-wrap: wrap; justify-content: flex-end; }
  .summary span { border: 1px solid var(--border-color); border-radius: 6px; padding: 7px 10px; color: var(--text-secondary); font-size: 0.68rem; text-transform: uppercase; letter-spacing: 1.2px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(240px, 1fr)); gap: 12px; }
  .panel { border: 1px solid var(--border-color); background: var(--card-bg); border-radius: 8px; min-height: 116px; padding: 16px; display: flex; flex-direction: column; justify-content: space-between; gap: 10px; }
  .panel span, .section-head small { color: var(--text-secondary); font-size: 0.65rem; letter-spacing: 1.8px; text-transform: uppercase; }
  .panel strong { color: var(--text-primary); font-size: 1.05rem; overflow-wrap: anywhere; }
  .panel small { color: var(--text-dim); line-height: 1.45; overflow-wrap: anywhere; }
  .section-head { display: flex; justify-content: space-between; align-items: center; color: var(--text-primary); font-weight: 800; letter-spacing: 1.5px; text-transform: uppercase; font-size: 0.72rem; }
  .cert-row { display: grid; grid-template-columns: 1fr auto auto; gap: 10px; align-items: center; padding: 10px 0; border-bottom: 1px solid rgba(255,255,255,0.05); font-size: 0.74rem; }
  .cert-row small { grid-column: 1 / -1; color: var(--text-secondary); overflow-wrap: anywhere; }
  .cert-row b { color: var(--text-primary); }
  .cert-row span { color: var(--text-secondary); }
  .bad, .panel.bad strong, .cert-row.bad span { color: var(--accent-red) !important; border-color: rgba(255,51,85,0.4); }
  .warn, .panel.warn strong, .cert-row.warn span { color: var(--accent-orange) !important; border-color: rgba(255,140,0,0.4); }
  .empty, .hint { min-height: 120px; display: grid; place-items: center; text-align: center; letter-spacing: 1px; }
</style>
