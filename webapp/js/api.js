// API client for Casita Assistant

export class Api {
    constructor(baseUrl = '') {
        this.baseUrl = baseUrl;
    }

    async request(method, path, body = null) {
        const options = {
            method,
            headers: {
                'Content-Type': 'application/json',
            },
        };

        if (body) {
            options.body = JSON.stringify(body);
        }

        const response = await fetch(`${this.baseUrl}${path}`, options);
        const data = await response.json();

        if (!data.success) {
            throw new Error(data.error || 'Unknown error');
        }

        return data.data;
    }

    get(path) {
        return this.request('GET', path);
    }

    post(path, body) {
        return this.request('POST', path, body);
    }

    put(path, body) {
        return this.request('PUT', path, body);
    }

    delete(path) {
        return this.request('DELETE', path);
    }

    // System endpoints
    getSystemInfo() {
        return this.get('/api/v1/system/info');
    }

    // Network endpoints
    getNetworkStatus() {
        return this.get('/api/v1/network/status');
    }

    permitJoin(duration = 60) {
        return this.post('/api/v1/network/permit-join', { duration });
    }

    // Device endpoints
    getDevices() {
        return this.get('/api/v1/devices');
    }

    getDevice(ieee) {
        return this.get(`/api/v1/devices/${ieee}`);
    }

    // Control endpoints (to be implemented)
    turnOn(ieee, endpoint) {
        return this.post(`/api/v1/devices/${ieee}/endpoints/${endpoint}/on`);
    }

    turnOff(ieee, endpoint) {
        return this.post(`/api/v1/devices/${ieee}/endpoints/${endpoint}/off`);
    }

    toggle(ieee, endpoint) {
        return this.post(`/api/v1/devices/${ieee}/endpoints/${endpoint}/toggle`);
    }

    setBrightness(ieee, endpoint, brightness, transition = 0) {
        return this.put(`/api/v1/devices/${ieee}/endpoints/${endpoint}/brightness`, {
            brightness,
            transition,
        });
    }
}
