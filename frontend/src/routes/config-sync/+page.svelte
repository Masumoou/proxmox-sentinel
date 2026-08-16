<script lang="ts">
  // State for Import Flow
  let importStage = $state('upload'); // upload, preview, confirming, success
  let fileError = $state('');
  
  // Preview State
  let importPreview = $state({
    format: '',
    version: 0,
    changes: {
      added: 0,
      modified: 0,
      removed: 0
    },
    details: [] as Array<{ action: string, type: string, name: string }>
  });

  function handleExport() {
    const mockExport = {
      format: 'proxmox-sentinel-config',
      version: 1,
      exported_at: new Date().toISOString(),
      monitors: [],
      rules: [],
      templates: [],
      notification_channels: [],
      maintenance_windows: []
    };
    
    const blob = new Blob([JSON.stringify(mockExport, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `sentinel-config-${new Date().toISOString().slice(0,10)}.json`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  }

  function handleFileUpload(event: Event) {
    const input = event.target as HTMLInputElement;
    if (!input.files || input.files.length === 0) return;
    
    const file = input.files[0];
    const reader = new FileReader();
    
    reader.onload = (e) => {
      try {
        const content = JSON.parse(e.target?.result as string);
        
        // Schema Validation
        if (content.format !== 'proxmox-sentinel-config') {
          fileError = 'Invalid file format. Expected proxmox-sentinel-config.';
          return;
        }
        if (content.version > 1) {
          fileError = `Unsupported version ${content.version}. This Sentinel instance supports up to version 1.`;
          return;
        }

        // Mock Preview Generation
        fileError = '';
        importPreview = {
          format: content.format,
          version: content.version,
          changes: { added: 12, modified: 3, removed: 1 },
          details: [
            { action: '+', type: 'Rule', name: 'nginx != running' },
            { action: '+', type: 'NotificationChannel', name: 'Slack Alerts Webhook' },
            { action: '~', type: 'Monitor', name: 'VM-101 redis.service' },
            { action: '-', type: 'MaintenanceWindow', name: 'Weekend Patching (mw-12)' }
          ]
        };
        importStage = 'preview';
      } catch (err) {
        fileError = 'Failed to parse JSON file.';
      }
    };
    
    reader.readAsText(file);
    // reset input so the same file can be uploaded again if canceled
    input.value = '';
  }

  function applyConfiguration() {
    importStage = 'confirming';
    // Simulate backend processing
    setTimeout(() => {
      importStage = 'success';
      setTimeout(() => importStage = 'upload', 3000);
    }, 1500);
  }

  function cancelImport() {
    importStage = 'upload';
    fileError = '';
  }
</script>

<div class="page">
  <div class="header">
    <div>
      <h2>Configuration Sync</h2>
      <p>Export or import monitoring state, rules, and maintenance definitions.</p>
    </div>
  </div>

  <!-- Architectural Integrity Note -->
  <div class="architecture-note mb-6">
    <div class="note-title">Architectural Boundary: Operational History vs. Configuration</div>
    <div class="note-grid">
      <div class="note-col">
        <h4>Included in Export/Import</h4>
        <ul>
          <li>Resource Monitoring toggles</li>
          <li>Rules & Templates</li>
          <li>Notification Channels</li>
          <li>Maintenance Windows</li>
        </ul>
      </div>
      <div class="note-divider"></div>
      <div class="note-col">
        <h4>Strictly Excluded</h4>
        <ul>
          <li>Telemetry databases</li>
          <li>Alerts & Incidents (Open or Resolved)</li>
          <li>Delivery / Discovery Audit history</li>
        </ul>
        <p class="warning">Importing a configuration will NEVER delete historical incidents or alerts, even if the rule that generated them is removed.</p>
      </div>
    </div>
  </div>

  <div class="dashboard-grid">
    <!-- Export Panel -->
    <div class="panel">
      <div class="panel-title">Export Configuration</div>
      <p class="panel-desc">Download the current configuration state as a portable JSON file. This acts as a declarative backup of how Sentinel is configured to monitor your infrastructure.</p>
      
      <div class="format-badge">
        <span class="label">Format:</span> proxmox-sentinel-config 
        <span class="label ml-2">Version:</span> 1
      </div>

      <button class="primary-btn mt-4" onclick={handleExport}>
        <span class="icon">↓</span> Download Configuration
      </button>
    </div>

    <!-- Import Panel -->
    <div class="panel">
      <div class="panel-title">Import Configuration</div>
      
      {#if importStage === 'upload'}
        <p class="panel-desc">Upload a valid Sentinel configuration JSON file. You will be able to preview the changes before they are applied.</p>
        
        <div class="upload-zone">
          <input type="file" id="file-upload" accept=".json" onchange={handleFileUpload} />
          <label for="file-upload" class="upload-label">
            <span class="icon">↑</span> Select JSON File
          </label>
        </div>

        {#if fileError}
          <div class="error-msg">⚠️ {fileError}</div>
        {/if}

      {:else if importStage === 'preview'}
        <div class="preview-box">
          <div class="preview-header">
            <h4>Configuration Preview</h4>
            <div class="version-tag">v{importPreview.version}</div>
          </div>
          
          <div class="change-summary">
            <div class="change-item added"><span class="num">+{importPreview.changes.added}</span> New</div>
            <div class="change-item modified"><span class="num">~{importPreview.changes.modified}</span> Modified</div>
            <div class="change-item removed"><span class="num">-{importPreview.changes.removed}</span> Removed</div>
          </div>

          <div class="change-details">
            {#each importPreview.details as detail}
              <div class="detail-row">
                {#if detail.action === '+'}
                  <span class="action added">+</span>
                {:else if detail.action === '~'}
                  <span class="action modified">~</span>
                {:else}
                  <span class="action removed">-</span>
                {/if}
                <span class="type">{detail.type}</span>
                <span class="name">{detail.name}</span>
              </div>
            {/each}
            <div class="detail-row more">...and {importPreview.changes.added + importPreview.changes.modified + importPreview.changes.removed - importPreview.details.length} more changes</div>
          </div>

          <div class="action-row mt-4">
            <button class="secondary-btn" onclick={cancelImport}>Cancel</button>
            <button class="primary-btn apply" onclick={applyConfiguration}>Apply Configuration</button>
          </div>
        </div>

      {:else if importStage === 'confirming'}
        <div class="loading-state">
          <div class="spinner"></div>
          <p>Validating and applying configuration...</p>
        </div>

      {:else if importStage === 'success'}
        <div class="success-state">
          <div class="success-icon">✓</div>
          <p>Configuration applied successfully.</p>
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .page { padding-bottom: 60px; max-width: 1100px; margin: 0 auto; display: flex; flex-direction: column; gap: 20px; }
  .header { display: flex; justify-content: space-between; align-items: flex-end; margin-bottom: 10px; }
  .header h2 { margin: 0 0 8px 0; font-size: 1.8rem; color: var(--text-primary); }
  .header p { margin: 0; color: var(--text-secondary); font-size: 0.95rem; }
  .mb-6 { margin-bottom: 24px; }
  .mt-4 { margin-top: 16px; }
  .ml-2 { margin-left: 12px; }

  /* Architectural Boundary Note */
  .architecture-note { background: rgba(0,0,0,0.3); border: 1px solid rgba(255,255,255,0.05); border-radius: 8px; padding: 20px; }
  .note-title { font-size: 0.8rem; font-weight: 800; color: var(--text-secondary); text-transform: uppercase; letter-spacing: 1px; margin-bottom: 16px; border-bottom: 1px solid rgba(255,255,255,0.05); padding-bottom: 8px; }
  .note-grid { display: grid; grid-template-columns: 1fr 1px 1fr; gap: 30px; }
  .note-divider { background: rgba(255,255,255,0.05); }
  
  .note-col h4 { margin: 0 0 12px 0; font-size: 0.95rem; color: var(--text-primary); }
  .note-col ul { margin: 0; padding-left: 20px; color: var(--text-dim); font-size: 0.9rem; line-height: 1.6; }
  .note-col .warning { margin: 16px 0 0 0; font-size: 0.8rem; font-weight: 700; color: var(--accent-orange); background: rgba(255,170,0,0.1); padding: 8px 12px; border-radius: 6px; border-left: 2px solid var(--accent-orange); line-height: 1.4; }

  /* Dashboard Grid */
  .dashboard-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 24px; align-items: start; }
  .panel { background: var(--card-bg); border: 1px solid var(--border-color); border-radius: 10px; padding: 24px; box-shadow: 0 4px 20px rgba(0,0,0,0.15); display: flex; flex-direction: column; }
  .panel-title { font-size: 0.8rem; letter-spacing: 2px; text-transform: uppercase; color: var(--text-secondary); font-weight: 800; margin-bottom: 12px; }
  .panel-desc { font-size: 0.9rem; color: var(--text-secondary); line-height: 1.5; margin-bottom: 20px; }

  .format-badge { background: rgba(255,255,255,0.03); border: 1px solid rgba(255,255,255,0.05); border-radius: 6px; padding: 12px; font-family: monospace; font-size: 0.85rem; color: var(--text-primary); margin-bottom: auto; }
  .format-badge .label { color: var(--text-dim); font-weight: bold; text-transform: uppercase; font-size: 0.75rem; font-family: system-ui, sans-serif; }

  /* Buttons */
  .primary-btn { background: rgba(0,212,255,0.15); border: 1px solid var(--accent-cyan); color: var(--accent-cyan); padding: 12px 16px; border-radius: 6px; font-weight: 800; font-size: 0.85rem; letter-spacing: 1px; text-transform: uppercase; cursor: pointer; transition: all 0.2s; display: flex; align-items: center; justify-content: center; gap: 8px; }
  .primary-btn:hover { background: rgba(0,212,255,0.25); box-shadow: 0 0 15px rgba(0,212,255,0.2); }
  
  .primary-btn.apply { background: rgba(0,255,136,0.15); border-color: var(--accent-green); color: var(--accent-green); }
  .primary-btn.apply:hover { background: rgba(0,255,136,0.25); box-shadow: 0 0 15px rgba(0,255,136,0.2); }

  .secondary-btn { background: rgba(255,255,255,0.05); border: 1px solid rgba(255,255,255,0.1); color: var(--text-primary); padding: 12px 16px; border-radius: 6px; font-weight: 800; font-size: 0.85rem; letter-spacing: 1px; text-transform: uppercase; cursor: pointer; transition: all 0.2s; text-align: center; }
  .secondary-btn:hover { background: rgba(255,255,255,0.1); border-color: rgba(255,255,255,0.3); }

  /* Upload Zone */
  .upload-zone { margin-bottom: auto; }
  .upload-zone input[type="file"] { display: none; }
  .upload-label { display: flex; align-items: center; justify-content: center; gap: 8px; background: rgba(255,255,255,0.02); border: 2px dashed rgba(255,255,255,0.1); border-radius: 8px; padding: 40px 20px; color: var(--text-primary); font-weight: 700; cursor: pointer; transition: all 0.2s; text-transform: uppercase; letter-spacing: 1px; font-size: 0.85rem; }
  .upload-label:hover { background: rgba(255,255,255,0.05); border-color: var(--accent-cyan); color: var(--accent-cyan); }
  .upload-label .icon { font-size: 1.2rem; }
  
  .error-msg { margin-top: 16px; color: var(--accent-red); font-size: 0.85rem; font-weight: 600; background: rgba(255,51,85,0.1); padding: 12px; border-radius: 6px; }

  /* Preview Box */
  .preview-box { background: rgba(0,0,0,0.2); border: 1px solid rgba(255,255,255,0.05); border-radius: 8px; padding: 20px; }
  .preview-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px; }
  .preview-header h4 { margin: 0; color: var(--text-primary); font-size: 0.95rem; }
  .version-tag { font-family: monospace; font-size: 0.75rem; background: rgba(255,255,255,0.1); padding: 2px 6px; border-radius: 4px; color: var(--text-dim); }

  .change-summary { display: flex; gap: 16px; margin-bottom: 20px; border-bottom: 1px solid rgba(255,255,255,0.05); padding-bottom: 16px; }
  .change-item { font-size: 0.8rem; font-weight: 700; color: var(--text-secondary); text-transform: uppercase; }
  .change-item .num { font-size: 1.2rem; font-weight: 900; margin-right: 4px; }
  .change-item.added .num { color: var(--accent-green); }
  .change-item.modified .num { color: var(--accent-cyan); }
  .change-item.removed .num { color: var(--accent-red); }

  .change-details { display: flex; flex-direction: column; gap: 8px; font-family: monospace; font-size: 0.85rem; }
  .detail-row { display: flex; align-items: center; gap: 12px; background: rgba(255,255,255,0.02); padding: 6px 10px; border-radius: 4px; }
  .detail-row.more { color: var(--text-dim); font-style: italic; background: transparent; padding-left: 24px; }
  .action { font-weight: bold; width: 12px; text-align: center; }
  .action.added { color: var(--accent-green); }
  .action.modified { color: var(--accent-cyan); }
  .action.removed { color: var(--accent-red); }
  .type { color: var(--text-dim); font-weight: 600; width: 140px; }
  .name { color: var(--text-primary); }

  .action-row { display: grid; grid-template-columns: 1fr 1.5fr; gap: 12px; }

  /* States */
  .loading-state { display: flex; flex-direction: column; align-items: center; justify-content: center; padding: 40px 0; gap: 16px; color: var(--text-secondary); }
  .spinner { width: 30px; height: 30px; border: 3px solid rgba(255,255,255,0.1); border-top-color: var(--accent-cyan); border-radius: 50%; animation: spin 1s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  .success-state { display: flex; flex-direction: column; align-items: center; justify-content: center; padding: 40px 0; gap: 16px; color: var(--accent-green); font-weight: 700; }
  .success-icon { width: 40px; height: 40px; background: rgba(0,255,136,0.1); border: 2px solid var(--accent-green); border-radius: 50%; display: flex; align-items: center; justify-content: center; font-size: 1.2rem; }
</style>
