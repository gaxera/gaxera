//! Dynamic ARP Resolution Cache with TTL Eviction.

use net_types::{ArpHeader, Ipv4Addr, MacAddress};

#[derive(Copy, Clone, Debug)]
pub struct ArpEntry {
    pub ip: Ipv4Addr,
    pub mac: MacAddress,
    pub timestamp_sec: u64,
}

pub struct ArpCache {
    entries: [Option<ArpEntry>; 64],
    ttl_sec: u64,
}

impl Default for ArpCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ArpCache {
    pub const DEFAULT_TTL: u64 = 300; // 300 seconds

    pub fn new() -> Self {
        Self {
            entries: [None; 64],
            ttl_sec: Self::DEFAULT_TTL,
        }
    }

    pub fn insert(&mut self, ip: Ipv4Addr, mac: MacAddress, current_time_sec: u64) {
        // Update existing entry if present
        for entry in self.entries.iter_mut().flatten() {
            if entry.ip == ip {
                entry.mac = mac;
                entry.timestamp_sec = current_time_sec;
                return;
            }
        }

        // Insert into first free slot
        for slot in self.entries.iter_mut() {
            if slot.is_none() {
                *slot = Some(ArpEntry {
                    ip,
                    mac,
                    timestamp_sec: current_time_sec,
                });
                return;
            }
        }

        // Evict first slot if cache is full
        self.entries[0] = Some(ArpEntry {
            ip,
            mac,
            timestamp_sec: current_time_sec,
        });
    }

    pub fn lookup(&mut self, ip: Ipv4Addr, current_time_sec: u64) -> Option<MacAddress> {
        for slot in self.entries.iter_mut() {
            if let Some(entry) = slot {
                if entry.ip == ip {
                    if current_time_sec.saturating_sub(entry.timestamp_sec) > self.ttl_sec {
                        *slot = None; // Evict expired
                        return None;
                    }
                    return Some(entry.mac);
                }
            }
        }
        None
    }

    pub fn process_arp_packet(
        &mut self,
        header: &ArpHeader,
        current_time_sec: u64,
    ) -> Option<ArpHeader> {
        self.insert(header.sender_ip, header.sender_mac, current_time_sec);

        if header.oper == ArpHeader::OPER_REQUEST {
            // Generate ARP Reply
            Some(ArpHeader {
                htype: 1,
                ptype: 0x0800,
                hlen: 6,
                plen: 4,
                oper: ArpHeader::OPER_REPLY,
                sender_mac: header.target_mac,
                sender_ip: header.target_ip,
                target_mac: header.sender_mac,
                target_ip: header.sender_ip,
            })
        } else {
            None
        }
    }
}
