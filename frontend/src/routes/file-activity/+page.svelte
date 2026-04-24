<script lang="ts">
  import { onMount } from 'svelte';
  import { Terminal, ShieldAlert, FileText, Search, Activity, User, Globe, Hash } from 'lucide-svelte';

  interface FileEvent {
    type: string;
    timestamp: string;
    file: string;
    line: string;
    matches: {
      ip?: string;
      user?: string;
      method?: string;
      path?: string;
      status?: string;
      size?: string;
    };
  }

  let events = $state<FileEvent[]>([]);
  let ipFilter = $state('');
  let pathFilter = $state('');
  let ws: WebSocket;
  let connected = $state(false);

  // Reactive stats computed from the events array
  let totalRequests = $derived(events.length);
  let failedRequests = $derived(events.filter(e => parseInt(e.matches?.status || '0') >= 400).length);
  let largestFile = $derived(Math.max(0, ...events.map(e => parseInt(e.matches?.size || '0'))));
  
  let mostActiveIP = $derived.by(() => {
    if (events.length === 0) return 'N/A';
    const counts: Record<string, number> = {};
    events.forEach(e => {
      const ip = e.matches?.ip || 'unknown';
      counts[ip] = (counts[ip] || 0) + 1;
    });
    return Object.entries(counts).sort((a, b) => b[1] - a[1])[0][0];
  });

  let filteredEvents = $derived(
    events.filter(e => 
      (e.matches?.ip || '').includes(ipFilter) && 
      (e.matches?.path || '').toLowerCase().includes(pathFilter.toLowerCase())
    ).slice(0, 500) // limit for performance
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
        if (data.type === 'security_event') {
          events = [data, ...events].slice(0, 1000);
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

  function formatSize(bytes: string | undefined): string {
    if (!bytes) return '0 B';
    const b = parseInt(bytes);
    if (isNaN(b)) return bytes;
    if (b < 1024) return `${b} B`;
    if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)} KB`;
    return `${(b / (1024 * 1024)).toFixed(1)} MB`;
  }

  function getStatusColor(status: string | undefined): string {
    const s = parseInt(status || '0');
    if (s >= 500) return 'text-red-400 border-red-500/50 bg-red-500/10';
    if (s >= 300) return 'text-yellow-400 border-yellow-500/50 bg-yellow-500/10';
    if (s >= 200) return 'text-emerald-400 border-emerald-500/50 bg-emerald-500/10';
    return 'text-zinc-400 border-zinc-500/50 bg-zinc-500/10';
  }
</script>

<div class="space-y-6">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-bold tracking-tight text-white flex items-center gap-2">
        <ShieldAlert class="w-8 h-8 text-cyan-400" />
        FILE ACTIVITY <span class="text-xs font-mono text-cyan-500/50 align-top">v0.2.2</span>
      </h1>
      <p class="text-zinc-400 text-sm">Real-time security auditing and access patterns</p>
    </div>
    <div class="flex items-center gap-2 px-3 py-1 bg-zinc-900 border border-zinc-800 rounded-full">
      <div class="w-2 h-2 rounded-full {connected ? 'bg-emerald-500 animate-pulse' : 'bg-red-500'}"></div>
      <span class="text-[10px] font-mono font-bold tracking-widest {connected ? 'text-emerald-500' : 'text-red-500'} uppercase">
        {connected ? 'LIVE STREAM' : 'DISCONNECTED'}
      </span>
    </div>
  </div>

  <!-- Stats Grid -->
  <div class="grid grid-cols-1 md:grid-cols-4 gap-4">
    <div class="bg-zinc-900/50 border border-zinc-800 p-4 rounded-xl">
      <div class="flex items-center gap-3 mb-2">
        <Activity class="w-5 h-5 text-cyan-400" />
        <span class="text-xs font-bold text-zinc-500 tracking-wider">TOTAL REQUESTS</span>
      </div>
      <div class="text-3xl font-mono text-white font-bold">{totalRequests}</div>
    </div>
    
    <div class="bg-zinc-900/50 border border-zinc-800 p-4 rounded-xl">
      <div class="flex items-center gap-3 mb-2">
        <ShieldAlert class="w-5 h-5 text-red-400" />
        <span class="text-xs font-bold text-zinc-500 tracking-wider">FAILED (4xx/5xx)</span>
      </div>
      <div class="text-3xl font-mono text-red-400 font-bold">{failedRequests}</div>
    </div>

    <div class="bg-zinc-900/50 border border-zinc-800 p-4 rounded-xl">
      <div class="flex items-center gap-3 mb-2">
        <FileText class="w-5 h-5 text-purple-400" />
        <span class="text-xs font-bold text-zinc-500 tracking-wider">LARGEST FILE</span>
      </div>
      <div class="text-3xl font-mono text-white font-bold">{formatSize(largestFile.toString())}</div>
    </div>

    <div class="bg-zinc-900/50 border border-zinc-800 p-4 rounded-xl">
      <div class="flex items-center gap-3 mb-2">
        <Globe class="w-5 h-5 text-emerald-400" />
        <span class="text-xs font-bold text-zinc-500 tracking-wider">MOST ACTIVE IP</span>
      </div>
      <div class="text-xl font-mono text-emerald-400 font-bold truncate">{mostActiveIP}</div>
    </div>
  </div>

  <!-- Filters -->
  <div class="flex flex-col md:flex-row gap-4 bg-zinc-900/50 border border-zinc-800 p-3 rounded-xl">
    <div class="flex-1 relative">
      <Search class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-zinc-500" />
      <input 
        type="text" 
        bind:value={ipFilter}
        placeholder="Filter by IP..." 
        class="w-full bg-zinc-950 border border-zinc-800 rounded-lg py-2 pl-10 pr-4 text-sm text-white focus:outline-none focus:border-cyan-500/50 transition-colors"
      />
    </div>
    <div class="flex-1 relative">
      <FileText class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-zinc-500" />
      <input 
        type="text" 
        bind:value={pathFilter}
        placeholder="Search path..." 
        class="w-full bg-zinc-950 border border-zinc-800 rounded-lg py-2 pl-10 pr-4 text-sm text-white focus:outline-none focus:border-cyan-500/50 transition-colors"
      />
    </div>
    <button 
      onclick={() => events = []}
      class="px-4 py-2 bg-zinc-800 hover:bg-zinc-700 text-zinc-300 rounded-lg text-xs font-bold tracking-widest transition-colors border border-zinc-700"
    >
      CLEAR LOGS
    </button>
  </div>

  <!-- Table -->
  <div class="relative bg-zinc-900/50 border border-zinc-800 rounded-xl overflow-hidden">
    <div class="overflow-x-auto max-h-[600px] custom-scrollbar">
      <table class="w-full text-left border-collapse">
        <thead class="sticky top-0 bg-zinc-900 z-10">
          <tr class="text-[10px] font-bold text-zinc-500 tracking-widest uppercase border-b border-zinc-800">
            <th class="px-4 py-3 min-w-[140px]">Timestamp</th>
            <th class="px-4 py-3 min-w-[150px]">Source File</th>
            <th class="px-4 py-3 min-w-[120px]">IP</th>
            <th class="px-4 py-3 min-w-[100px]">User</th>
            <th class="px-4 py-3">Method</th>
            <th class="px-4 py-3 min-w-[300px]">Path</th>
            <th class="px-4 py-3">Size</th>
            <th class="px-4 py-3">Status</th>
          </tr>
        </thead>
        <tbody class="divide-y divide-zinc-800/50 font-mono text-[11px]">
          {#each filteredEvents as event (event.timestamp + event.line)}
            <tr class="hover:bg-cyan-500/5 transition-colors group">
              <td class="px-4 py-2.5 text-zinc-500">{event.timestamp.split('T')[1].split('.')[0]}</td>
              <td class="px-4 py-2.5 text-cyan-500/70 truncate max-w-[200px]">{event.file.split('/').pop()}</td>
              <td class="px-4 py-2.5 text-emerald-400">{event.matches?.ip || '-'}</td>
              <td class="px-4 py-2.5 text-purple-400">{event.matches?.user || '-'}</td>
              <td class="px-4 py-2.5">
                <span class="px-1.5 py-0.5 rounded bg-zinc-800 text-white border border-zinc-700">
                  {event.matches?.method || '-'}
                </span>
              </td>
              <td class="px-4 py-2.5 text-zinc-300 truncate max-w-md">{event.matches?.path || '-'}</td>
              <td class="px-4 py-2.5 text-zinc-400">{formatSize(event.matches?.size)}</td>
              <td class="px-4 py-2.5">
                <span class="px-2 py-0.5 rounded-full border text-[10px] font-bold {getStatusColor(event.matches?.status)}">
                  {event.matches?.status || '-'}
                </span>
              </td>
            </tr>
          {/each}
          
          {#if filteredEvents.length === 0}
            <tr>
              <td colspan="8" class="px-4 py-12 text-center text-zinc-600 font-medium">
                No events recorded. Waiting for incoming activity...
              </td>
            </tr>
          {/if}
        </tbody>
      </table>
    </div>
  </div>
</div>

<style>
  .custom-scrollbar::-webkit-scrollbar {
    width: 6px;
    height: 6px;
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
