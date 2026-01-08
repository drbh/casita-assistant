// WebSocket manager for Casita Assistant

import { writable } from 'svelte/store';
import type { WsEvent } from './types';

export const wsConnected = writable(false);

type EventHandler = (event: WsEvent) => void;

class WebSocketManager {
  private url: string;
  private ws: WebSocket | null = null;
  private handlers = new Map<string, EventHandler[]>();
  private reconnectDelay = 1000;
  private maxReconnectDelay = 30000;

  constructor() {
    // Determine WebSocket URL based on environment
    // Use VITE_API_URL if set, otherwise use current origin
    const apiUrl = import.meta.env.VITE_API_URL;
    if (apiUrl) {
      // Convert http(s) URL to ws(s) URL
      this.url = apiUrl.replace(/^http/, 'ws') + '/ws';
    } else {
      const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
      this.url = `${protocol}//${window.location.host}/ws`;
    }
  }

  connect(): void {
    console.log('[WS] Connecting to:', this.url);
    this.ws = new WebSocket(this.url);

    this.ws.onopen = () => {
      console.log('[WS] Connected');
      this.reconnectDelay = 1000;
      wsConnected.set(true);
    };

    this.ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data) as WsEvent;
        console.log('[WS] Message:', data.type);
        this.dispatch(data.type, data);
      } catch (e) {
        console.error('[WS] Failed to parse message:', e);
      }
    };

    this.ws.onclose = () => {
      console.log('[WS] Closed, reconnecting in', this.reconnectDelay, 'ms');
      wsConnected.set(false);
      setTimeout(() => this.connect(), this.reconnectDelay);
      this.reconnectDelay = Math.min(this.reconnectDelay * 2, this.maxReconnectDelay);
    };

    this.ws.onerror = (error) => {
      console.error('[WS] Error:', error);
    };
  }

  on(eventType: string, handler: EventHandler): () => void {
    if (!this.handlers.has(eventType)) {
      this.handlers.set(eventType, []);
    }
    this.handlers.get(eventType)!.push(handler);

    // Return unsubscribe function
    return () => this.off(eventType, handler);
  }

  off(eventType: string, handler: EventHandler): void {
    const handlers = this.handlers.get(eventType);
    if (handlers) {
      const index = handlers.indexOf(handler);
      if (index !== -1) handlers.splice(index, 1);
    }
  }

  private dispatch(eventType: string, data: WsEvent): void {
    const handlers = this.handlers.get(eventType) || [];
    handlers.forEach(handler => {
      try {
        handler(data);
      } catch (e) {
        console.error('[WS] Handler error:', e);
      }
    });
  }

  send(data: unknown): void {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(data));
    }
  }
}

export const ws = new WebSocketManager();
