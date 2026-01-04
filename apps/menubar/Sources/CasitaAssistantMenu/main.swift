import Cocoa
import Foundation

// MARK: - API Models

struct ApiResponse<T: Codable>: Codable {
    let success: Bool
    let data: T?
    let error: String?
}

struct Device: Codable {
    let ieee_address: [UInt8]
    let nwk_address: UInt16
    let device_type: String
    let friendly_name: String?
    let model: String?
    let available: Bool
    let endpoints: [Endpoint]
}

struct Endpoint: Codable {
    let id: UInt8
    let profile_id: UInt16?
    let device_id: UInt16?
    let in_clusters: [UInt16]
    let out_clusters: [UInt16]
}

// MARK: - API Client

class CasitaAssistantAPI {
    static let shared = CasitaAssistantAPI()
    let baseURL = "http://localhost:3000"

    func fetchDevices(completion: @escaping ([Device]) -> Void) {
        guard let url = URL(string: "\(baseURL)/api/v1/devices") else {
            completion([])
            return
        }

        URLSession.shared.dataTask(with: url) { data, response, error in
            guard let data = data, error == nil else {
                DispatchQueue.main.async { completion([]) }
                return
            }

            do {
                let response = try JSONDecoder().decode(ApiResponse<[Device]>.self, from: data)
                DispatchQueue.main.async {
                    completion(response.data ?? [])
                }
            } catch {
                print("Decode error: \(error)")
                DispatchQueue.main.async { completion([]) }
            }
        }.resume()
    }

    func toggleDevice(ieee: String, endpoint: UInt8, completion: @escaping (Bool) -> Void) {
        guard let url = URL(string: "\(baseURL)/api/v1/devices/\(ieee)/endpoints/\(endpoint)/toggle") else {
            completion(false)
            return
        }

        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")

        URLSession.shared.dataTask(with: request) { data, response, error in
            let success = error == nil && (response as? HTTPURLResponse)?.statusCode == 200
            DispatchQueue.main.async { completion(success) }
        }.resume()
    }
}

// MARK: - App Delegate

class AppDelegate: NSObject, NSApplicationDelegate {
    var statusItem: NSStatusItem!
    var devices: [Device] = []
    var timer: Timer?

    func applicationDidFinishLaunching(_ notification: Notification) {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)

        if let button = statusItem.button {
            button.title = "Casita"
            button.font = NSFont.systemFont(ofSize: 12, weight: .medium)
        }

        buildMenu()
        fetchDevices()

        // Refresh every 5 seconds
        timer = Timer.scheduledTimer(withTimeInterval: 5.0, repeats: true) { [weak self] _ in
            self?.fetchDevices()
        }
        RunLoop.current.add(timer!, forMode: .common)
    }

    func fetchDevices() {
        CasitaAssistantAPI.shared.fetchDevices { [weak self] devices in
            self?.devices = devices
            self?.updateMenuWithDevices()
        }
    }

    func buildMenu() {
        let menu = NSMenu()

        // Loading state
        let loadingItem = NSMenuItem(title: "Loading devices...", action: nil, keyEquivalent: "")
        loadingItem.isEnabled = false
        menu.addItem(loadingItem)

        menu.addItem(NSMenuItem.separator())

        let refreshItem = NSMenuItem(title: "Refresh", action: #selector(refresh), keyEquivalent: "r")
        refreshItem.target = self
        menu.addItem(refreshItem)

        let quitItem = NSMenuItem(title: "Quit", action: #selector(quit), keyEquivalent: "q")
        quitItem.target = self
        menu.addItem(quitItem)

        statusItem.menu = menu
    }

    func updateMenuWithDevices() {
        let menu = NSMenu()

        if devices.isEmpty {
            let noDevices = NSMenuItem(title: "No devices", action: nil, keyEquivalent: "")
            noDevices.isEnabled = false
            menu.addItem(noDevices)
        } else {
            for device in devices {
                let name = device.friendly_name ?? device.model ?? formatIeee(device.ieee_address)
                let ieee = formatIeee(device.ieee_address)

                // Find the best endpoint (one with On/Off cluster 0x0006)
                let endpoint: UInt8 = device.endpoints.first { ep in
                    ep.in_clusters.contains(0x0006)
                }?.id ?? device.endpoints.first?.id ?? 1

                let item = NSMenuItem(title: name, action: #selector(toggleDevice(_:)), keyEquivalent: "")
                item.target = self
                item.representedObject = ["ieee": ieee, "endpoint": endpoint] as [String: Any]

                // Show availability
                if !device.available {
                    item.title = "\(name) (offline)"
                    item.isEnabled = false
                }

                menu.addItem(item)
            }
        }

        menu.addItem(NSMenuItem.separator())

        // Device count in status
        let countItem = NSMenuItem(title: "\(devices.count) device(s)", action: nil, keyEquivalent: "")
        countItem.isEnabled = false
        menu.addItem(countItem)

        menu.addItem(NSMenuItem.separator())

        let refreshItem = NSMenuItem(title: "Refresh", action: #selector(refresh), keyEquivalent: "r")
        refreshItem.target = self
        menu.addItem(refreshItem)

        let quitItem = NSMenuItem(title: "Quit", action: #selector(quit), keyEquivalent: "q")
        quitItem.target = self
        menu.addItem(quitItem)

        statusItem.menu = menu

        // Update button title with device count
        if let button = statusItem.button {
            button.title = devices.isEmpty ? "Casita" : "Casita (\(devices.count))"
        }
    }

    func formatIeee(_ bytes: [UInt8]) -> String {
        // IEEE addresses are displayed in reverse byte order
        return bytes.reversed().map { String(format: "%02x", $0) }.joined(separator: ":")
    }

    @objc func toggleDevice(_ sender: NSMenuItem) {
        guard let info = sender.representedObject as? [String: Any],
              let ieee = info["ieee"] as? String,
              let endpoint = info["endpoint"] as? UInt8 else {
            return
        }

        // Visual feedback
        sender.title = "\(sender.title) ..."

        CasitaAssistantAPI.shared.toggleDevice(ieee: ieee, endpoint: endpoint) { [weak self] success in
            if success {
                // Refresh to get updated state
                self?.fetchDevices()
            } else {
                print("Toggle failed for \(ieee)")
            }
        }
    }

    @objc func refresh() {
        fetchDevices()
    }

    @objc func quit() {
        NSApplication.shared.terminate(nil)
    }
}

// MARK: - Main

let app = NSApplication.shared
app.setActivationPolicy(.accessory)

let delegate = AppDelegate()
app.delegate = delegate

app.run()
