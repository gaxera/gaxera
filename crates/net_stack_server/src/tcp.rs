//! Stateful TCP Connection Engine with Sliding Window & NewReno Congestion Control.

use alloc::vec::Vec;
use gaxera_abi::GaxObjectId;
use net_types::{
    tcp_flags, NetEndpoint, ProviderError, SessionState, TcpHeader, TransportProvider,
};
use spinning_top::Spinlock;

/// TCP Congestion Control State (NewReno - RFC 5681).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum CongestionState {
    SlowStart,
    CongestionAvoidance,
    FastRecovery,
}

/// Pending In-Flight Segment for Retransmission Queue.
#[derive(Clone, Debug)]
pub struct PendingSegment {
    pub seq_num: u32,
    pub length: u32,
    pub rto_ticks: u32,
    pub retransmit_count: u32,
}

/// TCP Connection Control Block (TCB - RFC 793).
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
    pub dup_ack_count: u32,
    pub last_ack_num: u32,
    pub srtt: u32,   // Smoothed RTT in ms
    pub rttvar: u32, // RTT variance in ms
    pub rto: u32,    // Retransmission Timeout in ticks/ms (min 200ms)
    pub retransmit_queue: Vec<PendingSegment>,
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
            dup_ack_count: 0,
            last_ack_num: 1000,
            srtt: 100,
            rttvar: 50,
            rto: 200,
            retransmit_queue: Vec::new(),
        }
    }

    /// Process incoming TCP segment according to RFC 793 state machine & NewReno.
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
                // 1. Process ACK and NewReno Loss Detection
                if (header.flags & tcp_flags::ACK) != 0 {
                    let ack = header.ack_num;

                    if ack == self.last_ack_num && payload_len == 0 {
                        // Duplicate ACK received
                        self.dup_ack_count += 1;

                        if self.dup_ack_count == 3 {
                            // RFC 5681 Fast Retransmit & Fast Recovery Entry
                            self.ssthresh = (self.cwnd / 2).max(2 * 1460);
                            self.cwnd = self.ssthresh + 3 * 1460;
                            self.congestion_state = CongestionState::FastRecovery;

                            // Fast Retransmit missing segment immediately
                            return Some(TcpHeader {
                                src_port: self.local_endpoint.port,
                                dst_port: self.remote_endpoint.port,
                                seq_num: self.snd_una,
                                ack_num: self.rcv_nxt,
                                data_offset_reserved: 0x50,
                                flags: tcp_flags::ACK,
                                window_size: self.rcv_wnd,
                                checksum: 0,
                                urgent_pointer: 0,
                            });
                        } else if self.congestion_state == CongestionState::FastRecovery {
                            // Inflation during Fast Recovery
                            self.cwnd += 1460;
                        }
                    } else if ack > self.snd_una {
                        // New cumulative ACK received - advance snd_una
                        let bytes_acked = ack - self.snd_una;
                        self.snd_una = ack;
                        self.last_ack_num = ack;
                        self.dup_ack_count = 0;

                        // Purge acknowledged segments from retransmission queue
                        self.retransmit_queue
                            .retain(|seg| seg.seq_num + seg.length > ack);

                        if self.congestion_state == CongestionState::FastRecovery {
                            // Full ACK received: Exit Fast Recovery -> Congestion Avoidance
                            self.cwnd = self.ssthresh;
                            self.congestion_state = CongestionState::CongestionAvoidance;
                        } else if self.congestion_state == CongestionState::SlowStart {
                            self.cwnd += bytes_acked.min(1460);
                            if self.cwnd >= self.ssthresh {
                                self.congestion_state = CongestionState::CongestionAvoidance;
                            }
                        } else if self.congestion_state == CongestionState::CongestionAvoidance {
                            self.cwnd += (1460 * 1460) / self.cwnd;
                        }
                    }
                }

                // 2. Process Payload & Advance RCV.NXT
                if payload_len > 0 {
                    self.rcv_nxt = self.rcv_nxt.wrapping_add(payload_len as u32);
                }

                // 3. Process FIN (Passive Connection Teardown)
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
            SessionState::HalfClosed if (header.flags & tcp_flags::ACK) != 0 => {
                self.state = SessionState::Closed;
            }
            _ => {}
        }
        None
    }

    /// Timer tick processing: Evaluates RTO timeouts and performs exponential backoff.
    pub fn handle_timer_tick(&mut self) -> Option<TcpHeader> {
        for seg in &mut self.retransmit_queue {
            if seg.rto_ticks > 0 {
                seg.rto_ticks -= 1;
            } else {
                // RTO expired: Retransmit segment & double RTO (exponential backoff)
                seg.retransmit_count += 1;
                seg.rto_ticks = self.rto * (1 << seg.retransmit_count.min(4));

                // Collapse congestion window to SlowStart on timeout
                self.ssthresh = (self.cwnd / 2).max(2 * 1460);
                self.cwnd = 1460;
                self.congestion_state = CongestionState::SlowStart;

                return Some(TcpHeader {
                    src_port: self.local_endpoint.port,
                    dst_port: self.remote_endpoint.port,
                    seq_num: seg.seq_num,
                    ack_num: self.rcv_nxt,
                    data_offset_reserved: 0x50,
                    flags: tcp_flags::ACK,
                    window_size: self.rcv_wnd,
                    checksum: 0,
                    urgent_pointer: 0,
                });
            }
        }
        None
    }
}

/// Thread-safe TCP Transport Engine storing connection control blocks.
pub struct TcpTransportEngine {
    pub tcbs: Spinlock<Vec<TcpControlBlock>>,
}

impl Default for TcpTransportEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TcpTransportEngine {
    pub fn new() -> Self {
        Self {
            tcbs: Spinlock::new(Vec::new()),
        }
    }

    /// Polls timer ticks across all active TCP Control Blocks and collects retransmitted segment headers.
    pub fn poll_timer_ticks(&self) -> Vec<TcpHeader> {
        let mut guard = self.tcbs.lock();
        let mut retransmissions = Vec::new();
        for tcb in guard.iter_mut() {
            if let Some(retransmit_hdr) = tcb.handle_timer_tick() {
                retransmissions.push(retransmit_hdr);
            }
        }
        retransmissions
    }
}

impl TransportProvider for TcpTransportEngine {
    fn protocol_id(&self) -> u8 {
        6 // TCP
    }

    fn create_session(
        &self,
        local: NetEndpoint,
        remote: NetEndpoint,
    ) -> Result<GaxObjectId, ProviderError> {
        let tcb = TcpControlBlock::new(local, remote);
        let session_id = tcb.session_id;
        self.tcbs.lock().push(tcb);
        Ok(session_id)
    }

    fn close_session(&self, session_id: GaxObjectId) -> Result<(), ProviderError> {
        let mut guard = self.tcbs.lock();
        if let Some(tcb) = guard.iter_mut().find(|t| t.session_id == session_id) {
            tcb.state = SessionState::HalfClosed;
            return Ok(());
        }
        Err(ProviderError::NotReady)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tcp_retransmission_timer_silent_loss_recovery() {
        let engine = TcpTransportEngine::new();
        let local = NetEndpoint::new(net_types::IpAddr::V4(net_types::Ipv4Addr::LOOPBACK), 80);
        let remote = NetEndpoint::new(net_types::IpAddr::V4(net_types::Ipv4Addr::LOOPBACK), 54321);

        let sid = engine.create_session(local, remote).unwrap();

        // Access TCB and enqueue in-flight unacked segment with RTO 2 ticks
        {
            let mut guard = engine.tcbs.lock();
            let tcb = guard.iter_mut().find(|t| t.session_id == sid).unwrap();
            tcb.state = SessionState::Established;
            tcb.retransmit_queue.push(PendingSegment {
                seq_num: 1000,
                length: 1460,
                rto_ticks: 1,
                retransmit_count: 0,
            });
        }

        // Tick 1: No timeout yet
        let retrans1 = engine.poll_timer_ticks();
        assert!(retrans1.is_empty());

        // Tick 2: Timeout fires! RTO retransmission segment returned
        let retrans2 = engine.poll_timer_ticks();
        assert_eq!(retrans2.len(), 1);
        assert_eq!(retrans2[0].seq_num, 1000);
        assert_eq!(retrans2[0].flags, tcp_flags::ACK);
    }
}
