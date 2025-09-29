# Power Management Guide for Racing Wheel Suite

This document provides guidance on optimizing power management settings for the best racing wheel performance.

## Overview

Racing wheel software requires consistent, low-latency performance to deliver smooth force feedback at 1kHz. Power management features in modern operating systems can interfere with this real-time performance by introducing latency spikes and jitter.

## Windows Power Management

### High Performance Power Plan

For optimal performance, use the High Performance power plan:

1. Open Control Panel → Power Options
2. Select "High performance" plan
3. Click "Change plan settings"
4. Set both options to "Never"

**Command Line:**
```cmd
powercfg /setactive 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c
```

### USB Selective Suspend

USB selective suspend can cause racing wheels to disconnect or introduce latency:

**Disable via Device Manager:**
1. Open Device Manager
2. Expand "Universal Serial Bus controllers"
3. Right-click each "USB Root Hub" → Properties
4. Power Management tab → Uncheck "Allow the computer to turn off this device"

**Disable via Registry (Advanced):**
```reg
[HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Services\USB]
"DisableSelectiveSuspend"=dword:00000001
```

**Disable for specific devices via PowerShell:**
```powershell
Get-WmiObject -Class Win32_USBHub | ForEach-Object {
    $_.SetPowerState(1)
}
```

### CPU Power Management

Disable CPU throttling for consistent performance:

**Via Power Options:**
1. Control Panel → Power Options → Change plan settings
2. Change advanced power settings
3. Processor power management → Minimum processor state → 100%
4. System cooling policy → Active

**Via Command Line:**
```cmd
powercfg /setacvalueindex SCHEME_CURRENT SUB_PROCESSOR PROCTHROTTLEMIN 100
powercfg /setacvalueindex SCHEME_CURRENT SUB_PROCESSOR PROCTHROTTLEMAX 100
powercfg /setactive SCHEME_CURRENT
```

### Windows Game Mode

Enable Game Mode for better resource allocation:

1. Settings → Gaming → Game Mode → On
2. Settings → Gaming → Game Bar → Off (reduces overhead)

### MMCSS (Multimedia Class Scheduler Service)

The Racing Wheel Suite automatically configures MMCSS for real-time threads. Ensure the service is running:

```cmd
sc query MMCSS
sc config MMCSS start= auto
sc start MMCSS
```

## Linux Power Management

### CPU Governor

Set CPU governor to "performance" mode:

```bash
# Check current governor
cat /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# Set performance governor
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# Make permanent (add to /etc/rc.local or systemd service)
echo 'echo performance > /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor' | sudo tee -a /etc/rc.local
```

### USB Autosuspend

Disable USB autosuspend for racing wheels:

**Temporary:**
```bash
# Disable for all USB devices
echo -1 | sudo tee /sys/bus/usb/devices/*/power/autosuspend_delay_ms

# Disable for specific device (replace 1-1 with your device)
echo -1 | sudo tee /sys/bus/usb/devices/1-1/power/autosuspend_delay_ms
```

**Permanent via udev rules (included in installation):**
```udev
# /etc/udev/rules.d/99-racing-wheel-suite.rules
ACTION=="add", SUBSYSTEM=="usb", ATTRS{idVendor}=="046d", ATTR{power/autosuspend}="-1"
```

### Real-Time Scheduling

Configure real-time scheduling limits:

**Edit /etc/security/limits.conf:**
```
@audio   -  rtprio     95
@audio   -  memlock    unlimited
username -  rtprio     95
username -  memlock    unlimited
```

**Install rtkit for user-space RT scheduling:**
```bash
# Ubuntu/Debian
sudo apt install rtkit

# Fedora
sudo dnf install rtkit

# Arch
sudo pacman -S rtkit
```

### IRQ Affinity

Pin USB controller IRQs to specific CPU cores:

```bash
# Find USB controller IRQ
cat /proc/interrupts | grep -i usb

# Pin IRQ to CPU core (replace 23 with actual IRQ number)
echo 2 | sudo tee /proc/irq/23/smp_affinity
```

## Hardware-Specific Optimizations

### Intel Systems

**Disable C-States in BIOS:**
- Advanced → CPU Configuration → C-States → Disabled

**Disable Intel SpeedStep:**
- Advanced → CPU Configuration → Intel SpeedStep → Disabled

**Command line (if available):**
```bash
# Add to kernel parameters in GRUB
intel_idle.max_cstate=0 processor.max_cstate=1
```

### AMD Systems

**Disable Cool'n'Quiet in BIOS:**
- Advanced → CPU Configuration → Cool'n'Quiet → Disabled

**Disable C6 State:**
- Advanced → CPU Configuration → C6 Mode → Disabled

### NVIDIA Graphics

Disable power management for NVIDIA GPUs:

**Windows:**
```cmd
nvidia-smi -pm 1
nvidia-smi -pl 300  # Set power limit to maximum
```

**Linux:**
```bash
sudo nvidia-smi -pm 1
sudo nvidia-smi -pl 300
```

## Verification and Testing

### Performance Monitoring

**Windows - Use Performance Toolkit:**
```cmd
# Install Windows Performance Toolkit
# Run trace to monitor latency
wpa.exe
```

**Linux - Use cyclictest:**
```bash
sudo apt install rt-tests
sudo cyclictest -t1 -p 80 -n -i 1000 -l 100000
```

### Racing Wheel Suite Diagnostics

The software includes built-in diagnostics:

```bash
# Check real-time performance
wheelctl diagnostics rt-performance

# Monitor jitter over time
wheelctl diagnostics monitor --duration 60

# Generate performance report
wheelctl diagnostics report --output performance.json
```

## Troubleshooting

### High Jitter/Latency

1. **Check power plan:** Ensure High Performance mode is active
2. **Verify USB settings:** Disable selective suspend
3. **Monitor CPU usage:** Ensure no background processes consuming CPU
4. **Check thermal throttling:** Monitor CPU temperatures
5. **Update drivers:** Ensure latest USB and chipset drivers

### USB Disconnections

1. **Power management:** Disable USB selective suspend
2. **Cable quality:** Use high-quality, short USB cables
3. **USB ports:** Use USB 2.0 ports for better compatibility
4. **Hub isolation:** Connect directly to motherboard USB ports

### Inconsistent Performance

1. **Background processes:** Disable unnecessary services
2. **Windows updates:** Pause automatic updates during racing
3. **Antivirus:** Add Racing Wheel Suite to exclusions
4. **Network activity:** Disable network-intensive applications

## Automated Configuration

The Racing Wheel Suite installer can automatically configure many of these settings:

**Windows:**
```cmd
# Run installer with power optimization
RacingWheelSuite.msi OPTIMIZE_POWER=1
```

**Linux:**
```bash
# Run installer with power optimization
./install.sh --optimize-power
```

## Monitoring and Maintenance

### Regular Checks

1. **Weekly:** Verify power plan settings haven't changed
2. **Monthly:** Check for Windows/driver updates that might reset settings
3. **Before racing:** Run quick diagnostics to verify performance

### Performance Baselines

Establish performance baselines for your system:

```bash
# Create baseline
wheelctl diagnostics baseline --save system-baseline.json

# Compare current performance
wheelctl diagnostics compare --baseline system-baseline.json
```

## Advanced Configuration

### Custom Power Profiles

Create custom power profiles for racing:

**Windows PowerShell:**
```powershell
# Create racing power scheme
$racing_guid = [System.Guid]::NewGuid()
powercfg /duplicatescheme 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c $racing_guid
powercfg /changename $racing_guid "Racing Performance" "Optimized for racing wheel performance"

# Configure settings
powercfg /setacvalueindex $racing_guid SUB_PROCESSOR PROCTHROTTLEMIN 100
powercfg /setacvalueindex $racing_guid SUB_PROCESSOR PROCTHROTTLEMAX 100
powercfg /setacvalueindex $racing_guid SUB_USB USBSELECTIVESUSPEND 0
```

### System Service Optimization

**Windows - Disable unnecessary services:**
```cmd
sc config "Windows Search" start= disabled
sc config "Superfetch" start= disabled
sc config "Themes" start= disabled
```

**Linux - Optimize systemd:**
```bash
# Disable unnecessary services
sudo systemctl disable bluetooth
sudo systemctl disable cups
sudo systemctl disable NetworkManager-wait-online
```

## References

- [Microsoft Real-Time Communications Guidelines](https://docs.microsoft.com/en-us/windows/win32/procthread/multimedia-class-scheduler-service)
- [Linux Real-Time Kernel Documentation](https://www.kernel.org/doc/Documentation/scheduler/sched-rt-group.txt)
- [Intel Performance Optimization Guide](https://software.intel.com/content/www/us/en/develop/articles/intel-performance-optimization-methodology.html)

## Support

For power management issues:

1. Check the [troubleshooting guide](TROUBLESHOOTING.md)
2. Run diagnostics: `wheelctl diagnostics power-management`
3. Contact support with diagnostic output
4. Join the community forum for peer assistance