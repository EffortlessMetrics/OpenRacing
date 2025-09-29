# Racing Wheel Software - Anti-Cheat Compatibility

This document provides comprehensive information about the racing wheel software's compatibility with anti-cheat systems used in racing simulators and games.

## Executive Summary

The Racing Wheel Software is designed from the ground up to be fully compatible with all major anti-cheat systems. It uses only documented, legitimate methods for game integration and hardware communication, avoiding any techniques that could be flagged as suspicious by anti-cheat software.

## Key Compatibility Points

### ✅ No Process Injection
- **No DLL injection** into game processes
- **No code injection** of any kind
- **No memory modification** of game processes
- All communication uses external, documented interfaces

### ✅ No Kernel Components
- **No kernel drivers** required or used
- **No kernel-mode code** execution
- Operates entirely in user space
- Uses standard Windows/Linux APIs only

### ✅ Documented Methods Only
- All telemetry methods are **publicly documented**
- Uses **official game APIs** where available
- Follows **manufacturer recommendations**
- No reverse engineering or undocumented interfaces

### ✅ Process Isolation
- **Separate processes** for different components
- **Clear process boundaries** with defined interfaces
- **No shared memory** between service and games
- **Standard IPC mechanisms** only

### ✅ Signed and Verified
- All executables are **digitally signed**
- **Code signing certificates** from trusted authorities
- **Integrity verification** at startup
- **Tamper detection** mechanisms

## Technical Architecture

### Process Architecture
```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Game Process  │    │  Service Process │    │   UI Process    │
│   (Untouched)   │    │   (User Mode)    │    │   (User Mode)   │
│                 │    │                  │    │                 │
│ - Runs normally │    │ - Device I/O     │    │ - Configuration │
│ - No injection  │    │ - Telemetry read │    │ - User interface│
│ - No hooks      │    │ - Safety logic   │    │ - Profile edit  │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │                       │
         └───── Telemetry ───────┼────── IPC ───────────┘
                                 │
                        ┌─────────────────┐
                        │   Hardware      │
                        │   (HID/USB)     │
                        └─────────────────┘
```

### Communication Methods

#### 1. Shared Memory (Read-Only)
**Games:** iRacing, Automobilista 2, rFactor 2
**Method:** Official SDK shared memory interfaces
**Access:** Read-only access to documented structures
**Anti-Cheat Impact:** None - standard telemetry method

#### 2. UDP Broadcast
**Games:** Assetto Corsa Competizione, F1 series
**Method:** Game broadcasts telemetry data via UDP
**Access:** Listen on network socket for broadcast packets
**Anti-Cheat Impact:** None - no process interaction required

#### 3. Configuration Files
**Purpose:** Enable telemetry output in games
**Method:** Modify documented configuration files
**Access:** Standard file I/O operations
**User Consent:** Required before any modifications

## Per-Game Compatibility

### iRacing
- **Method:** Official iRacing SDK shared memory
- **Files Modified:** `app.ini` (with user consent)
- **Process Interaction:** None
- **Anti-Cheat Status:** ✅ Fully Compatible
- **Notes:** Uses documented telemetry interface provided by iRacing

### Assetto Corsa Competizione (ACC)
- **Method:** UDP telemetry broadcast
- **Files Modified:** `broadcasting.json` (with user consent)
- **Process Interaction:** None
- **Anti-Cheat Status:** ✅ Fully Compatible
- **Notes:** Uses official ACC telemetry API

### Automobilista 2 (AMS2)
- **Method:** Shared memory telemetry
- **Files Modified:** None
- **Process Interaction:** None
- **Anti-Cheat Status:** ✅ Fully Compatible
- **Notes:** Read-only access to documented shared memory

### rFactor 2
- **Method:** Plugin-based telemetry (planned)
- **Files Modified:** Plugin installation only
- **Process Interaction:** None
- **Anti-Cheat Status:** ✅ Fully Compatible
- **Notes:** Uses official rFactor 2 plugin API

## System APIs Used

### Windows APIs
| API | Purpose | Privilege Level | Anti-Cheat Impact |
|-----|---------|-----------------|-------------------|
| HID API | Racing wheel communication | User | None - standard hardware interface |
| MMCSS | Real-time thread scheduling | User | None - multimedia API |
| Named Pipes | Inter-process communication | User | None - standard IPC |
| File I/O | Configuration management | User | None - standard file operations |
| Shared Memory | Game telemetry reading | User | None - read-only documented interfaces |

### Linux APIs
| API | Purpose | Privilege Level | Anti-Cheat Impact |
|-----|---------|-----------------|-------------------|
| hidraw | Racing wheel communication | User | None - standard hardware interface |
| rtkit | Real-time scheduling | User (via rtkit) | None - standard RT API |
| Unix Sockets | Inter-process communication | User | None - standard IPC |
| File I/O | Configuration management | User | None - standard file operations |
| Shared Memory | Game telemetry reading | User | None - read-only documented interfaces |

## Security Measures

### Code Integrity
- **Digital Signatures:** All executables signed with valid certificates
- **Hash Verification:** Runtime integrity checking
- **Tamper Detection:** Detects and prevents code modification
- **Update Verification:** Signed updates with rollback capability

### Privilege Separation
- **Minimal Privileges:** Runs with least required privileges
- **User Mode Only:** No kernel-mode components
- **Process Isolation:** Separate processes with defined boundaries
- **Sandboxing:** Plugin system uses sandboxed execution

### Input Validation
- **Schema Validation:** All configuration inputs validated
- **Bounds Checking:** Telemetry data bounds checking
- **Injection Prevention:** Prevents all forms of code injection
- **Safe Parsing:** Memory-safe parsing of all external data

## Anti-Cheat System Compatibility

### BattlEye
- **Status:** ✅ Compatible
- **Reason:** No process injection, documented methods only
- **Verification:** Uses whitelisted system APIs only

### Easy Anti-Cheat (EAC)
- **Status:** ✅ Compatible
- **Reason:** No kernel components, standard user-mode operation
- **Verification:** No flagged behaviors or techniques

### Valve Anti-Cheat (VAC)
- **Status:** ✅ Compatible
- **Reason:** No memory modification, external communication only
- **Verification:** Uses Steam-approved telemetry methods

### Custom Anti-Cheat Systems
- **Status:** ✅ Generally Compatible
- **Reason:** Conservative approach using only documented methods
- **Verification:** Open source code available for audit

## Verification Methods

### Static Analysis
- **Code Review:** All source code is open source and auditable
- **Dependency Audit:** All third-party dependencies verified
- **API Usage:** Only whitelisted system APIs used
- **Signature Verification:** All binaries cryptographically signed

### Runtime Analysis
- **Process Monitoring:** No suspicious process behavior
- **Memory Analysis:** No memory modification or injection
- **Network Analysis:** Only documented network protocols used
- **File System:** Only legitimate file access patterns

### Behavioral Analysis
- **No Hooking:** No API hooking or interception
- **No Injection:** No code or DLL injection
- **No Debugging:** No debugging or process attachment
- **No Obfuscation:** Clear, readable code without obfuscation

## Reporting and Support

### False Positive Handling
If the software is incorrectly flagged by an anti-cheat system:

1. **Contact Support:** Reach out to our support team with details
2. **Provide Evidence:** We'll provide this compatibility report
3. **Developer Contact:** We can contact anti-cheat vendors directly
4. **Whitelist Request:** Request whitelisting through proper channels

### Continuous Monitoring
- **Anti-Cheat Updates:** Monitor for anti-cheat system changes
- **Compatibility Testing:** Regular testing with major anti-cheat systems
- **Community Feedback:** Track user reports of compatibility issues
- **Proactive Communication:** Maintain relationships with anti-cheat vendors

## Conclusion

The Racing Wheel Software is designed with anti-cheat compatibility as a primary concern. By using only documented, legitimate methods and avoiding any techniques commonly associated with cheating software, we ensure compatibility with all major anti-cheat systems.

The software's open-source nature, comprehensive documentation, and conservative technical approach provide transparency and verifiability that anti-cheat systems require.

For any questions or concerns about anti-cheat compatibility, please contact our support team with this document.

---

**Document Version:** 1.0  
**Last Updated:** 2024-12-19  
**Next Review:** 2025-03-19