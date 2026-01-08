<script lang="ts">
  import { devices, loading, formatIeee, startPermitJoin, permitJoinActive, permitJoinRemaining, loadDevices, updateDeviceState } from '../lib/stores/index';
  import { api } from '../lib/api';
  import type { Device, DeviceCategory } from '../lib/types';

  let editingDevice: Device | null = $state(null);
  let editName = $state('');
  let editCategory = $state<DeviceCategory>('other');

  const categories: DeviceCategory[] = ['light', 'outlet', 'switch', 'sensor', 'lock', 'thermostat', 'fan', 'blinds', 'other'];

  function getCategoryColor(cat: DeviceCategory): string {
    const colors: Record<DeviceCategory, string> = {
      light: 'tag-yellow', outlet: 'tag-blue', switch: 'tag-purple',
      sensor: 'tag-green', lock: 'tag-red', thermostat: 'tag-blue',
      fan: 'tag-purple', blinds: 'tag-yellow', other: ''
    };
    return colors[cat] || '';
  }

  async function toggleDevice(device: Device, endpoint: number) {
    const ieee = formatIeee(device.ieee_address);
    const previousState = device.state_on;
    // Optimistic update - toggle immediately
    updateDeviceState(ieee, !previousState);
    try {
      await api.toggle(ieee, endpoint);
    } catch (e) {
      // Revert on error
      updateDeviceState(ieee, previousState ?? false);
      console.error('Toggle failed:', e);
    }
  }

  function openEdit(device: Device) {
    editingDevice = device;
    editName = device.friendly_name || '';
    editCategory = device.category || 'other';
  }

  async function saveEdit() {
    if (!editingDevice) return;
    const ieee = formatIeee(editingDevice.ieee_address);
    try {
      await api.updateDevice(ieee, editName || undefined, editCategory);
      editingDevice = null;
      loadDevices();
    } catch (e) {
      console.error('Failed to update device:', e);
    }
  }

  // Categories that represent controllable output devices
  const controllableCategories: DeviceCategory[] = ['light', 'outlet', 'lock', 'thermostat', 'fan', 'blinds'];
  // Categories that represent input devices (don't show toggle)
  const inputCategories: DeviceCategory[] = ['switch', 'sensor'];

  function isControllable(device: Device): boolean {
    const category = device.category || 'other';
    // If explicitly marked as input device, don't show toggle
    if (inputCategories.includes(category)) return false;
    // If explicitly marked as controllable, show toggle
    if (controllableCategories.includes(category)) return true;
    // For 'other', check if device has OnOff in in_clusters
    return device.endpoints.some(ep =>
      ep.in_clusters.includes(0x0006) || ep.in_clusters.includes(0x0008)
    );
  }

  function getEndpointsWithOnOff(device: Device): number[] {
    // Check endpoints with OnOff cluster
    const eps = device.endpoints
      .filter(ep => ep.in_clusters.includes(0x0006) || ep.in_clusters.includes(0x0008))
      .map(ep => ep.id);
    // If device is controllable but no specific endpoints found, default to endpoint 1
    if (eps.length === 0 && isControllable(device)) {
      return [1];
    }
    return eps;
  }
</script>

<div class="toolbar">
  <button class="btn btn-sm" onclick={() => startPermitJoin(60)} disabled={$permitJoinActive}>
    <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M8 4a.5.5 0 0 1 .5.5v3h3a.5.5 0 0 1 0 1h-3v3a.5.5 0 0 1-1 0v-3h-3a.5.5 0 0 1 0-1h3v-3A.5.5 0 0 1 8 4z"/></svg>
    {$permitJoinActive ? `Joining (${$permitJoinRemaining}s)` : 'Pair'}
  </button>
  <div class="toolbar-spacer"></div>
  <span class="text-sm muted">{$devices.length} device(s)</span>
  <button class="btn btn-sm" onclick={() => loadDevices()} disabled={$loading.devices} title="Refresh">
    <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M11.534 7h3.932a.25.25 0 0 1 .192.41l-1.966 2.36a.25.25 0 0 1-.384 0l-1.966-2.36a.25.25 0 0 1 .192-.41zm-11 2h3.932a.25.25 0 0 0 .192-.41L2.692 6.23a.25.25 0 0 0-.384 0L.342 8.59A.25.25 0 0 0 .534 9z"/><path fill-rule="evenodd" d="M8 3c-1.552 0-2.94.707-3.857 1.818a.5.5 0 1 1-.771-.636A6.002 6.002 0 0 1 13.917 7H12.9A5.002 5.002 0 0 0 8 3zM3.1 9a5.002 5.002 0 0 0 8.757 2.182.5.5 0 1 1 .771.636A6.002 6.002 0 0 1 2.083 9H3.1z"/></svg>
  </button>
</div>

{#if $devices.length === 0}
  <div class="empty-state">No devices found. Click "Permit Join" to pair new devices.</div>
{:else}
  <div class="device-list">
    {#each $devices as device (formatIeee(device.ieee_address))}
      {@const ieee = formatIeee(device.ieee_address)}
      {@const name = device.friendly_name || device.model || ieee}
      {@const endpoints = getEndpointsWithOnOff(device)}
      <div class="device-row">
        <span class="device-name mono">{name}</span>
        <span class="device-tags">
          <span class="tag {getCategoryColor(device.category || 'other')}">{device.category || 'other'}</span>
          <span class="tag">{device.device_type}</span>
          {#if device.state_on !== undefined && isControllable(device)}
            <span class="tag" class:tag-green={device.state_on} class:tag-red={!device.state_on}>
              {device.state_on ? 'ON' : 'OFF'}
            </span>
          {/if}
        </span>
        <span class="device-meta mono text-xs muted">{ieee}</span>
        <span class="device-actions">
          {#each endpoints as ep}
            <button class="btn btn-sm" onclick={() => toggleDevice(device, ep)} title={endpoints.length > 1 ? `Toggle EP${ep}` : 'Toggle'}>
              <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M7.5 1v7h1V1h-1z"/><path d="M3 8.812a4.999 4.999 0 0 1 2.578-4.375l-.485-.874A6 6 0 1 0 11 3.616l-.501.865A5 5 0 1 1 3 8.812z"/></svg>
            </button>
          {/each}
          <button class="btn btn-sm" onclick={() => openEdit(device)} title="Edit">
            <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M12.146.146a.5.5 0 0 1 .708 0l3 3a.5.5 0 0 1 0 .708l-10 10a.5.5 0 0 1-.168.11l-5 2a.5.5 0 0 1-.65-.65l2-5a.5.5 0 0 1 .11-.168l10-10zM11.207 2.5 13.5 4.793 14.793 3.5 12.5 1.207 11.207 2.5zm1.586 3L10.5 3.207 4 9.707V10h.5a.5.5 0 0 1 .5.5v.5h.5a.5.5 0 0 1 .5.5v.5h.293l6.5-6.5zm-9.761 5.175-.106.106-1.528 3.821 3.821-1.528.106-.106A.5.5 0 0 1 5 12.5V12h-.5a.5.5 0 0 1-.5-.5V11h-.5a.5.5 0 0 1-.468-.325z"/></svg>
          </button>
        </span>
      </div>
    {/each}
  </div>
{/if}

{#if editingDevice}
  {@const ieee = formatIeee(editingDevice.ieee_address)}
  <div class="modal-backdrop" onclick={() => editingDevice = null}>
    <div class="modal" onclick={(e) => e.stopPropagation()}>
      <div class="modal-header">
        <span class="modal-title">Edit Device</span>
        <button class="btn btn-sm" onclick={() => editingDevice = null}>×</button>
      </div>
      <div class="modal-body">
        <div class="info-panel mb-md">
          <div class="info-row">
            <span class="info-label">IEEE</span>
            <span class="info-value">{ieee}</span>
          </div>
          <div class="info-row">
            <span class="info-label">Model</span>
            <span class="info-value">{editingDevice.model || 'Unknown'}</span>
          </div>
          <div class="info-row">
            <span class="info-label">Manufacturer</span>
            <span class="info-value">{editingDevice.manufacturer || 'Unknown'}</span>
          </div>
        </div>
        <div class="form-group">
          <label class="form-label" for="edit-name">Friendly Name</label>
          <input id="edit-name" class="form-input" type="text" bind:value={editName} placeholder="e.g., Living Room Light">
        </div>
        <div class="form-group">
          <label class="form-label" for="edit-category">Category</label>
          <select id="edit-category" class="form-select" bind:value={editCategory}>
            {#each categories as cat}
              <option value={cat}>{cat}</option>
            {/each}
          </select>
        </div>
      </div>
      <div class="modal-footer">
        <button class="btn" onclick={() => editingDevice = null}>Cancel</button>
        <button class="btn btn-primary" onclick={saveEdit}>Save</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .device-list {
    display: flex;
    flex-direction: column;
    gap: 1px;
    background: var(--border-color);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  .device-row {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
    padding: var(--space-sm) var(--space-md);
    background: var(--bg-secondary);
    flex-wrap: wrap;
  }

  .device-row:hover {
    background: var(--bg-tertiary);
  }

  .device-name {
    min-width: 80px;
    font-size: var(--font-size-sm);
    flex: 1;
  }

  .device-tags {
    display: flex;
    gap: var(--space-xs);
  }

  .device-meta {
    display: none;
  }

  .device-actions {
    display: flex;
    gap: var(--space-xs);
    flex-shrink: 0;
  }

  @media (min-width: 600px) {
    .device-row {
      flex-wrap: nowrap;
      gap: var(--space-md);
    }

    .device-name {
      min-width: 120px;
      flex: 0 0 auto;
    }

    .device-meta {
      display: block;
      flex: 1;
    }
  }
</style>
