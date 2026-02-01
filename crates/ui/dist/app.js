/**
 * OpenRacing UI Application
 * 
 * This JavaScript module handles the frontend logic for the OpenRacing
 * Tauri application. It communicates with the Rust backend via Tauri's
 * invoke API.
 * 
 * Requirements Coverage:
 * - 7.1: Device list display
 * - 7.2: Device status display
 * - 7.3: Profile loading and application
 * - 7.4: Real-time telemetry display
 * - 7.5: User-friendly error messages
 * - 7.6: IPC communication with wheeld service
 */

// Tauri API
const { invoke } = window.__TAURI__.core;

// Dialog plugin - Tauri 2.x uses separate plugin imports
// The dialog plugin provides file open/save dialogs for profile management
let dialogOpen = null;

// Initialize dialog plugin asynchronously
async function initDialogPlugin() {
    try {
        // In Tauri 2.x, plugins are accessed via window.__TAURI_PLUGIN_DIALOG__
        // or through the @tauri-apps/plugin-dialog npm package
        if (window.__TAURI_PLUGIN_DIALOG__) {
            dialogOpen = window.__TAURI_PLUGIN_DIALOG__.open;
        } else if (window.__TAURI__ && window.__TAURI__.dialog) {
            dialogOpen = window.__TAURI__.dialog.open;
        }
    } catch (e) {
        console.warn('Dialog plugin not available:', e);
    }
}

// Application State
const state = {
    connected: false,
    devices: [],
    selectedDeviceId: null,
    telemetryInterval: null,
    errorBannerTimeout: null,
};

// DOM Elements
const elements = {
    // Service status
    serviceStatus: document.getElementById('service-status'),
    statusIndicator: null,
    statusText: null,

    // Panels
    connectionPanel: document.getElementById('connection-panel'),
    devicePanel: document.getElementById('device-panel'),
    noDevicePanel: document.getElementById('no-device-panel'),

    // Connection
    connectBtn: document.getElementById('connect-btn'),
    connectionError: document.getElementById('connection-error'),

    // Device list
    deviceList: document.getElementById('device-list'),
    refreshDevicesBtn: document.getElementById('refresh-devices'),

    // Device details
    deviceName: document.getElementById('device-name'),
    deviceType: document.getElementById('device-type'),
    deviceState: document.getElementById('device-state'),
    deviceLastSeen: document.getElementById('device-last-seen'),
    deviceFaults: document.getElementById('device-faults'),
    deviceCapabilities: document.getElementById('device-capabilities'),

    // Telemetry
    telemetryAngle: document.getElementById('telemetry-angle'),
    telemetryTemp: document.getElementById('telemetry-temp'),
    telemetryHands: document.getElementById('telemetry-hands'),

    // Profile
    loadProfileBtn: document.getElementById('load-profile-btn'),
    currentProfile: document.getElementById('current-profile'),

    // Safety
    emergencyStopBtn: document.getElementById('emergency-stop-btn'),

    // Error banner
    errorBanner: document.getElementById('error-banner'),
    errorMessage: document.getElementById('error-message'),
    errorDismiss: document.getElementById('error-dismiss'),
};

// Initialize DOM element references
function initElements() {
    elements.statusIndicator = elements.serviceStatus.querySelector('.status-indicator');
    elements.statusText = elements.serviceStatus.querySelector('.status-text');
}

// ============================================================================
// Service Connection
// ============================================================================

/**
 * Connect to the wheeld service
 */
async function connectToService() {
    try {
        elements.connectBtn.disabled = true;
        elements.connectBtn.textContent = 'Connecting...';
        hideError(elements.connectionError);

        const status = await invoke('connect_service');

        state.connected = true;
        updateServiceStatus(status);

        // Show device panel or no-device panel
        elements.connectionPanel.classList.add('hidden');
        elements.noDevicePanel.classList.remove('hidden');

        // Load devices
        await refreshDevices();

    } catch (error) {
        showError(elements.connectionError, `Connection failed: ${error}`);
        elements.connectBtn.disabled = false;
        elements.connectBtn.textContent = 'Connect to Service';
    }
}

/**
 * Disconnect from the wheeld service
 */
async function disconnectFromService() {
    try {
        await invoke('disconnect_service');
        state.connected = false;
        state.devices = [];
        state.selectedDeviceId = null;

        stopTelemetryPolling();

        updateServiceStatus({ connected: false });

        elements.connectionPanel.classList.remove('hidden');
        elements.devicePanel.classList.add('hidden');
        elements.noDevicePanel.classList.add('hidden');

        renderDeviceList();

    } catch (error) {
        showErrorBanner(`Disconnect failed: ${error}`);
    }
}

/**
 * Update the service status display
 */
function updateServiceStatus(status) {
    if (status.connected) {
        elements.statusIndicator.classList.remove('disconnected');
        elements.statusIndicator.classList.add('connected');
        elements.statusText.textContent = `Connected (v${status.version})`;
    } else {
        elements.statusIndicator.classList.remove('connected');
        elements.statusIndicator.classList.add('disconnected');
        elements.statusText.textContent = 'Disconnected';
    }
}

// ============================================================================
// Device Management
// ============================================================================

/**
 * Refresh the device list
 * Requirement 7.1: THE Tauri_UI SHALL display a list of connected racing wheel devices
 */
async function refreshDevices() {
    if (!state.connected) return;

    try {
        elements.deviceList.innerHTML = '<div class="loading">Loading devices...</div>';

        const devices = await invoke('list_devices');
        state.devices = devices;

        renderDeviceList();

        // If we had a selected device, check if it's still available
        if (state.selectedDeviceId) {
            const stillExists = devices.some(d => d.id === state.selectedDeviceId);
            if (!stillExists) {
                state.selectedDeviceId = null;
                elements.devicePanel.classList.add('hidden');
                elements.noDevicePanel.classList.remove('hidden');
            }
        }

    } catch (error) {
        showErrorBanner(`Failed to load devices: ${error}`);
        elements.deviceList.innerHTML = '<div class="loading">Failed to load devices</div>';
    }
}

/**
 * Render the device list
 */
function renderDeviceList() {
    if (state.devices.length === 0) {
        elements.deviceList.innerHTML = '<div class="loading">No devices found</div>';
        return;
    }

    elements.deviceList.innerHTML = state.devices.map(device => `
        <div class="device-item ${device.id === state.selectedDeviceId ? 'selected' : ''}" 
             data-device-id="${device.id}">
            <span class="device-icon">${getDeviceIcon(device.device_type)}</span>
            <div class="device-info">
                <div class="device-name">${escapeHtml(device.name)}</div>
                <div class="device-type">${device.device_type}</div>
            </div>
            <span class="device-status ${device.state.toLowerCase()}"></span>
        </div>
    `).join('');

    // Add click handlers
    elements.deviceList.querySelectorAll('.device-item').forEach(item => {
        item.addEventListener('click', () => selectDevice(item.dataset.deviceId));
    });
}

/**
 * Get icon for device type
 */
function getDeviceIcon(deviceType) {
    switch (deviceType) {
        case 'WheelBase': return '🎮';
        case 'Pedals': return '🦶';
        case 'Shifter': return '🔧';
        case 'Handbrake': return '🛑';
        default: return '📟';
    }
}

/**
 * Select a device and show its details
 * Requirement 7.2: WHEN a device is selected, THE Tauri_UI SHALL show device status
 */
async function selectDevice(deviceId) {
    state.selectedDeviceId = deviceId;

    // Update selection in list
    elements.deviceList.querySelectorAll('.device-item').forEach(item => {
        item.classList.toggle('selected', item.dataset.deviceId === deviceId);
    });

    // Show device panel
    elements.noDevicePanel.classList.add('hidden');
    elements.devicePanel.classList.remove('hidden');

    // Load device status
    await loadDeviceStatus(deviceId);

    // Start telemetry polling
    startTelemetryPolling(deviceId);
}

/**
 * Load device status
 */
async function loadDeviceStatus(deviceId) {
    try {
        const status = await invoke('get_device_status', { deviceId });

        // Update device info
        elements.deviceName.textContent = status.device.name;
        elements.deviceType.textContent = status.device.device_type;
        elements.deviceState.textContent = status.device.state;
        elements.deviceLastSeen.textContent = formatTimestamp(status.last_seen);
        elements.deviceFaults.textContent = status.active_faults.length > 0
            ? status.active_faults.join(', ')
            : 'None';

        // Update capabilities
        renderCapabilities(status.device.capabilities);

        // Update telemetry if available
        if (status.telemetry) {
            updateTelemetryDisplay(status.telemetry);
        }

    } catch (error) {
        showErrorBanner(`Failed to load device status: ${error}`);
    }
}

/**
 * Render device capabilities
 */
function renderCapabilities(capabilities) {
    const caps = [
        { name: 'PID Control', enabled: capabilities.supports_pid },
        { name: '1kHz Torque', enabled: capabilities.supports_raw_torque_1khz },
        { name: 'Health Stream', enabled: capabilities.supports_health_stream },
        { name: 'LED Bus', enabled: capabilities.supports_led_bus },
    ];

    elements.deviceCapabilities.innerHTML = caps.map(cap => `
        <span class="capability-badge ${cap.enabled ? 'enabled' : 'disabled'}">
            ${cap.enabled ? '✓' : '✗'} ${cap.name}
        </span>
    `).join('');

    // Add max torque info
    if (capabilities.max_torque_cnm > 0) {
        const torqueNm = (capabilities.max_torque_cnm / 100).toFixed(1);
        elements.deviceCapabilities.innerHTML += `
            <span class="capability-badge enabled">Max ${torqueNm} Nm</span>
        `;
    }
}

// ============================================================================
// Telemetry
// ============================================================================

/**
 * Start polling for telemetry data
 * Requirement 7.4: THE Tauri_UI SHALL display real-time telemetry data
 */
function startTelemetryPolling(deviceId) {
    stopTelemetryPolling();

    state.telemetryInterval = setInterval(async () => {
        if (!state.connected || state.selectedDeviceId !== deviceId) {
            stopTelemetryPolling();
            return;
        }

        try {
            const telemetry = await invoke('get_telemetry', { deviceId });
            updateTelemetryDisplay(telemetry);
        } catch (error) {
            // Silently ignore telemetry errors to avoid spamming
            console.warn('Telemetry update failed:', error);
        }
    }, 100); // 10Hz update rate
}

/**
 * Stop telemetry polling
 */
function stopTelemetryPolling() {
    if (state.telemetryInterval) {
        clearInterval(state.telemetryInterval);
        state.telemetryInterval = null;
    }
}

/**
 * Update telemetry display
 */
function updateTelemetryDisplay(telemetry) {
    elements.telemetryAngle.textContent = `${telemetry.wheel_angle_deg.toFixed(1)}°`;
    elements.telemetryTemp.textContent = `${telemetry.temperature_c}°C`;
    elements.telemetryHands.textContent = telemetry.hands_on ? 'Yes' : 'No';
}

// ============================================================================
// Profile Management
// ============================================================================

/**
 * Load and apply a profile
 * Requirement 7.3: THE Tauri_UI SHALL allow loading and applying FFB profiles
 */
async function loadProfile() {
    if (!state.selectedDeviceId) return;

    try {
        // Check if dialog plugin is available
        if (!dialogOpen) {
            showErrorBanner('File dialog not available. Please check plugin configuration.');
            return;
        }

        // Open file dialog using the dialog plugin
        const filePath = await dialogOpen({
            multiple: false,
            filters: [{
                name: 'Profile',
                extensions: ['json']
            }]
        });

        if (!filePath) return;

        // Apply the profile
        const result = await invoke('apply_profile', {
            deviceId: state.selectedDeviceId,
            profilePath: filePath
        });

        if (result.success) {
            // Extract filename from path and display it
            const profileName = filePath.split(/[/\\]/).pop();
            elements.currentProfile.textContent = profileName;
            elements.currentProfile.classList.add('profile-loaded');
            showErrorBanner('Profile applied successfully', 'success');
        } else {
            showErrorBanner(`Failed to apply profile: ${result.message}`);
        }

    } catch (error) {
        showErrorBanner(`Failed to load profile: ${error}`);
    }
}

// ============================================================================
// Safety Controls
// ============================================================================

/**
 * Trigger emergency stop
 */
async function emergencyStop() {
    if (!state.selectedDeviceId) return;

    try {
        const result = await invoke('emergency_stop', {
            deviceId: state.selectedDeviceId
        });

        if (result.success) {
            showErrorBanner('Emergency stop executed', 'warning');
        } else {
            showErrorBanner(`Emergency stop failed: ${result.message}`);
        }

    } catch (error) {
        showErrorBanner(`Emergency stop failed: ${error}`);
    }
}

// ============================================================================
// Error Handling
// ============================================================================

/**
 * Show error in a specific element
 * Requirement 7.5: WHEN an error occurs, THE Tauri_UI SHALL display a user-friendly error message
 */
function showError(element, message) {
    element.textContent = message;
    element.classList.remove('hidden');
}

/**
 * Hide error element
 */
function hideError(element) {
    element.classList.add('hidden');
}

/**
 * Show error banner with user-friendly message
 * Requirement 7.5: WHEN an error occurs, THE Tauri_UI SHALL display a user-friendly error message
 * 
 * @param {string} message - The message to display
 * @param {string} type - The type of message: 'error', 'warning', or 'success'
 */
function showErrorBanner(message, type = 'error') {
    // Clear any existing timeout
    if (state.errorBannerTimeout) {
        clearTimeout(state.errorBannerTimeout);
        state.errorBannerTimeout = null;
    }

    // Set the message text
    elements.errorMessage.textContent = message;

    // Remove all type classes and add the appropriate one
    elements.errorBanner.classList.remove('error-banner-error', 'error-banner-warning', 'error-banner-success');
    elements.errorBanner.classList.add(`error-banner-${type}`);

    // Show the banner
    elements.errorBanner.classList.remove('hidden');

    // Auto-hide after a delay (longer for errors, shorter for success)
    const hideDelay = type === 'error' ? 8000 : type === 'warning' ? 6000 : 4000;
    state.errorBannerTimeout = setTimeout(() => {
        hideErrorBanner();
    }, hideDelay);
}

/**
 * Hide error banner
 */
function hideErrorBanner() {
    // Clear any pending timeout
    if (state.errorBannerTimeout) {
        clearTimeout(state.errorBannerTimeout);
        state.errorBannerTimeout = null;
    }
    elements.errorBanner.classList.add('hidden');
}

// ============================================================================
// Utilities
// ============================================================================

/**
 * Escape HTML to prevent XSS
 */
function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

/**
 * Format timestamp for display
 */
function formatTimestamp(timestamp) {
    if (!timestamp || timestamp === 'Unknown') return 'Unknown';

    try {
        const date = new Date(timestamp);
        return date.toLocaleString();
    } catch {
        return timestamp;
    }
}

// ============================================================================
// Event Listeners
// ============================================================================

function setupEventListeners() {
    // Connection
    elements.connectBtn.addEventListener('click', connectToService);

    // Device list
    elements.refreshDevicesBtn.addEventListener('click', refreshDevices);

    // Profile
    elements.loadProfileBtn.addEventListener('click', loadProfile);

    // Safety
    elements.emergencyStopBtn.addEventListener('click', emergencyStop);

    // Error banner
    elements.errorDismiss.addEventListener('click', hideErrorBanner);
}

// ============================================================================
// Initialization
// ============================================================================

/**
 * Initialize the application
 */
async function init() {
    console.log('OpenRacing UI initializing...');

    initElements();
    setupEventListeners();

    // Initialize dialog plugin for profile loading
    await initDialogPlugin();

    // Check initial service status
    try {
        const status = await invoke('get_service_status');
        if (status.connected) {
            state.connected = true;
            updateServiceStatus(status);
            elements.connectionPanel.classList.add('hidden');
            elements.noDevicePanel.classList.remove('hidden');
            await refreshDevices();
        }
    } catch (error) {
        console.log('Service not connected:', error);
    }

    console.log('OpenRacing UI initialized');
}

// Start the application when DOM is ready
document.addEventListener('DOMContentLoaded', init);
