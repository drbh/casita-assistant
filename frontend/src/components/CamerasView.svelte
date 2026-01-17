<script lang="ts">
  import { cameras, loading, loadCameras } from '../lib/stores/index';
  import { api } from '../lib/api';
  import type { Camera } from '../lib/types';
  import VideoPlayer from './VideoPlayer.svelte';

  let showModal = $state(false);
  let editingId = $state<string | null>(null);
  let name = $state('');
  let url = $state('');
  let streamType = $state<'mjpeg' | 'rtsp' | 'webrtc'>('mjpeg');
  let username = $state('');
  let password = $state('');

  let playingCameras = $state<Set<string>>(new Set());

  function resetForm() {
    editingId = null;
    name = '';
    url = '';
    streamType = 'mjpeg';
    username = '';
    password = '';
  }

  function openEdit(camera: Camera) {
    editingId = camera.id;
    name = camera.name;
    url = camera.stream_url;
    streamType = camera.stream_type;
    username = '';
    password = '';
    showModal = true;
  }

  async function saveCamera() {
    try {
      if (editingId) {
        await api.updateCamera(editingId, name, url, streamType, username || undefined, password || undefined);
      } else {
        await api.addCamera(name, url, streamType, username || undefined, password || undefined);
      }
      showModal = false;
      resetForm();
      loadCameras();
    } catch (e) {
      console.error('Failed to save camera:', e);
      alert('Failed to save camera');
    }
  }

  async function deleteCamera(camera: Camera) {
    if (confirm(`Delete "${camera.name}"?`)) {
      try {
        await api.deleteCamera(camera.id);
        loadCameras();
      } catch (e) {
        console.error('Failed to delete:', e);
      }
    }
  }

  function togglePlay(id: string) {
    if (playingCameras.has(id)) {
      playingCameras.delete(id);
      playingCameras = new Set(playingCameras);
    } else {
      playingCameras.add(id);
      playingCameras = new Set(playingCameras);
    }
  }

  function getStreamUrl(id: string): string {
    return api.getCameraStreamUrl(id);
  }
</script>

<div class="toolbar">
  <button class="btn btn-sm" onclick={() => { resetForm(); showModal = true; }}>
    <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M8 4a.5.5 0 0 1 .5.5v3h3a.5.5 0 0 1 0 1h-3v3a.5.5 0 0 1-1 0v-3h-3a.5.5 0 0 1 0-1h3v-3A.5.5 0 0 1 8 4z"/></svg>
    Add
  </button>
  <div class="toolbar-spacer"></div>
  <span class="text-sm muted">{$cameras.length} camera(s)</span>
  <button class="btn btn-sm" onclick={() => loadCameras()} disabled={$loading.cameras} title="Refresh">
    <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M11.534 7h3.932a.25.25 0 0 1 .192.41l-1.966 2.36a.25.25 0 0 1-.384 0l-1.966-2.36a.25.25 0 0 1 .192-.41zm-11 2h3.932a.25.25 0 0 0 .192-.41L2.692 6.23a.25.25 0 0 0-.384 0L.342 8.59A.25.25 0 0 0 .534 9z"/><path fill-rule="evenodd" d="M8 3c-1.552 0-2.94.707-3.857 1.818a.5.5 0 1 1-.771-.636A6.002 6.002 0 0 1 13.917 7H12.9A5.002 5.002 0 0 0 8 3zM3.1 9a5.002 5.002 0 0 0 8.757 2.182.5.5 0 1 1 .771.636A6.002 6.002 0 0 1 2.083 9H3.1z"/></svg>
  </button>
</div>

{#if $cameras.length === 0}
  <div class="empty-state">No cameras configured.</div>
{:else}
  <div class="camera-list">
    {#each $cameras as camera (camera.id)}
      {@const isPlaying = playingCameras.has(camera.id)}
      <div class="camera-row">
        <span class="camera-name mono">{camera.name}</span>
        <span class="camera-tags">
          <span class="tag">{camera.stream_type.toUpperCase()}</span>
          <span class="tag" class:tag-green={camera.enabled} class:tag-red={!camera.enabled}>{camera.enabled ? 'ON' : 'OFF'}</span>
        </span>
        <span class="camera-actions">
          <button class="btn btn-sm" onclick={() => togglePlay(camera.id)} title={isPlaying ? 'Stop' : 'Play'}>
            {#if isPlaying}
              <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M5 3.5h6A1.5 1.5 0 0 1 12.5 5v6a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 11V5A1.5 1.5 0 0 1 5 3.5z"/></svg>
            {:else}
              <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="m11.596 8.697-6.363 3.692c-.54.313-1.233-.066-1.233-.697V4.308c0-.63.692-1.01 1.233-.696l6.363 3.692a.802.802 0 0 1 0 1.393z"/></svg>
            {/if}
          </button>
          <button class="btn btn-sm" onclick={() => openEdit(camera)} title="Edit">
            <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M12.146.146a.5.5 0 0 1 .708 0l3 3a.5.5 0 0 1 0 .708l-10 10a.5.5 0 0 1-.168.11l-5 2a.5.5 0 0 1-.65-.65l2-5a.5.5 0 0 1 .11-.168l10-10zM11.207 2.5 13.5 4.793 14.793 3.5 12.5 1.207 11.207 2.5zm1.586 3L10.5 3.207 4 9.707V10h.5a.5.5 0 0 1 .5.5v.5h.5a.5.5 0 0 1 .5.5v.5h.293l6.5-6.5zm-9.761 5.175-.106.106-1.528 3.821 3.821-1.528.106-.106A.5.5 0 0 1 5 12.5V12h-.5a.5.5 0 0 1-.5-.5V11h-.5a.5.5 0 0 1-.468-.325z"/></svg>
          </button>
          <button class="btn btn-sm btn-danger" onclick={() => deleteCamera(camera)} title="Delete">
            <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M5.5 5.5A.5.5 0 0 1 6 6v6a.5.5 0 0 1-1 0V6a.5.5 0 0 1 .5-.5zm2.5 0a.5.5 0 0 1 .5.5v6a.5.5 0 0 1-1 0V6a.5.5 0 0 1 .5-.5zm3 .5a.5.5 0 0 0-1 0v6a.5.5 0 0 0 1 0V6z"/><path fill-rule="evenodd" d="M14.5 3a1 1 0 0 1-1 1H13v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V4h-.5a1 1 0 0 1-1-1V2a1 1 0 0 1 1-1H6a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1h3.5a1 1 0 0 1 1 1v1zM4.118 4 4 4.059V13a1 1 0 0 0 1 1h6a1 1 0 0 0 1-1V4.059L11.882 4H4.118zM2.5 3V2h11v1h-11z"/></svg>
          </button>
        </span>
      </div>
      {#if isPlaying}
        <div class="camera-stream-row">
          <VideoPlayer
            streamUrl={getStreamUrl(camera.id)}
            streamType={camera.stream_type}
            name={camera.name}
          />
        </div>
      {/if}
    {/each}
  </div>
{/if}

{#if showModal}
  <div class="modal-backdrop" onclick={() => showModal = false}>
    <div class="modal" onclick={(e) => e.stopPropagation()}>
      <div class="modal-header">
        <span class="modal-title">{editingId ? 'Edit' : 'Add'} Camera</span>
        <button class="btn btn-sm" onclick={() => showModal = false}>×</button>
      </div>
      <div class="modal-body">
        <div class="form-group">
          <label class="form-label">Name</label>
          <input class="form-input" type="text" bind:value={name} placeholder="Living Room" required>
        </div>
        <div class="form-group">
          <label class="form-label">Stream Type</label>
          <select class="form-select" bind:value={streamType}>
            <option value="mjpeg">MJPEG (HTTP)</option>
            <option value="rtsp">RTSP</option>
            <option value="webrtc">WebRTC</option>
          </select>
        </div>
        <div class="form-group">
          <label class="form-label">Stream URL</label>
          <input class="form-input" type="text" bind:value={url}
            placeholder={streamType === 'rtsp' ? 'rtsp://192.168.1.x:554/stream' : 'http://192.168.1.x:8000/video_feed'}
            required>
          {#if streamType === 'rtsp'}
            <div class="form-hint">H.264 streams are efficiently served as fMP4 (no transcoding)</div>
          {/if}
        </div>
        {#if streamType === 'rtsp'}
          <div class="form-group">
            <label class="form-label">Username (optional)</label>
            <input class="form-input" type="text" bind:value={username} placeholder="admin">
          </div>
          <div class="form-group">
            <label class="form-label">Password (optional)</label>
            <input class="form-input" type="password" bind:value={password}>
          </div>
        {/if}
      </div>
      <div class="modal-footer">
        <button class="btn" onclick={() => showModal = false}>Cancel</button>
        <button class="btn btn-primary" onclick={saveCamera}>{editingId ? 'Save' : 'Add'}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .camera-list {
    display: flex;
    flex-direction: column;
    gap: 1px;
    background: var(--border-color);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  .camera-row {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
    padding: var(--space-sm) var(--space-md);
    background: var(--bg-secondary);
    flex-wrap: wrap;
  }

  .camera-row:hover {
    background: var(--bg-tertiary);
  }

  .camera-name {
    min-width: 60px;
    flex: 1;
    font-size: var(--font-size-sm);
  }

  .camera-tags {
    display: flex;
    gap: var(--space-xs);
  }

  .camera-actions {
    display: flex;
    gap: var(--space-xs);
    flex-shrink: 0;
  }

  @media (min-width: 600px) {
    .camera-row {
      flex-wrap: nowrap;
      gap: var(--space-md);
    }

    .camera-name {
      min-width: 100px;
    }
  }

  .camera-stream-row {
    background: var(--bg-primary);
    aspect-ratio: 16 / 9;
    max-height: 300px;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }

</style>
