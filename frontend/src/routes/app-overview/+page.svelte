<script lang="ts">
  import { onMount } from 'svelte';
  import { 
    LayoutDashboard, Activity, Users, FileText, Database, 
    AlertTriangle, CheckCircle2, Info, Clock, ExternalLink 
  } from 'lucide-svelte';

  interface AppMetric {
    value: number;
    label: string;
    unit: string;
  }

  interface AppLogEvent {
    type: string;
    app: string;
    timestamp: string;
    level: number;
    line: string;
    matches: Record<string, any>;
  }

  interface AppLogStats {
    type: string;
    app: string;
    timestamp: string;
    requests_per_min: number;
    errors_per_min: number;
    auth_failures_per_min: number;
  }

  let apps = $state<Record<string, Record<string, AppMetric>>>({});
  let logs = $state<AppLogEvent[]>([]);
  let stats = $state<Record<string, AppLogStats>>({});
  let requestHistory = $state<number[]>(new Array(60).fill(0));
  let ws: WebSocket;
  let connected = $state(false);

  // Computed values for the SVG polyline
  let maxHistory = $derived(Math.max(10, ...requestHistory));
  let polylinePoints = $derived(
    requestHistory.map((val, i) => {
      const x = i * (540 / 59);
      const y = 120 - (val / maxHistory) * 120;
      return `${x},${y}`;
    }).join(' ')
  );

  function connect() {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    ws = new WebSocket(`${protocol}//${window.location.host}/ws`);

    ws.onopen = () => { connected = true; };
    ws.onclose = () => {
      connected = false;
      setTimeout(connect, 3000);
    };

    ws.onmessage = (msg) => {
      try {
        const data = JSON.parse(msg.data);
        
        if (data.type === 'app_metrics_update') {
          apps[data.app] = { ...apps[data.app], ...data.metrics };
        } else if (data.type === 'app_log_event') {
          logs = [data, ...logs].slice(0, 100);
        } else if (data.type === 'app_log_stats') {
          stats[data.app] = data;
          // Update rolling history for the charts
          requestHistory = [...requestHistory.slice(1), data.requests_per_min];
        }
      } catch (e) {
        console.error('WS parse error', e);
      }
    };
  }

  onMount(() => {
    connect();
    return () => ws?.close();
  });

  function getLevelColor(level: number): string {
    if (level >= 3) return 'text-red-400 border-red-500/30 bg-red-500/10';
    if (level >= 2) return 'text-yellow-400 border-yellow-500/30 bg-yellow-500/10';
    return 'text-zinc-400 border-zinc-500/30 bg-zinc-500/10';
  }

  function getLevelLabel(level: number): string {
    if (level >= 3) return 'ERROR';
    if (level >= 2) return 'WARN';
    return 'INFO';
  }
</script>

<div class="space-y-6 pb-12">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-bold tracking-tight text-white flex items-center gap-2">
        <LayoutDashboard class="w-8 h-8 text-indigo-400" />
        APP OVERVIEW <span class="text-xs font-mono text-indigo-500/50 align-top">v0.2.0</span>
      </h1>
      <p class="text-zinc-400 text-sm">Universal application monitoring and log aggregation</p>
    </div>
    <div class="flex items-center gap-2 px-3 py-1 bg-zinc-900 border border-zinc-800 rounded-full">
      <div class="w-2 h-2 rounded-full {connected ? 'bg-emerald-500 animate-pulse' : 'bg-red-500'}"></div>
      <span class="text-[10px] font-mono font-bold tracking-widest {connected ? 'text-emerald-500' : 'text-red-500'} uppercase">
        {connected ? 'REAL-TIME CONNECTED' : 'DISCONNECTED'}
      </span>
    </div>
  </div>

  <!-- App Cards Grid -->
  {#each Object.entries(apps) as [appName, metrics]}
    <div class="bg-zinc-900/50 border border-zinc-800 rounded-2xl overflow-hidden backdrop-blur-sm">
      <!-- Title Bar -->
      <div class="bg-zinc-800/50 px-6 py-4 border-b border-zinc-800 flex items-center justify-between">
        <div class="flex items-center gap-3">
          <div class="w-8 h-8 rounded-lg bg-indigo-500/20 flex items-center justify-center border border-indigo-500/30">
            <Activity class="w-4 h-4 text-indigo-400" />
          </div>
          <h2 class="text-lg font-bold text-white tracking-widest uppercase">{appName}</h2>
        </div>
        <div class="flex items-center gap-6">
          {#if stats[appName]}
            <div class="flex items-center gap-4 text-xs font-mono">
              <div class="flex flex-col items-end">
                <span class="text-zinc-500 text-[10px]">REQS/MIN</span>
                <span class="text-white font-bold">{stats[appName].requests_per_min}</span>
              </div>
              <div class="flex flex-col items-end">
                <span class="text-zinc-500 text-[10px]">ERRORS</span>
                <span class="text-red-400 font-bold">{stats[appName].errors_per_min}</span>
              </div>
              <div class="flex flex-col items-end">
                <span class="text-zinc-500 text-[10px]">AUTH FAILS</span>
                <span class="text-yellow-400 font-bold">{stats[appName].auth_failures_per_min}</span>
              </div>
            </div>
          {/if}
          <div class="h-8 w-px bg-zinc-800"></div>
          <button class="text-zinc-500 hover:text-white transition-colors">
            <ExternalLink class="w-4 h-4" />
          </button>
        </div>
      </div>

      <div class="p-6 grid grid-cols-1 lg:grid-cols-3 gap-8">
        <!-- Row 1: Key Metrics Gauges -->
        <div class="grid grid-cols-2 gap-4">
          {#each Object.entries(metrics) as [key, m]}
            <div class="bg-zinc-950/50 border border-zinc-800 p-4 rounded-xl relative group overflow-hidden">
               <!-- Decorative Gradient -->
               <div class="absolute inset-0 bg-gradient-to-br from-indigo-500/5 to-transparent pointer-events-none"></div>
               
               <div class="flex flex-col items-center">
                 <div class="text-[10px] font-bold text-zinc-500 tracking-widest uppercase mb-1">{m.label}</div>
                 <div class="text-2xl font-mono text-white font-bold">{m.value}</div>
                 <div class="text-[10px] font-mono text-zinc-600 uppercase mt-1">{m.unit}</div>
               </div>
            </div>
          {/each}
          
          {#if Object.keys(metrics).length === 0}
            <div class="col-span-2 flex flex-col items-center justify-center p-8 border border-zinc-800 border-dashed rounded-xl text-zinc-600 italic text-sm">
              Waiting for metrics...
            </div>
          {/if}
        </div>

        <!-- Row 2: Live Activity Stream -->
        <div class="lg:col-span-2 flex flex-col min-h-[140px]">
          <div class="flex items-center gap-2 mb-3 text-[10px] font-bold text-zinc-500 tracking-widest uppercase">
            <Clock class="w-3 h-3" />
            LIVE REQUEST RATE (60s)
          </div>
          <div class="flex-1 bg-zinc-950/50 border border-zinc-800 rounded-xl p-4 relative overflow-hidden">
            <svg class="w-full h-full" viewBox="0 0 540 120" preserveAspectRatio="none">
              <defs>
                <linearGradient id="areaGrad" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="0%" stop-color="#818cf8" stop-opacity="0.3" />
                  <stop offset="100%" stop-color="#818cf8" stop-opacity="0" />
                </linearGradient>
              </defs>
              <path 
                d={`M 0,120 ${polylinePoints} L 540,120 Z`}
                fill="url(#areaGrad)"
              />
              <polyline
                fill="none"
                stroke="#6366f1"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
                points={polylinePoints}
              />
            </svg>
            <!-- Grid Lines -->
            <div class="absolute inset-4 pointer-events-none border-b border-zinc-800/50"></div>
            <div class="absolute bottom-1 right-2 text-[8px] font-mono text-zinc-700">MAX: {maxHistory}/min</div>
          </div>
        </div>
      </div>
    </div>
  {:else}
    <div class="flex flex-col items-center justify-center py-20 border border-dashed border-zinc-800 rounded-3xl opacity-50">
      <Users class="w-12 h-12 text-zinc-700 mb-4" />
      <p class="text-zinc-500 font-mono text-sm tracking-widest uppercase">NO APPLICATIONS CONFIGURED</p>
    </div>
  {/each}

  {#if logs.length > 0}
    <!-- Log Section -->
    <div class="bg-zinc-900/50 border border-zinc-800 rounded-2xl overflow-hidden">
      <div class="bg-zinc-800/50 px-6 py-3 border-b border-zinc-800 flex items-center justify-between">
        <div class="flex items-center gap-2">
          <FileText class="w-4 h-4 text-purple-400" />
          <h3 class="text-xs font-bold text-zinc-400 tracking-widest uppercase">UNIFIED LOG STREAM</h3>
        </div>
        <button 
          onclick={() => logs = []}
          class="text-[10px] font-bold text-zinc-500 hover:text-white uppercase transition-colors"
        >
          CLEAR CONSOLE
        </button>
      </div>
      <div class="p-2 bg-black/40 font-mono text-[11px] h-[300px] overflow-y-auto custom-scrollbar">
        {#each logs as log (log.timestamp + log.line)}
          <div class="flex items-start gap-4 px-4 py-1.5 border-b border-zinc-800/30 hover:bg-zinc-800/20 transition-colors">
             <div class="min-w-[80px] text-zinc-600">{log.timestamp.split('T')[1].split('.')[0]}</div>
             <div class="min-w-[100px] text-indigo-500 font-bold uppercase text-[10px]">{log.app}</div>
             <div class="min-w-[60px]">
               <span class="px-1.5 py-0.5 rounded border text-[10px] font-bold {getLevelColor(log.level)}">
                 {getLevelLabel(log.level)}
               </span>
             </div>
             <div class="flex-1 text-zinc-300 break-all">{log.line}</div>
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .custom-scrollbar::-webkit-scrollbar {
    width: 6px;
  }
  .custom-scrollbar::-webkit-scrollbar-track {
    background: transparent;
  }
  .custom-scrollbar::-webkit-scrollbar-thumb {
    background: #27272a;
    border-radius: 10px;
  }
  .custom-scrollbar::-webkit-scrollbar-thumb:hover {
    background: #3f3f46;
  }
</style>
