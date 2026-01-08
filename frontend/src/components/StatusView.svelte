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
  <span class="text-sm">
    Overall:
    <span class="tag {getStateClass($overallHealth)}">{getStateLabel($overallHealth)}</span>
  </span>
  <div class="toolbar-spacer"></div>
  <button class="btn btn-sm" onclick={refresh} disabled={$loading.network} title="Refresh">
    <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M11.534 7h3.932a.25.25 0 0 1 .192.41l-1.966 2.36a.25.25 0 0 1-.384 0l-1.966-2.36a.25.25 0 0 1 .192-.41zm-11 2h3.932a.25.25 0 0 0 .192-.41L2.692 6.23a.25.25 0 0 0-.384 0L.342 8.59A.25.25 0 0 0 .534 9z"/><path fill-rule="evenodd" d="M8 3c-1.552 0-2.94.707-3.857 1.818a.5.5 0 1 1-.771-.636A6.002 6.002 0 0 1 13.917 7H12.9A5.002 5.002 0 0 0 8 3zM3.1 9a5.002 5.002 0 0 0 8.757 2.182.5.5 0 1 1 .771.636A6.002 6.002 0 0 1 2.083 9H3.1z"/></svg>
  </button>
</div>

<!-- Connection Status Table -->
<div class="section">
  <div class="section-title">Connections</div>
  <div class="connection-list">
    <div class="connection-row">
      <span class="connection-name mono">Backend API</span>
      <span class="tag {getStateClass($connectionStatus.backend.state)}">{getStateLabel($connectionStatus.backend.state)}</span>
      <span class="connection-detail mono text-xs muted">{$connectionStatus.backend.detail}</span>
    </div>
    <div class="connection-row">
      <span class="connection-name mono">Zigbee Adapter</span>
      <span class="tag {getStateClass($connectionStatus.coordinator.state)}">{getStateLabel($connectionStatus.coordinator.state)}</span>
      <span class="connection-detail mono text-xs muted">{$connectionStatus.coordinator.detail}</span>
    </div>
    <div class="connection-row">
      <span class="connection-name mono">Zigbee Network</span>
      <span class="tag {getStateClass($connectionStatus.zigbee.state)}">{getStateLabel($connectionStatus.zigbee.state)}</span>
      <span class="connection-detail mono text-xs muted">{$connectionStatus.zigbee.detail}</span>
    </div>
    <div class="connection-row">
      <span class="connection-name mono">WebSocket</span>
      <span class="tag {getStateClass($connectionStatus.websocket.state)}">{getStateLabel($connectionStatus.websocket.state)}</span>
      <span class="connection-detail mono text-xs muted">{$connectionStatus.websocket.detail}</span>
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
  .connection-list {
    display: flex;
    flex-direction: column;
    gap: 1px;
    background: var(--border-color);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  .connection-row {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
    padding: var(--space-sm) var(--space-md);
    background: var(--bg-secondary);
    flex-wrap: wrap;
  }

  .connection-row:hover {
    background: var(--bg-tertiary);
  }

  .connection-name {
    min-width: 80px;
    font-size: var(--font-size-sm);
    flex: 1;
  }

  .connection-detail {
    display: none;
  }

  @media (min-width: 600px) {
    .connection-row {
      flex-wrap: nowrap;
      gap: var(--space-md);
    }

    .connection-name {
      min-width: 120px;
      flex: 0 0 auto;
    }

    .connection-detail {
      display: block;
      flex: 1;
    }
  }

  .status-panels {
    display: flex;
    flex-direction: column;
    gap: var(--space-md);
    margin-top: var(--space-md);
  }
</style>
