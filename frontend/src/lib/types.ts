// Type definitions for Casita Assistant

export interface Endpoint {
  id: number;
  in_clusters: number[];
  out_clusters: number[];
}

export interface Device {
  ieee_address: number[];
  nwk_address: number;
  device_type: 'Router' | 'EndDevice' | 'Coordinator';
  manufacturer?: string;
  model?: string;
  friendly_name?: string;
  category?: DeviceCategory;
  endpoints: Endpoint[];
  lqi?: number;
  state_on?: boolean;
}

export type DeviceCategory =
  | 'light' | 'outlet' | 'switch' | 'sensor'
  | 'lock' | 'thermostat' | 'fan' | 'blinds' | 'other';

export interface NetworkStatus {
  connected: boolean;
  channel: number;
  pan_id: number;
  extended_pan_id: string;
  permit_join: boolean;
  device_count: number;
}

export interface SystemInfo {
  name: string;
  version: string;
  firmware?: string;
}

export interface Camera {
  id: string;
  name: string;
  stream_url: string;
  stream_type: 'mjpeg' | 'rtsp' | 'webrtc';
  enabled: boolean;
  username?: string;
}

export interface Automation {
  id: string;
  name: string;
  description?: string;
  enabled: boolean;
  trigger: Trigger;
  actions: Action[];
}

export type Trigger =
  | { type: 'manual' }
  | { type: 'schedule'; schedule: Schedule }
  | { type: 'device_state'; device_ieee: string; state_change: StateChange };

export type Schedule =
  | { type: 'time_of_day'; time: string; days: number[] }
  | { type: 'interval'; seconds: number };

export interface StateChange {
  type: 'any' | 'turned_on' | 'turned_off' | 'toggled' | 'joined' | 'left' | 'available' | 'unavailable';
}

export type Action =
  | { type: 'device_control'; device_ieee: string; endpoint: number; command: Command }
  | { type: 'delay'; seconds: number }
  | { type: 'log'; message: string; level?: string };

export interface Command {
  type: 'turn_on' | 'turn_off' | 'toggle';
}

export interface CreateAutomationRequest {
  name: string;
  description?: string;
  trigger: Trigger;
  actions: Action[];
}

export interface WsEvent {
  type: string;
  [key: string]: unknown;
}
