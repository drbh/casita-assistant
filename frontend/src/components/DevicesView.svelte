<script lang="ts">
  import { devices, loading, formatIeee, startPermitJoin, permitJoinActive, permitJoinRemaining, loadDevices } from '../lib/stores/index';
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
    try {
      await api.toggle(ieee, endpoint);
    } catch (e) {
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

  function getEndpointsWithOnOff(device: Device): number[] {
    const eps = device.endpoints
      .filter(ep => ep.in_clusters.includes(0x0006) || ep.in_clusters.includes(0x0008))
      .map(ep => ep.id);
    return eps.length > 0 ? eps : [1];
  }
</script>

<div class="toolbar">
  <button class="btn btn-primary" onclick={() => startPermitJoin(60)} disabled={$permitJoinActive}>
    {$permitJoinActive ? `Joining (${$permitJoinRemaining}s)` : 'Permit Join'}
  </button>
  <button class="btn" onclick={() => loadDevices()} disabled={$loading.devices}>
    Refresh
  </button>
  <div class="toolbar-spacer"></div>
  <span class="text-sm muted">{$devices.length} device(s)</span>
</div>

{#if $devices.length === 0}
  <div class="empty-state">No devices found. Click "Permit Join" to pair new devices.</div>
{:else}
  <div class="data-grid device-grid">
    <div class="data-row data-row-header">
      <span>Name</span>
      <span>Type</span>
      <span>IEEE Address</span>
      <span>NWK</span>
      <span>LQI</span>
      <span>State</span>
      <span>Actions</span>
    </div>
    {#each $devices as device (formatIeee(device.ieee_address))}
      {@const ieee = formatIeee(device.ieee_address)}
      {@const name = device.friendly_name || device.model || ieee}
      {@const endpoints = getEndpointsWithOnOff(device)}
      <div class="data-row">
        <span class="mono text-sm" title={name}>{name.length > 20 ? name.slice(0, 20) + '...' : name}</span>
        <span>
          <span class="tag {getCategoryColor(device.category || 'other')}">{device.category || 'other'}</span>
          <span class="tag">{device.device_type}</span>
        </span>
        <span class="mono text-xs muted">{ieee}</span>
        <span class="mono text-xs">0x{device.nwk_address.toString(16).padStart(4, '0').toUpperCase()}</span>
        <span class="mono text-xs">{device.lqi ?? '-'}</span>
        <span>
          {#if device.state_on !== undefined}
            <span class="tag" class:tag-green={device.state_on} class:tag-red={!device.state_on}>
              {device.state_on ? 'ON' : 'OFF'}
            </span>
          {:else}
            <span class="muted">-</span>
          {/if}
        </span>
        <span class="flex gap-xs">
          {#each endpoints as ep}
            <button class="btn btn-sm" onclick={() => toggleDevice(device, ep)}>
              {endpoints.length > 1 ? `EP${ep}` : 'Toggle'}
            </button>
          {/each}
          <button class="btn btn-sm" onclick={() => openEdit(device)}>Edit</button>
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
  .device-grid {
    grid-template-columns: minmax(120px, 1fr) auto minmax(140px, 180px) 70px 40px 60px auto;
  }

  @media (max-width: 900px) {
    .device-grid {
      grid-template-columns: 1fr;
    }
    .data-row-header {
      display: none;
    }
    .data-row {
      display: flex;
      flex-wrap: wrap;
      gap: var(--space-sm);
    }
  }
</style>
