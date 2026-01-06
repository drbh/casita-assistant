// API client for Casita Assistant

import type {
  Device, NetworkStatus, SystemInfo, Camera, Automation, CreateAutomationRequest
} from './types';

interface ApiResponse<T> {
  success: boolean;
  data?: T;
  error?: string;
}

class ApiClient {
  private baseUrl: string;

  constructor(baseUrl = '') {
    this.baseUrl = baseUrl;
  }

  private async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const options: RequestInit = {
      method,
      headers: { 'Content-Type': 'application/json' },
    };

    if (body) {
      options.body = JSON.stringify(body);
    }

    const response = await fetch(`${this.baseUrl}${path}`, options);
    const data: ApiResponse<T> = await response.json();

    if (!data.success) {
      throw new Error(data.error || 'Unknown error');
    }

    return data.data as T;
  }

  // System
  getSystemInfo(): Promise<SystemInfo> {
    return this.request('GET', '/api/v1/system/info');
  }

  // Network
  getNetworkStatus(): Promise<NetworkStatus> {
    return this.request('GET', '/api/v1/network/status');
  }

  permitJoin(duration = 60): Promise<{ duration: number }> {
    return this.request('POST', '/api/v1/network/permit-join', { duration });
  }

  // Devices
  getDevices(): Promise<Device[]> {
    return this.request('GET', '/api/v1/devices');
  }

  getDevice(ieee: string): Promise<Device> {
    return this.request('GET', `/api/v1/devices/${ieee}`);
  }

  updateDevice(ieee: string, friendlyName?: string, category?: string): Promise<Device> {
    return this.request('PUT', `/api/v1/devices/${ieee}`, {
      friendly_name: friendlyName || null,
      category: category || null,
    });
  }

  turnOn(ieee: string, endpoint: number): Promise<void> {
    return this.request('POST', `/api/v1/devices/${ieee}/endpoints/${endpoint}/on`);
  }

  turnOff(ieee: string, endpoint: number): Promise<void> {
    return this.request('POST', `/api/v1/devices/${ieee}/endpoints/${endpoint}/off`);
  }

  toggle(ieee: string, endpoint: number): Promise<void> {
    return this.request('POST', `/api/v1/devices/${ieee}/endpoints/${endpoint}/toggle`);
  }

  // Cameras
  getCameras(): Promise<Camera[]> {
    return this.request('GET', '/api/v1/cameras');
  }

  addCamera(
    name: string,
    streamUrl: string,
    streamType: string,
    username?: string,
    password?: string
  ): Promise<Camera> {
    const body: Record<string, unknown> = {
      name,
      stream_url: streamUrl,
      stream_type: streamType,
    };
    if (username) body.username = username;
    if (password) body.password = password;
    return this.request('POST', '/api/v1/cameras', body);
  }

  deleteCamera(id: string): Promise<void> {
    return this.request('DELETE', `/api/v1/cameras/${id}`);
  }

  getCameraStreamUrl(id: string): string {
    return `${this.baseUrl}/api/v1/cameras/${id}/stream`;
  }

  // Automations
  getAutomations(): Promise<Automation[]> {
    return this.request('GET', '/api/v1/automations');
  }

  getAutomation(id: string): Promise<Automation> {
    return this.request('GET', `/api/v1/automations/${id}`);
  }

  createAutomation(automation: CreateAutomationRequest): Promise<Automation> {
    return this.request('POST', '/api/v1/automations', automation);
  }

  updateAutomation(id: string, automation: Partial<CreateAutomationRequest>): Promise<Automation> {
    return this.request('PUT', `/api/v1/automations/${id}`, automation);
  }

  deleteAutomation(id: string): Promise<void> {
    return this.request('DELETE', `/api/v1/automations/${id}`);
  }

  triggerAutomation(id: string): Promise<void> {
    return this.request('POST', `/api/v1/automations/${id}/trigger`);
  }

  enableAutomation(id: string): Promise<Automation> {
    return this.request('POST', `/api/v1/automations/${id}/enable`);
  }

  disableAutomation(id: string): Promise<Automation> {
    return this.request('POST', `/api/v1/automations/${id}/disable`);
  }
}

export const api = new ApiClient();
