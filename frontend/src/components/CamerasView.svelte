<script lang="ts">
  import { cameras, loading, loadCameras } from '../lib/stores/index';
  import { api } from '../lib/api';
  import type { Camera } from '../lib/types';

  let showModal = $state(false);
  let name = $state('');
  let url = $state('');
  let streamType = $state<'mjpeg' | 'rtsp' | 'webrtc'>('mjpeg');
  let username = $state('');
  let password = $state('');

  let playingCameras = $state<Set<string>>(new Set());

  function resetForm() {
    name = '';
    url = '';
    streamType = 'mjpeg';
    username = '';
    password = '';
  }

  async function addCamera() {
    try {
      await api.addCamera(name, url, streamType, username || undefined, password || undefined);
      showModal = false;
      resetForm();
      loadCameras();
    } catch (e) {
      console.error('Failed to add camera:', e);
      alert('Failed to add camera');
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
  <button class="btn btn-primary" onclick={() => { resetForm(); showModal = true; }}>Add Camera</button>
  <button class="btn" onclick={() => loadCameras()} disabled={$loading.cameras}>Refresh</button>
  <div class="toolbar-spacer"></div>
  <span class="text-sm muted">{$cameras.length} camera(s)</span>
</div>

{#if $cameras.length === 0}
  <div class="empty-state">No cameras configured.</div>
{:else}
  <div class="camera-grid">
    {#each $cameras as camera (camera.id)}
      {@const isPlaying = playingCameras.has(camera.id)}
      <div class="card camera-card">
        <div class="card-header">
          <span class="card-title mono">{camera.name}</span>
          <div class="flex gap-xs">
            <span class="tag">{camera.stream_type.toUpperCase()}</span>
            <span class="tag" class:tag-green={camera.enabled}>{camera.enabled ? 'ON' : 'OFF'}</span>
          </div>
        </div>
        <div class="camera-stream">
          {#if camera.stream_type === 'webrtc'}
            <div class="stream-placeholder">WebRTC not supported in UI</div>
          {:else if isPlaying}
            <img src={getStreamUrl(camera.id)} alt={camera.name} class="stream-img">
          {:else}
            <div class="stream-placeholder">Click Play to start</div>
          {/if}
        </div>
        <div class="camera-controls">
          <button class="btn btn-sm" onclick={() => togglePlay(camera.id)}>
            {isPlaying ? 'Stop' : 'Play'}
          </button>
          <button class="btn btn-sm btn-danger" onclick={() => deleteCamera(camera)}>Delete</button>
        </div>
      </div>
    {/each}
  </div>
{/if}

{#if showModal}
  <div class="modal-backdrop" onclick={() => showModal = false}>
    <div class="modal" onclick={(e) => e.stopPropagation()}>
      <div class="modal-header">
        <span class="modal-title">Add Camera</span>
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
            <div class="form-hint">Format: rtsp://host:554/path</div>
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
        <button class="btn btn-primary" onclick={addCamera}>Add</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .camera-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(350px, 1fr));
    gap: var(--space-md);
  }

  .camera-card {
    display: flex;
    flex-direction: column;
  }

  .camera-stream {
    background: var(--bg-primary);
    border-radius: var(--radius-sm);
    aspect-ratio: 16 / 9;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
    margin: var(--space-sm) 0;
  }

  .stream-img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .stream-placeholder {
    color: var(--text-muted);
    font-size: var(--font-size-sm);
  }

  .camera-controls {
    display: flex;
    gap: var(--space-sm);
    justify-content: flex-end;
  }
</style>
