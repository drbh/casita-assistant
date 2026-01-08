<script lang="ts">
  import { onMount } from 'svelte';
  import { ws, wsConnected } from './lib/websocket';
  import {
    systemInfo,
    deviceCount,
    overallHealth,
    connectionStatus,
    topPaneView,
    bottomPaneView,
    bottomPaneCollapsed,
    loadAll,
    loadDevices,
    loadCameras,
    loadAutomations,
    loadNetworkStatus,
    updateDeviceState,
  } from './lib/stores/index';
  import type { ConnectionState } from './lib/stores/index';
  import Pane from './components/Pane.svelte';

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
    ws.on('device_state_changed', (event: { ieee?: string; state_on?: boolean }) => {
      if (event.ieee && event.state_on !== undefined) {
        updateDeviceState(event.ieee, event.state_on);
      }
    });
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

  <div class="panes-container">
    <Pane view={topPaneView} />
    <Pane view={bottomPaneView} collapsible={true} collapsed={bottomPaneCollapsed} />
  </div>

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
