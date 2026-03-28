import { writable } from 'svelte/store';

// ── Shared Cluster State Store ───────────────────────────────────
// This store is populated by the WebSocket connection in the dashboard
// and consumed by all sub-pages.

export interface NodeData {
  node: string;
  cpu: number;
  mem_used: number;
  mem_total: number;
  status: string;
}

export interface GuestData {
  vmid: number;
  name: string;
  node: string;
  type: string;
  status: string;
  cpu_usage: number;
  mem: number;
  maxmem: number;
  services: { name: string; status: string }[];
  disk_mounts: { mountpoint: string; total: number; used: number; use_pct: number }[];
}

export const nodes = writable<NodeData[]>([]);
export const guests = writable<GuestData[]>([]);
export const wsConnected = writable(false);
export const lastUpdate = writable('');

// Detail maps for LXC and VM
export const detailMap = writable<Record<number, any>>({});

// ── Utility ──────────────────────────────────────────────────────
export function formatBytes(bytes: number, decimals = 1): string {
  if (!+bytes) return '0 B';
  const k = 1024;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(dm))} ${sizes[i]}`;
}
