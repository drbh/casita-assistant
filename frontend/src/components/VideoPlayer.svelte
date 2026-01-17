<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { untrack } from 'svelte';

  interface Props {
    streamUrl: string;
    streamType: 'mjpeg' | 'rtsp' | 'webrtc';
    name: string;
  }

  let { streamUrl, streamType, name }: Props = $props();

  let videoEl: HTMLVideoElement | null = $state(null);
  let error = $state<string | null>(null);
  let isLoading = $state(true);

  // Non-reactive state for MSE (to avoid infinite loops)
  let mediaSource: MediaSource | null = null;
  let sourceBuffer: SourceBuffer | null = null;
  let bufferQueue: ArrayBuffer[] = [];
  let isAppending = false;
  let initialized = false;

  // Use $effect only to detect when videoEl becomes available
  $effect(() => {
    if (streamType === 'rtsp' && videoEl && !initialized) {
      untrack(() => {
        initialized = true;
        initMSE();
      });
    }
  });

  onDestroy(() => {
    cleanup();
  });

  function cleanup() {
    if (mediaSource && mediaSource.readyState === 'open') {
      try {
        mediaSource.endOfStream();
      } catch (e) {
        // Ignore
      }
    }
    mediaSource = null;
    sourceBuffer = null;
    bufferQueue = [];
    isAppending = false;
    initialized = false;
  }

  async function initMSE() {
    if (!('MediaSource' in window)) {
      error = 'MediaSource API not supported';
      return;
    }

    if (mediaSource && mediaSource.readyState === 'open') {
      try {
        mediaSource.endOfStream();
      } catch (e) {
        // Ignore
      }
    }
    mediaSource = null;
    sourceBuffer = null;
    bufferQueue = [];
    isAppending = false;

    isLoading = true;
    error = null;

    mediaSource = new MediaSource();

    mediaSource.addEventListener('sourceopen', async () => {
      try {
        // Try High Profile first (most IP cameras use this)
        // Format: avc1.PPCCLL where PP=profile, CC=constraints, LL=level
        const codecsToTry = [
          'video/mp4; codecs="avc1.640028"', // High Profile Level 4.0
          'video/mp4; codecs="avc1.64001f"', // High Profile Level 3.1
          'video/mp4; codecs="avc1.4d401f"', // Main Profile Level 3.1
          'video/mp4; codecs="avc1.42E01E"', // Baseline Profile Level 3.0
        ];

        let supportedCodec: string | null = null;
        for (const codec of codecsToTry) {
          if (MediaSource.isTypeSupported(codec)) {
            supportedCodec = codec;
            break;
          }
        }

        if (!supportedCodec) {
          error = 'H.264 codec not supported';
          return;
        }

        console.log('Using codec:', supportedCodec);
        sourceBuffer = mediaSource!.addSourceBuffer(supportedCodec);

        sourceBuffer.mode = 'segments';

        sourceBuffer.addEventListener('updateend', () => {
          isAppending = false;
          appendNextBuffer();
        });

        sourceBuffer.addEventListener('error', (e) => {
          console.error('SourceBuffer error:', e);
          error = 'Stream decode error';
        });

        await fetchStream();

      } catch (e) {
        console.error('MSE init error:', e);
        error = `Failed to initialize: ${e}`;
      }
    });

    mediaSource.addEventListener('sourceended', () => {
      console.log('MediaSource ended');
    });

    mediaSource.addEventListener('sourceclose', () => {
      console.log('MediaSource closed');
    });

    if (videoEl) {
      videoEl.src = URL.createObjectURL(mediaSource);
    }
  }

  async function fetchStream() {
    try {
      console.log('Fetching stream:', streamUrl);
      const response = await fetch(streamUrl);

      if (!response.ok) {
        error = `Stream error: ${response.status}`;
        isLoading = false;
        return;
      }

      const reader = response.body?.getReader();
      if (!reader) {
        error = 'No stream body';
        isLoading = false;
        return;
      }

      isLoading = false;
      console.log('Stream connected, reading data...');

      while (true) {
        const { done, value } = await reader.read();

        if (done) {
          console.log('Stream ended');
          break;
        }

        if (value && sourceBuffer && mediaSource?.readyState === 'open') {
          bufferQueue.push(value.buffer);
          appendNextBuffer();
        }
      }

    } catch (e) {
      console.error('Fetch error:', e);
      error = `Connection error: ${e}`;
      isLoading = false;
    }
  }

  function appendNextBuffer() {
    if (isAppending || !sourceBuffer || bufferQueue.length === 0) {
      return;
    }

    if (sourceBuffer.updating) {
      return;
    }

    if (mediaSource?.readyState !== 'open') {
      console.warn('MediaSource not open, skipping append');
      bufferQueue = []; // Clear queue since we can't append
      return;
    }

    // Remove old data to prevent buffer overflow
    try {
      if (videoEl && sourceBuffer.buffered.length > 0) {
        const currentTime = videoEl.currentTime;
        const bufferStart = sourceBuffer.buffered.start(0);

        // Keep only 30 seconds of buffer behind current time
        if (currentTime - bufferStart > 30) {
          sourceBuffer.remove(bufferStart, currentTime - 10);
          return;
        }
      }
    } catch (e) {
      // ignore
    }

    isAppending = true;
    const buffer = bufferQueue.shift()!;

    try {
      sourceBuffer.appendBuffer(buffer);
    } catch (e: any) {
      console.error('Append error:', e?.name, e?.message);
      isAppending = false;

      if (e instanceof DOMException && e.name === 'QuotaExceededError') {
        if (sourceBuffer.buffered.length > 0) {
          const start = sourceBuffer.buffered.start(0);
          const end = sourceBuffer.buffered.end(0);
          if (end - start > 5) {
            try {
              sourceBuffer.remove(start, start + (end - start) / 2);
            } catch (removeErr) {
              console.error('Remove error:', removeErr);
            }
          }
        }
      } else {
        bufferQueue = [];
      }
    }
  }

  function handleVideoError(e: Event) {
    const video = e.target as HTMLVideoElement;
    const mediaError = video.error;

    if (mediaError) {
      switch (mediaError.code) {
        case MediaError.MEDIA_ERR_ABORTED:
          error = 'Playback aborted';
          break;
        case MediaError.MEDIA_ERR_NETWORK:
          error = 'Network error';
          break;
        case MediaError.MEDIA_ERR_DECODE:
          error = 'Decode error';
          break;
        case MediaError.MEDIA_ERR_SRC_NOT_SUPPORTED:
          error = 'Format not supported';
          break;
        default:
          error = 'Unknown error';
      }
    }
  }

  function handleImgLoad() {
    isLoading = false;
    error = null;
  }

  function handleImgError() {
    isLoading = false;
    error = 'Failed to load stream';
  }
</script>

<div class="video-player">
  {#if error}
    <div class="error-overlay">
      <span class="error-icon">!</span>
      <span>{error}</span>
    </div>
  {:else if isLoading}
    <div class="loading-overlay">
      <span class="loading-spinner"></span>
      <span>Connecting...</span>
    </div>
  {/if}

  {#if streamType === 'mjpeg'}
    <img
      src={streamUrl}
      alt={name}
      class="stream-media"
      onload={handleImgLoad}
      onerror={handleImgError}
    >
  {:else if streamType === 'rtsp'}
    <video
      bind:this={videoEl}
      class="stream-media"
      autoplay
      muted
      playsinline
      onerror={handleVideoError}
    ></video>
  {:else}
    <div class="unsupported">
      <span>WebRTC not yet supported</span>
    </div>
  {/if}
</div>

<style>
  .video-player {
    position: relative;
    width: 100%;
    height: 100%;
    background: var(--bg-primary);
  }

  .stream-media {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .loading-overlay,
  .error-overlay,
  .unsupported {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-sm);
    color: var(--text-muted);
    font-size: var(--font-size-sm);
    background: var(--bg-primary);
  }

  .error-overlay {
    color: var(--color-danger);
  }

  .error-icon {
    width: 24px;
    height: 24px;
    border: 2px solid currentColor;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: bold;
  }

  .loading-spinner {
    width: 20px;
    height: 20px;
    border: 2px solid var(--border-color);
    border-top-color: var(--text-secondary);
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
