import { derived, get, writable } from 'svelte/store';

export interface NodeData {
  node: string;
  cpu: number;
  mem_used: number;
  mem_total: number;
  swap_used?: number;
  swap_total?: number;
  disk_used?: number;
  disk_total?: number;
  status: string;
}

export interface ServiceData {
  name: string;
  status: string;
  load?: string;
  state?: string;
  sub_state?: string;
  description?: string;
  classification?: string;
  running?: boolean;
  failed?: boolean;
  ports?: string[];
}

export interface DiskMount {
  mountpoint: string;
  total: number;
  used: number;
  avail?: number;
  use_pct: number;
  fstype?: string;
}

export interface GuestData {
  vmid: number;
  id: number;
  name: string;
  node: string;
  type: 'LXC' | 'QEMU';
  status: string;
  cpu: number;
  maxcpu: number;
  mem: number;
  maxmem: number;
  os_name?: string | null;
  os_version?: string | null;
}

export interface GuestDiskSummary {
  total: number;
  used: number;
  free: number;
  used_pct: number;
  max_mount_pct: number;
  max_mount?: string;
  root_used_pct?: number;
  root_total?: number;
  root_used?: number;
  available: boolean;
}

export interface GuestDetail {
  services?: ServiceData[];
  service_total?: number;
  service_running?: number;
  service_failed?: number;
  disk_mounts?: DiskMount[];
  agent?: boolean;
  ssh?: boolean;
  ip?: string | null;
  os_name?: string | null;
  os_version?: string | null;
  mem_current?: number;
  mem_limit?: number;
  pids?: number;
}

export interface StoragePool {
  storage: string;
  node: string;
  type: string;
  content?: string;
  used: number;
  total: number;
  avail: number;
  active: boolean;
  enabled: boolean;
}

export interface LogEntry {
  time: string;
  level: string;
  source: string;
  message: string;
}

export const nodes = writable<NodeData[]>([]);
export const guests = writable<GuestData[]>([]);
export const detailMap = writable<Record<number, GuestDetail>>({});
export const storagePools = writable<StoragePool[]>([]);
export const haproxyStats = writable<any>(null);
export const logs = writable<LogEntry[]>([]);
export const securityEvents = writable<any[]>([]);
export const appMetrics = writable<Record<string, any>>({});
export const appLogEvents = writable<any[]>([]);
export const appLogStats = writable<Record<string, any>>({});
export const platformHealth = writable<any>({
  zfs: [],
  backups: [],
  tasks: [],
  cluster: null,
  ceph: null,
  thin_pools: [],
  snapshots: [],
  security: [],
  certificates: [],
});
export const wsConnected = writable(false);
export const reconnectAttempts = writable(0);
export const lastUpdate = writable('');

let socket: WebSocket | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let started = false;

export const enrichedGuests = derived([guests, detailMap], ([$guests, $detailMap]) =>
  $guests.map((guest) => {
    const detail = $detailMap[guest.vmid] || {};
    const services = detail.services || [];
    const runningServices = detail.service_running ?? services.filter(isServiceRunning).length;
    const failedServices = detail.service_failed ?? services.filter(isServiceFailed).length;
    return {
      ...guest,
      services,
      service_total: detail.service_total ?? services.length,
      service_running: runningServices,
      service_failed: failedServices,
      disk_mounts: detail.disk_mounts || [],
      disk_summary: summarizeGuestDisks(detail.disk_mounts || []),
      agent: detail.agent,
      ssh: detail.ssh,
      ip: detail.ip,
      os_name: detail.os_name ?? guest.os_name,
      os_version: detail.os_version ?? guest.os_version,
      mem_current: detail.mem_current,
      mem_limit: detail.mem_limit,
      pids: detail.pids,
    };
  })
);

export const guestDiskMounts = derived(enrichedGuests, ($guests) =>
  $guests.flatMap((guest) =>
    (guest.disk_mounts || []).map((mount) => ({
      ...mount,
      guest: guest.name,
      vmid: guest.vmid,
      node: guest.node,
      type: guest.type,
    }))
  )
);

export function initWebSocket() {
  if (started || typeof window === 'undefined') return;
  started = true;
  connect();
}

function connect() {
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  socket = new WebSocket(`${protocol}//${window.location.host}/ws`);

  socket.onopen = () => {
    wsConnected.set(true);
    reconnectAttempts.set(0);
    addUiLog('INFO', 'UI', 'Live telemetry connected');
  };

  socket.onclose = () => {
    wsConnected.set(false);
    const nextAttempt = get(reconnectAttempts) + 1;
    reconnectAttempts.set(nextAttempt);
    const backoff = Math.min(3000 * 2 ** Math.max(0, nextAttempt - 1), 30000);
    if (reconnectTimer) clearTimeout(reconnectTimer);
    reconnectTimer = setTimeout(connect, backoff);
  };

  socket.onmessage = (event) => {
    try {
      handleMessage(JSON.parse(event.data));
    } catch {
      addUiLog('WARN', 'UI', 'Received malformed telemetry event');
    }
  };
}

function handleMessage(payload: any) {
  const time = new Date().toLocaleTimeString('en-US', { hour12: false });

  if (payload.type === 'cluster_update') {
    nodes.set([...(payload.nodes || [])].sort((a, b) => String(a.node).localeCompare(String(b.node))));
    storagePools.set([...(payload.storage || [])].sort((a, b) => `${a.node}/${a.storage}`.localeCompare(`${b.node}/${b.storage}`)));
    guests.set((payload.guests || []).map(normalizeGuest).sort((a: GuestData, b: GuestData) => a.vmid - b.vmid));
    lastUpdate.set(payload.timestamp || new Date().toISOString());

    const running = (payload.guests || []).filter((g: any) => g.status === 'running').length;
    addLog(time, 'INFO', 'CLUSTER', `Polled ${(payload.nodes || []).length} nodes, ${(payload.guests || []).length} guests (${running} running)`);
    return;
  }

  if (payload.type === 'lxc_detail') {
    mergeDetails(payload.lxc || [], (item) => ({
      services: item.services || [],
      service_total: item.service_total,
      service_running: item.service_running,
      service_failed: item.service_failed,
      disk_mounts: item.disk_mounts || [],
      os_name: item.os_name,
      os_version: item.os_version,
      mem_current: item.mem_current,
      mem_limit: item.mem_limit,
      pids: item.pids,
    }));
    for (const lxc of payload.lxc || []) {
      addLog(time, 'INFO', `LXC-${lxc.vmid}`, `${lxc.name}: ${(lxc.services || []).length} services, ${(lxc.disk_mounts || []).length} disks`);
    }
    return;
  }

  if (payload.type === 'vm_detail') {
    mergeDetails(payload.vms || [], (item) => ({
      services: item.services || [],
      service_total: item.service_total,
      service_running: item.service_running,
      service_failed: item.service_failed,
      disk_mounts: item.disk_mounts || [],
      agent: item.agent,
      ssh: item.ssh,
      ip: item.ip,
      os_name: item.os_name,
      os_version: item.os_version,
    }));
    for (const vm of payload.vms || []) {
      const via = vm.agent ? 'AGENT' : vm.ssh ? 'SSH' : 'NONE';
      addLog(time, 'INFO', `VM-${vm.vmid}`, `${vm.name}: via ${via}, ${(vm.services || []).length} services`);
    }
    return;
  }

  if (payload.type === 'haproxy_update') {
    haproxyStats.set(payload);
    return;
  }

  if (payload.type === 'log_line') {
    addLog(time, payload.severity?.toUpperCase?.() || 'INFO', payload.source || 'LOG', payload.line || '');
    return;
  }

  if (payload.type === 'security_event') {
    securityEvents.update((items) => [payload, ...items].slice(0, 1000));
    return;
  }

  if (payload.type === 'app_metrics_update') {
    appMetrics.update((apps) => ({
      ...apps,
      [payload.app]: { ...(apps[payload.app] || {}), ...(payload.metrics || {}) },
    }));
    return;
  }

  if (payload.type === 'app_log_event') {
    appLogEvents.update((items) => [payload, ...items].slice(0, 200));
    return;
  }

  if (payload.type === 'app_log_stats') {
    appLogStats.update((stats) => ({ ...stats, [payload.app]: payload }));
    return;
  }

  if (payload.type === 'platform_health') {
    platformHealth.set(payload);
  }
}

function normalizeGuest(guest: any): GuestData {
  return {
    vmid: guest.vmid,
    id: guest.vmid,
    name: guest.name,
    node: guest.node,
    type: guest.type === 'lxc' ? 'LXC' : 'QEMU',
    status: guest.status,
    cpu: guest.cpu || 0,
    maxcpu: guest.maxcpu || 0,
    mem: guest.mem || 0,
    maxmem: guest.maxmem || 0,
    os_name: guest.os_name || null,
    os_version: guest.os_version || null,
  };
}

function mergeDetails(items: any[], projector: (item: any) => GuestDetail) {
  detailMap.update((current) => {
    const next = { ...current };
    for (const item of items) {
      next[item.vmid] = { ...(next[item.vmid] || {}), ...projector(item) };
    }
    return next;
  });
}

function addUiLog(level: string, source: string, message: string) {
  addLog(new Date().toLocaleTimeString('en-US', { hour12: false }), level, source, message);
}

function addLog(time: string, level: string, source: string, message: string) {
  logs.update((items) => [...items.slice(-300), { time, level, source, message }]);
}

export function clearLogs() {
  logs.set([]);
}

export function clearSecurityEvents() {
  securityEvents.set([]);
}

const APPLICATION_CLASSES = new Set(['web', 'php', 'database', 'container', 'proxy/lb', 'monitoring']);
const IMPORTANT_SERVICE_ORDER = [
  'apache2',
  'nginx',
  'caddy',
  'traefik',
  'php8.3-fpm',
  'php8.2-fpm',
  'php8.1-fpm',
  'php-fpm',
  'postgresql',
  'mariadb',
  'mysql',
  'redis',
  'redis-server',
  'mongodb',
  'mongod',
  'haproxy',
  'keepalived',
  'docker',
  'containerd',
  'podman',
  'ssh',
  'sshd',
];

export function normalizeServiceName(name: string): string {
  return String(name || '')
    .trim()
    .replace(/\.service$/i, '')
    .toLowerCase();
}

export function isServiceRunning(service: ServiceData): boolean {
  if (typeof service.running === 'boolean') return service.running;
  const status = String(service.status || '').toLowerCase();
  const active = String(service.state || '').toLowerCase();
  const sub = String(service.sub_state || '').toLowerCase();
  return status === 'running' || (active === 'active' && sub === 'running');
}

export function isServiceFailed(service: ServiceData): boolean {
  if (typeof service.failed === 'boolean') return service.failed;
  const status = String(service.status || '').toLowerCase();
  const active = String(service.state || '').toLowerCase();
  const sub = String(service.sub_state || '').toLowerCase();
  return status === 'failed' || active === 'failed' || sub === 'failed';
}

export function serviceClassification(service: ServiceData): string {
  const existing = String(service.classification || '').toLowerCase();
  if (existing) return existing;
  const name = normalizeServiceName(service.name);
  if (['apache2', 'nginx', 'caddy', 'traefik'].includes(name)) return 'web';
  if (name.startsWith('php') && name.includes('fpm')) return 'php';
  if (['mysql', 'mariadb', 'postgresql', 'redis', 'redis-server', 'mongodb', 'mongod'].includes(name)) return 'database';
  if (['docker', 'containerd', 'podman'].includes(name)) return 'container';
  if (['haproxy', 'keepalived'].includes(name)) return 'proxy/lb';
  if (['prometheus', 'grafana', 'node_exporter', 'node-exporter', 'zabbix-agent'].includes(name)) return 'monitoring';
  if (
    name.startsWith('systemd-') ||
    ['dbus', 'dbus-broker', 'cron', 'crond', 'rsyslog', 'qemu-guest-agent', 'fwupd', 'fwupd-refresh', 'getty', 'serial-getty', 'chronyd', 'networkmanager'].includes(name)
  ) return 'system';
  return 'other';
}

export function isApplicationService(service: ServiceData): boolean {
  return APPLICATION_CLASSES.has(serviceClassification(service));
}

function importantServiceIndex(service: ServiceData): number {
  const name = normalizeServiceName(service.name);
  const exact = IMPORTANT_SERVICE_ORDER.indexOf(name);
  if (exact >= 0) return exact;
  if (/^php\d+(\.\d+)?-fpm$/.test(name)) return IMPORTANT_SERVICE_ORDER.indexOf('php8.3-fpm');
  return -1;
}

export function servicePriority(service: ServiceData): number {
  if (isServiceFailed(service)) return 0;
  const important = importantServiceIndex(service);
  if (isServiceRunning(service) && important >= 0) return 10 + important;
  if (isServiceRunning(service) && isApplicationService(service)) return 50;
  if (important >= 0) return 70 + important;
  if (isServiceRunning(service) && serviceClassification(service) !== 'system') return 100;
  if (isServiceRunning(service)) return 130;
  return 180;
}

export function summarizeGuestDisks(mounts: DiskMount[]): GuestDiskSummary {
  const real = (mounts || []).filter((mount) => mount.total > 0);
  if (real.length === 0) {
    return { total: 0, used: 0, free: 0, used_pct: 0, max_mount_pct: 0, available: false };
  }
  const total = real.reduce((sum, mount) => sum + mount.total, 0);
  const used = real.reduce((sum, mount) => sum + mount.used, 0);
  const free = real.reduce((sum, mount) => sum + (mount.avail ?? Math.max(0, mount.total - mount.used)), 0);
  const max = [...real].sort((a, b) => (b.use_pct || 0) - (a.use_pct || 0))[0];
  const root = real.find((mount) => mount.mountpoint === '/');
  return {
    total,
    used,
    free,
    used_pct: pct(used, total),
    max_mount_pct: max?.use_pct || 0,
    max_mount: max?.mountpoint,
    root_used_pct: root?.use_pct,
    root_total: root?.total,
    root_used: root?.used,
    available: true,
  };
}

export function diskSummaryLabel(summary?: GuestDiskSummary): string {
  if (!summary?.available) return 'storage unavailable';
  const percent = summary.root_used_pct ?? summary.used_pct;
  const total = summary.root_total ?? summary.total;
  if (summary.max_mount && summary.max_mount !== '/' && summary.max_mount_pct > percent + 10) {
    return `total ${Math.round(summary.used_pct)}%, max ${summary.max_mount} ${Math.round(summary.max_mount_pct)}%`;
  }
  return `${percent.toFixed(1)}% / ${formatBytes(total)}`;
}

export function sortedServices(services: ServiceData[]): ServiceData[] {
  return [...(services || [])].sort((a, b) => {
    const priority = servicePriority(a) - servicePriority(b);
    if (priority !== 0) return priority;
    return normalizeServiceName(a.name).localeCompare(normalizeServiceName(b.name));
  });
}

export function previewServices(services: ServiceData[], limit = 8): ServiceData[] {
  return sortedServices(services).slice(0, limit);
}

export function serviceDisplayStatus(service: ServiceData): string {
  if (isServiceFailed(service)) return 'failed';
  if (isServiceRunning(service)) return 'running';
  if (service.load === 'not-found') return 'not-found';
  return service.sub_state || service.status || service.state || 'unknown';
}

export function formatBytes(bytes: number, decimals = 1): string {
  if (!+bytes) return '0 B';
  const k = 1024;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(k)), sizes.length - 1);
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(dm))} ${sizes[i]}`;
}

export function pct(value: number, total: number): number {
  if (!total || total <= 0) return 0;
  return Math.min(100, Math.max(0, (value / total) * 100));
}
