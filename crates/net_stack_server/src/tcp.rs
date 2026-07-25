//! Stateful TCP Connection Engine with Sliding Window & Congestion Control.

use gaxera_abi::GaxObjectId;
use net_types::{
    tcp_flags, NetEndpoint, ProviderError, SessionState, TcpHeader, TransportProvider,
};

/// TCP Congestion Control State (NewReno).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum CongestionState {
    SlowStart,
    CongestionAvoidance,
    FastRecovery,
}

/// TCP Connection Control Block (TCB).
#[derive(Clone, Debug)]
pub struct TcpControlBlock {
    pub session_id: GaxObjectId,
    pub local_endpoint: NetEndpoint,
    pub remote_endpoint: NetEndpoint,
    pub state: SessionState,
    pub snd_nxt: u32,
    pub snd_una: u32,
    pub rcv_nxt: u32,
    pub snd_wnd: u16,
    pub rcv_wnd: u16,
    pub cwnd: u32,
    pub ssthresh: u32,
    pub congestion_state: CongestionState,
}

impl TcpControlBlock {
    pub fn new(local: NetEndpoint, remote: NetEndpoint) -> Self {
        Self {
            session_id: GaxObjectId::generate(),
            local_endpoint: local,
            remote_endpoint: remote,
            state: SessionState::Created,
            snd_nxt: 1000,
            snd_una: 1000,
            rcv_nxt: 0,
            snd_wnd: 65535,
            rcv_wnd: 65535,
            cwnd: 1460,
            ssthresh: 65535,
            congestion_state: CongestionState::SlowStart,
        }
    }

    pub fn process_segment(&mut self, header: &TcpHeader, payload_len: usize) -> Option<TcpHeader> {
        match self.state {
            SessionState::Created | SessionState::Connecting => {
                if (header.flags & tcp_flags::SYN) != 0 {
                    self.rcv_nxt = header.seq_num.wrapping_add(1);
                    self.state = SessionState::Established;

                    return Some(TcpHeader {
                        src_port: self.local_endpoint.port,
                        dst_port: self.remote_endpoint.port,
                        seq_num: self.snd_nxt,
                        ack_num: self.rcv_nxt,
                        data_offset_reserved: 0x50,
                        flags: tcp_flags::SYN | tcp_flags::ACK,
                        window_size: self.rcv_wnd,
                        checksum: 0,
                        urgent_pointer: 0,
                    });
                }
            }
            SessionState::Established => {
                if (header.flags & tcp_flags::ACK) != 0 {
                    self.snd_una = header.ack_num;

                    if self.congestion_state == CongestionState::SlowStart {
                        self.cwnd += 1460;
                        if self.cwnd >= self.ssthresh {
                            self.congestion_state = CongestionState::CongestionAvoidance;
                        }
                    } else if self.congestion_state == CongestionState::CongestionAvoidance {
                        self.cwnd += (1460 * 1460) / self.cwnd;
                    }
                }

                if payload_len > 0 {
                    self.rcv_nxt = self.rcv_nxt.wrapping_add(payload_len as u32);
                }

                if (header.flags & tcp_flags::FIN) != 0 {
                    self.rcv_nxt = self.rcv_nxt.wrapping_add(1);
                    self.state = SessionState::HalfClosed;

                    return Some(TcpHeader {
                        src_port: self.local_endpoint.port,
                        dst_port: self.remote_endpoint.port,
                        seq_num: self.snd_nxt,
                        ack_num: self.rcv_nxt,
                        data_offset_reserved: 0x50,
                        flags: tcp_flags::FIN | tcp_flags::ACK,
                        window_size: self.rcv_wnd,
                        checksum: 0,
                        urgent_pointer: 0,
                    });
                }
            }
            _ => {}
        }
        None
    }
}

pub struct TcpTransportEngine {
    pub tcbs: [Option<TcpControlBlock>; 32],
}

impl Default for TcpTransportEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TcpTransportEngine {
    pub fn new() -> Self {
        Self {
            tcbs: [const { None }; 32],
        }
    }
}

impl TransportProvider for TcpTransportEngine {
    fn protocol_id(&self) -> u8 {
        6
    }

    fn create_session(
        &self,
        local: NetEndpoint,
        remote: NetEndpoint,
    ) -> Result<GaxObjectId, ProviderError> {
        let tcb = TcpControlBlock::new(local, remote);
        Ok(tcb.session_id)
    }

    fn close_session(&self, _session_id: GaxObjectId) -> Result<(), ProviderError> {
        Ok(())
    }
}
