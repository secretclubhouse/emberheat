//! Network diagnostics for Matter commissioning
//!
//! This module provides network diagnostic checks to help troubleshoot
//! Matter commissioning issues. It checks:
//!
//! - UDP port 5540 availability (standard Matter port)
//! - Avahi/mDNS service status (required for device discovery)
//! - Network interface accessibility
//!
//! # Example
//!
//! ```no_run
//! use thermos_rust::network_diagnostics::run_network_diagnostics;
//!
//! #[tokio::main]
//! async fn main() {
//!     if let Err(warnings) = run_network_diagnostics(5540).await {
//!         for warning in warnings {
//!             println!("Warning: {}", warning);
//!         }
//!     }
//! }
//! ```

use std::net::{SocketAddr, UdpSocket};
use std::process::Command;
use tracing::{info, warn};

/// Network diagnostic warnings
#[derive(Debug)]
pub struct NetworkWarning {
    pub category: String,
    pub message: String,
    pub suggestion: Option<String>,
}

impl NetworkWarning {
    fn new(category: &str, message: &str, suggestion: Option<&str>) -> Self {
        Self {
            category: category.to_string(),
            message: message.to_string(),
            suggestion: suggestion.map(|s| s.to_string()),
        }
    }
}

/// Run comprehensive network diagnostics for Matter
///
/// Returns Ok(()) if all checks pass, or Err(warnings) if issues are found.
/// Issues are non-fatal but may affect Matter commissioning.
pub async fn run_network_diagnostics(matter_port: u16) -> Result<(), Vec<NetworkWarning>> {
    info!("Running network diagnostics for Matter commissioning...");

    let mut warnings = Vec::new();

    // Check 1: UDP port availability
    check_udp_port_binding(matter_port, &mut warnings);

    // Check 2: Avahi/mDNS service
    check_avahi_service(&mut warnings);

    // Check 3: Network interfaces
    check_network_interfaces(&mut warnings);

    // Summary
    if warnings.is_empty() {
        info!("✓ Network diagnostics passed: All checks OK");
        Ok(())
    } else {
        warn!("⚠ Network diagnostics found {} potential issue(s):", warnings.len());
        for warning in &warnings {
            warn!("  [{}] {}", warning.category, warning.message);
            if let Some(ref suggestion) = warning.suggestion {
                warn!("    → {}", suggestion);
            }
        }
        Err(warnings)
    }
}

/// Check if UDP port can be bound
fn check_udp_port_binding(port: u16, warnings: &mut Vec<NetworkWarning>) {
    let addr: SocketAddr = ([0, 0, 0, 0], port).into();

    match UdpSocket::bind(addr) {
        Ok(_socket) => {
            info!("✓ UDP port {} is available and can be bound", port);
            // Socket is automatically closed when dropped
        }
        Err(e) => {
            let warning = NetworkWarning::new(
                "Port Binding",
                &format!("Failed to bind UDP port {}: {}", port, e),
                Some(&format!(
                    "Check if another process is using port {}. Try: sudo netstat -ulnp | grep {}",
                    port, port
                )),
            );
            warnings.push(warning);
        }
    }
}

/// Check Avahi/mDNS service status
fn check_avahi_service(warnings: &mut Vec<NetworkWarning>) {
    // Try to check if avahi-daemon is running using systemctl
    match Command::new("systemctl")
        .args(["is-active", "avahi-daemon"])
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                let status = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if status == "active" {
                    info!("✓ Avahi daemon is running (required for mDNS device discovery)");
                } else {
                    let warning = NetworkWarning::new(
                        "mDNS Service",
                        &format!("Avahi daemon status: {}", status),
                        Some("Start Avahi: sudo systemctl start avahi-daemon"),
                    );
                    warnings.push(warning);
                }
            } else {
                let warning = NetworkWarning::new(
                    "mDNS Service",
                    "Avahi daemon is not active",
                    Some("Install and start Avahi: sudo apt install avahi-daemon && sudo systemctl start avahi-daemon"),
                );
                warnings.push(warning);
            }
        }
        Err(e) => {
            // systemctl command failed - might not be a systemd system or avahi not installed
            warn!("Could not check Avahi status via systemctl: {}", e);

            // Try alternative check - see if avahi-daemon process exists
            match Command::new("pgrep").arg("avahi-daemon").output() {
                Ok(output) if output.status.success() && !output.stdout.is_empty() => {
                    info!("✓ Avahi daemon process is running");
                }
                _ => {
                    let warning = NetworkWarning::new(
                        "mDNS Service",
                        "Could not verify Avahi daemon is running",
                        Some("Matter requires mDNS for device discovery. Install Avahi: sudo apt install avahi-daemon"),
                    );
                    warnings.push(warning);
                }
            }
        }
    }
}

/// Check network interfaces
fn check_network_interfaces(warnings: &mut Vec<NetworkWarning>) {
    // Try to list network interfaces using ip command
    match Command::new("ip").args(["addr", "show"]).output() {
        Ok(output) if output.status.success() => {
            let interfaces = String::from_utf8_lossy(&output.stdout);

            // Check for at least one non-loopback interface with an IP
            let has_network = interfaces.lines().any(|line| {
                (line.contains("inet ") && !line.contains("127.0.0.1"))
                    || (line.contains("inet6 ") && !line.contains("::1"))
            });

            if has_network {
                info!("✓ Network interfaces configured with IP addresses");
            } else {
                let warning = NetworkWarning::new(
                    "Network",
                    "No active network interfaces found with IP addresses",
                    Some("Matter requires network connectivity. Check your network configuration."),
                );
                warnings.push(warning);
            }

            // Log interface info for debugging
            for line in interfaces.lines() {
                if line.starts_with(char::is_numeric) || line.contains("inet") {
                    tracing::debug!("Network interface: {}", line.trim());
                }
            }
        }
        Ok(_) => {
            // Command succeeded but returned non-zero exit status
            warn!("'ip addr show' command failed");
            let warning = NetworkWarning::new(
                "Network",
                "Could not list network interfaces",
                Some("Ensure you have active network connectivity for Matter commissioning"),
            );
            warnings.push(warning);
        }
        Err(e) => {
            warn!("Could not check network interfaces: {}", e);
            let warning = NetworkWarning::new(
                "Network",
                "Could not verify network interface configuration",
                Some("Ensure you have active network connectivity for Matter commissioning"),
            );
            warnings.push(warning);
        }
    }
}

/// Check firewall rules (optional, informational only)
pub async fn check_firewall_rules(port: u16) {
    info!("Checking firewall rules for Matter port {}...", port);

    // Try to check ufw status
    match Command::new("sudo").args(["ufw", "status"]).output() {
        Ok(output) if output.status.success() => {
            let status = String::from_utf8_lossy(&output.stdout);
            if status.contains("Status: active") {
                if status.contains(&format!("{}/udp", port)) {
                    info!("✓ Firewall allows UDP port {}", port);
                } else {
                    warn!("Firewall is active but UDP port {} may not be allowed", port);
                    warn!("  → To allow: sudo ufw allow {}/udp", port);
                }
            } else {
                info!("  Firewall (ufw) is inactive");
            }
        }
        Ok(_) => {
            info!("  Firewall command failed or permission denied");
        }
        Err(_) => {
            info!("  Could not check firewall status (ufw not available or requires sudo)");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_port_binding_check() {
        // Test with a high port that's likely available
        let mut warnings = Vec::new();
        check_udp_port_binding(9999, &mut warnings);
        // Should not produce warnings on most systems
    }

    #[tokio::test]
    async fn test_network_diagnostics_runs() {
        // Just verify the function runs without panicking
        let _ = run_network_diagnostics(5540).await;
    }
}
