<script lang="ts">
  import { onMount } from 'svelte';
  import { ws, wsConnected } from './lib/websocket';
  import {
    activeView,
    systemInfo,
    deviceCount,
    overallHealth,
    connectionStatus,
    loadAll,
    loadDevices,
    loadCameras,
    loadAutomations,
    loadNetworkStatus,
  } from './lib/stores/index';
  import type { ConnectionState } from './lib/stores/index';
  import DevicesView from './components/DevicesView.svelte';
  import AutomationsView from './components/AutomationsView.svelte';
  import CamerasView from './components/CamerasView.svelte';
  import StatusView from './components/StatusView.svelte';

  const views = [
    { id: 'devices', label: 'Devices' },
    { id: 'automations', label: 'Automations' },
    { id: 'cameras', label: 'Cameras' },
    { id: 'status', label: 'Status' },
  ] as const;

  function switchView(view: typeof $activeView) {
    $activeView = view;
    // Refresh data when switching views
    if (view === 'devices') loadDevices();
    else if (view === 'automations') loadAutomations();
    else if (view === 'cameras') loadCameras();
    else if (view === 'status') loadNetworkStatus();
  }

  function getHealthClass(state: ConnectionState): string {
    switch (state) {
      case 'online': return 'online';
      case 'degraded': return 'warning';
      case 'offline': return 'offline';
      default: return 'warning';
    }
  }

  function getHealthLabel(state: ConnectionState): string {
    switch (state) {
      case 'online': return 'all systems operational';
      case 'degraded': return 'degraded';
      case 'offline': return 'offline';
      default: return 'unknown';
    }
  }

  onMount(() => {
    // Connect WebSocket
    ws.connect();

    // Set up WebSocket event handlers
    ws.on('device_joined', () => loadDevices());
    ws.on('device_left', () => loadDevices());
    ws.on('device_updated', () => loadDevices());
    ws.on('network_state_changed', () => loadNetworkStatus());
    ws.on('automation_created', () => loadAutomations());
    ws.on('automation_updated', () => loadAutomations());
    ws.on('automation_deleted', () => loadAutomations());

    // Initial data load
    loadAll();
  });
</script>

<div id="app">
  <header class="header">
    <div class="header-title">casita</div>
    <div class="header-status">
      <span class="status-dot {getHealthClass($overallHealth)}"></span>
      <span class="mono">{getHealthLabel($overallHealth)}</span>
      <span class="muted">|</span>
      <span class="mono">{$deviceCount} devices</span>
    </div>
  </header>

  <nav class="nav">
    {#each views as view}
      <button
        class="nav-btn"
        class:active={$activeView === view.id}
        onclick={() => switchView(view.id)}
      >
        {view.label}
      </button>
    {/each}
  </nav>

  <main class="main">
    {#if $activeView === 'devices'}
      <DevicesView />
    {:else if $activeView === 'automations'}
      <AutomationsView />
    {:else if $activeView === 'cameras'}
      <CamerasView />
    {:else if $activeView === 'status'}
      <StatusView />
    {/if}
  </main>

  <footer class="footer">
    <div class="footer-item">
      <span class="status-dot" class:online={$wsConnected} class:offline={!$wsConnected}></span>
      <span>ws</span>
    </div>
    <div class="footer-item">
      <span>v{$systemInfo?.version ?? '?'}</span>
    </div>
    {#if $connectionStatus.coordinator.state === 'online'}
      <div class="footer-item">
        <span>{$connectionStatus.coordinator.detail}</span>
      </div>
    {/if}
  </footer>
</div>
