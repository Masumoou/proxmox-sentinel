<script lang="ts">
  import '../app.css';
  import { page } from '$app/state';
  let { children } = $props();
  let currentPath = $derived(page.url.pathname);
</script>

<div class="dashboard-container">
  <!-- ── Sidebar ─────────────────────────────────────────── -->
  <aside class="sidebar">
    <div class="logo-area">
      <span class="logo-icon">✕</span>
      <span class="logo-text">PROXMOX</span>
    </div>

    <nav class="nav-links">
      <a href="/" class="nav-item" class:nav-active={currentPath === '/'}>
        <span class="nav-icon">◉</span> DASHBOARD
      </a>
      <a href="/nodes" class="nav-item" class:nav-active={currentPath === '/nodes'}>
        <span class="nav-icon">⊞</span> NODES
      </a>
      <a href="/guests" class="nav-item" class:nav-active={currentPath === '/guests'}>
        <span class="nav-icon">▦</span> VMS
      </a>
      <a href="/containers" class="nav-item" class:nav-active={currentPath === '/containers'}>
        <span class="nav-icon">⊡</span> CONTAINERS
      </a>
      <a href="/storage" class="nav-item" class:nav-active={currentPath === '/storage'}>
        <span class="nav-icon">◫</span> STORAGE
      </a>
      <a href="/backups" class="nav-item" class:nav-active={currentPath === '/backups'}>
        <span class="nav-icon">⊘</span> BACKUPS
      </a>
      <a href="/settings" class="nav-item" class:nav-active={currentPath === '/settings'}>
        <span class="nav-icon">⊛</span> SETTINGS
      </a>
      <a href="/logs" class="nav-item" class:nav-active={currentPath === '/logs'}>
        <span class="nav-icon">▤</span> LOGS
      </a>
      <a href="/haproxy" class="nav-item" class:nav-active={currentPath === '/haproxy'}>
        <span class="nav-icon">◓</span> HAPROXY
      </a>
    </nav>
  </aside>

  <!-- ── Main Area ───────────────────────────────────────── -->
  <section class="page-wrapper">
    <header class="header">
      <div class="header-title">
        <span class="text-cyan" style="font-weight: 800; letter-spacing: 2px;">PROXMOX</span>
        <span style="color: var(--text-secondary); margin: 0 8px;">VE — NODE:</span>
        <span class="text-cyan" style="font-weight: 700;">CYBER-CORE-01</span>
      </div>
    </header>

    <main class="main-content">
      {@render children()}
    </main>
  </section>
</div>

<style>
  .page-wrapper {
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
    width: 100%;
  }

  /* ── Sidebar ──────────────────────────────────────────── */
  .logo-area {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 8px 20px;
    border-bottom: 1px solid var(--border-color);
    margin-bottom: 12px;
  }

  .logo-icon {
    font-size: 1.3rem;
    color: var(--accent-cyan);
    font-weight: 900;
  }

  .logo-text {
    font-size: 1.1rem;
    font-weight: 800;
    letter-spacing: 3px;
    color: var(--text-primary);
  }

  .nav-links {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .nav-item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 12px;
    border-radius: 6px;
    font-size: 0.72rem;
    font-weight: 600;
    letter-spacing: 1.5px;
    color: var(--text-secondary);
    text-decoration: none;
    transition: all 0.2s ease;
  }

  .nav-item:hover {
    background: rgba(0, 212, 255, 0.05);
    color: var(--text-primary);
  }

  .nav-active {
    background: rgba(0, 212, 255, 0.1) !important;
    color: var(--accent-cyan) !important;
    border-left: 2px solid var(--accent-cyan);
    box-shadow: 0 0 8px rgba(0, 212, 255, 0.05);
  }

  .nav-icon {
    font-size: 0.85rem;
    width: 18px;
    text-align: center;
  }

  /* ── Header ───────────────────────────────────────────── */
  .header-title {
    font-size: 0.82rem;
    letter-spacing: 1px;
  }
</style>
