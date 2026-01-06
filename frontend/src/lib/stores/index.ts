// Svelte stores for Casita Assistant

import { writable, derived } from 'svelte/store';
import { api } from '../api';
import type { Device, Camera, Automation, NetworkStatus, SystemInfo } from '../types';
import { wsConnected } from '../websocket';

// Raw data stores
export const devices = writable<Device[]>([]);
export const cameras = writable<Camera[]>([]);
export const automations = writable<Automation[]>([]);
export const networkStatus = writable<NetworkStatus | null>(null);
export const systemInfo = writable<SystemInfo | null>(null);

// UI state
export const activeView = writable<'devices' | 'automations' | 'cameras' | 'status'>('devices');
export const permitJoinActive = writable(false);
export const permitJoinRemaining = writable(0);

// Loading states
export const loading = writable({
  devices: false,
  cameras: false,
  automations: false,
  network: false,
});

// Error state
export const lastError = writable<string | null>(null);
export const apiReachable = writable(true);
export const lastApiCheck = writable<Date | null>(null);

// Connection status types
export type ConnectionState = 'online' | 'offline' | 'degraded' | 'unknown';

export interface ConnectionStatus {
  backend: { state: ConnectionState; detail: string };
  coordinator: { state: ConnectionState; detail: string };
  zigbee: { state: ConnectionState; detail: string };
  websocket: { state: ConnectionState; detail: string };
}

// Derived connection status
export const connectionStatus = derived(
  [systemInfo, networkStatus, wsConnected, apiReachable],
  ([$systemInfo, $networkStatus, $wsConnected, $apiReachable]): ConnectionStatus => {
    // Backend API
    const backend: ConnectionStatus['backend'] = $apiReachable
      ? { state: 'online', detail: $systemInfo ? `v${$systemInfo.version}` : 'responding' }
      : { state: 'offline', detail: 'not reachable' };

    // Coordinator (ConBee II) - inferred from firmware presence
    const hasFirmware = $systemInfo?.firmware && $systemInfo.firmware !== 'Unknown';
    const coordinator: ConnectionStatus['coordinator'] = hasFirmware
      ? { state: 'online', detail: `fw:${$systemInfo!.firmware}` }
      : { state: 'offline', detail: 'not detected' };

    // Zigbee Network
    let zigbee: ConnectionStatus['zigbee'];
    if (!hasFirmware) {
      zigbee = { state: 'offline', detail: 'no coordinator' };
    } else if ($networkStatus?.connected) {
      zigbee = { state: 'online', detail: `ch:${$networkStatus.channel} ${$networkStatus.device_count} devices` };
    } else {
      zigbee = { state: 'degraded', detail: 'not joined' };
    }

    // WebSocket
    const websocket: ConnectionStatus['websocket'] = $wsConnected
      ? { state: 'online', detail: 'connected' }
      : { state: 'offline', detail: 'reconnecting...' };

    return { backend, coordinator, zigbee, websocket };
  }
);

// Aggregate health status
export const overallHealth = derived(connectionStatus, ($cs): ConnectionState => {
  const states = [
    $cs.backend.state,
    $cs.coordinator.state,
    $cs.zigbee.state,
    $cs.websocket.state,
  ];

  if (states.every(s => s === 'online')) return 'online';
  if (states.some(s => s === 'offline')) return 'degraded';
  if (states.some(s => s === 'degraded')) return 'degraded';
  return 'unknown';
});

// Derived stores
export const deviceCount = derived(devices, $devices => $devices.length);
export const enabledAutomations = derived(automations, $a => $a.filter(a => a.enabled).length);

// Helper: format IEEE address from bytes to colon-separated hex
export function formatIeee(bytes: number[]): string {
  if (Array.isArray(bytes)) {
    return bytes.slice().reverse().map(b => b.toString(16).padStart(2, '0')).join(':');
  }
  return String(bytes);
}

// Data loading functions
export async function loadDevices(): Promise<void> {
  loading.update(l => ({ ...l, devices: true }));
  try {
    const data = await api.getDevices();
    devices.set(data);
  } catch (e) {
    lastError.set(`Failed to load devices: ${e}`);
  } finally {
    loading.update(l => ({ ...l, devices: false }));
  }
}

export async function loadCameras(): Promise<void> {
  loading.update(l => ({ ...l, cameras: true }));
  try {
    const data = await api.getCameras();
    cameras.set(data);
  } catch (e) {
    lastError.set(`Failed to load cameras: ${e}`);
  } finally {
    loading.update(l => ({ ...l, cameras: false }));
  }
}

export async function loadAutomations(): Promise<void> {
  loading.update(l => ({ ...l, automations: true }));
  try {
    const data = await api.getAutomations();
    automations.set(data);
  } catch (e) {
    lastError.set(`Failed to load automations: ${e}`);
  } finally {
    loading.update(l => ({ ...l, automations: false }));
  }
}

export async function loadNetworkStatus(): Promise<void> {
  loading.update(l => ({ ...l, network: true }));
  try {
    const data = await api.getNetworkStatus();
    networkStatus.set(data);
  } catch (e) {
    lastError.set(`Failed to load network status: ${e}`);
  } finally {
    loading.update(l => ({ ...l, network: false }));
  }
}

export async function loadSystemInfo(): Promise<void> {
  try {
    const data = await api.getSystemInfo();
    systemInfo.set(data);
  } catch (e) {
    lastError.set(`Failed to load system info: ${e}`);
  }
}

export async function loadAll(): Promise<void> {
  await Promise.all([
    loadDevices(),
    loadCameras(),
    loadAutomations(),
    loadNetworkStatus(),
    loadSystemInfo(),
  ]);
}

// Permit join with countdown
let permitJoinTimer: ReturnType<typeof setInterval> | null = null;

export async function startPermitJoin(duration = 60): Promise<void> {
  try {
    await api.permitJoin(duration);
    permitJoinActive.set(true);
    permitJoinRemaining.set(duration);

    if (permitJoinTimer) clearInterval(permitJoinTimer);
    permitJoinTimer = setInterval(() => {
      permitJoinRemaining.update(r => {
        if (r <= 1) {
          clearInterval(permitJoinTimer!);
          permitJoinTimer = null;
          permitJoinActive.set(false);
          return 0;
        }
        return r - 1;
      });
    }, 1000);
  } catch (e) {
    lastError.set(`Failed to permit join: ${e}`);
  }
}
