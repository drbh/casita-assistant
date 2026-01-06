<script lang="ts">
  import {
    networkStatus,
    systemInfo,
    loading,
    loadNetworkStatus,
    loadSystemInfo,
    connectionStatus,
    overallHealth,
    deviceCount,
    enabledAutomations,
    automations,
  } from '../lib/stores/index';
  import { wsConnected } from '../lib/websocket';
  import type { ConnectionState } from '../lib/stores/index';

  function formatPanId(id: number): string {
    return '0x' + id.toString(16).padStart(4, '0').toUpperCase();
  }

  function refresh() {
    loadNetworkStatus();
    loadSystemInfo();
  }

  function getStateClass(state: ConnectionState): string {
    switch (state) {
      case 'online': return 'tag-green';
      case 'offline': return 'tag-red';
      case 'degraded': return 'tag-yellow';
      default: return '';
    }
  }

  function getStateLabel(state: ConnectionState): string {
    switch (state) {
      case 'online': return 'ONLINE';
      case 'offline': return 'OFFLINE';
      case 'degraded': return 'DEGRADED';
      default: return 'UNKNOWN';
    }
  }
</script>

<div class="toolbar">
  <button class="btn" onclick={refresh} disabled={$loading.network}>Refresh</button>
  <div class="toolbar-spacer"></div>
  <span class="text-sm">
    Overall:
    <span class="tag {getStateClass($overallHealth)}">{getStateLabel($overallHealth)}</span>
  </span>
</div>

<!-- Connection Status Table -->
<div class="section">
  <div class="section-title">Connections</div>
  <div class="data-grid connection-grid">
    <div class="data-row data-row-header">
      <span>Component</span>
      <span>Status</span>
      <span>Details</span>
    </div>
    <div class="data-row">
      <span class="mono">Backend API</span>
      <span><span class="tag {getStateClass($connectionStatus.backend.state)}">{getStateLabel($connectionStatus.backend.state)}</span></span>
      <span class="mono text-xs muted">{$connectionStatus.backend.detail}</span>
    </div>
    <div class="data-row">
      <span class="mono">Zigbee Adapter</span>
      <span><span class="tag {getStateClass($connectionStatus.coordinator.state)}">{getStateLabel($connectionStatus.coordinator.state)}</span></span>
      <span class="mono text-xs muted">{$connectionStatus.coordinator.detail}</span>
    </div>
    <div class="data-row">
      <span class="mono">Zigbee Network</span>
      <span><span class="tag {getStateClass($connectionStatus.zigbee.state)}">{getStateLabel($connectionStatus.zigbee.state)}</span></span>
      <span class="mono text-xs muted">{$connectionStatus.zigbee.detail}</span>
    </div>
    <div class="data-row">
      <span class="mono">WebSocket</span>
      <span><span class="tag {getStateClass($connectionStatus.websocket.state)}">{getStateLabel($connectionStatus.websocket.state)}</span></span>
      <span class="mono text-xs muted">{$connectionStatus.websocket.detail}</span>
    </div>
  </div>
</div>

<!-- System Details -->
<div class="status-panels">
  <div class="info-panel">
    <div class="info-panel-title">System</div>
    {#if $systemInfo}
      <div class="info-row">
        <span class="info-label">Application</span>
        <span class="info-value">{$systemInfo.name}</span>
      </div>
      <div class="info-row">
        <span class="info-label">Version</span>
        <span class="info-value">{$systemInfo.version}</span>
      </div>
      <div class="info-row">
        <span class="info-label">Firmware</span>
        <span class="info-value">{$systemInfo.firmware ?? 'Not detected'}</span>
      </div>
    {:else}
      <div class="muted text-sm">Loading...</div>
    {/if}
  </div>

  <div class="info-panel">
    <div class="info-panel-title">Zigbee Network</div>
    {#if $networkStatus}
      <div class="info-row">
        <span class="info-label">State</span>
        <span class="info-value">
          <span class="tag" class:tag-green={$networkStatus.connected} class:tag-red={!$networkStatus.connected}>
            {$networkStatus.connected ? 'Joined' : 'Disconnected'}
          </span>
        </span>
      </div>
      <div class="info-row">
        <span class="info-label">Channel</span>
        <span class="info-value">{$networkStatus.channel}</span>
      </div>
      <div class="info-row">
        <span class="info-label">PAN ID</span>
        <span class="info-value">{formatPanId($networkStatus.pan_id)}</span>
      </div>
      <div class="info-row">
        <span class="info-label">Ext PAN ID</span>
        <span class="info-value text-xs">{$networkStatus.extended_pan_id}</span>
      </div>
      <div class="info-row">
        <span class="info-label">Permit Join</span>
        <span class="info-value">
          <span class="tag" class:tag-yellow={$networkStatus.permit_join}>
            {$networkStatus.permit_join ? 'Active' : 'No'}
          </span>
        </span>
      </div>
    {:else if $connectionStatus.coordinator.state === 'offline'}
      <div class="muted text-sm">No Zigbee adapter detected</div>
    {:else}
      <div class="muted text-sm">Loading...</div>
    {/if}
  </div>

  <div class="info-panel">
    <div class="info-panel-title">Statistics</div>
    <div class="info-row">
      <span class="info-label">Devices</span>
      <span class="info-value">{$deviceCount}</span>
    </div>
    <div class="info-row">
      <span class="info-label">Automations</span>
      <span class="info-value">{$automations.length} ({$enabledAutomations} active)</span>
    </div>
    <div class="info-row">
      <span class="info-label">WebSocket</span>
      <span class="info-value">
        <span class="tag" class:tag-green={$wsConnected} class:tag-red={!$wsConnected}>
          {$wsConnected ? 'Live' : 'Disconnected'}
        </span>
      </span>
    </div>
  </div>
</div>

<style>
  .connection-grid {
    grid-template-columns: 140px 90px 1fr;
  }

  .status-panels {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: var(--space-md);
    margin-top: var(--space-md);
  }

  @media (max-width: 600px) {
    .connection-grid {
      grid-template-columns: 1fr;
    }
    .data-row-header {
      display: none;
    }
  }
</style>
