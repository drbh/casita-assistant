<script lang="ts">
  import { automations, devices, loading, loadAutomations, formatIeee } from '../lib/stores/index';
  import { api } from '../lib/api';
  import type { Automation, Trigger, Action, Schedule, Command, Device } from '../lib/types';

  // Quick Link modal state
  let showLinkModal = $state(false);
  let linkSourceIeee = $state('');
  let linkTrigger = $state<'toggled' | 'turned_on' | 'turned_off'>('toggled');
  let linkTargetIeee = $state('');
  let linkAction = $state<'toggle' | 'turn_on' | 'turn_off'>('toggle');

  // Full automation modal state
  let showModal = $state(false);
  let editingId = $state<string | null>(null);
  let name = $state('');
  let description = $state('');
  let triggerType = $state<'manual' | 'schedule' | 'device_state'>('manual');
  let scheduleType = $state<'time_of_day' | 'interval'>('time_of_day');
  let scheduleTime = $state('18:00');
  let scheduleDays = $state<number[]>([1, 2, 3, 4, 5]);
  let intervalSeconds = $state(300);
  let triggerDeviceIeee = $state('');
  let stateChangeType = $state('any');
  let actions = $state<{ type: string; deviceIeee: string; endpoint: number; command: string; seconds: number; message: string }[]>([]);

  // Helper to get device name by IEEE
  function getDeviceName(ieee: string): string {
    const device = $devices.find(d => formatIeee(d.ieee_address) === ieee);
    return device?.friendly_name || device?.model || ieee.slice(0, 11) + '...';
  }

  // Check if automation is a simple device interaction
  function isInteraction(auto: Automation): boolean {
    return auto.trigger.type === 'device_state' &&
           auto.actions.length === 1 &&
           auto.actions[0].type === 'device_control';
  }

  // Get interaction summary for display
  function getInteractionSummary(auto: Automation): { source: string; trigger: string; target: string; action: string } | null {
    if (!isInteraction(auto)) return null;
    const trigger = auto.trigger as { type: 'device_state'; device_ieee: string; state_change: { type: string } };
    const action = auto.actions[0] as { type: 'device_control'; device_ieee: string; command: { type: string } };
    return {
      source: getDeviceName(trigger.device_ieee),
      trigger: trigger.state_change.type,
      target: getDeviceName(action.device_ieee),
      action: action.command.type,
    };
  }

  function resetForm() {
    editingId = null;
    name = '';
    description = '';
    triggerType = 'manual';
    scheduleType = 'time_of_day';
    scheduleTime = '18:00';
    scheduleDays = [1, 2, 3, 4, 5];
    intervalSeconds = 300;
    triggerDeviceIeee = '';
    stateChangeType = 'any';
    actions = [{ type: 'device_control', deviceIeee: '', endpoint: 1, command: 'toggle', seconds: 5, message: '' }];
  }

  function resetLinkForm() {
    linkSourceIeee = '';
    linkTrigger = 'toggled';
    linkTargetIeee = '';
    linkAction = 'toggle';
  }

  function openAdd() {
    resetForm();
    showModal = true;
  }

  function openLink() {
    resetLinkForm();
    showLinkModal = true;
  }

  function openEdit(automation: Automation) {
    editingId = automation.id;
    name = automation.name;
    description = automation.description || '';
    triggerType = automation.trigger.type;

    if (automation.trigger.type === 'schedule') {
      scheduleType = automation.trigger.schedule.type;
      if (automation.trigger.schedule.type === 'time_of_day') {
        scheduleTime = automation.trigger.schedule.time;
        scheduleDays = [...automation.trigger.schedule.days];
      } else {
        intervalSeconds = automation.trigger.schedule.seconds;
      }
    } else if (automation.trigger.type === 'device_state') {
      triggerDeviceIeee = automation.trigger.device_ieee;
      stateChangeType = automation.trigger.state_change.type;
    }

    actions = automation.actions.map(a => {
      if (a.type === 'device_control') {
        return { type: 'device_control', deviceIeee: a.device_ieee, endpoint: a.endpoint, command: a.command.type, seconds: 5, message: '' };
      } else if (a.type === 'delay') {
        return { type: 'delay', deviceIeee: '', endpoint: 1, command: 'toggle', seconds: a.seconds, message: '' };
      } else {
        return { type: 'log', deviceIeee: '', endpoint: 1, command: 'toggle', seconds: 5, message: a.message };
      }
    });

    showModal = true;
  }

  function addAction() {
    actions = [...actions, { type: 'device_control', deviceIeee: '', endpoint: 1, command: 'toggle', seconds: 5, message: '' }];
  }

  function removeAction(index: number) {
    actions = actions.filter((_, i) => i !== index);
  }

  async function saveLink() {
    if (!linkSourceIeee || !linkTargetIeee) return;

    const sourceName = getDeviceName(linkSourceIeee);
    const targetName = getDeviceName(linkTargetIeee);
    const autoName = `${sourceName} → ${targetName}`;

    try {
      await api.createAutomation({
        name: autoName,
        description: `When ${sourceName} ${linkTrigger.replace('_', ' ')}, ${linkAction.replace('_', ' ')} ${targetName}`,
        trigger: {
          type: 'device_state',
          device_ieee: linkSourceIeee,
          state_change: { type: linkTrigger },
        },
        actions: [{
          type: 'device_control',
          device_ieee: linkTargetIeee,
          endpoint: 1,
          command: { type: linkAction },
        }],
      });
      showLinkModal = false;
      loadAutomations();
    } catch (e) {
      console.error('Failed to create link:', e);
    }
  }

  async function save() {
    let trigger: Trigger;
    if (triggerType === 'manual') {
      trigger = { type: 'manual' };
    } else if (triggerType === 'schedule') {
      const schedule: Schedule = scheduleType === 'time_of_day'
        ? { type: 'time_of_day', time: scheduleTime, days: scheduleDays }
        : { type: 'interval', seconds: intervalSeconds };
      trigger = { type: 'schedule', schedule };
    } else {
      trigger = { type: 'device_state', device_ieee: triggerDeviceIeee, state_change: { type: stateChangeType as any } };
    }

    const builtActions: Action[] = actions
      .filter(a => a.type === 'delay' || a.type === 'log' || (a.type === 'device_control' && a.deviceIeee))
      .map(a => {
        if (a.type === 'device_control') {
          return { type: 'device_control', device_ieee: a.deviceIeee, endpoint: a.endpoint, command: { type: a.command } as Command };
        } else if (a.type === 'delay') {
          return { type: 'delay', seconds: a.seconds };
        } else {
          return { type: 'log', message: a.message, level: 'info' };
        }
      });

    try {
      if (editingId) {
        await api.updateAutomation(editingId, { name, description: description || undefined, trigger, actions: builtActions });
      } else {
        await api.createAutomation({ name, description: description || undefined, trigger, actions: builtActions });
      }
      showModal = false;
      loadAutomations();
    } catch (e) {
      console.error('Failed to save:', e);
    }
  }

  async function toggleEnabled(automation: Automation) {
    try {
      if (automation.enabled) {
        await api.disableAutomation(automation.id);
      } else {
        await api.enableAutomation(automation.id);
      }
      loadAutomations();
    } catch (e) {
      console.error('Failed to toggle:', e);
    }
  }

  async function runNow(id: string) {
    try {
      await api.triggerAutomation(id);
    } catch (e) {
      console.error('Failed to trigger:', e);
    }
  }

  async function deleteAutomation(id: string, name: string) {
    if (confirm(`Delete "${name}"?`)) {
      try {
        await api.deleteAutomation(id);
        loadAutomations();
      } catch (e) {
        console.error('Failed to delete:', e);
      }
    }
  }

  function getTriggerText(trigger: Trigger): string {
    if (trigger.type === 'manual') return 'Manual';
    if (trigger.type === 'schedule') {
      if (trigger.schedule.type === 'time_of_day') {
        const days = trigger.schedule.days.length === 0 ? 'daily' : trigger.schedule.days.map(d => ['Su', 'Mo', 'Tu', 'We', 'Th', 'Fr', 'Sa'][d]).join(',');
        return `${trigger.schedule.time} (${days})`;
      }
      return `Every ${trigger.schedule.seconds}s`;
    }
    return getDeviceName(trigger.device_ieee);
  }
</script>

<div class="toolbar">
  <button class="btn btn-primary" onclick={openLink}>Link Devices</button>
  <button class="btn" onclick={openAdd}>Add Automation</button>
  <button class="btn" onclick={() => loadAutomations()} disabled={$loading.automations}>Refresh</button>
  <div class="toolbar-spacer"></div>
  <span class="text-sm muted">{$automations.length} rule(s)</span>
</div>

{#if $automations.length === 0}
  <div class="empty-state">
    No automations configured.<br>
    <span class="text-xs">Use "Link Devices" to connect a button to control another device.</span>
  </div>
{:else}
  <div class="automation-list">
    {#each $automations as automation (automation.id)}
      {@const interaction = getInteractionSummary(automation)}
      <div class="automation-card" class:disabled-card={!automation.enabled}>
        <div class="automation-header">
          <div class="automation-name">
            {#if interaction}
              <span class="interaction-flow">
                <span class="mono">{interaction.source}</span>
                <span class="flow-arrow">→</span>
                <span class="mono">{interaction.target}</span>
              </span>
              <span class="tag tag-blue">interaction</span>
            {:else}
              <span class="mono">{automation.name}</span>
            {/if}
            <span class="tag" class:tag-green={automation.enabled} class:tag-red={!automation.enabled}>
              {automation.enabled ? 'ON' : 'OFF'}
            </span>
          </div>
          <div class="automation-actions">
            <button class="btn btn-sm" onclick={() => toggleEnabled(automation)}>
              {automation.enabled ? 'Disable' : 'Enable'}
            </button>
            <button class="btn btn-sm btn-primary" onclick={() => runNow(automation.id)}>Run</button>
            <button class="btn btn-sm" onclick={() => openEdit(automation)}>Edit</button>
            <button class="btn btn-sm btn-danger" onclick={() => deleteAutomation(automation.id, automation.name)}>Del</button>
          </div>
        </div>
        {#if interaction}
          <div class="automation-desc text-sm muted">
            When <strong>{interaction.trigger.replace('_', ' ')}</strong>, <strong>{interaction.action.replace('_', ' ')}</strong> target
          </div>
        {:else if automation.description}
          <div class="automation-desc text-sm muted">{automation.description}</div>
        {/if}
        {#if !interaction}
          <div class="automation-meta">
            <span class="meta-item">
              <span class="meta-label">Trigger:</span>
              <span class="mono">{getTriggerText(automation.trigger)}</span>
            </span>
            <span class="meta-item">
              <span class="meta-label">Actions:</span>
              <span class="mono">{automation.actions.length}</span>
            </span>
          </div>
        {/if}
      </div>
    {/each}
  </div>
{/if}

<!-- Quick Link Modal -->
{#if showLinkModal}
  <div class="modal-backdrop" onclick={() => showLinkModal = false} role="button" tabindex="-1" onkeydown={(e) => e.key === 'Escape' && (showLinkModal = false)}>
    <div class="modal" onclick={(e) => e.stopPropagation()} role="dialog">
      <div class="modal-header">
        <span class="modal-title">Link Devices</span>
        <button class="btn btn-sm" onclick={() => showLinkModal = false}>×</button>
      </div>
      <div class="modal-body">
        <p class="text-sm muted mb-md">Create a simple interaction: when one device triggers, control another.</p>

        <div class="link-builder">
          <div class="link-row">
            <span class="link-label">When</span>
            <select class="form-select" bind:value={linkSourceIeee}>
              <option value="">Select source device...</option>
              {#each $devices as device}
                {@const ieee = formatIeee(device.ieee_address)}
                <option value={ieee}>{device.friendly_name || device.model || ieee}</option>
              {/each}
            </select>
          </div>

          <div class="link-row">
            <span class="link-label">is</span>
            <select class="form-select" bind:value={linkTrigger}>
              <option value="toggled">toggled</option>
              <option value="turned_on">turned on</option>
              <option value="turned_off">turned off</option>
            </select>
          </div>

          <div class="link-row">
            <span class="link-label">then</span>
            <select class="form-select" bind:value={linkAction}>
              <option value="toggle">toggle</option>
              <option value="turn_on">turn on</option>
              <option value="turn_off">turn off</option>
            </select>
          </div>

          <div class="link-row">
            <span class="link-label"></span>
            <select class="form-select" bind:value={linkTargetIeee}>
              <option value="">Select target device...</option>
              {#each $devices as device}
                {@const ieee = formatIeee(device.ieee_address)}
                <option value={ieee}>{device.friendly_name || device.model || ieee}</option>
              {/each}
            </select>
          </div>
        </div>

        {#if linkSourceIeee && linkTargetIeee}
          <div class="link-preview">
            <span class="mono">{getDeviceName(linkSourceIeee)}</span>
            <span class="flow-arrow">→</span>
            <span class="mono">{getDeviceName(linkTargetIeee)}</span>
          </div>
        {/if}
      </div>
      <div class="modal-footer">
        <button class="btn" onclick={() => showLinkModal = false}>Cancel</button>
        <button class="btn btn-primary" onclick={saveLink} disabled={!linkSourceIeee || !linkTargetIeee}>Create Link</button>
      </div>
    </div>
  </div>
{/if}

<!-- Full Automation Modal -->
{#if showModal}
  <div class="modal-backdrop" onclick={() => showModal = false} role="button" tabindex="-1" onkeydown={(e) => e.key === 'Escape' && (showModal = false)}>
    <div class="modal modal-lg" onclick={(e) => e.stopPropagation()} role="dialog">
      <div class="modal-header">
        <span class="modal-title">{editingId ? 'Edit' : 'Add'} Automation</span>
        <button class="btn btn-sm" onclick={() => showModal = false}>×</button>
      </div>
      <div class="modal-body">
        <div class="form-group">
          <label class="form-label" for="auto-name">Name</label>
          <input id="auto-name" class="form-input" type="text" bind:value={name} placeholder="e.g., Evening Lights" required>
        </div>
        <div class="form-group">
          <label class="form-label" for="auto-desc">Description</label>
          <input id="auto-desc" class="form-input" type="text" bind:value={description} placeholder="Optional description">
        </div>

        <div class="section">
          <div class="section-title">Trigger</div>
          <div class="form-group">
            <label class="form-label" for="trigger-type">Type</label>
            <select id="trigger-type" class="form-select" bind:value={triggerType}>
              <option value="manual">Manual Only</option>
              <option value="schedule">Schedule</option>
              <option value="device_state">Device State</option>
            </select>
          </div>

          {#if triggerType === 'schedule'}
            <div class="form-group">
              <label class="form-label" for="sched-type">Schedule Type</label>
              <select id="sched-type" class="form-select" bind:value={scheduleType}>
                <option value="time_of_day">Time of Day</option>
                <option value="interval">Interval</option>
              </select>
            </div>
            {#if scheduleType === 'time_of_day'}
              <div class="form-group">
                <label class="form-label" for="sched-time">Time</label>
                <input id="sched-time" class="form-input" type="time" bind:value={scheduleTime}>
              </div>
              <div class="form-group">
                <label class="form-label">Days</label>
                <div class="flex gap-sm">
                  {#each ['Su', 'Mo', 'Tu', 'We', 'Th', 'Fr', 'Sa'] as day, i}
                    <label class="form-check">
                      <input type="checkbox" checked={scheduleDays.includes(i)} onchange={() => {
                        if (scheduleDays.includes(i)) {
                          scheduleDays = scheduleDays.filter(d => d !== i);
                        } else {
                          scheduleDays = [...scheduleDays, i].sort();
                        }
                      }}>
                      {day}
                    </label>
                  {/each}
                </div>
              </div>
            {:else}
              <div class="form-group">
                <label class="form-label" for="interval-sec">Interval (seconds)</label>
                <input id="interval-sec" class="form-input" type="number" bind:value={intervalSeconds} min="10">
              </div>
            {/if}
          {:else if triggerType === 'device_state'}
            <div class="form-group">
              <label class="form-label" for="trigger-device">Device</label>
              <select id="trigger-device" class="form-select" bind:value={triggerDeviceIeee}>
                <option value="">Select...</option>
                {#each $devices as device}
                  {@const ieee = formatIeee(device.ieee_address)}
                  <option value={ieee}>{device.friendly_name || device.model || ieee}</option>
                {/each}
              </select>
            </div>
            <div class="form-group">
              <label class="form-label" for="state-change">State Change</label>
              <select id="state-change" class="form-select" bind:value={stateChangeType}>
                <option value="any">Any</option>
                <option value="turned_on">Turned On</option>
                <option value="turned_off">Turned Off</option>
                <option value="toggled">Toggled</option>
              </select>
            </div>
          {/if}
        </div>

        <div class="section">
          <div class="section-title">Actions</div>
          {#each actions as action, i}
            <div class="action-row">
              <select class="form-select action-type-select" bind:value={action.type}>
                <option value="device_control">Device</option>
                <option value="delay">Delay</option>
                <option value="log">Log</option>
              </select>
              {#if action.type === 'device_control'}
                <select class="form-select" bind:value={action.deviceIeee}>
                  <option value="">Device...</option>
                  {#each $devices as device}
                    {@const ieee = formatIeee(device.ieee_address)}
                    <option value={ieee}>{device.friendly_name || ieee.slice(0, 8)}</option>
                  {/each}
                </select>
                <select class="form-select action-cmd-select" bind:value={action.command}>
                  <option value="turn_on">On</option>
                  <option value="turn_off">Off</option>
                  <option value="toggle">Toggle</option>
                </select>
              {:else if action.type === 'delay'}
                <input class="form-input" type="number" bind:value={action.seconds} placeholder="Seconds" style="width: 80px">
                <span class="text-sm muted">seconds</span>
              {:else}
                <input class="form-input" type="text" bind:value={action.message} placeholder="Log message">
              {/if}
              <button class="btn btn-sm btn-danger" onclick={() => removeAction(i)}>×</button>
            </div>
          {/each}
          <button class="btn btn-sm" onclick={addAction}>+ Add Action</button>
        </div>
      </div>
      <div class="modal-footer">
        <button class="btn" onclick={() => showModal = false}>Cancel</button>
        <button class="btn btn-primary" onclick={save}>Save</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .automation-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-sm);
  }

  .automation-card {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-md);
    padding: var(--space-md);
  }

  .automation-card:hover {
    border-color: var(--border-light);
  }

  .disabled-card {
    opacity: 0.5;
  }

  .automation-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: var(--space-md);
    flex-wrap: wrap;
  }

  .automation-name {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
    font-weight: 500;
  }

  .interaction-flow {
    display: flex;
    align-items: center;
    gap: var(--space-xs);
  }

  .flow-arrow {
    color: var(--accent-blue);
    font-weight: bold;
  }

  .automation-actions {
    display: flex;
    gap: var(--space-xs);
    flex-wrap: wrap;
  }

  .automation-desc {
    margin-top: var(--space-xs);
  }

  .automation-desc strong {
    color: var(--text-primary);
  }

  .automation-meta {
    display: flex;
    gap: var(--space-lg);
    margin-top: var(--space-sm);
    padding-top: var(--space-sm);
    border-top: 1px solid var(--border-color);
    font-size: var(--font-size-sm);
  }

  .meta-item {
    display: flex;
    gap: var(--space-xs);
  }

  .meta-label {
    color: var(--text-muted);
  }

  /* Link builder styles */
  .link-builder {
    display: flex;
    flex-direction: column;
    gap: var(--space-sm);
  }

  .link-row {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
  }

  .link-label {
    width: 50px;
    font-size: var(--font-size-sm);
    color: var(--text-muted);
    text-align: right;
  }

  .link-row .form-select {
    flex: 1;
  }

  .link-preview {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-sm);
    margin-top: var(--space-md);
    padding: var(--space-md);
    background: var(--bg-tertiary);
    border-radius: var(--radius-md);
    font-size: var(--font-size-lg);
  }

  /* Action row */
  .action-row {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
    margin-bottom: var(--space-sm);
    padding: var(--space-sm);
    background: var(--bg-secondary);
    border-radius: var(--radius-sm);
  }

  .action-type-select {
    width: 100px;
  }

  .action-cmd-select {
    width: 80px;
  }

  @media (max-width: 600px) {
    .automation-header {
      flex-direction: column;
      align-items: flex-start;
    }
    .automation-meta {
      flex-direction: column;
      gap: var(--space-xs);
    }
    .link-row {
      flex-direction: column;
      align-items: stretch;
    }
    .link-label {
      width: auto;
      text-align: left;
    }
  }
</style>
