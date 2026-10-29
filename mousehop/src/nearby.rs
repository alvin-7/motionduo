//! Email is a LAN discovery namespace, never an authorization credential.
use mousehop_ipc::DiscoveredDevice;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

pub(crate) fn normalize_email(value: &str) -> Result<Option<String>, &'static str> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() > 254
        || !value.is_ascii()
        || value
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
    {
        return Err("Enter a valid email address");
    }
    let parts: Vec<_> = value.split('@').collect();
    if parts.len() != 2
        || parts[0].is_empty()
        || parts[0].len() > 64
        || !parts[1].contains('.')
        || parts[1].split('.').any(|s| {
            s.is_empty()
                || s.starts_with('-')
                || s.ends_with('-')
                || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
    {
        return Err("Enter a valid email address");
    }
    Ok(Some(value.to_ascii_lowercase()))
}

pub(crate) fn email_group(email: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("mousehop-lan-discovery-v1\0{email}"))
    )
}

#[derive(Default)]
pub(crate) struct NearbyDevices {
    devices: HashMap<String, DiscoveredDevice>,
}

impl NearbyDevices {
    pub(crate) fn clear(&mut self) {
        self.devices.clear();
    }
    pub(crate) fn upsert(&mut self, device: DiscoveredDevice) {
        if self.devices.len() < 128 || self.devices.contains_key(&device.id) {
            self.devices.insert(device.id.clone(), device);
        }
    }
    pub(crate) fn remove(&mut self, id: &str) {
        self.devices.remove(id);
    }
    pub(crate) fn snapshot(&self) -> Vec<DiscoveredDevice> {
        let mut devices: Vec<_> = self.devices.values().cloned().collect();
        devices.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        devices
    }
    pub(crate) fn get(&self, id: &str) -> Option<DiscoveredDevice> {
        self.devices.get(id).cloned()
    }
    pub(crate) fn by_fingerprint(&self, fingerprint: &str) -> Option<DiscoveredDevice> {
        self.devices
            .values()
            .find(|d| d.fingerprint == fingerprint)
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn email_namespace_is_normalized_but_not_provider_rewritten() {
        assert_eq!(
            normalize_email("  Alice@Example.COM ").unwrap().as_deref(),
            Some("alice@example.com")
        );
        assert_eq!(normalize_email(" ").unwrap(), None);
        for email in [
            "a",
            "@example.com",
            "a@@example.com",
            "a@.com",
            "a@x..com",
            "a@-example.com",
            "a b@example.com",
        ] {
            assert!(normalize_email(email).is_err(), "{email}");
        }
        assert_ne!(email_group("a+b@example.com"), email_group("a@example.com"));
        assert_eq!(email_group("a@example.com").len(), 64);
    }
    #[test]
    fn cache_is_bounded_and_removes_goodbye_devices() {
        let mut cache = NearbyDevices::default();
        for i in 0..150 {
            cache.upsert(DiscoveredDevice {
                id: i.to_string(),
                name: i.to_string(),
                hostname: "peer.local".into(),
                addresses: vec![],
                port: 4252,
                fingerprint: "fp".into(),
                platform: "test".into(),
            });
        }
        assert_eq!(cache.snapshot().len(), 128);
        cache.remove("0");
        assert!(cache.get("0").is_none());
        assert_eq!(cache.snapshot().len(), 127);
    }
}
