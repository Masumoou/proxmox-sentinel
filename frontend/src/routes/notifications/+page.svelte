<script lang="ts">
  // Form State
  let channelType = $state('Webhook'); // Webhook, Email
  let name = $state('');
  let configEndpoint = $state(''); // for webhook or SMTP server
  let configSecret = $state(''); // webhook secret or SMTP password
  let enabled = $state(true);
  
  let testing = $state(false);
  let testSuccess = $state(false);

  // Channels State
  let channels = $state([
    {
      id: 'ch-1',
      name: 'Slack Alerts Webhook',
      type: 'Webhook',
      endpoint: 'https://hooks.slack.com/services/T00...',
      status: 'ENABLED'
    },
    {
      id: 'ch-2',
      name: 'Ops Team Email',
      type: 'Email',
      endpoint: 'smtp.gmail.com (ops@company.com)',
      status: 'DISABLED'
    }
  ]);

  // Delivery History State
  let deliveryHistory = $state([
    {
      id: 'del-101',
      timestamp: '2026-08-16T16:41:13',
      channelId: 'ch-1',
      channelName: 'Slack Alerts Webhook',
      incidentRef: 'VM 101 unreachable',
      status: 'DELIVERED',
      latency: '240ms'
    },
    {
      id: 'del-100',
      timestamp: '2026-08-16T12:15:02',
      channelId: 'ch-1',
      channelName: 'Slack Alerts Webhook',
      incidentRef: 'Test Connectivity Payload',
      status: 'DELIVERED',
      latency: '180ms'
    },
    {
      id: 'del-99',
      timestamp: '2026-08-15T09:30:00',
      channelId: 'ch-2',
      channelName: 'Ops Team Email',
      incidentRef: 'PostgreSQL memory critical',
      status: 'FAILED',
      latency: '5000ms'
    }
  ]);

  function handleCreate() {
    if (!name || !configEndpoint) return;
    
    channels = [{
      id: `ch-${Date.now()}`,
      name,
      type: channelType,
      endpoint: configEndpoint,
      status: enabled ? 'ENABLED' : 'DISABLED'
    }, ...channels];

    name = '';
    configEndpoint = '';
    configSecret = '';
  }

  function toggleChannel(channelId: string) {
    channels = channels.map(c => {
      if (c.id === channelId) {
        return { ...c, status: c.status === 'ENABLED' ? 'DISABLED' : 'ENABLED' };
      }
      return c;
    });
  }

  function handleTestConnectivity() {
    testing = true;
    testSuccess = false;
    
    // Simulate connectivity check without modifying incident state
    setTimeout(() => {
      testing = false;
      testSuccess = true;
      
      // Prepend a connectivity log to delivery history
      deliveryHistory = [{
        id: `del-${Date.now()}`,
        timestamp: new Date().toISOString().slice(0, 19),
        channelId: 'mock',
        channelName: name || 'Unsaved Channel',
        incidentRef: 'Test Connectivity Payload',
        status: 'DELIVERED',
        latency: Math.floor(Math.random() * 200 + 100) + 'ms'
      }, ...deliveryHistory];

      setTimeout(() => testSuccess = false, 3000);
    }, 800);
  }
</script>

<div class="page">
  <div class="header">
    <div>
      <h2>Notification Channels</h2>
      <p>Configure how Sentinel routes alerts. Disabling a channel only pauses delivery, incidents remain open.</p>
    </div>
  </div>

  <div class="dashboard-grid">
    <!-- Left Column: Channel Config -->
    <div class="left-col">
      <div class="panel creation-panel">
        <div class="panel-title">Add New Channel</div>
        
        <div class="form-group">
          <label for="channelType">Channel Type</label>
          <select id="channelType" bind:value={channelType} class="input">
            <option value="Webhook">Webhook (Slack, Discord, Custom)</option>
            <option value="Email">Email (SMTP)</option>
          </select>
        </div>

        <div class="form-group">
          <label for="name">Channel Name</label>
          <input type="text" id="name" bind:value={name} class="input" placeholder="e.g. Core Infra Slack" />
        </div>

        {#if channelType === 'Webhook'}
          <div class="form-group">
            <label for="configEndpoint">Webhook URL</label>
            <input type="text" id="configEndpoint" bind:value={configEndpoint} class="input" placeholder="https://..." />
          </div>
          <div class="form-group">
            <label for="configSecret">Secret Token (Optional)</label>
            <input type="password" id="configSecret" bind:value={configSecret} class="input" placeholder="Bearer or signing token" />
          </div>
        {:else if channelType === 'Email'}
          <div class="form-group">
            <label for="configEndpoint">SMTP Server / Connection String</label>
            <input type="text" id="configEndpoint" bind:value={configEndpoint} class="input" placeholder="smtp.example.com:587" />
          </div>
          <div class="form-group">
            <label for="configSecret">SMTP Password / API Key</label>
            <input type="password" id="configSecret" bind:value={configSecret} class="input" />
          </div>
        {/if}

        <div class="form-group checkbox-group mt-2">
          <label class="toggle-label">
            <input type="checkbox" bind:checked={enabled} />
            Enable immediately upon creation
          </label>
        </div>

        <div class="action-row mt-4">
          <button class="secondary-btn" onclick={handleTestConnectivity} disabled={!configEndpoint || testing}>
            {#if testing} Testing... {:else if testSuccess} <span class="success-text">✓ Success</span> {:else} Test Connectivity {/if}
          </button>
          <button class="primary-btn" onclick={handleCreate} disabled={!name || !configEndpoint}>
            Save Channel
          </button>
        </div>
      </div>

      <div class="panel list-panel">
        <div class="panel-title">Configured Channels</div>
        
        <div class="channel-list">
          {#each channels as channel}
            <div class="channel-card {channel.status.toLowerCase()}">
              <div class="card-header">
                <div class="title-row">
                  <span class="type-icon">{channel.type === 'Webhook' ? '🔗' : '📧'}</span>
                  <h3>{channel.name}</h3>
                </div>
                <button 
                  class="toggle-btn {channel.status.toLowerCase()}" 
                  onclick={() => toggleChannel(channel.id)}
                >
                  {channel.status}
                </button>
              </div>
              <div class="card-body">
                <div class="detail-row">
                  <span class="detail-label">Endpoint:</span>
                  <span class="detail-value truncate" title={channel.endpoint}>{channel.endpoint}</span>
                </div>
              </div>
            </div>
          {/each}
        </div>
      </div>
    </div>

    <!-- Right Column: Delivery History -->
    <div class="right-col">
      <div class="panel history-panel">
        <div class="panel-title">Delivery History</div>
        
        <div class="history-list">
          {#each deliveryHistory as log}
            <div class="history-item">
              <div class="history-status">
                {#if log.status === 'DELIVERED'}
                  <div class="status-dot success"></div>
                {:else}
                  <div class="status-dot error"></div>
                {/if}
              </div>
              <div class="history-content">
                <div class="history-header">
                  <span class="history-channel">{log.channelName}</span>
                  <span class="history-time">{new Date(log.timestamp).toLocaleTimeString()}</span>
                </div>
                <div class="history-ref" class:is-test={log.incidentRef === 'Test Connectivity Payload'}>
                  {log.incidentRef}
                </div>
                <div class="history-meta">
                  <span class="meta-status {log.status.toLowerCase()}">{log.status}</span>
                  <span class="meta-latency">Latency: {log.latency}</span>
                </div>
              </div>
            </div>
          {/each}
        </div>
      </div>
      
      <!-- Architectural Note -->
      <div class="architecture-note">
        <div class="note-title">Architectural Boundary</div>
        <p>Notifications are one-way dispatch mechanisms.</p>
        <p>A delivery failure <strong>does not</strong> affect the underlying Incident. Disabling a channel <strong>does not</strong> resolve active alerts. Sentinel will continue monitoring and correlating telemetry locally.</p>
      </div>
    </div>
  </div>
</div>

<style>
  .page { padding-bottom: 60px; max-width: 1200px; margin: 0 auto; display: flex; flex-direction: column; gap: 20px; }
  .header { display: flex; justify-content: space-between; align-items: flex-end; margin-bottom: 10px; }
  .header h2 { margin: 0 0 8px 0; font-size: 1.8rem; color: var(--text-primary); }
  .header p { margin: 0; color: var(--text-secondary); font-size: 0.95rem; }

  .dashboard-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 24px; align-items: start; }
  .left-col { display: flex; flex-direction: column; gap: 24px; }
  .right-col { display: flex; flex-direction: column; gap: 24px; }

  .panel { background: var(--card-bg); border: 1px solid var(--border-color); border-radius: 10px; padding: 24px; box-shadow: 0 4px 20px rgba(0,0,0,0.15); }
  .panel-title { font-size: 0.8rem; letter-spacing: 2px; text-transform: uppercase; color: var(--text-secondary); font-weight: 800; margin-bottom: 20px; border-bottom: 1px solid rgba(255,255,255,0.05); padding-bottom: 10px; }

  /* Form Styles */
  .form-group { display: flex; flex-direction: column; gap: 6px; margin-bottom: 16px; }
  label { font-size: 0.8rem; font-weight: 700; color: var(--text-secondary); text-transform: uppercase; letter-spacing: 0.5px; }
  .input { background: rgba(0,0,0,0.2); border: 1px solid rgba(255,255,255,0.1); border-radius: 6px; padding: 10px 12px; color: var(--text-primary); font-size: 0.95rem; transition: border-color 0.2s; font-family: inherit; }
  .input:focus { outline: none; border-color: var(--accent-cyan); }
  .input:disabled { opacity: 0.5; cursor: not-allowed; }
  select.input { appearance: none; background-image: url("data:image/svg+xml;charset=US-ASCII,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20width%3D%22292.4%22%20height%3D%22292.4%22%3E%3Cpath%20fill%3D%22%23FFFFFF%22%20d%3D%22M287%2069.4a17.6%2017.6%200%200%200-13-5.4H18.4c-5%200-9.3%201.8-12.9%205.4A17.6%2017.6%200%200%200%200%2082.2c0%205%201.8%209.3%205.4%2012.9l128%20127.9c3.6%203.6%207.8%205.4%2012.8%205.4s9.2-1.8%2012.8-5.4L287%2095c3.5-3.5%205.4-7.8%205.4-12.8%200-5-1.9-9.2-5.5-12.8z%22%2F%3E%3C%2Fsvg%3E"); background-repeat: no-repeat; background-position: right 12px top 50%; background-size: 10px auto; padding-right: 30px; }
  .mt-2 { margin-top: 8px; }
  .mt-4 { margin-top: 16px; }

  .checkbox-group { flex-direction: row; align-items: center; }
  .toggle-label { display: flex; align-items: center; gap: 8px; font-size: 0.9rem; text-transform: none; letter-spacing: normal; color: var(--text-primary); cursor: pointer; }
  .toggle-label input { cursor: pointer; width: 16px; height: 16px; accent-color: var(--accent-cyan); }

  .action-row { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .primary-btn { background: rgba(0,212,255,0.15); border: 1px solid var(--accent-cyan); color: var(--accent-cyan); padding: 12px 16px; border-radius: 6px; font-weight: 800; font-size: 0.85rem; letter-spacing: 1px; text-transform: uppercase; cursor: pointer; transition: all 0.2s; }
  .primary-btn:hover:not(:disabled) { background: rgba(0,212,255,0.25); box-shadow: 0 0 15px rgba(0,212,255,0.2); }
  .primary-btn:disabled { opacity: 0.5; border-color: rgba(255,255,255,0.1); color: var(--text-dim); cursor: not-allowed; }
  
  .secondary-btn { background: rgba(255,255,255,0.05); border: 1px solid rgba(255,255,255,0.1); color: var(--text-primary); padding: 12px 16px; border-radius: 6px; font-weight: 800; font-size: 0.85rem; letter-spacing: 1px; text-transform: uppercase; cursor: pointer; transition: all 0.2s; }
  .secondary-btn:hover:not(:disabled) { background: rgba(255,255,255,0.1); border-color: rgba(255,255,255,0.3); }
  .secondary-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .success-text { color: var(--accent-green); }

  /* Channel List */
  .channel-list { display: flex; flex-direction: column; gap: 12px; }
  .channel-card { background: rgba(255,255,255,0.02); border: 1px solid rgba(255,255,255,0.05); border-radius: 8px; padding: 16px; transition: all 0.2s; }
  .channel-card.enabled { border-left: 3px solid var(--accent-green); }
  .channel-card.disabled { border-left: 3px solid var(--text-dim); opacity: 0.7; }
  
  .card-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px; }
  .title-row { display: flex; align-items: center; gap: 8px; }
  .title-row h3 { margin: 0; font-size: 1rem; color: var(--text-primary); }
  .type-icon { font-size: 1.1rem; }
  
  .toggle-btn { padding: 4px 10px; border-radius: 4px; font-size: 0.7rem; font-weight: 800; letter-spacing: 1px; cursor: pointer; transition: all 0.2s; border: 1px solid transparent; }
  .toggle-btn.enabled { background: rgba(0, 255, 136, 0.1); color: var(--accent-green); border-color: rgba(0, 255, 136, 0.2); }
  .toggle-btn.disabled { background: rgba(255, 255, 255, 0.05); color: var(--text-dim); border-color: rgba(255, 255, 255, 0.1); }
  .toggle-btn:hover { filter: brightness(1.2); }

  .card-body { font-size: 0.85rem; }
  .detail-row { display: flex; gap: 8px; align-items: baseline; }
  .detail-label { color: var(--text-dim); font-weight: 700; text-transform: uppercase; font-size: 0.7rem; letter-spacing: 0.5px; }
  .detail-value { color: var(--text-secondary); font-family: monospace; }
  .truncate { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 250px; display: inline-block; vertical-align: bottom; }

  /* Delivery History */
  .history-panel { background: rgba(0,0,0,0.2); border: 1px solid rgba(255,255,255,0.03); }
  .history-list { display: flex; flex-direction: column; gap: 16px; }
  .history-item { display: flex; gap: 12px; }
  .history-status { padding-top: 4px; }
  .status-dot { width: 10px; height: 10px; border-radius: 50%; }
  .status-dot.success { background: var(--accent-green); box-shadow: 0 0 8px rgba(0,255,136,0.4); }
  .status-dot.error { background: var(--accent-red); box-shadow: 0 0 8px rgba(255,51,85,0.4); }
  
  .history-content { flex: 1; display: flex; flex-direction: column; gap: 4px; padding-bottom: 16px; border-bottom: 1px solid rgba(255,255,255,0.05); }
  .history-item:last-child .history-content { border-bottom: none; padding-bottom: 0; }
  
  .history-header { display: flex; justify-content: space-between; align-items: baseline; }
  .history-channel { font-size: 0.85rem; font-weight: 700; color: var(--text-secondary); }
  .history-time { font-size: 0.75rem; color: var(--text-dim); font-family: monospace; }
  
  .history-ref { font-size: 0.95rem; color: var(--text-primary); font-weight: 600; }
  .history-ref.is-test { color: var(--text-dim); font-style: italic; }
  
  .history-meta { display: flex; gap: 12px; font-size: 0.75rem; font-family: monospace; margin-top: 2px; }
  .meta-status { font-weight: 800; letter-spacing: 0.5px; }
  .meta-status.delivered { color: var(--accent-green); }
  .meta-status.failed { color: var(--accent-red); }
  .meta-latency { color: var(--text-dim); }

  /* Architectural Note */
  .architecture-note { background: rgba(255,51,85,0.05); border: 1px solid rgba(255,51,85,0.2); border-radius: 8px; padding: 20px; }
  .architecture-note .note-title { font-size: 0.8rem; font-weight: 800; color: var(--accent-red); text-transform: uppercase; letter-spacing: 1px; margin-bottom: 8px; }
  .architecture-note p { font-size: 0.85rem; color: var(--text-secondary); margin: 0 0 8px 0; line-height: 1.5; }
  .architecture-note p:last-child { margin-bottom: 0; }
  .architecture-note strong { color: var(--text-primary); }
</style>
