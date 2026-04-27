<script lang="ts">
  import { onMount } from 'svelte';
  import {
    diskSummaryLabel,
    enrichedGuests,
    isServiceRunning,
    logs,
    nodes,
    normalizeServiceName,
    storagePools,
    type ServiceData,
  } from '$lib/store';

  type Rule = {
    id?: number | string;
    source?: 'config' | 'ui';
    readonly?: boolean;
    enabled: boolean;
    name: string;
    target: string;
    node?: string;
    vmid?: number;
    service?: string;
    mount?: string;
    metric?: string;
    operator?: string;
    threshold?: number;
    value?: string;
    condition?: string;
    duration_secs: number;
    severity: 'info' | 'warning' | 'critical';
    notification_channel?: string;
    notes?: string;
  };

  const targets = ['vm', 'lxc', 'node', 'storage', 'service', 'guest_disk', 'guest'];
  const severities = ['info', 'warning', 'critical'];
  const operators = ['>', '>=', '<', '<=', '==', '!='];
  const conditions = ['down', 'not_running', 'failed', 'inactive', 'missing', 'running'];

  let rules = $state<Rule[]>([]);
  let form = $state<Rule>(emptyRule());
  let editingId = $state<number | string | null>(null);
  let error = $state('');
  let message = $state('');
  let preview = $state('');
  let loading = $state(false);

  let alertLogs = $derived($logs.filter((log) => ['WARN', 'ERROR', 'CRITICAL'].includes(log.level)).slice(0, 80));
  let selectedGuest = $derived($enrichedGuests.find((guest) => guest.vmid === Number(form.vmid)) || null);
  let serviceNames = $derived((selectedGuest?.services || []).map((service: ServiceData) => normalizeServiceName(service.name)).sort());
  let mountPaths = $derived((selectedGuest?.disk_mounts || []).map((mount: any) => mount.mountpoint).sort());

  onMount(loadRules);

  function emptyRule(): Rule {
    return {
      enabled: true,
      name: '',
      target: 'vm',
      metric: 'cpu',
      operator: '>',
      threshold: 86,
      duration_secs: 120,
      severity: 'warning',
      notes: '',
      notification_channel: '',
    };
  }

  async function loadRules() {
    loading = true;
    error = '';
    try {
      const res = await fetch('/api/v1/alert-rules');
      if (!res.ok) throw new Error(await res.text());
      rules = await res.json();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not load alert rules';
    } finally {
      loading = false;
    }
  }

  function editRule(rule: Rule) {
    editingId = rule.id ?? null;
    form = { ...emptyRule(), ...rule };
    preview = evaluateRule(form);
    message = '';
    error = '';
  }

  function resetForm() {
    editingId = null;
    form = emptyRule();
    preview = '';
    error = '';
  }

  function normalizeForm(rule: Rule): Rule {
    const normalized: Rule = {
      enabled: Boolean(rule.enabled),
      name: rule.name.trim(),
      target: rule.target,
      duration_secs: Number(rule.duration_secs || 0),
      severity: rule.severity || 'warning',
    };
    for (const key of ['node', 'service', 'mount', 'metric', 'operator', 'value', 'condition', 'notification_channel', 'notes'] as const) {
      const value = rule[key];
      if (typeof value === 'string' && value.trim()) normalized[key] = value.trim() as never;
    }
    if (rule.vmid !== undefined && String(rule.vmid).trim() !== '') normalized.vmid = Number(rule.vmid);
    if (rule.threshold !== undefined && String(rule.threshold).trim() !== '') normalized.threshold = Number(rule.threshold);
    return normalized;
  }

  function validateRule(rule: Rule): string | null {
    if (!rule.name?.trim()) return 'Rule name is required.';
    if (rule.duration_secs < 0) return 'Duration must be 0 or greater.';
    if (['vm', 'lxc', 'service', 'guest_disk', 'guest'].includes(rule.target)) {
      if (!rule.vmid || !$enrichedGuests.some((guest) => guest.vmid === Number(rule.vmid))) return 'Select a valid VMID.';
    }
    if (rule.target === 'service') {
      if (!rule.service?.trim()) return 'Service name is required.';
      if (selectedGuest?.services?.length && !serviceNames.includes(normalizeServiceName(rule.service))) {
        return `Service '${rule.service}' is not in the latest inventory for VMID ${rule.vmid}.`;
      }
    }
    if (rule.target === 'guest_disk' && rule.mount && selectedGuest?.disk_mounts?.length && !mountPaths.includes(rule.mount)) {
      return `Mount '${rule.mount}' is not in the latest disk inventory for VMID ${rule.vmid}.`;
    }
    if (!['service'].includes(rule.target) && ['>', '>=', '<', '<='].includes(rule.operator || '>') && Number.isNaN(Number(rule.threshold))) {
      return 'Threshold must be numeric.';
    }
    return null;
  }

  async function saveRule() {
    const payload = normalizeForm(form);
    const problem = validateRule(payload);
    if (problem) {
      error = problem;
      return;
    }
    error = '';
    message = '';
    try {
      const editingUiRule = typeof editingId === 'number';
      const res = await fetch(editingUiRule ? `/api/v1/alert-rules/${editingId}` : '/api/v1/alert-rules', {
        method: editingUiRule ? 'PUT' : 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      });
      if (!res.ok) throw new Error(await res.text());
      message = editingUiRule ? 'Rule updated.' : 'Rule created.';
      resetForm();
      await loadRules();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not save rule';
    }
  }

  async function deleteRule(rule: Rule) {
    if (typeof rule.id !== 'number') return;
    error = '';
    try {
      const res = await fetch(`/api/v1/alert-rules/${rule.id}`, { method: 'DELETE' });
      if (!res.ok) throw new Error(await res.text());
      message = 'Rule deleted.';
      await loadRules();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not delete rule';
    }
  }

  function updateTarget(target: string) {
    form.target = target;
    if (target === 'service') {
      form.metric = undefined;
      form.condition = 'down';
      form.operator = undefined;
      form.threshold = undefined;
    } else if (target === 'guest_disk') {
      form.metric = 'used_percent';
      form.operator = '>';
      form.threshold = 85;
      form.condition = undefined;
    } else if (target === 'guest') {
      form.metric = 'status';
      form.operator = '==';
      form.value = 'stopped';
      form.condition = undefined;
    } else if (target === 'storage') {
      form.metric = 'usage';
      form.operator = '>';
      form.threshold = 85;
      form.condition = undefined;
    } else {
      form.metric = target === 'node' ? 'memory' : 'cpu';
      form.operator = '>';
      form.threshold = target === 'node' ? 90 : 86;
      form.condition = undefined;
    }
    preview = evaluateRule(form);
  }

  function evaluateRule(rule: Rule): string {
    const normalized = normalizeForm(rule);
    if (normalized.target === 'node') {
      const node = $nodes.find((item) => !normalized.node || item.node === normalized.node);
      if (!node) return 'No node telemetry available yet.';
      const actual = normalized.metric === 'memory' ? pct(node.mem_used, node.mem_total) : normalized.metric === 'storage' ? pct(node.disk_used || 0, node.disk_total || 0) : (node.cpu || 0) * 100;
      return compare(actual, normalized) ? `FIRING now: ${actual.toFixed(1)}%` : `OK now: ${actual.toFixed(1)}%`;
    }
    if (normalized.target === 'storage') {
      const pool = $storagePools.find((item) => !normalized.node || item.node === normalized.node);
      if (!pool) return 'No storage telemetry available yet.';
      const actual = pct(pool.used, pool.total);
      return compare(actual, normalized) ? `FIRING now: ${actual.toFixed(1)}%` : `OK now: ${actual.toFixed(1)}%`;
    }
    const guest = $enrichedGuests.find((item) => item.vmid === normalized.vmid);
    if (!guest) return 'Select a guest with current telemetry.';
    if (normalized.target === 'service') {
      const service = guest.services.find((item: ServiceData) => normalizeServiceName(item.name) === normalizeServiceName(normalized.service || ''));
      const running = service ? isServiceRunning(service) : false;
      const condition = normalized.condition || 'down';
      const firing = condition === 'running' ? running : !running;
      return firing ? `FIRING now: ${normalized.service} ${condition}` : `OK now: ${normalized.service} running`;
    }
    if (normalized.target === 'guest_disk') {
      if (!guest.disk_summary?.available) return 'No guest disk telemetry yet.';
      const mount = normalized.mount ? guest.disk_mounts.find((item: any) => item.mountpoint === normalized.mount) : null;
      const actual = mount ? mount.use_pct : (guest.disk_summary.root_used_pct ?? guest.disk_summary.used_pct);
      return compare(actual, normalized) ? `FIRING now: ${actual.toFixed(1)}%` : `OK now: ${diskSummaryLabel(guest.disk_summary)}`;
    }
    if (normalized.metric === 'status') {
      const firing = normalized.operator === '!=' ? guest.status !== normalized.value : guest.status === normalized.value;
      return firing ? `FIRING now: status is ${guest.status}` : `OK now: status is ${guest.status}`;
    }
    const actual = normalized.metric === 'memory' || normalized.metric === 'ram' ? pct(guest.mem, guest.maxmem) : guest.cpu * 100;
    return compare(actual, normalized) ? `FIRING now: ${actual.toFixed(1)}%` : `OK now: ${actual.toFixed(1)}%`;
  }

  function compare(actual: number, rule: Rule) {
    const threshold = Number(rule.threshold || 0);
    switch (rule.operator || '>') {
      case '>=': return actual >= threshold;
      case '<': return actual < threshold;
      case '<=': return actual <= threshold;
      case '==': return actual === threshold;
      case '!=': return actual !== threshold;
      default: return actual > threshold;
    }
  }

  function pct(value: number, total: number): number {
    return total > 0 ? Math.min(100, Math.max(0, (value / total) * 100)) : 0;
  }

  function ruleSummary(rule: Rule) {
    if (rule.target === 'service') return `${rule.service} ${rule.condition || 'down'} on ${rule.vmid}`;
    if (rule.target === 'guest_disk') return `${rule.mount || 'max mount'} ${rule.metric || 'used_percent'} ${rule.operator || '>'} ${rule.threshold}% on ${rule.vmid}`;
    if (rule.metric === 'status') return `${rule.metric} ${rule.operator || '=='} ${rule.value}`;
    return `${rule.metric || 'metric'} ${rule.operator || '>'} ${rule.threshold}`;
  }
</script>

<div class="page">
  <div class="page-header">
    <div>
      <h2>Alerts & Custom Rules</h2>
      <p>Create durable UI rules in SQLite, while config.toml rules remain read-only and versionable.</p>
    </div>
    <button onclick={resetForm}>Create rule</button>
  </div>

  {#if error}<div class="notice bad">{error}</div>{/if}
  {#if message}<div class="notice ok">{message}</div>{/if}

  <section class="editor panel">
    <div class="panel-title">{editingId ? 'Edit Rule' : 'Create Rule'}</div>
    <div class="form-grid">
      <label>Rule name<input bind:value={form.name} placeholder="diskbg-wfe CPU high" /></label>
      <label class="check-row"><input type="checkbox" bind:checked={form.enabled} /> Enabled</label>
      <label>Target<select bind:value={form.target} onchange={(e) => updateTarget((e.target as HTMLSelectElement).value)}>{#each targets as target}<option value={target}>{target}</option>{/each}</select></label>
      <label>Severity<select bind:value={form.severity}>{#each severities as severity}<option value={severity}>{severity}</option>{/each}</select></label>
      <label>Node<select bind:value={form.node}><option value="">any</option>{#each $nodes as node}<option value={node.node}>{node.node}</option>{/each}</select></label>
      <label>VMID<select bind:value={form.vmid}><option value="">select</option>{#each $enrichedGuests as guest}<option value={guest.vmid}>{guest.vmid} · {guest.name}</option>{/each}</select></label>
      {#if form.target === 'service'}
        <label>Service<input bind:value={form.service} list="services" placeholder="apache2" /></label>
        <datalist id="services">{#each serviceNames as service}<option value={service}></option>{/each}</datalist>
        <label>Condition<select bind:value={form.condition}>{#each conditions as condition}<option value={condition}>{condition}</option>{/each}</select></label>
      {:else}
        <label>Metric<input bind:value={form.metric} placeholder={form.target === 'guest_disk' ? 'used_percent' : 'cpu'} /></label>
        <label>Operator<select bind:value={form.operator}>{#each operators as op}<option value={op}>{op}</option>{/each}</select></label>
        <label>Threshold<input type="number" bind:value={form.threshold} placeholder="86" /></label>
        <label>Value<input bind:value={form.value} placeholder="stopped" /></label>
      {/if}
      {#if form.target === 'guest_disk'}
        <label>Mount<input bind:value={form.mount} list="mounts" placeholder="/" /></label>
        <datalist id="mounts">{#each mountPaths as mount}<option value={mount}></option>{/each}</datalist>
      {/if}
      <label>Duration seconds<input type="number" min="0" bind:value={form.duration_secs} /></label>
      <label>Notification channel<input bind:value={form.notification_channel} placeholder="optional" /></label>
      <label class="wide">Notes<textarea bind:value={form.notes} placeholder="Why this rule matters"></textarea></label>
    </div>
    <div class="actions">
      <button onclick={() => (preview = evaluateRule(form))}>Test / Evaluate</button>
      <button class="primary" onclick={saveRule}>{editingId ? 'Save changes' : 'Create rule'}</button>
      {#if editingId}<button onclick={resetForm}>Cancel</button>{/if}
      {#if preview}<span>{preview}</span>{/if}
    </div>
  </section>

  <section class="panel">
    <div class="panel-title">Custom Alert Rules <small>{loading ? 'loading...' : `${rules.length} rules`}</small></div>
    <div class="rule-table">
      <div class="rule-head"><span>Name</span><span>Source</span><span>Target</span><span>Rule</span><span>Duration</span><span>Severity</span><span>Status</span><span>Action</span></div>
      {#each rules as rule (rule.id || rule.name)}
        <div class="rule-row" class:disabled={!rule.enabled}>
          <span class="name">{rule.name}</span>
          <span>{rule.source || 'ui'}</span>
          <span>{rule.target}{rule.vmid ? ` ${rule.vmid}` : ''}</span>
          <span>{ruleSummary(rule)}</span>
          <span>{rule.duration_secs}s</span>
          <span class={rule.severity}>{rule.severity}</span>
          <span>{rule.enabled ? evaluateRule(rule) : 'disabled'}</span>
          <span class="row-actions">
            <button onclick={() => editRule(rule)}>Edit</button>
            <button disabled={rule.readonly} onclick={() => deleteRule(rule)}>Delete</button>
          </span>
        </div>
      {:else}
        <div class="empty">No alert rules yet.</div>
      {/each}
    </div>
  </section>

  <section class="panel">
    <div class="panel-title">Recent Alert Stream</div>
    {#if alertLogs.length === 0}
      <div class="empty">No warning or error events yet.</div>
    {:else}
      {#each alertLogs as log}
        <div class="log-row">
          <span>{log.time}</span>
          <b class:bad={log.level !== 'WARN'}>{log.level}</b>
          <p>{log.message}</p>
        </div>
      {/each}
    {/if}
  </section>
</div>

<style>
  .page { display: flex; flex-direction: column; gap: 18px; }
  .page-header { display: flex; justify-content: space-between; gap: 16px; align-items: flex-start; }
  .page-header h2 { font-size: 0.95rem; letter-spacing: 3px; color: var(--text-primary); text-transform: uppercase; }
  .page-header p, .empty { color: var(--text-secondary); font-size: 0.82rem; margin-top: 6px; line-height: 1.45; }
  .panel { border: 1px solid var(--border-color); background: var(--card-bg); border-radius: 8px; padding: 16px; }
  .panel-title { display: flex; justify-content: space-between; color: var(--text-primary); font-weight: 800; letter-spacing: 1.5px; text-transform: uppercase; font-size: 0.78rem; margin-bottom: 12px; }
  .form-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(210px, 1fr)); gap: 12px; }
  label { display: flex; flex-direction: column; gap: 6px; color: var(--text-secondary); font-size: 0.74rem; font-weight: 700; }
  .check-row { flex-direction: row; align-items: center; justify-content: flex-start; }
  .check-row input { width: auto; }
  label.wide { grid-column: 1 / -1; }
  input, select, textarea { width: 100%; background: var(--panel-bg); border: 1px solid var(--border-color); color: var(--text-primary); border-radius: 6px; padding: 9px 10px; font-size: 0.82rem; }
  textarea { min-height: 72px; resize: vertical; }
  button { border: 1px solid var(--border-color); background: rgba(255,255,255,0.04); color: var(--text-primary); border-radius: 6px; padding: 8px 11px; cursor: pointer; font-weight: 800; font-size: 0.68rem; letter-spacing: 1px; }
  button.primary { background: rgba(0,212,255,0.12); color: var(--accent-cyan); }
  button:hover:not(:disabled) { border-color: var(--accent-cyan); color: var(--accent-cyan); }
  button:disabled { color: var(--text-secondary); opacity: 0.55; cursor: not-allowed; }
  .actions { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; margin-top: 14px; }
  .actions span, .notice { color: var(--text-secondary); font-size: 0.82rem; }
  .notice { border: 1px solid rgba(255,255,255,0.08); border-radius: 6px; padding: 10px 12px; }
  .notice.ok, .ok { color: var(--accent-green); }
  .notice.bad, .bad, .critical { color: var(--accent-red); }
  .warning { color: var(--accent-orange); }
  .info { color: var(--accent-cyan); }
  .rule-table { overflow-x: auto; }
  .rule-head, .rule-row { min-width: 1180px; display: grid; grid-template-columns: 1.3fr 80px 120px 1.8fr 90px 90px 1.6fr 150px; gap: 12px; align-items: center; padding: 9px 10px; }
  .rule-head { color: var(--text-secondary); font-size: 0.62rem; letter-spacing: 1.8px; text-transform: uppercase; border-bottom: 1px solid var(--border-color); }
  .rule-row { color: var(--text-secondary); border-bottom: 1px solid rgba(255,255,255,0.05); font-size: 0.78rem; }
  .rule-row.disabled { opacity: 0.62; }
  .name { color: var(--text-primary); font-weight: 800; overflow-wrap: anywhere; }
  .row-actions { display: flex; gap: 6px; }
  .log-row { display: grid; grid-template-columns: 90px 90px minmax(0, 1fr); gap: 12px; padding: 9px 0; border-bottom: 1px solid rgba(255,255,255,0.05); font-size: 0.8rem; }
  .log-row span { color: var(--text-secondary); }
  .log-row b { color: var(--accent-orange); }
  .log-row p { min-width: 0; overflow-wrap: anywhere; }
</style>
