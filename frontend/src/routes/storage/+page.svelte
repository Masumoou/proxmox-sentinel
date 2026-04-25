<script lang="ts">
  import { formatBytes, guestDiskMounts, pct, platformHealth, storagePools, wsConnected } from '$lib/store';

  let zfs = $derived($platformHealth.zfs || []);
  let thinPools = $derived($platformHealth.thin_pools || []);
  let ceph = $derived($platformHealth.ceph);
</script>

<div class="page">
  <h2 class="page-title">STORAGE OVERVIEW</h2>

  <section class="panel">
    <div class="section-head">
      <span>Proxmox Storage Pools</span>
      <small>{$storagePools.length} pools</small>
    </div>
    {#if $storagePools.length === 0}
      <div class="empty">{$wsConnected ? 'WAITING FOR PROXMOX STORAGE DATA...' : 'CONNECTING...'}</div>
    {:else}
      <div class="storage-grid">
        {#each $storagePools as pool (`${pool.node}-${pool.storage}`)}
          <article class="pool-card" class:inactive={!pool.active || !pool.enabled}>
            <div class="pool-head">
              <div>
                <h3>{pool.storage}</h3>
                <p>{pool.node} · {pool.type}</p>
              </div>
              <span class:ok={pool.active && pool.enabled} class:bad={!pool.active || !pool.enabled}>{pool.active && pool.enabled ? 'ACTIVE' : 'DOWN'}</span>
            </div>
            <div class="usage">
              <strong>{Math.round(pct(pool.used, pool.total))}%</strong>
              <div class="bar"><div style="width:{Math.round(pct(pool.used, pool.total))}%"></div></div>
              <small>{formatBytes(pool.used)} used · {formatBytes(pool.avail)} free · {formatBytes(pool.total)} total</small>
            </div>
            <div class="content">{pool.content || 'no content metadata'}</div>
          </article>
        {/each}
      </div>
    {/if}
  </section>

  <section class="health-grid">
    <div class="panel">
      <div class="section-head"><span>ZFS Pools</span><small>{zfs.length} pools</small></div>
      {#if zfs.length === 0}
        <div class="hint">No ZFS pools detected on this node.</div>
      {:else}
        {#each zfs as pool (pool.name)}
          <div class="health-row" class:bad={pool.state !== 'ONLINE'}>
            <b>{pool.name}</b>
            <span>{pool.state}</span>
            <span>{pool.capacity_pct}% used</span>
            <small>{pool.scrub}</small>
          </div>
        {/each}
      {/if}
    </div>

    <div class="panel">
      <div class="section-head"><span>LVM Thin Pools</span><small>{thinPools.length} pools</small></div>
      {#if thinPools.length === 0}
        <div class="hint">No LVM-thin metadata data detected yet.</div>
      {:else}
        {#each thinPools as pool (`${pool.vg}/${pool.lv}`)}
          <div class="health-row" class:bad={pool.status === 'critical'} class:warn={pool.status === 'warning'}>
            <b>{pool.vg}/{pool.lv}</b>
            <span>data {pool.data_pct}%</span>
            <span>meta {pool.meta_pct}%</span>
            <small>{pool.status}</small>
          </div>
        {/each}
      {/if}
    </div>

    <div class="panel">
      <div class="section-head"><span>Ceph</span><small>{ceph?.installed ? ceph.health : 'not installed'}</small></div>
      {#if !ceph?.installed}
        <div class="hint">Ceph command unavailable or not configured.</div>
      {:else}
        <div class="health-row" class:bad={ceph.health !== 'HEALTH_OK'}>
          <b>{ceph.health}</b>
          <span>OSD {ceph.osd_up ?? '--'}/{ceph.osd_total ?? '--'}</span>
          <span>MON {ceph.mons?.join(', ') || '--'}</span>
          <small>{ceph.detail || 'no detail'}</small>
        </div>
      {/if}
    </div>
  </section>

  <section class="panel">
    <div class="section-head">
      <span>Guest Filesystems</span>
      <small>{$guestDiskMounts.length} mounts</small>
    </div>
    {#if $guestDiskMounts.length === 0}
      <div class="hint">Guest mount data requires QEMU Guest Agent or SSH for VMs. LXC mount data is available when containers exist on the monitored host.</div>
    {:else}
      <div class="mount-table">
        <div class="table-head"><span>Guest</span><span>Mount</span><span>Node</span><span>Used</span><span>Total</span><span>Usage</span></div>
        {#each $guestDiskMounts as mount (`${mount.vmid}-${mount.mountpoint}`)}
          <div class="table-row">
            <span>{mount.guest} <small>({mount.vmid})</small></span>
            <span class="mono">{mount.mountpoint}</span>
            <span>{mount.node}</span>
            <span>{formatBytes(mount.used)}</span>
            <span>{formatBytes(mount.total)}</span>
            <span class:bad={mount.use_pct > 90}>{mount.use_pct.toFixed(0)}%</span>
          </div>
        {/each}
      </div>
    {/if}
  </section>
</div>

<style>
  .page { display: flex; flex-direction: column; gap: 16px; }
  .page-title { font-size: 0.85rem; letter-spacing: 3px; color: var(--text-secondary); font-weight: 800; }
  .panel { background: var(--card-bg); border: 1px solid var(--border-color); border-radius: 8px; padding: 16px; }
  .section-head { display: flex; justify-content: space-between; align-items: center; margin-bottom: 14px; color: var(--text-primary); font-weight: 800; letter-spacing: 1.5px; text-transform: uppercase; font-size: 0.7rem; }
  .section-head small { color: var(--text-secondary); }
  .storage-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 12px; }
  .health-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(320px, 1fr)); gap: 16px; }
  .health-row { display: grid; grid-template-columns: 1fr auto auto; gap: 10px; align-items: center; padding: 10px 0; border-bottom: 1px solid rgba(255,255,255,0.05); font-size: 0.74rem; }
  .health-row small { grid-column: 1 / -1; overflow-wrap: anywhere; }
  .health-row b { color: var(--text-primary); }
  .health-row span { color: var(--text-secondary); }
  .pool-card { min-height: 174px; border: 1px solid rgba(255,255,255,0.08); border-radius: 8px; padding: 14px; display: flex; flex-direction: column; gap: 14px; }
  .pool-card.inactive { border-color: rgba(255,51,85,0.35); }
  .pool-head { display: flex; justify-content: space-between; gap: 12px; align-items: flex-start; }
  h3 { font-size: 1rem; overflow-wrap: anywhere; }
  p, small, .content, .hint, .empty { color: var(--text-secondary); }
  p { font-size: 0.68rem; margin-top: 4px; }
  .ok { color: var(--accent-green); }
  .bad { color: var(--accent-red); }
  .usage strong { color: var(--accent-cyan); font-size: 1.2rem; }
  .bar { margin: 8px 0; height: 7px; border-radius: 999px; background: rgba(255,255,255,0.07); overflow: hidden; }
  .bar div { height: 100%; background: var(--accent-cyan); border-radius: inherit; }
  .content { font-size: 0.64rem; line-height: 1.35; min-height: 28px; }
  .mount-table { overflow-x: auto; }
  .table-head, .table-row { min-width: 860px; display: grid; grid-template-columns: 1.3fr 1.4fr 1fr 0.8fr 0.8fr 0.6fr; gap: 12px; padding: 9px 10px; align-items: center; }
  .table-head { color: var(--text-secondary); font-size: 0.58rem; letter-spacing: 2px; text-transform: uppercase; border-bottom: 1px solid var(--border-color); }
  .table-row { border-bottom: 1px solid rgba(255,255,255,0.04); font-size: 0.74rem; }
  .mono { font-family: 'Courier New', monospace; color: var(--text-secondary); }
  .hint, .empty { min-height: 120px; display: grid; place-items: center; text-align: center; letter-spacing: 1px; }
</style>
