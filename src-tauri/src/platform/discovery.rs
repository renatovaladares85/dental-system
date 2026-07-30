use std::time::Duration;

use mdns_sd::{ServiceDaemon, ServiceInfo};
use uuid::Uuid;

use super::{PlatformError, PlatformResult};

const SERVICE_TYPE: &str = "_dental._tcp.local.";

/// RAII registration for LAN discovery. Construction explicitly requires the
/// persisted startup state to be READY; dropping unregisters the service.
pub struct MdnsRegistration {
    daemon: ServiceDaemon,
    fullname: String,
}

impl MdnsRegistration {
    pub fn register_if_ready(
        startup_ready: bool,
        listener_covers_all_ip_families: bool,
        hostname: &str,
        port: u16,
        installation_id: &str,
    ) -> PlatformResult<Self> {
        // Address auto-discovery may publish both A and AAAA records. It is
        // therefore allowed only after the caller has bound a verified
        // IPv4/IPv6 dual-stack wildcard listener.
        if !startup_ready || !listener_covers_all_ip_families || port == 0 {
            return Err(PlatformError::invalid_state());
        }
        let installation =
            Uuid::parse_str(installation_id).map_err(|_| PlatformError::invalid_input())?;
        if hostname != format!("dental-{installation}.local") {
            return Err(PlatformError::invalid_input());
        }

        let daemon = ServiceDaemon::new().map_err(|_| PlatformError::unavailable())?;
        let mdns_hostname = format!("{hostname}.");
        let instance = format!(
            "Offline Dental System {}",
            installation
                .simple()
                .to_string()
                .chars()
                .take(8)
                .collect::<String>()
        );
        let properties = [("path", "/"), ("tls", "1"), ("api", "v1")];
        let service = ServiceInfo::new(
            SERVICE_TYPE,
            &instance,
            &mdns_hostname,
            "",
            port,
            &properties[..],
        )
        .map_err(|_| PlatformError::invalid_input())?
        .enable_addr_auto();
        let fullname = service.get_fullname().to_owned();
        daemon
            .register(service)
            .map_err(|_| PlatformError::unavailable())?;

        Ok(Self { daemon, fullname })
    }

    pub fn fullname(&self) -> &str {
        &self.fullname
    }
}

impl Drop for MdnsRegistration {
    fn drop(&mut self) {
        if let Ok(receiver) = self.daemon.unregister(&self.fullname) {
            let _ = receiver.recv_timeout(Duration::from_secs(2));
        }
        if let Ok(receiver) = self.daemon.shutdown() {
            let _ = receiver.recv_timeout(Duration::from_secs(2));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MdnsRegistration;

    #[test]
    fn discovery_is_refused_before_ready_without_starting_a_daemon() {
        let result = MdnsRegistration::register_if_ready(
            false,
            true,
            "dental-018f0f7d-82ab-7d6e-b234-0123456789ab.local",
            8743,
            "018f0f7d-82ab-7d6e-b234-0123456789ab",
        );
        assert!(result.is_err());
    }

    #[test]
    fn discovery_is_refused_for_a_single_family_listener() {
        let result = MdnsRegistration::register_if_ready(
            true,
            false,
            "dental-018f0f7d-82ab-7d6e-b234-0123456789ab.local",
            8743,
            "018f0f7d-82ab-7d6e-b234-0123456789ab",
        );
        assert!(result.is_err());
    }
}
