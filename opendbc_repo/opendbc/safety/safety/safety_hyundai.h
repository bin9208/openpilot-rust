#pragma once

#include "safety_declarations.h"
#include "safety_hyundai_common.h"
#include "safety_pedal_interceptor.h"

#define HYUNDAI_LIMITS(steer, rate_up, rate_down) { \
  .max_steer = (steer), \
  .max_rate_up = (rate_up), \
  .max_rate_down = (rate_down), \
  .max_rt_delta = 112, \
  .max_rt_interval = 250000, \
  .driver_torque_allowance = 50, \
  .driver_torque_multiplier = 2, \
  .type = TorqueDriverLimited, \
   /* the EPS faults when the steering angle is above a certain threshold for too long. to prevent this, */ \
   /* we allow setting CF_Lkas_ActToi bit to 0 while maintaining the requested torque value for two consecutive frames */ \
  .min_valid_request_frames = 89, \
  .max_invalid_request_frames = 2, \
  .min_valid_request_rt_interval = 810000,  /* 810ms; a ~10% buffer on cutting every 90 frames */ \
  .has_steer_req_tolerance = true, \
}

extern const LongitudinalLimits HYUNDAI_LONG_LIMITS;
const LongitudinalLimits HYUNDAI_LONG_LIMITS = {
  .max_accel = 250,   // 1/100 m/s2
  .min_accel = -400,  // 1/100 m/s2
};

static const CanMsg HYUNDAI_TX_MSGS[] = {
  {0x340, 0, 8}, // LKAS11 Bus 0
  {0x4F1, 0, 4}, // CLU11 Bus 0
  {0x485, 0, 8}, // LFAHDA_MFC Bus 0
  {593, 2, 8},                              // MDPS12, Bus 2
  {1056, 0, 8},                             // SCC11, Bus 0
  {1057, 0, 8},                             // SCC12, Bus 0
  {1290, 0, 8},                             // SCC13, Bus 0
  {905, 0, 8},                              // SCC14, Bus 0
  {909, 0, 8},                              // FCA11 Bus 0
  {1155, 0, 8},                             // FCA12 Bus 0
  {1186, 0, 8},                             // FRT_RADAR11, Bus 0
  {1265, 2, 4},               // CLU11, Bus 0, 2
  {0x7D0, 0, 8}, // radar UDS TX addr Bus 0 (for radar disable)   // 2000
  {0x7b1, 0, 8},
};

#define HYUNDAI_COMMON_RX_CHECKS(legacy)                                                                                                                  \
  {.msg = {{0x260, 0, 8, .max_counter = 3U, .frequency = 100U},                                                                                           \
           {0x371, 0, 8, .ignore_checksum = true, .ignore_counter = true, .frequency = 100U},                                                             \
           {0x91,  0, 8, .ignore_checksum = true, .ignore_counter = true, .frequency = 100U}}},                                                           \
  {.msg = {{0x386, 0, 8, .ignore_checksum = (legacy), .ignore_counter = (legacy), .max_counter = (legacy) ? 0U : 15U, .frequency = 100U}, { 0 }, { 0 }}}, \
  {.msg = {{0x394, 0, 8, .ignore_checksum = (legacy), .ignore_counter = (legacy), .max_counter = (legacy) ? 0U : 7U, .frequency = 100U}, { 0 }, { 0 }}},  \

#define HYUNDAI_SCC12_ADDR_CHECK(scc_bus)                                               \
  {.msg = {{0x421, (scc_bus), 8, .max_counter = 15U, .frequency = 50U}, { 0 }, { 0 }}}, \

static bool hyundai_legacy = false;
static bool hyundai_cruise_buttons_alt = false;
static bool hyundai_pedal_interceptor = false;
static bool hyundai_pedal_interceptor_healthy = false;
static uint32_t hyundai_pedal_interceptor_last_rx = 0U;
static RxCheck *hyundai_pedal_interceptor_rx = NULL;
static RxCheck *hyundai_pedal_interceptor_cruise_rx = NULL;
static bool hyundai_pedal_interceptor_cruise_blocked = true;

// Ray EV only: x2 command increment from raw neutral, target cap 0.70.
// Input/driver thresholds and track-correlation tolerance are unchanged.
static const PedalInterceptorLimits HYUNDAI_RAY_PEDAL_INTERCEPTOR_LIMITS = {
  .min_track1 = 264U, .max_track1 = 795U, .min_track2 = 497U, .max_track2 = 1572U,
  .track2_ratio = 2, .track2_tolerance = 40,
  .driver_threshold1 = 272U, .driver_threshold2 = 513U,
};

// ClassicCgwBlinker (classic CAN only, opt-in, safetyParam 1024 = HyundaiSafetyFlags.CLASSIC_CGW_BLINKER): the
// BCM's own CGW1 (0x541) with the lever side and that side's lamp bit, the way joongyu01/carrot-wip-dev drove the
// Palisade (exterior lamps lit, 10-27) and the K5 DL3 (cluster only, 11-14). Without the flag nothing here runs:
// 0x541 is not in any TX list, the tx hook refuses it and the rx/tx hook never sends.
// Our frame is held and sent right behind the next stock CGW1 (10 Hz), never next to it: sending it directly at
// 20 Hz made 32 bus-0 errors in 0.5 s on the Palisade (00000e46).
#define HYUNDAI_CGW_FLAG 1024U
#define HYUNDAI_CGW_TEMPLATE_US 250000U  // the stock frame we copy must be this fresh
#define HYUNDAI_CGW_HOLD_US 150000U      // a held frame older than this is dropped, not sent
#define HYUNDAI_CGW_RATE_US 50000U       // at most one accepted frame per 50 ms (the host sends every 60 ms)
#define HYUNDAI_CGW_TRIAL_US 30000000U   // one continuous request at most 30 s ...
#define HYUNDAI_CGW_REARM_US 1000000U    // ... then 1 s without any request before the next one

static struct {
  bool enabled, template_valid, attempted, sent, trial_active, cutoff, pending;
  uint8_t template[8];
  uint32_t template_ts, attempt_ts, sent_ts, trial_ts, push_ts;
  CANPacket_t pkt;
} hyundai_cgw;

static void hyundai_cgw_init(uint16_t param) {
  hyundai_cgw = (__typeof__(hyundai_cgw)){0};  // no libc in the host libsafety build
  hyundai_cgw.enabled = GET_FLAG(param, HYUNDAI_CGW_FLAG);
}

// The frame may differ from the stock one (tmpl) only in the lever side (byte 5 bits 1..2: 2 left, 1 right) and that
// side's lamp bit (CF_Gway_TurnSigLh byte 2 bit 3, CF_Gway_TurnSigRh byte 7 bit 6), on or off. The stock frame must
// show no hazard switch (byte 4 bits 1..2) and no lever of the driver's own.
static bool hyundai_cgw_fits(const CANPacket_t *msg, const uint8_t *tmpl) {
  unsigned int left = (GET_BYTE(msg, 2) >> 3) & 3U;
  unsigned int right = GET_BYTE(msg, 7) >> 6;
  unsigned int lever = (GET_BYTE(msg, 5) >> 1) & 3U;
  bool fits = ((lever == 2U) && (left <= 1U) && (right == 0U)) || ((lever == 1U) && (left == 0U) && (right <= 1U));
  fits = fits && ((tmpl[4] & 0x06U) == 0U) && ((tmpl[5] & 0x06U) == 0U);
  for (int i = 0; i < 8; i++) {
    uint8_t mask = (i == 2) ? 0xE7U : ((i == 7) ? 0x3FU : ((i == 5) ? 0xF9U : 0xFFU));
    fits = fits && ((GET_BYTE(msg, i) & mask) == (tmpl[i] & mask));
  }
  return fits;
}

static void hyundai_cgw_rx(const CANPacket_t *to_push) {
  if (hyundai_cgw.enabled && (GET_BUS(to_push) == 0) && (GET_ADDR(to_push) == 0x541)) {
    bool valid = (GET_LEN(to_push) == 8) && !to_push->extended && !to_push->fd;
    hyundai_cgw.template_valid = valid;
    if (valid) {
      for (int i = 0; i < 8; i++) {
        hyundai_cgw.template[i] = to_push->data[i];
      }
      hyundai_cgw.template_ts = microsecond_timer_get();
    }
  }
}

static bool hyundai_cgw_tx(const CANPacket_t *msg) {
  bool allowed = false;
  if (hyundai_cgw.enabled) {
    uint32_t now = microsecond_timer_get();
    // A rejected request counts as activity too, so a sender cannot rearm itself by being refused.
    if (hyundai_cgw.attempted && (get_ts_elapsed(now, hyundai_cgw.attempt_ts) >= HYUNDAI_CGW_REARM_US)) {
      hyundai_cgw.trial_active = false;
      hyundai_cgw.cutoff = false;
    }
    hyundai_cgw.attempted = true;
    hyundai_cgw.attempt_ts = now;
    if (hyundai_cgw.trial_active && (get_ts_elapsed(now, hyundai_cgw.trial_ts) >= HYUNDAI_CGW_TRIAL_US)) {
      hyundai_cgw.cutoff = true;
    }
    allowed = !relay_malfunction && (GET_BUS(msg) == 0) && (GET_LEN(msg) == 8) && !msg->extended && !msg->fd &&
              hyundai_cgw.template_valid && !hyundai_cgw.cutoff &&
              (get_ts_elapsed(now, hyundai_cgw.template_ts) <= HYUNDAI_CGW_TEMPLATE_US) &&
              (!hyundai_cgw.sent || (get_ts_elapsed(now, hyundai_cgw.sent_ts) >= HYUNDAI_CGW_RATE_US)) &&
              hyundai_cgw_fits(msg, hyundai_cgw.template);
    if (allowed) {
      if (!hyundai_cgw.trial_active) {
        hyundai_cgw.trial_active = true;
        hyundai_cgw.trial_ts = now;
      }
      hyundai_cgw.sent = true;
      hyundai_cgw.sent_ts = now;
      hyundai_cgw.pkt = *msg;
      hyundai_cgw.pending = true;
      hyundai_cgw.push_ts = now;
      extern bool safety_tx_buffered_for_fwd;
      safety_tx_buffered_for_fwd = true;  // held here, not queued: hyundai_rx_tx_hook sends it
    }
  }
  return allowed;
}

static bool hyundai_rx_tx_hook(const CANPacket_t *to_push, CANPacket_t *to_send) {
  bool send = false;
  if (hyundai_cgw.enabled && hyundai_cgw.pending && (GET_BUS(to_push) == 0) && (GET_ADDR(to_push) == 0x541)) {
    hyundai_cgw.pending = false;
    // Checked again against the stock frame it rides behind: the driver's lever or hazard there now wins.
    if ((get_ts_elapsed(microsecond_timer_get(), hyundai_cgw.push_ts) < HYUNDAI_CGW_HOLD_US) &&
        (GET_LEN(to_push) == 8) && !to_push->extended && !to_push->fd &&
        hyundai_cgw_fits(&hyundai_cgw.pkt, to_push->data)) {
      *to_send = hyundai_cgw.pkt;
      send = true;
    }
  }
  return send;
}

// Only with the flag: the active TX list plus CGW1 on bus 0.
static safety_config hyundai_cgw_tx_list(safety_config ret) {
  safety_config out = ret;
  if (hyundai_cgw.enabled && (ret.tx_msgs != NULL)) {
    static CanMsg cgw_tx_msgs[24];
    if ((ret.tx_msgs_len + 1) <= (int)(sizeof(cgw_tx_msgs) / sizeof(cgw_tx_msgs[0]))) {
      for (int i = 0; i < ret.tx_msgs_len; i++) {
        cgw_tx_msgs[i] = ret.tx_msgs[i];
      }
      cgw_tx_msgs[ret.tx_msgs_len] = (CanMsg){0x541, 0, 8};
      out.tx_msgs = cgw_tx_msgs;
      out.tx_msgs_len = ret.tx_msgs_len + 1;
    }
  }
  return out;
}

static uint8_t hyundai_get_counter(const CANPacket_t *to_push) {
  int addr = GET_ADDR(to_push);

  uint8_t cnt = 0;
  if (addr == 0x260) {
    cnt = (GET_BYTE(to_push, 7) >> 4) & 0x3U;
  } else if (addr == 0x386) {
    cnt = ((GET_BYTE(to_push, 3) >> 6) << 2) | (GET_BYTE(to_push, 1) >> 6);
  } else if (addr == 0x394) {
    cnt = (GET_BYTE(to_push, 1) >> 5) & 0x7U;
  } else if (addr == 0x421) {
    cnt = GET_BYTE(to_push, 7) & 0xFU;
  } else if (addr == 0x4F1) {
    cnt = (GET_BYTE(to_push, 3) >> 4) & 0xFU;
  } else if (hyundai_pedal_interceptor && (addr == 0x201)) {
    cnt = GET_BYTE(to_push, 4) & 0xFU;
  } else {
  }
  return cnt;
}

static uint32_t hyundai_get_checksum(const CANPacket_t *to_push) {
  int addr = GET_ADDR(to_push);

  uint8_t chksum = 0;
  if (addr == 0x260) {
    chksum = GET_BYTE(to_push, 7) & 0xFU;
  } else if (addr == 0x386) {
    chksum = ((GET_BYTE(to_push, 7) >> 6) << 2) | (GET_BYTE(to_push, 5) >> 6);
  } else if (addr == 0x394) {
    chksum = GET_BYTE(to_push, 6) & 0xFU;
  } else if (addr == 0x421) {
    chksum = GET_BYTE(to_push, 7) >> 4;
  } else if (hyundai_pedal_interceptor && (addr == 0x201)) {
    chksum = GET_BYTE(to_push, 5);
  } else {
  }
  return chksum;
}

static uint32_t hyundai_compute_checksum(const CANPacket_t *to_push) {
  if (hyundai_pedal_interceptor && (GET_ADDR(to_push) == 0x201)) {
    return pedal_interceptor_checksum(to_push);
  }
  int addr = GET_ADDR(to_push);

  uint8_t chksum = 0;
  if (addr == 0x386) {
    // count the bits
    for (int i = 0; i < 8; i++) {
      uint8_t b = GET_BYTE(to_push, i);
      for (int j = 0; j < 8; j++) {
        uint8_t bit = 0;
        // exclude checksum and counter
        if (((i != 1) || (j < 6)) && ((i != 3) || (j < 6)) && ((i != 5) || (j < 6)) && ((i != 7) || (j < 6))) {
          bit = (b >> (uint8_t)j) & 1U;
        }
        chksum += bit;
      }
    }
    chksum = (chksum ^ 9U) & 15U;
  } else {
    // sum of nibbles
    for (int i = 0; i < 8; i++) {
      if ((addr == 0x394) && (i == 7)) {
        continue; // exclude
      }
      uint8_t b = GET_BYTE(to_push, i);
      if (((addr == 0x260) && (i == 7)) || ((addr == 0x394) && (i == 6)) || ((addr == 0x421) && (i == 7))) {
        b &= (addr == 0x421) ? 0x0FU : 0xF0U; // remove checksum
      }
      chksum += (b % 16U) + (b / 16U);
    }
    chksum = (16U - (chksum %  16U)) % 16U;
  }

  return chksum;
}

static void hyundai_rx_hook(const CANPacket_t *to_push) {
  int bus = GET_BUS(to_push);
  int addr = GET_ADDR(to_push);

  hyundai_cgw_rx(to_push);

  // SCC12 is on bus 2 for camera-based SCC cars, bus 0 on all others
  if (addr == 0x421) {
    if (((bus == 0) && !hyundai_camera_scc) || ((bus == 2) && hyundai_camera_scc)) {
      // 2 bits: 13-14
      int cruise_engaged = (GET_BYTES(to_push, 0, 4) >> 13) & 0x3U;
      hyundai_common_cruise_state_check(cruise_engaged);
    }
  }

  if (bus == 0) {
    if (hyundai_pedal_interceptor && (addr == 0x592) && (GET_LEN(to_push) == 8)) {
      hyundai_pedal_interceptor_cruise_blocked = GET_BIT(to_push, 34U) || GET_BIT(to_push, 35U);
    }
    if (addr == 0x251) {
      int torque_driver_new = (GET_BYTES(to_push, 0, 2) & 0x7ffU) - 1024U;
      // update array of samples
      update_sample(&torque_driver, torque_driver_new);
    }

    // ACC steering wheel buttons
    if (addr == 1007) hyundai_cruise_buttons_alt = true; // CASPER_EV: 1007
    if (addr == 1007) {      
      int cruise_button = (GET_BYTE(to_push, 7) >> 4) & 0x07U;
      bool main_button = GET_BIT(to_push, 58U);
      hyundai_common_cruise_buttons_check(cruise_button, main_button);
    }
    else if (addr == 0x4F1 && !hyundai_cruise_buttons_alt) {
      int cruise_button = GET_BYTE(to_push, 0) & 0x7U;
      bool main_button = GET_BIT(to_push, 3U);
      hyundai_common_cruise_buttons_check(cruise_button, main_button);
    }

    // gas press, different for EV, hybrid, and ICE models
    if (hyundai_pedal_interceptor) {
      if ((addr == 0x201) && (GET_LEN(to_push) == 6)) {
        const PedalInterceptorLimits limits = HYUNDAI_RAY_PEDAL_INTERCEPTOR_LIMITS;
        gas_pressed = (pedal_interceptor_track(to_push, 0) > limits.driver_threshold1) ||
                      (pedal_interceptor_track(to_push, 2) > limits.driver_threshold2);
        hyundai_pedal_interceptor_healthy = (GET_BYTE(to_push, 4) >> 4) == 0U;
        hyundai_pedal_interceptor_last_rx = microsecond_timer_get();
      }
    } else if ((addr == 0x371) && hyundai_ev_gas_signal) {
      gas_pressed = (((GET_BYTE(to_push, 4) & 0x7FU) << 1) | GET_BYTE(to_push, 3) >> 7) != 0U;
    } else if ((addr == 0x371) && hyundai_hybrid_gas_signal) {
      gas_pressed = GET_BYTE(to_push, 7) != 0U;
    } else if ((addr == 0x91) && hyundai_fcev_gas_signal) {
      gas_pressed = GET_BYTE(to_push, 6) != 0U;
    } else if ((addr == 0x260) && !hyundai_ev_gas_signal && !hyundai_hybrid_gas_signal) {
      gas_pressed = (GET_BYTE(to_push, 7) >> 6) != 0U;
    } else {
    }

    // sample wheel speed, averaging opposite corners
    if (addr == 0x386) {
      uint32_t front_left_speed = GET_BYTES(to_push, 0, 2) & 0x3FFFU;
      uint32_t rear_right_speed = GET_BYTES(to_push, 6, 2) & 0x3FFFU;
      vehicle_moving = (front_left_speed > HYUNDAI_STANDSTILL_THRSLD) || (rear_right_speed > HYUNDAI_STANDSTILL_THRSLD);
    }

    if (addr == 0x394) {
      brake_pressed = ((GET_BYTE(to_push, 5) >> 5U) & 0x3U) == 0x2U;
    }

    bool stock_ecu_detected = (addr == 0x340);

    // If openpilot is controlling longitudinal we need to ensure the radar is turned off
    // Enforce by checking we don't see SCC12
    if (hyundai_longitudinal && (addr == 0x421)) {
      stock_ecu_detected = true;
    }
    generic_rx_checks(stock_ecu_detected);
  }
}

uint32_t last_ts_lkas11_from_op = 0;
uint32_t last_ts_scc12_from_op = 0;
uint32_t last_ts_scc13_from_op = 0;
uint32_t last_ts_mdps12_from_op = 0;
uint32_t last_ts_fca11_from_op = 0;
uint32_t last_ts_fca12_from_op = 0;
uint32_t last_ts_lfahda_mfc_from_op = 0;

static bool hyundai_tx_hook(const CANPacket_t *to_send) {
  const TorqueSteeringLimits HYUNDAI_STEERING_LIMITS = HYUNDAI_LIMITS(512, 10, 10);
  const TorqueSteeringLimits HYUNDAI_STEERING_LIMITS_ALT = HYUNDAI_LIMITS(512, 10, 10);
  const TorqueSteeringLimits HYUNDAI_STEERING_LIMITS_ALT_2 = HYUNDAI_LIMITS(170, 2, 3);

  bool tx = true;
  int addr = GET_ADDR(to_send);

  // ClassicCgwBlinker: refused unless the flag is set (and 0x541 is in no TX list without it)
  if (addr == 0x541) {
    return hyundai_cgw_tx(to_send);
  }

  if (hyundai_pedal_interceptor) {
    // Reject before the existing SCC hook can change controls_allowed, even
    // when the outer TX allowlist would also reject the packet.
    if ((addr != 0x340) && (addr != 0x4F1) && (addr != 0x485) && (addr != 0x200)) {
      return false;
    }
    if (addr == 0x200) {
      if ((GET_BUS(to_send) != 0) || (GET_LEN(to_send) != 6)) {
        return false;
      }
      const bool enabled = GET_BIT(to_send, 39U);
      const bool sensor_valid = hyundai_pedal_interceptor_healthy && (hyundai_pedal_interceptor_rx != NULL) &&
        hyundai_pedal_interceptor_rx->status.msg_seen && hyundai_pedal_interceptor_rx->status.valid_checksum &&
        (hyundai_pedal_interceptor_rx->status.wrong_counters < MAX_WRONG_COUNTERS) &&
        (get_ts_elapsed(microsecond_timer_get(), hyundai_pedal_interceptor_last_rx) <= 200000U);
      // LABEL11 is 10 Hz; retain Carrot's existing ten-missed-message/1 s policy.
      const bool cruise_valid = !hyundai_pedal_interceptor_cruise_blocked && (hyundai_pedal_interceptor_cruise_rx != NULL) &&
        hyundai_pedal_interceptor_cruise_rx->status.msg_seen &&
        (get_ts_elapsed(microsecond_timer_get(), hyundai_pedal_interceptor_cruise_rx->status.last_timestamp) <= 1000000U);
      return !pedal_interceptor_command_checks(to_send, HYUNDAI_RAY_PEDAL_INTERCEPTOR_LIMITS) &&
             (!enabled || (sensor_valid && cruise_valid && get_longitudinal_allowed() && !brake_pressed_prev));
    }
  }

  // FCA11: Block any potential actuation
  if (false && addr == 0x38D) {
    int CR_VSM_DecCmd = GET_BYTE(to_send, 1);
    bool FCA_CmdAct = GET_BIT(to_send, 20U);
    bool CF_VSM_DecCmdAct = GET_BIT(to_send, 31U);

    if ((CR_VSM_DecCmd != 0) || FCA_CmdAct || CF_VSM_DecCmdAct) {
      tx = false;
    }
  }

  // ACCEL: safety check
  if (addr == 0x421) {
    int cruise_engaged = (GET_BYTES(to_send, 0, 4) >> 13) & 0x3U;
    if (cruise_engaged) {
      if(!controls_allowed) print("auto engage controls_allowed....\n");
      controls_allowed = true;
    }
    int desired_accel_raw = (((GET_BYTE(to_send, 4) & 0x7U) << 8) | GET_BYTE(to_send, 3)) - 1023U;
    int desired_accel_val = ((GET_BYTE(to_send, 5) << 3) | (GET_BYTE(to_send, 4) >> 5)) - 1023U;

    //int aeb_decel_cmd = GET_BYTE(to_send, 2);
    //bool aeb_req = GET_BIT(to_send, 54U);

    bool violation = false;

    violation |= longitudinal_accel_checks(desired_accel_raw, HYUNDAI_LONG_LIMITS);
    violation |= longitudinal_accel_checks(desired_accel_val, HYUNDAI_LONG_LIMITS);
    //violation |= (aeb_decel_cmd != 0);
    //violation |= aeb_req;

    if (violation) {
      tx = false;
    }
  }

  // LKA STEER: safety check
  if (addr == 0x340) {
    int desired_torque = ((GET_BYTES(to_send, 0, 4) >> 16) & 0x7ffU) - 1024U;
    bool steer_req = GET_BIT(to_send, 27U);

    const TorqueSteeringLimits limits = hyundai_alt_limits_2 ? HYUNDAI_STEERING_LIMITS_ALT_2 :
                                        hyundai_alt_limits ? HYUNDAI_STEERING_LIMITS_ALT : HYUNDAI_STEERING_LIMITS;

    if (steer_torque_cmd_checks(desired_torque, steer_req, limits)) {
      //tx = false;
    }
  }

  // UDS: Only tester present ("\x02\x3E\x80\x00\x00\x00\x00\x00") allowed on diagnostics address
  if (addr == 0x7D0) {
    if ((GET_BYTES(to_send, 0, 4) != 0x00803E02U) || (GET_BYTES(to_send, 4, 4) != 0x0U)) {
      tx = false;
    }
  }

  // BUTTONS: used for resume spamming and cruise cancellation
  if ((addr == 0x4F1) && !hyundai_longitudinal) {
    int button = GET_BYTE(to_send, 0) & 0x7U;

    bool allowed_resume = (button == 1);// && controls_allowed;
    bool allowed_set_decel = (button == 2) && controls_allowed;
    bool allowed_cancel = (button == 4) && cruise_engaged_prev;
    bool allowed_gap_dist = (button == 3) && controls_allowed;
    if (!(allowed_resume || allowed_set_decel || allowed_cancel || allowed_gap_dist)) {
      tx = false;
    }
  }
  uint32_t now = microsecond_timer_get();
  if(addr == 832)
    last_ts_lkas11_from_op = (tx == 0 ? 0 : now);
  else if(addr == 1057)
    last_ts_scc12_from_op = (tx == 0 ? 0 : now);
  else if(addr == 593)
    last_ts_mdps12_from_op = (tx == 0 ? 0 : now);
  else if (addr == 909)
    last_ts_fca11_from_op = (tx == 0 ? 0 : now);
  else if (addr == 1155)
    last_ts_fca12_from_op = (tx == 0 ? 0 : now);
  else if(addr == 1290)
    last_ts_scc13_from_op = (tx == 0 ? 0 : now);
  else if(addr == 0x485)
    last_ts_lfahda_mfc_from_op = (tx == 0 ? 0 : now);

  return tx;
}

static int hyundai_fwd_hook(CANPacket_t* to_send) {
  const int bus_num = GET_BUS(to_send);
  const int addr = GET_ADDR(to_send);


  int bus_fwd = -1;

  uint32_t now = microsecond_timer_get();

  // forward cam to ccan and viceversa, except lkas cmd
  if (bus_num == 0) {
    bus_fwd = 2;

    if(addr == 593) {
      if(now - last_ts_mdps12_from_op < 200000) {
        bus_fwd = -1;
      }
    }
  }

  if (bus_num == 2) {
    bool is_lkas_msg = addr == 832;
    bool is_lfahda_msg = addr == 1157;
    bool is_scc_msg = addr == 1056 || addr == 1057 || addr == 905;
    bool is_scc13_msg = addr == 1290;
    bool is_fca11_msg = addr == 909;
    bool is_fca12_msg = addr == 1155;

    bool block_msg = is_lkas_msg || is_lfahda_msg || is_scc_msg || is_scc13_msg || is_fca11_msg || is_fca12_msg;
    if (!block_msg) {
      bus_fwd = 0;
    }
    else {
      if(is_lkas_msg) {
        if(now - last_ts_lkas11_from_op >= 200000) {
          bus_fwd = 0;
        }
      }
      else if(is_lfahda_msg) {
        if (now - last_ts_lfahda_mfc_from_op >= 200000)
          bus_fwd = 0;
      }
      else if (is_scc_msg) {
        if (now - last_ts_scc12_from_op >= 400000)
          bus_fwd = 0;
      }
      else if (is_scc13_msg) {
        if (now - last_ts_scc13_from_op >= 800000)
          bus_fwd = 0;
      }
      else if (is_fca11_msg) {
        if (now - last_ts_fca11_from_op >= 400000)
          bus_fwd = 0;
      }
      else if (is_fca12_msg) {
        if (now - last_ts_fca12_from_op >= 400000)
          bus_fwd = 0;
      }
    }
  }

  return bus_fwd;
}

/* case
  - legacy(on/off) + camera_scc(allways longitudinal on) + longitudinal(scc off)
*/
static safety_config hyundai_init_carrot(bool legacy_car) {
    static const CanMsg HYUNDAI_LONG_TX_MSGS[] = {
      {0x340, 0, 8}, // LKAS11 Bus 0
      {0x4F1, 0, 4}, // CLU11 Bus 0
      {0x485, 0, 8}, // LFAHDA_MFC Bus 0
      {0x420, 0, 8}, // SCC11 Bus 0
      {0x421, 0, 8}, // SCC12 Bus 0
      {0x50A, 0, 8}, // SCC13 Bus 0
      {0x389, 0, 8}, // SCC14 Bus 0
      {0x4A2, 0, 2}, // FRT_RADAR11 Bus 0
      {0x38D, 0, 8}, // FCA11 Bus 0
      {0x483, 0, 8}, // FCA12 Bus 0
      {0x7D0, 0, 8}, // radar UDS TX addr Bus 0 (for radar disable)
    };

    static const CanMsg HYUNDAI_CAMERA_SCC_TX_MSGS[] = {
      {0x340, 0, 8}, // LKAS11 Bus 0
      {0x4F1, 2, 4}, // CLU11 Bus 2
      {0x485, 0, 8}, // LFAHDA_MFC Bus 0
      {593, 2, 8},                              // MDPS12, Bus 2
      {1056, 0, 8},                             // SCC11, Bus 0
      {1057, 0, 8},                             // SCC12, Bus 0
      {1290, 0, 8},                             // SCC13, Bus 0
      {905, 0, 8},                              // SCC14, Bus 0
      {909, 0, 8},                              // FCA11 Bus 0
      {1155, 0, 8},                             // FCA12 Bus 0
      {1186, 0, 8},                             // FRT_RADAR11, Bus 0
      {0x4F1, 0, 4}, // CLU11 Bus 0
    };

    safety_config ret;
    if (hyundai_camera_scc) {
        static RxCheck hyundai_cam_scc_rx_checks[] = {
          HYUNDAI_COMMON_RX_CHECKS(false)
          HYUNDAI_SCC12_ADDR_CHECK(2)
        };
        static RxCheck hyundai_cam_scc_rx_checks_legacy[] = {
          HYUNDAI_COMMON_RX_CHECKS(true)
          HYUNDAI_SCC12_ADDR_CHECK(2)
        };
        if(legacy_car) ret = BUILD_SAFETY_CFG(hyundai_cam_scc_rx_checks_legacy, HYUNDAI_CAMERA_SCC_TX_MSGS);
        else ret = BUILD_SAFETY_CFG(hyundai_cam_scc_rx_checks, HYUNDAI_CAMERA_SCC_TX_MSGS);
    }
    else if (hyundai_longitudinal) {
        static RxCheck hyundai_long_rx_checks[] = {
          HYUNDAI_COMMON_RX_CHECKS(false)
          // Use CLU11 (buttons) to manage controls allowed instead of SCC cruise state
          {.msg = {{0x4F1, 0, 4, .ignore_checksum = true, .max_counter = 15U, .frequency = 50U}, { 0 }, { 0 }}
},
        };
        static RxCheck hyundai_long_rx_checks_legacy[] = {
          HYUNDAI_COMMON_RX_CHECKS(true)
          // Use CLU11 (buttons) to manage controls allowed instead of SCC cruise state
          {.msg = {{0x4F1, 0, 4, .ignore_checksum = true, .max_counter = 15U, .frequency = 50U}, { 0 }, { 0 }}
},
        };

        if(legacy_car) ret = BUILD_SAFETY_CFG(hyundai_long_rx_checks_legacy, HYUNDAI_LONG_TX_MSGS);
        else ret = BUILD_SAFETY_CFG(hyundai_long_rx_checks, HYUNDAI_LONG_TX_MSGS);
    }
    else {
        static RxCheck hyundai_rx_checks[] = {
           HYUNDAI_COMMON_RX_CHECKS(false)
           HYUNDAI_SCC12_ADDR_CHECK(0)
        };
        static RxCheck hyundai_rx_checks_legacy[] = {
           HYUNDAI_COMMON_RX_CHECKS(true)
           //HYUNDAI_SCC12_ADDR_CHECK(0)
        };

        if(legacy_car) ret = BUILD_SAFETY_CFG(hyundai_rx_checks_legacy, HYUNDAI_TX_MSGS);
        else ret = BUILD_SAFETY_CFG(hyundai_rx_checks, HYUNDAI_TX_MSGS);
    }
    return ret;
}


static safety_config hyundai_init(uint16_t param) {
  static const CanMsg HYUNDAI_LONG_TX_MSGS[] = {
    {0x340, 0, 8}, // LKAS11 Bus 0
    {0x4F1, 0, 4}, // CLU11 Bus 0
    {0x485, 0, 8}, // LFAHDA_MFC Bus 0
    {0x420, 0, 8}, // SCC11 Bus 0
    {0x421, 0, 8}, // SCC12 Bus 0
    {0x50A, 0, 8}, // SCC13 Bus 0
    {0x389, 0, 8}, // SCC14 Bus 0
    {0x4A2, 0, 2}, // FRT_RADAR11 Bus 0
    {0x38D, 0, 8}, // FCA11 Bus 0
    {0x483, 0, 8}, // FCA12 Bus 0
    {0x7D0, 0, 8}, // radar UDS TX addr Bus 0 (for radar disable)
  };

  static const CanMsg HYUNDAI_CAMERA_SCC_TX_MSGS[] = {
    {0x340, 0, 8}, // LKAS11 Bus 0
    {0x4F1, 2, 4}, // CLU11 Bus 2
    {0x485, 0, 8}, // LFAHDA_MFC Bus 0
  };

  hyundai_common_init(param);
  hyundai_cgw_init(param);
  hyundai_legacy = false;
  hyundai_pedal_interceptor = false;
  hyundai_pedal_interceptor_healthy = false;
  hyundai_pedal_interceptor_rx = NULL;
  hyundai_pedal_interceptor_cruise_rx = NULL;
  hyundai_pedal_interceptor_cruise_blocked = true;
  hyundai_pedal_interceptor_last_rx = 0U;
  if (GET_FLAG(param, 8192U)) {  // HyundaiSafetyFlags.PEDAL_INTERCEPTOR
    // Respect Carrot's existing LONG/ALLOW_DEBUG policy. The Ray profile is
    // explicit and cannot be selected by ordinary EV/LONG or CAN-FD flags.
    if (!hyundai_longitudinal || (param != (8192U | 16384U | 4U | 1U))) {
      return (safety_config){0};
    }
    static RxCheck hyundai_pedal_interceptor_rx_checks[] = {
      HYUNDAI_COMMON_RX_CHECKS(false)
      {.msg = {{0x4F1, 0, 4, .ignore_checksum = true, .max_counter = 15U, .frequency = 50U}, { 0 }, { 0 }}},
      {.msg = {{0x592, 0, 8, .ignore_checksum = true, .ignore_counter = true, .frequency = 10U}, { 0 }, { 0 }}},
      {.msg = {{0x201, 0, 6, .max_counter = 15U, .frequency = 50U}, { 0 }, { 0 }}},
    };
    static const CanMsg HYUNDAI_PEDAL_INTERCEPTOR_TX_MSGS[] = {
      {0x340, 0, 8}, {0x4F1, 0, 4}, {0x485, 0, 8}, {0x200, 0, 6},
    };
    hyundai_pedal_interceptor = true;
    hyundai_pedal_interceptor_rx = &hyundai_pedal_interceptor_rx_checks[
      sizeof(hyundai_pedal_interceptor_rx_checks) / sizeof(hyundai_pedal_interceptor_rx_checks[0]) - 1U];
    hyundai_pedal_interceptor_cruise_rx = hyundai_pedal_interceptor_rx - 1;
    return BUILD_SAFETY_CFG(hyundai_pedal_interceptor_rx_checks, HYUNDAI_PEDAL_INTERCEPTOR_TX_MSGS);
  }
  return hyundai_cgw_tx_list(hyundai_init_carrot(hyundai_legacy));

  safety_config ret;
  if (hyundai_longitudinal) {
    static RxCheck hyundai_long_rx_checks[] = {
      HYUNDAI_COMMON_RX_CHECKS(false)
      // Use CLU11 (buttons) to manage controls allowed instead of SCC cruise state
      {.msg = {{0x4F1, 0, 4, .ignore_checksum = true, .max_counter = 15U, .frequency = 50U}, { 0 }, { 0 }}},
    };

    ret = BUILD_SAFETY_CFG(hyundai_long_rx_checks, HYUNDAI_LONG_TX_MSGS);
  } else if (hyundai_camera_scc) {
    static RxCheck hyundai_cam_scc_rx_checks[] = {
      HYUNDAI_COMMON_RX_CHECKS(false)
      HYUNDAI_SCC12_ADDR_CHECK(2)
    };

    ret = BUILD_SAFETY_CFG(hyundai_cam_scc_rx_checks, HYUNDAI_CAMERA_SCC_TX_MSGS);
  } else {
    static RxCheck hyundai_rx_checks[] = {
       HYUNDAI_COMMON_RX_CHECKS(false)
       HYUNDAI_SCC12_ADDR_CHECK(0)
    };

    ret = BUILD_SAFETY_CFG(hyundai_rx_checks, HYUNDAI_TX_MSGS);
  }
  return ret;
}

static safety_config hyundai_legacy_init(uint16_t param) {
  // older hyundai models have less checks due to missing counters and checksums
  static RxCheck hyundai_legacy_rx_checks[] = {
    HYUNDAI_COMMON_RX_CHECKS(true)
    //HYUNDAI_SCC12_ADDR_CHECK(0)
  };

  hyundai_common_init(param);
  hyundai_cgw_init(param);
  hyundai_legacy = true;
  hyundai_pedal_interceptor = false;
  hyundai_pedal_interceptor_rx = NULL;

  return hyundai_cgw_tx_list(hyundai_init_carrot(hyundai_legacy));

  hyundai_longitudinal = false;
  hyundai_camera_scc = false;
  return BUILD_SAFETY_CFG(hyundai_legacy_rx_checks, HYUNDAI_TX_MSGS);
}

const safety_hooks hyundai_hooks = {
  .init = hyundai_init,
  .rx = hyundai_rx_hook,
  .tx = hyundai_tx_hook,
  .fwd = hyundai_fwd_hook,
  .rx_tx = hyundai_rx_tx_hook,
  .get_counter = hyundai_get_counter,
  .get_checksum = hyundai_get_checksum,
  .compute_checksum = hyundai_compute_checksum,
};

const safety_hooks hyundai_legacy_hooks = {
  .init = hyundai_legacy_init,
  .rx = hyundai_rx_hook,
  .tx = hyundai_tx_hook,
  .fwd = hyundai_fwd_hook,
  .rx_tx = hyundai_rx_tx_hook,
  .get_counter = hyundai_get_counter,
  .get_checksum = hyundai_get_checksum,
  .compute_checksum = hyundai_compute_checksum,
};
