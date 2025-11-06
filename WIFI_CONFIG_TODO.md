# WiFi Configuration TODOs

This document tracks future enhancements for the WiFi configuration system.

## Current Implementation

The current implementation:
- ✅ Starts WiFi in AP mode
- ✅ Serves HTTP configuration page at 192.168.4.1
- ✅ Accepts SSID and password from user via web form
- ✅ Returns credentials to application
- ❌ **Requires manual restart to switch from AP to client mode**

## Priority TODOs

### 1. List Local WiFi Networks
**Status:** Not Started  
**Priority:** High  
**Description:** Add functionality to scan and list available WiFi networks in the configuration page.

**Implementation Notes:**
- Use `cyw43::Control::scan()` method to enumerate networks
- Display networks in the configuration page as selectable options
- Show signal strength and security type for each network
- Pre-fill SSID when user selects a network

**Files to Modify:**
- `src/wifi_config.rs` - Add scan function and update HTML page
- `src/wifi.rs` - Expose control handle for scanning

---

### 2. Save Credentials Between Reboots (But Not Forever)
**Status:** Not Started  
**Priority:** High  
**Description:** Persist WiFi credentials across device reboots, but not permanently (e.g., expire after 30 days or allow factory reset).

**Implementation Options:**

#### Option A: Flash Storage
- Use `embedded-storage` crate with RP2040 flash
- Store credentials in last flash sector
- Add expiration timestamp
- Implement factory reset button combo

#### Option B: External EEPROM
- If available, use external EEPROM
- More write cycles available
- Safer for frequent updates

**Files to Create/Modify:**
- `src/credential_storage.rs` - New module for persistence
- `examples/log_time.rs` - Check for saved credentials on boot
- Add factory reset logic (e.g., hold button during boot)

**Security Considerations:**
- Encrypt credentials before storage
- Use device-unique key for encryption
- Implement secure erase on factory reset

---

### 3. Runtime WiFi Mode Switching
**Status:** Not Started  
**Priority:** Medium  
**Description:** Support switching from AP mode to client mode without device restart.

**Current Limitation:**
The `cyw43` driver and Embassy networking stack are currently initialized once and run indefinitely. Switching modes requires re-initialization.

**Possible Solutions:**

#### Option A: Dual Stack
- Run both AP and client interfaces simultaneously
- Switch which one is "active"
- May have higher memory overhead

#### Option B: Dynamic Re-initialization
- Implement teardown logic for network stack
- Re-initialize with new mode
- Requires careful resource management

**Files to Modify:**
- `src/wifi.rs` - Add mode switching logic
- `src/time_sync.rs` - Handle mode changes
- May need to modify `embassy-net` integration

---

## Lower Priority Enhancements

### 4. Configuration Validation
- Verify WiFi credentials before switching modes
- Attempt connection with timeout
- Fall back to AP mode if connection fails
- Provide user feedback during connection attempt

### 5. Web UI Improvements
- Add CSS styling and better UX
- Show connection status/progress
- Display device information (IP, MAC address, etc.)
- Add JavaScript for client-side validation

### 6. Alternative Configuration Methods
- Serial console configuration
- Bluetooth configuration (if hardware supports)
- WPS button support
- QR code configuration

### 7. Multi-Network Support
- Store multiple WiFi networks
- Automatic fallback between networks
- Priority ordering

### 8. Security Enhancements
- WPA3 support (if firmware supports)
- Certificate-based authentication for enterprise networks
- Secure boot integration
- Rate limiting on configuration page

---

## Implementation Order (Recommended)

1. **List Local WiFi Networks** - Greatly improves user experience
2. **Save Credentials Between Reboots** - Makes device usable without reconfiguration
3. **Runtime WiFi Mode Switching** - Eliminates restart requirement
4. **Configuration Validation** - Improves reliability
5. **Web UI Improvements** - Polish the user experience
6. Other enhancements as needed

---

## Notes

- The current implementation is functional but requires environment variables (`WIFI_SSID`, `WIFI_PASS`) for client mode
- All TODOs maintain the requirement that credentials should NOT be permanent (security feature)
- Consider power consumption when implementing persistent storage
- Test all configuration scenarios (successful config, failed connection, timeout, etc.)

