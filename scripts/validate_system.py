#!/usr/bin/env python3
"""
System validation script for Racing Wheel Software

Validates the complete system integration including:
- Configuration validation
- Performance requirements
- Hardware compatibility
- Anti-cheat compliance
- Security measures
"""

import os
import sys
import json
import subprocess
import time
import platform
import argparse
from pathlib import Path
from typing import Dict, List, Optional, Tuple
import tempfile

class SystemValidator:
    """Comprehensive system validation"""
    
    def __init__(self, verbose: bool = False):
        self.verbose = verbose
        self.results = []
        self.errors = []
        self.warnings = []
        
    def log(self, message: str, level: str = "INFO"):
        """Log message with level"""
        if self.verbose or level != "DEBUG":
            print(f"[{level}] {message}")
            
    def add_result(self, test_name: str, passed: bool, message: str, details: Optional[Dict] = None):
        """Add test result"""
        result = {
            "test": test_name,
            "passed": passed,
            "message": message,
            "details": details or {}
        }
        self.results.append(result)
        
        if not passed:
            self.errors.append(f"{test_name}: {message}")
        
        status = "PASS" if passed else "FAIL"
        self.log(f"{status}: {test_name} - {message}")
        
    def add_warning(self, test_name: str, message: str):
        """Add warning"""
        self.warnings.append(f"{test_name}: {message}")
        self.log(f"WARN: {test_name} - {message}", "WARN")
        
    def validate_system_requirements(self) -> bool:
        """Validate system meets minimum requirements"""
        self.log("Validating system requirements...")
        
        # Check OS
        os_name = platform.system()
        os_version = platform.version()
        
        if os_name == "Windows":
            # Check Windows version (should be 10+)
            version_parts = platform.version().split('.')
            if len(version_parts) >= 2:
                major = int(version_parts[0])
                minor = int(version_parts[1])
                if major < 10:
                    self.add_result("os_version", False, f"Windows 10+ required, found {os_version}")
                    return False
                    
        elif os_name == "Linux":
            # Check kernel version
            kernel_version = platform.release()
            self.log(f"Linux kernel: {kernel_version}", "DEBUG")
            
        else:
            self.add_result("os_support", False, f"Unsupported OS: {os_name}")
            return False
            
        self.add_result("os_version", True, f"{os_name} {os_version}")
        
        # Check architecture
        arch = platform.machine()
        if arch not in ["x86_64", "AMD64"]:
            self.add_result("architecture", False, f"x86_64 required, found {arch}")
            return False
            
        self.add_result("architecture", True, f"Architecture: {arch}")
        
        # Check CPU cores
        try:
            import multiprocessing
            cpu_count = multiprocessing.cpu_count()
            if cpu_count < 2:
                self.add_warning("cpu_cores", f"Only {cpu_count} CPU core(s), 2+ recommended")
            else:
                self.add_result("cpu_cores", True, f"CPU cores: {cpu_count}")
        except:
            self.add_warning("cpu_cores", "Could not determine CPU count")
            
        # Check memory
        try:
            if os_name == "Linux":
                with open("/proc/meminfo", "r") as f:
                    meminfo = f.read()
                    for line in meminfo.split('\n'):
                        if line.startswith("MemTotal:"):
                            mem_kb = int(line.split()[1])
                            mem_gb = mem_kb / 1024 / 1024
                            if mem_gb < 4:
                                self.add_warning("memory", f"Only {mem_gb:.1f}GB RAM, 4GB+ recommended")
                            else:
                                self.add_result("memory", True, f"Memory: {mem_gb:.1f}GB")
                            break
            elif os_name == "Windows":
                # Use wmic to get memory info
                try:
                    result = subprocess.run(
                        ["wmic", "computersystem", "get", "TotalPhysicalMemory", "/value"],
                        capture_output=True, text=True, check=True
                    )
                    for line in result.stdout.split('\n'):
                        if line.startswith("TotalPhysicalMemory="):
                            mem_bytes = int(line.split('=')[1])
                            mem_gb = mem_bytes / 1024 / 1024 / 1024
                            if mem_gb < 4:
                                self.add_warning("memory", f"Only {mem_gb:.1f}GB RAM, 4GB+ recommended")
                            else:
                                self.add_result("memory", True, f"Memory: {mem_gb:.1f}GB")
                            break
                except:
                    self.add_warning("memory", "Could not determine memory size")
        except:
            self.add_warning("memory", "Could not check memory")
            
        return True
        
    def validate_build_system(self) -> bool:
        """Validate build system and dependencies"""
        self.log("Validating build system...")
        
        # Check Rust toolchain
        try:
            result = subprocess.run(["rustc", "--version"], capture_output=True, text=True, check=True)
            rust_version = result.stdout.strip()
            self.add_result("rust_toolchain", True, f"Rust: {rust_version}")
        except (subprocess.CalledProcessError, FileNotFoundError):
            self.add_result("rust_toolchain", False, "Rust toolchain not found")
            return False
            
        # Check Cargo
        try:
            result = subprocess.run(["cargo", "--version"], capture_output=True, text=True, check=True)
            cargo_version = result.stdout.strip()
            self.add_result("cargo", True, f"Cargo: {cargo_version}")
        except (subprocess.CalledProcessError, FileNotFoundError):
            self.add_result("cargo", False, "Cargo not found")
            return False
            
        return True
        
    def validate_configuration(self) -> bool:
        """Validate system configuration"""
        self.log("Validating configuration...")
        
        # Try to build and run config validation
        try:
            # Build the service
            result = subprocess.run(
                ["cargo", "build", "--bin", "wheeld", "--features", "validation"],
                cwd="crates/service",
                capture_output=True,
                text=True,
                timeout=120
            )
            
            if result.returncode != 0:
                self.add_result("build", False, f"Build failed: {result.stderr}")
                return False
                
            self.add_result("build", True, "Service builds successfully")
            
            # Run configuration validation
            result = subprocess.run(
                ["cargo", "run", "--bin", "wheeld", "--", "validate"],
                cwd="crates/service",
                capture_output=True,
                text=True,
                timeout=30
            )
            
            if result.returncode == 0:
                self.add_result("config_validation", True, "Configuration validation passed")
            else:
                self.add_result("config_validation", False, f"Config validation failed: {result.stderr}")
                return False
                
        except subprocess.TimeoutExpired:
            self.add_result("build_timeout", False, "Build or validation timed out")
            return False
        except Exception as e:
            self.add_result("build_error", False, f"Build error: {e}")
            return False
            
        return True
        
    def validate_performance(self) -> bool:
        """Validate performance requirements"""
        self.log("Validating performance...")
        
        # Run timing tests
        try:
            result = subprocess.run(
                ["cargo", "test", "--release", "test_timing", "--", "--nocapture"],
                cwd="crates/engine",
                capture_output=True,
                text=True,
                timeout=60
            )
            
            if result.returncode == 0:
                self.add_result("timing_tests", True, "Timing tests passed")
            else:
                self.add_result("timing_tests", False, f"Timing tests failed: {result.stderr}")
                
        except subprocess.TimeoutExpired:
            self.add_result("timing_timeout", False, "Timing tests timed out")
        except Exception as e:
            self.add_result("timing_error", False, f"Timing test error: {e}")
            
        # Run performance benchmarks
        try:
            result = subprocess.run(
                ["cargo", "bench", "--bench", "rt_timing"],
                capture_output=True,
                text=True,
                timeout=120
            )
            
            if result.returncode == 0:
                self.add_result("benchmarks", True, "Performance benchmarks completed")
                # Parse benchmark results for jitter analysis
                if "p99 jitter" in result.stdout:
                    self.log("Benchmark results available in output", "DEBUG")
            else:
                self.add_warning("benchmarks", f"Benchmarks failed: {result.stderr}")
                
        except subprocess.TimeoutExpired:
            self.add_warning("benchmark_timeout", "Benchmarks timed out")
        except Exception as e:
            self.add_warning("benchmark_error", f"Benchmark error: {e}")
            
        return True
        
    def validate_hardware_support(self) -> bool:
        """Validate hardware support"""
        self.log("Validating hardware support...")
        
        # Check HID support
        if platform.system() == "Linux":
            # Check for hidraw devices
            hidraw_devices = list(Path("/dev").glob("hidraw*"))
            if hidraw_devices:
                self.add_result("hidraw_devices", True, f"Found {len(hidraw_devices)} HID devices")
            else:
                self.add_warning("hidraw_devices", "No HID devices found")
                
            # Check udev rules
            udev_rules_path = Path("/etc/udev/rules.d")
            wheel_rules = list(udev_rules_path.glob("*wheel*")) + list(udev_rules_path.glob("*racing*"))
            if wheel_rules:
                self.add_result("udev_rules", True, f"Found {len(wheel_rules)} wheel udev rules")
            else:
                self.add_warning("udev_rules", "No wheel-specific udev rules found")
                
        elif platform.system() == "Windows":
            # Check for HID devices via PowerShell
            try:
                result = subprocess.run([
                    "powershell", "-Command",
                    "Get-PnpDevice -Class HIDClass | Where-Object {$_.Status -eq 'OK'} | Measure-Object | Select-Object -ExpandProperty Count"
                ], capture_output=True, text=True, check=True)
                
                hid_count = int(result.stdout.strip())
                if hid_count > 0:
                    self.add_result("hid_devices", True, f"Found {hid_count} HID devices")
                else:
                    self.add_warning("hid_devices", "No HID devices found")
                    
            except:
                self.add_warning("hid_check", "Could not check HID devices")
                
        return True
        
    def validate_security(self) -> bool:
        """Validate security measures"""
        self.log("Validating security...")
        
        # Check for debug symbols in release builds
        try:
            result = subprocess.run(
                ["cargo", "build", "--release"],
                capture_output=True,
                text=True,
                timeout=120
            )
            
            if result.returncode == 0:
                # Check if debug symbols are stripped
                target_dir = Path("target/release")
                if target_dir.exists():
                    executables = list(target_dir.glob("wheeld*"))
                    if executables:
                        self.add_result("release_build", True, "Release build successful")
                    else:
                        self.add_warning("release_executables", "No release executables found")
                        
        except subprocess.TimeoutExpired:
            self.add_warning("release_build_timeout", "Release build timed out")
        except Exception as e:
            self.add_warning("release_build_error", f"Release build error: {e}")
            
        # Validate anti-cheat compatibility
        try:
            result = subprocess.run(
                ["cargo", "run", "--bin", "wheeld", "--", "anti-cheat"],
                cwd="crates/service",
                capture_output=True,
                text=True,
                timeout=30
            )
            
            if result.returncode == 0:
                self.add_result("anticheat_report", True, "Anti-cheat compatibility report generated")
            else:
                self.add_warning("anticheat_report", "Could not generate anti-cheat report")
                
        except Exception as e:
            self.add_warning("anticheat_error", f"Anti-cheat validation error: {e}")
            
        return True
        
    def validate_integration(self) -> bool:
        """Validate system integration"""
        self.log("Validating integration...")
        
        # Run integration tests
        try:
            result = subprocess.run(
                ["cargo", "test", "--release", "integration", "--", "--nocapture"],
                cwd="crates/service",
                capture_output=True,
                text=True,
                timeout=180
            )
            
            if result.returncode == 0:
                self.add_result("integration_tests", True, "Integration tests passed")
            else:
                self.add_result("integration_tests", False, f"Integration tests failed: {result.stderr}")
                return False
                
        except subprocess.TimeoutExpired:
            self.add_result("integration_timeout", False, "Integration tests timed out")
            return False
        except Exception as e:
            self.add_result("integration_error", False, f"Integration test error: {e}")
            return False
            
        return True
        
    def run_diagnostics(self) -> bool:
        """Run system diagnostics"""
        self.log("Running system diagnostics...")
        
        try:
            result = subprocess.run(
                ["cargo", "run", "--bin", "wheeld", "--", "diagnostics"],
                cwd="crates/service",
                capture_output=True,
                text=True,
                timeout=60
            )
            
            if result.returncode == 0:
                self.add_result("system_diagnostics", True, "System diagnostics completed")
                
                # Parse diagnostic output
                if "✓" in result.stdout:
                    passed_count = result.stdout.count("✓")
                    self.log(f"Diagnostics: {passed_count} tests passed", "DEBUG")
                    
                if "✗" in result.stdout:
                    failed_count = result.stdout.count("✗")
                    self.add_warning("diagnostic_failures", f"{failed_count} diagnostic tests failed")
                    
            else:
                self.add_warning("system_diagnostics", f"Diagnostics failed: {result.stderr}")
                
        except subprocess.TimeoutExpired:
            self.add_warning("diagnostics_timeout", "System diagnostics timed out")
        except Exception as e:
            self.add_warning("diagnostics_error", f"Diagnostics error: {e}")
            
        return True
        
    def generate_report(self) -> Dict:
        """Generate validation report"""
        passed_tests = sum(1 for r in self.results if r["passed"])
        total_tests = len(self.results)
        
        report = {
            "timestamp": time.strftime("%Y-%m-%d %H:%M:%S UTC", time.gmtime()),
            "platform": {
                "system": platform.system(),
                "version": platform.version(),
                "architecture": platform.machine(),
                "python_version": platform.python_version()
            },
            "summary": {
                "total_tests": total_tests,
                "passed_tests": passed_tests,
                "failed_tests": total_tests - passed_tests,
                "warnings": len(self.warnings),
                "success_rate": (passed_tests / total_tests * 100) if total_tests > 0 else 0
            },
            "results": self.results,
            "warnings": self.warnings,
            "errors": self.errors
        }
        
        return report
        
    def run_all_validations(self) -> bool:
        """Run all validation tests"""
        self.log("Starting comprehensive system validation...")
        
        success = True
        
        # Run validation steps
        success &= self.validate_system_requirements()
        success &= self.validate_build_system()
        success &= self.validate_configuration()
        success &= self.validate_performance()
        success &= self.validate_hardware_support()
        success &= self.validate_security()
        success &= self.validate_integration()
        success &= self.run_diagnostics()
        
        return success

def main():
    parser = argparse.ArgumentParser(description="Racing Wheel System Validator")
    parser.add_argument("-v", "--verbose", action="store_true", help="Verbose output")
    parser.add_argument("-o", "--output", help="Output report file (JSON)")
    parser.add_argument("--quick", action="store_true", help="Skip time-consuming tests")
    
    args = parser.parse_args()
    
    validator = SystemValidator(verbose=args.verbose)
    
    try:
        success = validator.run_all_validations()
        
        # Generate report
        report = validator.generate_report()
        
        # Print summary
        print(f"\n{'='*60}")
        print("VALIDATION SUMMARY")
        print(f"{'='*60}")
        print(f"Total Tests: {report['summary']['total_tests']}")
        print(f"Passed: {report['summary']['passed_tests']}")
        print(f"Failed: {report['summary']['failed_tests']}")
        print(f"Warnings: {report['summary']['warnings']}")
        print(f"Success Rate: {report['summary']['success_rate']:.1f}%")
        
        if report['summary']['failed_tests'] > 0:
            print(f"\nFAILED TESTS:")
            for error in validator.errors:
                print(f"  ✗ {error}")
                
        if report['summary']['warnings'] > 0:
            print(f"\nWARNINGS:")
            for warning in validator.warnings:
                print(f"  ⚠ {warning}")
        
        # Save report if requested
        if args.output:
            with open(args.output, 'w') as f:
                json.dump(report, f, indent=2)
            print(f"\nDetailed report saved to: {args.output}")
            
        # Exit with appropriate code
        sys.exit(0 if success else 1)
        
    except KeyboardInterrupt:
        print("\nValidation interrupted by user")
        sys.exit(130)
    except Exception as e:
        print(f"Validation failed with error: {e}")
        sys.exit(1)

if __name__ == "__main__":
    main()