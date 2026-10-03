use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct Health {
    pub uptime: u32,
    pub voltage: u32,
    pub current: u32,
    pub safety_tx_blocked: u32,
    pub safety_rx_invalid: u32,
    pub tx_overflow: u32,
    pub rx_overflow: u32,
    pub faults: u32,
    pub ignition_line: u8,
    pub ignition_can: u8,
    pub controls_allowed: u8,
    pub harness_status: u8,
    pub safety_model: u8,
    pub safety_param: u16,
    pub fault_status: u8,
    pub power_save: u8,
    pub heartbeat_lost: u8,
    pub alternative_experience: u16,
    pub interrupt_load: f32,
    pub fan_power: u8,
    pub safety_rx_checks_invalid: u8,
    pub spi_checksum_errors: u16,
    pub fan_stall_count: u8,
    pub sbu1_mv: u16,
    pub sbu2_mv: u16,
    pub som_reset_triggered: u8,
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

impl Health {
    pub const PACKET_SIZE: usize = 58;

    pub fn from_packet(bytes: &[u8; Self::PACKET_SIZE]) -> Self {
        Self {
            uptime: u32_at(bytes, 0),
            voltage: u32_at(bytes, 4),
            current: u32_at(bytes, 8),
            safety_tx_blocked: u32_at(bytes, 12),
            safety_rx_invalid: u32_at(bytes, 16),
            tx_overflow: u32_at(bytes, 20),
            rx_overflow: u32_at(bytes, 24),
            faults: u32_at(bytes, 28),
            ignition_line: bytes[32],
            ignition_can: bytes[33],
            controls_allowed: bytes[34],
            harness_status: bytes[35],
            safety_model: bytes[36],
            safety_param: u16_at(bytes, 37),
            fault_status: bytes[39],
            power_save: bytes[40],
            heartbeat_lost: bytes[41],
            alternative_experience: u16_at(bytes, 42),
            interrupt_load: f32::from_bits(u32_at(bytes, 44)),
            fan_power: bytes[48],
            safety_rx_checks_invalid: bytes[49],
            spi_checksum_errors: u16_at(bytes, 50),
            fan_stall_count: bytes[52],
            sbu1_mv: u16_at(bytes, 53),
            sbu2_mv: u16_at(bytes, 55),
            som_reset_triggered: bytes[57],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct CanHealth {
    pub bus_off: u8,
    pub bus_off_count: u32,
    pub error_warning: u8,
    pub error_passive: u8,
    pub last_error: u8,
    pub last_stored_error: u8,
    pub last_data_error: u8,
    pub last_data_stored_error: u8,
    pub receive_error_count: u8,
    pub transmit_error_count: u8,
    pub total_errors: u32,
    pub total_tx_lost: u32,
    pub total_rx_lost: u32,
    pub total_tx: u32,
    pub total_rx: u32,
    pub total_forwarded: u32,
    pub total_tx_checksum_errors: u32,
    pub can_speed: u16,
    pub can_data_speed: u16,
    pub canfd_enabled: u8,
    pub brs_enabled: u8,
    pub canfd_non_iso: u8,
    pub irq0_rate: u32,
    pub irq1_rate: u32,
    pub irq2_rate: u32,
    pub core_reset_count: u32,
}

impl CanHealth {
    pub const PACKET_SIZE: usize = 64;

    pub fn from_packet(bytes: &[u8; Self::PACKET_SIZE]) -> Self {
        Self {
            bus_off: bytes[0],
            bus_off_count: u32_at(bytes, 1),
            error_warning: bytes[5],
            error_passive: bytes[6],
            last_error: bytes[7],
            last_stored_error: bytes[8],
            last_data_error: bytes[9],
            last_data_stored_error: bytes[10],
            receive_error_count: bytes[11],
            transmit_error_count: bytes[12],
            total_errors: u32_at(bytes, 13),
            total_tx_lost: u32_at(bytes, 17),
            total_rx_lost: u32_at(bytes, 21),
            total_tx: u32_at(bytes, 25),
            total_rx: u32_at(bytes, 29),
            total_forwarded: u32_at(bytes, 33),
            total_tx_checksum_errors: u32_at(bytes, 37),
            can_speed: u16_at(bytes, 41),
            can_data_speed: u16_at(bytes, 43),
            canfd_enabled: bytes[45],
            brs_enabled: bytes[46],
            canfd_non_iso: bytes[47],
            irq0_rate: u32_at(bytes, 48),
            irq1_rate: u32_at(bytes, 52),
            irq2_rate: u32_at(bytes, 56),
            core_reset_count: u32_at(bytes, 60),
        }
    }
}
