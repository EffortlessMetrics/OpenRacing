# Power Management Guide for Racing Wheel Suite

This document provides comprehensive guidance on power management settings to optimize performance for the Racing Wheel Suite, particularly for real-time force feedback operations.

## Overview

The Racing Wheel Suite requires consistent, low-latency performance to maintain 1kHz force feedback timing. Modern operating systems implement various power management features that can interfere with real-time performance. This guide explains how to configure your system for optimal performance.

## Windows Power Management

### System Power Settings

#### High Performance Power Plan
1. Open Control Panel → Power Options
2. Select "High performance" power plan
3. Click "Change plan settings" → "Change advanced power settings"
4. Configure the following settings:

```
Processor power management:
├── Minimum processor state: 100%
├── Maximum processor state: 100%
└── System cooling policy: Active

Hard disk:
└── Turn off hard disk after: Never

Sleep:
├── Sleep after: Never
├── Allow hybrid sleep: Off
└── Hibernate after: Never

USB settings:
└── USB selective suspend setting: Disabled

PCI Express:
└── Link State Power Management: Off

Processor power management:
└── Processor idle disable: Enabled (if available)
```

#### Windows 10/11 Game Mode
1. Open Settings → Gaming → Game Mode
2. Enable "Game Mode"
3. This automatically optimizes system resources for gaming applications

### USB Power Management

#### Disable USB Selective Suspend
USB selective suspend can cause racing wheels to disconnect or experience latency issues.

**Via Group Policy (Pro/Enterprise):**
1. Run `gpedit.msc`
2. Navigate to: Computer Configuration → Administrative Templates → System → Power Management → USB Settings
3. Enable "Prohibit enabling of USB selective suspend"

**Via Registry (All editions):**
```batch
reg add "HKLM\SYSTEM\CurrentControlSet\Services\USB" /v DisableSelectiveSuspend /t REG_DWORD /d 1 /f
```

**Per-Device (Device Manager):**
1. Open Device Manager
2. Expand "Universal Serial Bus controllers"
3. Right-click each "USB Root Hub" → Properties
4. Power Management tab → Uncheck "Allow the computer to turn off this device"
5. Repeat for all USB hubs

#### Racing Wheel Specific Settings
For each racing wheel device:
1. Device Manager → Human Interface Devices
2. Find your racing wheel device
3. Properties → Power Management
4. Uncheck "Allow the computer to turn off this device to save power"

### CPU Power Management

#### Disable CPU Throttling
```batch
# Disable CPU throttling for wheeld.exe
powercfg /powerthrottling disable /path "C:\Program Files\RacingWheelSuite\bin\wheeld.exe"

# Set process to high performance mode
powercfg /setacvalueindex SCHEME_CURRENT SUB_PROCESSOR PROCTHROTTLEMAX 100
powercfg /setactive SCHEME_CURRENT
```

#### MMCSS (Multimedia Class Scheduler Service)
The Racing Wheel Suite automatically registers with MMCSS for real-time priority. Ensure MMCSS is running:

```batch
sc config MMCSS start= auto
sc start MMCSS
```

### Windows Registry Optimizations

Create a `.reg` file with these optimizations:

```registry
Windows Registry Editor Version 5.00

; Disable USB power management
[HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Services\USB]
"DisableSelectiveSuspend"=dword:00000001

; Multimedia timer resolution
[HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Session Manager\kernel]
"GlobalTimerResolutionRequests"=dword:00000001

; Disable CPU idle states (if needed for extreme latency requirements)
[HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Processor]
"Capabilities"=dword:0007e066

; Network throttling (if using network telemetry)
[HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile]
"NetworkThrottlingIndex"=dword:ffffffff
"SystemResponsiveness"=dword:00000000

; Gaming optimizations
[HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Games]
"GPU Priority"=dword:00000008
"Priority"=dword:00000006
"Scheduling Category"="High"
"SFIO Priority"="High"
```

## Linux Power Management

### CPU Governor Settings

#### Set Performance Governor
```bash
# Check current governor
cat /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# Set performance governor for all CPUs
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# Make permanent (systemd)
sudo systemctl enable cpupower
echo 'GOVERNOR="performance"' | sudo tee /etc/default/cpupower
```

#### Disable CPU Idle States (Extreme Performance)
```bash
# Disable C-states via kernel parameter
sudo sed -i 's/GRUB_CMDLINE_LINUX_DEFAULT="/&intel_idle.max_cstate=0 processor.max_cstate=1 /' /etc/default/grub
sudo update-grub
```

### USB Power Management

#### Disable USB Autosuspend
```bash
# Temporary (current session)
echo -1 | sudo tee /sys/bus/usb/devices/*/power/autosuspend_delay_ms

# Permanent via udev rules (already included in package)
# See: /etc/udev/rules.d/99-racing-wheel-suite.rules

# Kernel parameter method
sudo sed -i 's/GRUB_CMDLINE_LINUX_DEFAULT="/&usbcore.autosuspend=-1 /' /etc/default/grub
sudo update-grub
```

### Real-Time Scheduling

#### Install rtkit
```bash
# Ubuntu/Debian
sudo apt install rtkit

# Fedora/RHEL
sudo dnf install rtkit

# Arch Linux
sudo pacman -S rtkit
```

#### Configure Real-Time Limits
Add to `/etc/security/limits.conf`:
```
@audio          -       rtprio          95
@audio          -       memlock         unlimited
username        -       rtprio          95
username        -       memlock         unlimited
```

#### Verify RT Capabilities
```bash
# Check if user can acquire RT priority
ulimit -r

# Test RT scheduling
chrt -f 50 echo "RT test successful"
```

### Power Management Services

#### Disable Power Management Daemons (Optional)
For extreme performance requirements:
```bash
# Disable TLP (if installed)
sudo systemctl disable tlp
sudo systemctl stop tlp

# Disable power-profiles-daemon
sudo systemctl disable power-profiles-daemon
sudo systemctl stop power-profiles-daemon

# Use performance governor service instead
sudo systemctl enable cpupower
```

### Kernel Parameters

Add to `/etc/default/grub` GRUB_CMDLINE_LINUX_DEFAULT:
```bash
# Complete performance-oriented kernel parameters
intel_idle.max_cstate=0 processor.max_cstate=1 idle=poll usbcore.autosuspend=-1 pcie_aspm=off
```

## Hardware-Specific Optimizations

### Intel Systems

#### Disable Intel SpeedStep
```bash
# Via kernel parameter
intel_pstate=disable

# Or force performance mode
intel_pstate=active intel_pstate=performance
```

#### Disable Intel Turbo Boost (for consistency)
```bash
echo 1 | sudo tee /sys/devices/system/cpu/intel_pstate/no_turbo
```

### AMD Systems

#### AMD P-State Driver
```bash
# Use ACPI CPUFreq driver instead of AMD P-State for older systems
amd_pstate=disable
```

### NVIDIA Graphics

#### Disable GPU Power Management
```bash
# Set maximum performance mode
nvidia-smi -pm 1
nvidia-smi -ac 4004,1911  # Adjust memory,graphics clocks as appropriate
```

## Verification and Monitoring

### Windows Performance Monitoring

#### Check Power Plan
```powershell
powercfg /query SCHEME_CURRENT
```

#### Monitor CPU Frequency
```powershell
# PowerShell script to monitor CPU frequency
while ($true) {
    Get-Counter "\Processor Information(_Total)\% Processor Performance"
    Start-Sleep 1
}
```

### Linux Performance Monitoring

#### Check CPU Governor and Frequency
```bash
# Current governor
cat /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# Current frequency
cat /sys/devices/system/cpu/cpu*/cpufreq/scaling_cur_freq

# Monitor frequency changes
watch -n 1 'cat /proc/cpuinfo | grep "cpu MHz"'
```

#### Check USB Power Management
```bash
# Check USB autosuspend settings
for dev in /sys/bus/usb/devices/*; do
    if [ -f "$dev/power/autosuspend_delay_ms" ]; then
        echo "$dev: $(cat $dev/power/autosuspend_delay_ms)"
    fi
done
```

#### Monitor Real-Time Performance
```bash
# Check for missed deadlines (requires RT kernel)
cat /proc/sys/kernel/sched_rt_runtime_us

# Monitor context switches
vmstat 1

# Check interrupt latency
cyclictest -p 95 -m -n -q
```

## Racing Wheel Suite Integration

### Automatic Power Management

The Racing Wheel Suite automatically applies several optimizations:

#### Windows
- Registers with MMCSS for "Games" category
- Disables process power throttling
- Requests high-resolution timers
- Sets thread priority to TIME_CRITICAL for RT threads

#### Linux
- Requests RT priority via rtkit
- Locks memory pages (mlockall)
- Sets SCHED_FIFO scheduling policy
- Disables swap for the process

### Configuration Options

#### Service Configuration
```toml
# ~/.config/racing-wheel-suite/service.toml
[power_management]
# Request maximum performance mode
high_performance = true

# Disable power saving features
disable_usb_suspend = true

# RT thread priority (1-99, higher = more priority)
rt_priority = 85

# Memory locking
lock_memory = true
```

#### Runtime Verification
```bash
# Check if optimizations are active
wheelctl system status --power-management

# Monitor real-time performance
wheelctl diagnostics rt-performance --duration 60s
```

## Troubleshooting

### Common Issues

#### High Jitter on Windows
1. Check if "High performance" power plan is active
2. Verify USB selective suspend is disabled
3. Check for background processes using high CPU
4. Disable Windows Update during racing sessions

#### RT Priority Issues on Linux
1. Verify rtkit is installed and running
2. Check user limits in `/etc/security/limits.conf`
3. Ensure user is in `audio` group: `sudo usermod -a -G audio $USER`
4. Reboot after group changes

#### USB Device Disconnections
1. Disable USB power management for all hubs
2. Check USB cable quality and connections
3. Try different USB ports (prefer USB 3.0)
4. Update USB controller drivers

### Performance Testing

#### Latency Testing
```bash
# Test system latency
cyclictest -p 80 -n -m -q -D 60s

# Racing Wheel Suite built-in test
wheelctl diagnostics latency-test --duration 300s --target-jitter 0.25ms
```

#### Stress Testing
```bash
# CPU stress test while monitoring RT performance
stress-ng --cpu 4 --timeout 300s &
wheelctl diagnostics rt-performance --duration 300s
```

## Best Practices

### System Configuration
1. **Dedicated System**: Use a dedicated system for racing if possible
2. **Minimal Software**: Install only necessary software
3. **Regular Maintenance**: Keep drivers and OS updated
4. **Monitoring**: Regularly check performance metrics

### Racing Session Preparation
1. Close unnecessary applications
2. Disable Windows Update/automatic updates
3. Ensure adequate cooling (CPU throttling affects performance)
4. Use wired network connection for telemetry
5. Disable antivirus real-time scanning during racing

### Hardware Recommendations
1. **CPU**: Modern multi-core processor with high single-thread performance
2. **RAM**: At least 16GB, preferably 32GB for headroom
3. **Storage**: NVMe SSD for OS and Racing Wheel Suite
4. **USB**: Dedicated USB controller for racing peripherals
5. **Power Supply**: Adequate wattage with clean power delivery

## Advanced Configurations

### Custom Power Plans (Windows)

Create a custom power plan optimized for racing:
```batch
# Create new power plan based on High Performance
powercfg -duplicatescheme 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c "Racing Performance"

# Set as active
powercfg -setactive "Racing Performance"

# Customize settings
powercfg -setacvalueindex "Racing Performance" SUB_PROCESSOR PROCTHROTTLEMIN 100
powercfg -setacvalueindex "Racing Performance" SUB_PROCESSOR PROCTHROTTLEMAX 100
powercfg -setacvalueindex "Racing Performance" SUB_PROCESSOR PERFBOOSTMODE 1
```

### Kernel Compilation (Linux Advanced Users)

For ultimate performance, compile a custom kernel:
```bash
# Enable RT patches
CONFIG_PREEMPT_RT=y
CONFIG_HIGH_RES_TIMERS=y
CONFIG_NO_HZ_FULL=y

# Disable power management
CONFIG_CPU_IDLE=n
CONFIG_CPU_FREQ=n
CONFIG_PM=n
```

This guide provides comprehensive power management optimization for the Racing Wheel Suite. Apply settings gradually and test performance after each change to identify the optimal configuration for your specific hardware.