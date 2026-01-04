// Casita Assistant - Main Application

import { Api } from './api.js';
import { WebSocketManager } from './websocket.js';

class App {
    constructor() {
        this.api = new Api();
        this.ws = new WebSocketManager(`ws://${location.host}/ws`);
        this.devices = [];

        this.init();
    }

    init() {
        // Set up navigation
        this.setupNavigation();

        // Set up button handlers
        this.setupButtons();

        // Set up WebSocket
        this.setupWebSocket();

        // Initial data load
        this.loadData();
    }

    setupNavigation() {
        const navButtons = document.querySelectorAll('nav button');
        navButtons.forEach(btn => {
            btn.addEventListener('click', () => {
                const viewName = btn.dataset.view;
                this.switchView(viewName);

                navButtons.forEach(b => b.classList.remove('active'));
                btn.classList.add('active');
            });
        });
    }

    switchView(viewName) {
        document.querySelectorAll('.view').forEach(v => v.classList.remove('active'));
        document.getElementById(`${viewName}-view`).classList.add('active');

        // Refresh data when switching views
        if (viewName === 'network') {
            this.loadNetworkInfo();
        } else if (viewName === 'devices') {
            this.loadDevices();
        }
    }

    setupButtons() {
        // Permit Join button
        const permitJoinBtn = document.getElementById('permit-join-btn');
        permitJoinBtn.addEventListener('click', async () => {
            permitJoinBtn.disabled = true;
            permitJoinBtn.textContent = 'Joining...';

            try {
                await this.api.permitJoin(60);
                permitJoinBtn.textContent = 'Permit Join Active';

                // Reset after 60 seconds
                setTimeout(() => {
                    permitJoinBtn.disabled = false;
                    permitJoinBtn.textContent = 'Permit Join (60s)';
                }, 60000);
            } catch (e) {
                console.error('Failed to permit join:', e);
                permitJoinBtn.disabled = false;
                permitJoinBtn.textContent = 'Permit Join (60s)';
            }
        });

        // Refresh button
        const refreshBtn = document.getElementById('refresh-btn');
        refreshBtn.addEventListener('click', () => this.loadDevices());
    }

    setupWebSocket() {
        // Connection status
        this.ws.onStatusChange = (connected) => {
            const wsStatus = document.getElementById('ws-status');
            wsStatus.classList.toggle('connected', connected);
            wsStatus.classList.toggle('disconnected', !connected);
        };

        // Event handlers
        this.ws.on('connected', () => {
            console.log('WebSocket connected event');
        });

        this.ws.on('device_joined', (event) => {
            console.log('Device joined:', event.ieee_address);
            this.loadDevices();
        });

        this.ws.on('device_left', (event) => {
            console.log('Device left:', event.ieee_address);
            this.loadDevices();
        });

        this.ws.on('device_updated', (event) => {
            console.log('Device updated:', event.ieee_address);
            this.loadDevices();
        });

        this.ws.on('network_state_changed', (event) => {
            console.log('Network state changed:', event.connected);
            this.updateNetworkStatus(event.connected);
        });

        // Connect
        this.ws.connect();
    }

    async loadData() {
        await Promise.all([
            this.loadNetworkInfo(),
            this.loadDevices(),
            this.loadSystemInfo(),
        ]);
    }

    async loadNetworkInfo() {
        try {
            const status = await this.api.getNetworkStatus();
            this.updateNetworkStatus(status.connected);

            document.getElementById('net-status').textContent =
                status.connected ? 'Connected' : 'Disconnected';
            document.getElementById('net-channel').textContent = status.channel;
            document.getElementById('net-panid').textContent =
                `0x${status.pan_id.toString(16).padStart(4, '0').toUpperCase()}`;
            document.getElementById('net-extpanid').textContent = status.extended_pan_id;
            document.getElementById('net-permitjoin').textContent =
                status.permit_join ? 'Yes' : 'No';
            document.getElementById('net-devices').textContent = status.device_count;
        } catch (e) {
            console.error('Failed to load network info:', e);
        }
    }

    async loadSystemInfo() {
        try {
            const info = await this.api.getSystemInfo();
            document.getElementById('sys-name').textContent = info.name;
            document.getElementById('sys-version').textContent = info.version;
            document.getElementById('sys-firmware').textContent = info.firmware || 'Unknown';
        } catch (e) {
            console.error('Failed to load system info:', e);
        }
    }

    async loadDevices() {
        try {
            this.devices = await this.api.getDevices();
            this.renderDevices();
        } catch (e) {
            console.error('Failed to load devices:', e);
        }
    }

    updateNetworkStatus(connected) {
        const statusEl = document.getElementById('network-status');
        statusEl.classList.toggle('connected', connected);
        statusEl.classList.toggle('disconnected', !connected);
        statusEl.querySelector('.status-text').textContent =
            connected ? 'Connected' : 'Disconnected';
    }

    renderDevices() {
        const container = document.getElementById('device-list');

        if (this.devices.length === 0) {
            container.innerHTML = `
                <p class="empty-state">
                    No devices found. Click "Permit Join" to pair new devices.
                </p>
            `;
            return;
        }

        container.innerHTML = this.devices.map(device => this.renderDeviceCard(device)).join('');

        // Attach toggle button handlers
        container.querySelectorAll('.toggle-btn').forEach(btn => {
            btn.addEventListener('click', async (e) => {
                const card = e.target.closest('.device-card');
                const ieee = card.dataset.ieee;
                const endpoint = parseInt(btn.dataset.endpoint, 10);

                btn.disabled = true;
                try {
                    await this.api.toggle(ieee, endpoint);
                    btn.classList.toggle('on');
                    btn.classList.toggle('off');
                } catch (err) {
                    console.error('Toggle failed:', err);
                } finally {
                    btn.disabled = false;
                }
            });
        });
    }

    renderDeviceCard(device) {
        const name = device.friendly_name || device.model || this.formatIeee(device.ieee_address);
        const ieee = this.formatIeee(device.ieee_address);

        return `
            <div class="device-card" data-ieee="${ieee}">
                <h3>${this.escapeHtml(name)}</h3>
                <div class="device-info">
                    <span class="device-type">${device.device_type}</span>
                    ${device.manufacturer ? `<span> - ${this.escapeHtml(device.manufacturer)}</span>` : ''}
                </div>
                <div class="device-info">
                    IEEE: ${ieee}
                </div>
                <div class="device-info">
                    NWK: 0x${device.nwk_address.toString(16).padStart(4, '0').toUpperCase()}
                    ${device.lqi !== null ? ` | LQI: ${device.lqi}` : ''}
                </div>
                ${this.renderEndpoints(device)}
            </div>
        `;
    }

    renderEndpoints(device) {
        const lightEndpoints = device.endpoints.filter(ep =>
            ep.in_clusters.includes(0x0006) || // On/Off
            ep.in_clusters.includes(0x0008)    // Level Control
        );

        // If we have discovered endpoints with On/Off capability, show those
        if (lightEndpoints.length > 0) {
            return `
                <div class="device-controls">
                    ${lightEndpoints.map(ep => `
                        <button class="toggle-btn off" data-endpoint="${ep.id}">
                            EP ${ep.id}
                        </button>
                    `).join('')}
                </div>
            `;
        }

        // Otherwise show a default toggle for endpoint 1 (most common)
        return `
            <div class="device-controls">
                <button class="toggle-btn off" data-endpoint="1">Toggle</button>
            </div>
        `;
    }

    formatIeee(bytes) {
        if (Array.isArray(bytes)) {
            return bytes.slice().reverse().map(b =>
                b.toString(16).padStart(2, '0')
            ).join(':');
        }
        return bytes;
    }

    escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }
}

// Start the app
document.addEventListener('DOMContentLoaded', () => {
    window.app = new App();
});
