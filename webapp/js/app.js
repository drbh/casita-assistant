// Casita Assistant - Main Application

import { Api } from './api.js';
import { WebSocketManager } from './websocket.js';

class App {
    constructor() {
        this.api = new Api();
        this.ws = new WebSocketManager(`ws://${location.host}/ws`);
        this.devices = [];
        this.cameras = [];
        this.automations = [];
        this.editingAutomation = null;
        this.actionCounter = 0;

        this.init();
    }

    init() {
        // Set up navigation
        this.setupNavigation();

        // Set up button handlers
        this.setupButtons();

        // Set up camera modal
        this.setupCameraModal();

        // Set up device edit modal
        this.setupDeviceModal();

        // Set up automation modal
        this.setupAutomationModal();

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
        } else if (viewName === 'cameras') {
            this.loadCameras();
        } else if (viewName === 'automations') {
            this.loadAutomations();
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

        // Automation events
        this.ws.on('automation_triggered', (event) => {
            console.log('Automation triggered:', event.automation_id, event.trigger_reason);
            this.showNotification(`Automation triggered: ${event.automation_id}`);
        });

        this.ws.on('automation_created', () => {
            this.loadAutomations();
        });

        this.ws.on('automation_updated', () => {
            this.loadAutomations();
        });

        this.ws.on('automation_deleted', () => {
            this.loadAutomations();
        });

        // Connect
        this.ws.connect();
    }

    showNotification(message) {
        // Simple notification - could be enhanced with a toast library
        console.log('Notification:', message);
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

        // Attach edit button handlers
        container.querySelectorAll('.edit-device-btn').forEach(btn => {
            btn.addEventListener('click', () => {
                const ieee = btn.dataset.ieee;
                this.openDeviceEditModal(ieee);
            });
        });
    }

    renderDeviceCard(device) {
        const name = device.friendly_name || device.model || this.formatIeee(device.ieee_address);
        const ieee = this.formatIeee(device.ieee_address);
        const category = device.category || 'other';
        const categoryLabel = this.getCategoryLabel(category);
        const stateText = device.state_on === true ? 'On' : device.state_on === false ? 'Off' : '';

        return `
            <div class="device-card" data-ieee="${ieee}">
                <div class="device-header">
                    <h3>${this.escapeHtml(name)}</h3>
                    <button class="edit-device-btn btn btn-small" data-ieee="${ieee}" title="Edit device">Edit</button>
                </div>
                <div class="device-info">
                    <span class="device-category">${categoryLabel}</span>
                    <span class="device-role">${device.device_type}</span>
                    ${stateText ? `<span class="device-state ${device.state_on ? 'on' : 'off'}">${stateText}</span>` : ''}
                </div>
                <div class="device-info">
                    ${device.manufacturer ? `${this.escapeHtml(device.manufacturer)} - ` : ''}IEEE: ${ieee}
                </div>
                <div class="device-info">
                    NWK: 0x${device.nwk_address.toString(16).padStart(4, '0').toUpperCase()}
                    ${device.lqi !== null ? ` | LQI: ${device.lqi}` : ''}
                </div>
                ${this.renderEndpoints(device)}
            </div>
        `;
    }

    getCategoryLabel(category) {
        const labels = {
            light: 'Light',
            outlet: 'Outlet',
            switch: 'Switch',
            sensor: 'Sensor',
            lock: 'Lock',
            thermostat: 'Thermostat',
            fan: 'Fan',
            blinds: 'Blinds',
            other: 'Other'
        };
        return labels[category] || 'Other';
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

    // Camera methods
    setupCameraModal() {
        const modal = document.getElementById('add-camera-modal');
        const addBtn = document.getElementById('add-camera-btn');
        const cancelBtn = document.getElementById('cancel-camera-btn');
        const form = document.getElementById('add-camera-form');
        const typeSelect = document.getElementById('camera-type');
        const rtspCredentials = document.getElementById('rtsp-credentials');
        const urlInput = document.getElementById('camera-url');
        const urlHint = document.getElementById('camera-url-hint');

        // Show/hide RTSP credentials based on stream type
        typeSelect.addEventListener('change', () => {
            const isRtsp = typeSelect.value === 'rtsp';
            rtspCredentials.style.display = isRtsp ? 'block' : 'none';
            urlInput.placeholder = isRtsp
                ? 'rtsp://192.168.1.166:554/stream1'
                : 'http://192.168.1.100:8000/video_feed';
            urlHint.style.display = isRtsp ? 'block' : 'none';
        });

        addBtn.addEventListener('click', () => {
            modal.classList.remove('hidden');
        });

        cancelBtn.addEventListener('click', () => {
            modal.classList.add('hidden');
            form.reset();
            rtspCredentials.style.display = 'none';
        });

        // Close modal on outside click
        modal.addEventListener('click', (e) => {
            if (e.target === modal) {
                modal.classList.add('hidden');
                form.reset();
                rtspCredentials.style.display = 'none';
            }
        });

        form.addEventListener('submit', async (e) => {
            e.preventDefault();
            const name = document.getElementById('camera-name').value;
            const url = document.getElementById('camera-url').value;
            const type = document.getElementById('camera-type').value;
            const username = document.getElementById('camera-username').value || null;
            const password = document.getElementById('camera-password').value || null;

            try {
                await this.api.addCamera(name, url, type, username, password);
                modal.classList.add('hidden');
                form.reset();
                rtspCredentials.style.display = 'none';
                this.loadCameras();
            } catch (err) {
                console.error('Failed to add camera:', err);
                alert('Failed to add camera: ' + err.message);
            }
        });
    }

    async loadCameras() {
        try {
            this.cameras = await this.api.getCameras();
            this.renderCameras();
        } catch (e) {
            console.error('Failed to load cameras:', e);
        }
    }

    renderCameras() {
        const container = document.getElementById('camera-list');

        if (this.cameras.length === 0) {
            container.innerHTML = `
                <p class="empty-state">
                    No cameras configured. Click "Add Camera" to add one.
                </p>
            `;
            return;
        }

        container.innerHTML = this.cameras.map(camera => this.renderCameraCard(camera)).join('');

        // Attach play/pause button handlers
        container.querySelectorAll('.play-pause-btn').forEach(btn => {
            btn.addEventListener('click', (e) => {
                const card = e.target.closest('.camera-card');
                const img = card.querySelector('.camera-stream img');
                const placeholder = card.querySelector('.paused-message');
                const isPlaying = btn.dataset.playing === 'true';

                if (isPlaying) {
                    // Pause - stop the stream
                    img.src = '';
                    img.style.display = 'none';
                    if (placeholder) placeholder.style.display = 'block';
                    btn.textContent = 'Play';
                    btn.dataset.playing = 'false';
                    btn.title = 'Play stream';
                } else {
                    // Play - start the stream
                    const streamUrl = img.dataset.streamUrl;
                    img.src = streamUrl;
                    img.style.display = 'block';
                    if (placeholder) placeholder.style.display = 'none';
                    btn.textContent = 'Pause';
                    btn.dataset.playing = 'true';
                    btn.title = 'Pause stream';
                }
            });
        });

        // Attach delete button handlers
        container.querySelectorAll('.delete-camera-btn').forEach(btn => {
            btn.addEventListener('click', async (e) => {
                const card = e.target.closest('.camera-card');
                const id = card.dataset.id;
                const name = card.dataset.name;

                if (confirm(`Delete camera "${name}"?`)) {
                    try {
                        await this.api.deleteCamera(id);
                        this.loadCameras();
                    } catch (err) {
                        console.error('Failed to delete camera:', err);
                    }
                }
            });
        });
    }

    renderCameraCard(camera) {
        const streamUrl = this.api.getCameraStreamUrl(camera.id);
        const isWebRTC = camera.stream_type === 'webrtc';

        return `
            <div class="camera-card" data-id="${camera.id}" data-name="${this.escapeHtml(camera.name)}" data-stream-url="${streamUrl}">
                <div class="camera-header">
                    <h3>${this.escapeHtml(camera.name)}</h3>
                    <div class="camera-controls">
                        <button class="play-pause-btn btn btn-small" data-playing="false" title="Play stream">Play</button>
                        <button class="delete-camera-btn btn btn-small btn-danger">Delete</button>
                    </div>
                </div>
                <div class="camera-stream">
                    ${isWebRTC
                        ? `<p class="stream-placeholder">WebRTC streams not yet supported in UI</p>`
                        : `<img src="" alt="${this.escapeHtml(camera.name)}" data-stream-url="${streamUrl}">
                           <p class="stream-placeholder paused-message">Click Play to start stream</p>`
                    }
                </div>
                <div class="camera-info">
                    <span class="camera-type">${camera.stream_type.toUpperCase()}</span>
                    <span class="camera-status ${camera.enabled ? 'enabled' : 'disabled'}">
                        ${camera.enabled ? 'Enabled' : 'Disabled'}
                    </span>
                </div>
            </div>
        `;
    }

    // ========================================================================
    // Device edit methods
    // ========================================================================

    setupDeviceModal() {
        const modal = document.getElementById('edit-device-modal');
        const cancelBtn = document.getElementById('cancel-device-btn');
        const form = document.getElementById('edit-device-form');

        cancelBtn.addEventListener('click', () => {
            modal.classList.add('hidden');
            form.reset();
        });

        // Close modal on outside click
        modal.addEventListener('click', (e) => {
            if (e.target === modal) {
                modal.classList.add('hidden');
                form.reset();
            }
        });

        form.addEventListener('submit', async (e) => {
            e.preventDefault();
            const ieee = document.getElementById('edit-device-ieee').value;
            const friendlyName = document.getElementById('edit-device-name').value;
            const category = document.getElementById('edit-device-category').value;

            try {
                await this.api.updateDevice(ieee, friendlyName, category);
                modal.classList.add('hidden');
                form.reset();
                this.loadDevices();
            } catch (err) {
                console.error('Failed to update device:', err);
                alert('Failed to update device: ' + err.message);
            }
        });
    }

    openDeviceEditModal(ieee) {
        const device = this.devices.find(d => this.formatIeee(d.ieee_address) === ieee);
        if (!device) return;

        document.getElementById('edit-device-ieee').value = ieee;
        document.getElementById('edit-device-name').value = device.friendly_name || '';
        document.getElementById('edit-device-category').value = device.category || 'other';

        // Show device info
        const infoEl = document.getElementById('edit-device-info');
        infoEl.innerHTML = `
            <p><strong>IEEE:</strong> ${ieee}</p>
            <p><strong>Model:</strong> ${device.model || 'Unknown'}</p>
            <p><strong>Manufacturer:</strong> ${device.manufacturer || 'Unknown'}</p>
        `;

        document.getElementById('edit-device-modal').classList.remove('hidden');
    }

    // ========================================================================
    // Automation methods
    // ========================================================================

    setupAutomationModal() {
        const modal = document.getElementById('automation-modal');
        const addBtn = document.getElementById('add-automation-btn');
        const cancelBtn = document.getElementById('cancel-automation-btn');
        const form = document.getElementById('automation-form');
        const triggerType = document.getElementById('trigger-type');
        const scheduleType = document.getElementById('schedule-type');
        const addActionBtn = document.getElementById('add-action-btn');
        const refreshBtn = document.getElementById('refresh-automations-btn');

        // Trigger type change handler
        triggerType.addEventListener('change', () => {
            document.querySelectorAll('.trigger-options').forEach(el => el.classList.add('hidden'));
            if (triggerType.value === 'schedule') {
                document.getElementById('schedule-options').classList.remove('hidden');
            } else if (triggerType.value === 'device_state') {
                document.getElementById('device-state-options').classList.remove('hidden');
                this.populateTriggerDeviceSelect();
            }
        });

        // Schedule type change handler
        scheduleType.addEventListener('change', () => {
            document.getElementById('time-of-day-options').classList.toggle('hidden', scheduleType.value !== 'time_of_day');
            document.getElementById('interval-options').classList.toggle('hidden', scheduleType.value !== 'interval');
        });

        // Add automation button
        addBtn.addEventListener('click', () => {
            this.editingAutomation = null;
            this.resetAutomationForm();
            document.getElementById('automation-modal-title').textContent = 'Add Automation';
            modal.classList.remove('hidden');
        });

        // Cancel button
        cancelBtn.addEventListener('click', () => {
            modal.classList.add('hidden');
            this.resetAutomationForm();
        });

        // Close on outside click
        modal.addEventListener('click', (e) => {
            if (e.target === modal) {
                modal.classList.add('hidden');
                this.resetAutomationForm();
            }
        });

        // Add action button
        addActionBtn.addEventListener('click', () => {
            this.addActionRow();
        });

        // Refresh button
        refreshBtn.addEventListener('click', () => this.loadAutomations());

        // Form submit
        form.addEventListener('submit', async (e) => {
            e.preventDefault();
            await this.saveAutomation();
        });
    }

    resetAutomationForm() {
        const form = document.getElementById('automation-form');
        form.reset();
        document.getElementById('automation-id').value = '';
        document.querySelectorAll('.trigger-options').forEach(el => el.classList.add('hidden'));
        document.getElementById('actions-list').innerHTML = '';
        this.actionCounter = 0;
        // Add one default action
        this.addActionRow();
    }

    populateTriggerDeviceSelect() {
        const select = document.getElementById('trigger-device');
        select.innerHTML = '<option value="">Select a device...</option>';
        this.devices.forEach(device => {
            const name = device.friendly_name || device.model || this.formatIeee(device.ieee_address);
            const ieee = this.formatIeee(device.ieee_address);
            select.innerHTML += `<option value="${ieee}">${this.escapeHtml(name)}</option>`;
        });
    }

    addActionRow(action = null) {
        const container = document.getElementById('actions-list');
        const actionId = this.actionCounter++;

        const div = document.createElement('div');
        div.className = 'action-row';
        div.dataset.actionId = actionId;
        div.innerHTML = `
            <div class="action-fields">
                <select class="action-type" data-action-id="${actionId}">
                    <option value="device_control">Device Control</option>
                    <option value="delay">Delay</option>
                    <option value="log">Log Message</option>
                </select>
                <div class="action-device-fields" data-action-id="${actionId}">
                    <select class="action-device" data-action-id="${actionId}">
                        <option value="">Select device...</option>
                        ${this.devices.map(d => {
                            const name = d.friendly_name || d.model || this.formatIeee(d.ieee_address);
                            const ieee = this.formatIeee(d.ieee_address);
                            return `<option value="${ieee}">${this.escapeHtml(name)}</option>`;
                        }).join('')}
                    </select>
                    <select class="action-endpoint" data-action-id="${actionId}">
                        <option value="1">Endpoint 1</option>
                    </select>
                    <select class="action-command" data-action-id="${actionId}">
                        <option value="turn_on">Turn On</option>
                        <option value="turn_off">Turn Off</option>
                        <option value="toggle">Toggle</option>
                    </select>
                </div>
                <div class="action-delay-fields hidden" data-action-id="${actionId}">
                    <input type="number" class="action-delay-seconds" placeholder="Seconds" value="5" min="1">
                </div>
                <div class="action-log-fields hidden" data-action-id="${actionId}">
                    <input type="text" class="action-log-message" placeholder="Log message">
                </div>
                <button type="button" class="btn btn-small btn-danger remove-action-btn">×</button>
            </div>
        `;

        // Handle action type change
        const typeSelect = div.querySelector('.action-type');
        typeSelect.addEventListener('change', () => {
            const deviceFields = div.querySelector('.action-device-fields');
            const delayFields = div.querySelector('.action-delay-fields');
            const logFields = div.querySelector('.action-log-fields');

            deviceFields.classList.toggle('hidden', typeSelect.value !== 'device_control');
            delayFields.classList.toggle('hidden', typeSelect.value !== 'delay');
            logFields.classList.toggle('hidden', typeSelect.value !== 'log');
        });

        // Handle device change to populate endpoints
        const deviceSelect = div.querySelector('.action-device');
        deviceSelect.addEventListener('change', () => {
            const ieee = deviceSelect.value;
            const device = this.devices.find(d => this.formatIeee(d.ieee_address) === ieee);
            const endpointSelect = div.querySelector('.action-endpoint');

            if (device && device.endpoints.length > 0) {
                endpointSelect.innerHTML = device.endpoints.map(ep =>
                    `<option value="${ep.id}">Endpoint ${ep.id}</option>`
                ).join('');
            } else {
                endpointSelect.innerHTML = '<option value="1">Endpoint 1</option>';
            }
        });

        // Handle remove button
        div.querySelector('.remove-action-btn').addEventListener('click', () => {
            div.remove();
        });

        container.appendChild(div);

        // Populate with existing action data if provided
        if (action) {
            if (action.type === 'device_control') {
                typeSelect.value = 'device_control';
                deviceSelect.value = action.device_ieee;
                div.querySelector('.action-endpoint').value = action.endpoint;
                div.querySelector('.action-command').value = action.command.type;
            } else if (action.type === 'delay') {
                typeSelect.value = 'delay';
                div.querySelector('.action-delay-seconds').value = action.seconds;
            } else if (action.type === 'log') {
                typeSelect.value = 'log';
                div.querySelector('.action-log-message').value = action.message;
            }
            typeSelect.dispatchEvent(new Event('change'));
        }
    }

    async loadAutomations() {
        try {
            this.automations = await this.api.getAutomations();
            this.renderAutomations();
        } catch (e) {
            console.error('Failed to load automations:', e);
        }
    }

    renderAutomations() {
        const container = document.getElementById('automation-list');

        if (this.automations.length === 0) {
            container.innerHTML = `
                <p class="empty-state">
                    No automations configured. Click "Add Automation" to create one.
                </p>
            `;
            return;
        }

        container.innerHTML = this.automations.map(a => this.renderAutomationCard(a)).join('');

        // Attach event handlers
        container.querySelectorAll('.toggle-automation-btn').forEach(btn => {
            btn.addEventListener('click', async (e) => {
                const card = e.target.closest('.automation-card');
                const id = card.dataset.id;
                const enabled = btn.dataset.enabled === 'true';

                btn.disabled = true;
                try {
                    if (enabled) {
                        await this.api.disableAutomation(id);
                    } else {
                        await this.api.enableAutomation(id);
                    }
                    this.loadAutomations();
                } catch (err) {
                    console.error('Failed to toggle automation:', err);
                } finally {
                    btn.disabled = false;
                }
            });
        });

        container.querySelectorAll('.trigger-automation-btn').forEach(btn => {
            btn.addEventListener('click', async (e) => {
                const card = e.target.closest('.automation-card');
                const id = card.dataset.id;

                btn.disabled = true;
                try {
                    await this.api.triggerAutomation(id);
                    btn.textContent = 'Triggered!';
                    setTimeout(() => {
                        btn.textContent = 'Run Now';
                        btn.disabled = false;
                    }, 1000);
                } catch (err) {
                    console.error('Failed to trigger automation:', err);
                    btn.disabled = false;
                }
            });
        });

        container.querySelectorAll('.edit-automation-btn').forEach(btn => {
            btn.addEventListener('click', (e) => {
                const card = e.target.closest('.automation-card');
                const id = card.dataset.id;
                this.editAutomation(id);
            });
        });

        container.querySelectorAll('.delete-automation-btn').forEach(btn => {
            btn.addEventListener('click', async (e) => {
                const card = e.target.closest('.automation-card');
                const id = card.dataset.id;
                const name = card.dataset.name;

                if (confirm(`Delete automation "${name}"?`)) {
                    try {
                        await this.api.deleteAutomation(id);
                        this.loadAutomations();
                    } catch (err) {
                        console.error('Failed to delete automation:', err);
                    }
                }
            });
        });
    }

    renderAutomationCard(automation) {
        const triggerText = this.getTriggerDescription(automation.trigger);
        const actionCount = automation.actions.length;

        return `
            <div class="automation-card ${automation.enabled ? '' : 'disabled'}"
                 data-id="${automation.id}"
                 data-name="${this.escapeHtml(automation.name)}">
                <div class="automation-header">
                    <h3>${this.escapeHtml(automation.name)}</h3>
                    <div class="automation-controls">
                        <button class="toggle-automation-btn btn btn-small ${automation.enabled ? 'btn-success' : ''}"
                                data-enabled="${automation.enabled}">
                            ${automation.enabled ? 'Enabled' : 'Disabled'}
                        </button>
                    </div>
                </div>
                ${automation.description ? `<p class="automation-description">${this.escapeHtml(automation.description)}</p>` : ''}
                <div class="automation-info">
                    <div class="automation-trigger">
                        <span class="label">Trigger:</span> ${triggerText}
                    </div>
                    <div class="automation-actions-count">
                        <span class="label">Actions:</span> ${actionCount} action${actionCount !== 1 ? 's' : ''}
                    </div>
                </div>
                <div class="automation-card-actions">
                    <button class="trigger-automation-btn btn btn-small btn-primary">Run Now</button>
                    <button class="edit-automation-btn btn btn-small">Edit</button>
                    <button class="delete-automation-btn btn btn-small btn-danger">Delete</button>
                </div>
            </div>
        `;
    }

    getTriggerDescription(trigger) {
        switch (trigger.type) {
            case 'manual':
                return 'Manual only';
            case 'schedule':
                if (trigger.schedule.type === 'time_of_day') {
                    const days = trigger.schedule.days.length === 0
                        ? 'every day'
                        : trigger.schedule.days.map(d => ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'][d]).join(', ');
                    return `At ${trigger.schedule.time} (${days})`;
                } else if (trigger.schedule.type === 'interval') {
                    return `Every ${trigger.schedule.seconds} seconds`;
                }
                return 'Scheduled';
            case 'device_state':
                return `Device ${trigger.device_ieee} - ${trigger.state_change.type}`;
            default:
                return 'Unknown';
        }
    }

    editAutomation(id) {
        const automation = this.automations.find(a => a.id === id);
        if (!automation) return;

        this.editingAutomation = automation;
        document.getElementById('automation-modal-title').textContent = 'Edit Automation';
        document.getElementById('automation-id').value = automation.id;
        document.getElementById('automation-name').value = automation.name;
        document.getElementById('automation-description').value = automation.description || '';

        // Set trigger
        const triggerType = document.getElementById('trigger-type');
        triggerType.value = automation.trigger.type;
        triggerType.dispatchEvent(new Event('change'));

        if (automation.trigger.type === 'schedule') {
            const scheduleType = document.getElementById('schedule-type');
            scheduleType.value = automation.trigger.schedule.type;
            scheduleType.dispatchEvent(new Event('change'));

            if (automation.trigger.schedule.type === 'time_of_day') {
                document.getElementById('schedule-time').value = automation.trigger.schedule.time;
                document.querySelectorAll('input[name="schedule-day"]').forEach(cb => {
                    cb.checked = automation.trigger.schedule.days.includes(parseInt(cb.value));
                });
            } else if (automation.trigger.schedule.type === 'interval') {
                document.getElementById('interval-seconds').value = automation.trigger.schedule.seconds;
            }
        } else if (automation.trigger.type === 'device_state') {
            document.getElementById('trigger-device').value = automation.trigger.device_ieee;
            document.getElementById('state-change').value = automation.trigger.state_change.type;
        }

        // Set actions
        document.getElementById('actions-list').innerHTML = '';
        this.actionCounter = 0;
        automation.actions.forEach(action => this.addActionRow(action));

        document.getElementById('automation-modal').classList.remove('hidden');
    }

    async saveAutomation() {
        const id = document.getElementById('automation-id').value;
        const name = document.getElementById('automation-name').value;
        const description = document.getElementById('automation-description').value || null;
        const triggerType = document.getElementById('trigger-type').value;

        // Build trigger
        let trigger;
        if (triggerType === 'manual') {
            trigger = { type: 'manual' };
        } else if (triggerType === 'schedule') {
            const scheduleType = document.getElementById('schedule-type').value;
            if (scheduleType === 'time_of_day') {
                const time = document.getElementById('schedule-time').value;
                const days = Array.from(document.querySelectorAll('input[name="schedule-day"]:checked'))
                    .map(cb => parseInt(cb.value));
                trigger = {
                    type: 'schedule',
                    schedule: { type: 'time_of_day', time, days }
                };
            } else {
                const seconds = parseInt(document.getElementById('interval-seconds').value);
                trigger = {
                    type: 'schedule',
                    schedule: { type: 'interval', seconds }
                };
            }
        } else if (triggerType === 'device_state') {
            const deviceIeee = document.getElementById('trigger-device').value;
            const stateChange = document.getElementById('state-change').value;
            trigger = {
                type: 'device_state',
                device_ieee: deviceIeee,
                state_change: { type: stateChange }
            };
        }

        // Build actions
        const actions = [];
        document.querySelectorAll('.action-row').forEach(row => {
            const type = row.querySelector('.action-type').value;

            if (type === 'device_control') {
                const deviceIeee = row.querySelector('.action-device').value;
                const endpoint = parseInt(row.querySelector('.action-endpoint').value);
                const command = row.querySelector('.action-command').value;

                if (deviceIeee) {
                    actions.push({
                        type: 'device_control',
                        device_ieee: deviceIeee,
                        endpoint,
                        command: { type: command }
                    });
                }
            } else if (type === 'delay') {
                const seconds = parseInt(row.querySelector('.action-delay-seconds').value) || 5;
                actions.push({ type: 'delay', seconds });
            } else if (type === 'log') {
                const message = row.querySelector('.action-log-message').value;
                if (message) {
                    actions.push({ type: 'log', message, level: 'info' });
                }
            }
        });

        const automation = { name, description, trigger, actions };

        try {
            if (id) {
                await this.api.updateAutomation(id, automation);
            } else {
                await this.api.createAutomation(automation);
            }
            document.getElementById('automation-modal').classList.add('hidden');
            this.resetAutomationForm();
            this.loadAutomations();
        } catch (err) {
            console.error('Failed to save automation:', err);
            alert('Failed to save automation: ' + err.message);
        }
    }
}

// Start the app
document.addEventListener('DOMContentLoaded', () => {
    window.app = new App();
});
