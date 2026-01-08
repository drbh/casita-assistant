<script lang="ts">
  import type { Writable } from 'svelte/store';
  import type { ViewId } from '../lib/stores/index';
  import { loadDevices, loadAutomations, loadCameras, loadNetworkStatus } from '../lib/stores/index';
  import DevicesView from './DevicesView.svelte';
  import AutomationsView from './AutomationsView.svelte';
  import CamerasView from './CamerasView.svelte';
  import StatusView from './StatusView.svelte';

  interface Props {
    view: Writable<ViewId>;
    collapsible?: boolean;
    collapsed?: Writable<boolean>;
  }

  let { view, collapsible = false, collapsed }: Props = $props();

  const views: { id: ViewId; label: string }[] = [
    { id: 'devices', label: 'Devices' },
    { id: 'automations', label: 'Auto' },
    { id: 'cameras', label: 'Cameras' },
    { id: 'status', label: 'Status' },
  ];

  function switchView(newView: ViewId) {
    view.set(newView);
    if (newView === 'devices') loadDevices();
    else if (newView === 'automations') loadAutomations();
    else if (newView === 'cameras') loadCameras();
    else if (newView === 'status') loadNetworkStatus();
  }

  function toggleCollapse() {
    if (collapsed) {
      collapsed.update(v => !v);
    }
  }

  $effect(() => {
    // When expanding, refresh the current view's data
    if (collapsed && !$collapsed) {
      switchView($view);
    }
  });
</script>

<div class="pane" class:pane-collapsed={collapsed && $collapsed}>
  <nav class="pane-nav">
    {#if collapsible}
      <button class="pane-nav-btn pane-toggle" onclick={toggleCollapse} title={collapsed && $collapsed ? 'Expand' : 'Collapse'}>
        <svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor">
          {#if collapsed && $collapsed}
            <path d="M7.646 4.646a.5.5 0 0 1 .708 0l6 6a.5.5 0 0 1-.708.708L8 5.707l-5.646 5.647a.5.5 0 0 1-.708-.708l6-6z"/>
          {:else}
            <path d="M1.646 4.646a.5.5 0 0 1 .708 0L8 10.293l5.646-5.647a.5.5 0 0 1 .708.708l-6 6a.5.5 0 0 1-.708 0l-6-6a.5.5 0 0 1 0-.708z"/>
          {/if}
        </svg>
      </button>
    {/if}
    {#each views as v}
      <button
        class="pane-nav-btn"
        class:active={$view === v.id}
        onclick={() => switchView(v.id)}
      >
        {v.label}
      </button>
    {/each}
  </nav>
  {#if !collapsed || !$collapsed}
    <div class="pane-content">
      {#if $view === 'devices'}
        <DevicesView />
      {:else if $view === 'automations'}
        <AutomationsView />
      {:else if $view === 'cameras'}
        <CamerasView />
      {:else if $view === 'status'}
        <StatusView />
      {/if}
    </div>
  {/if}
</div>
