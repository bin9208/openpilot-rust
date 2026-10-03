// Test-only adapter. The three original sensor translation units are unchanged.
#include <array>
#include <iomanip>
#include <iostream>
#include <string>

#include "system/camerad/sensors/sensor.h"

template <class T> void values(const T &items) {
  std::cout << '[';
  bool first = true;
  for (const auto &item : items) {
    if (!first) std::cout << ',';
    first = false;
    std::cout << item;
  }
  std::cout << ']';
}

void registers(const std::vector<i2c_random_wr_payload> &items) {
  std::cout << '[';
  bool first = true;
  for (const auto &item : items) {
    if (!first) std::cout << ',';
    first = false;
    std::cout << '[' << item.reg_addr << ',' << item.reg_data << ']';
  }
  std::cout << ']';
}

void configuration(const SensorInfo &sensor) {
  std::cout << "{\"image_sensor\":" << sensor.num();
#define FIELD(name) std::cout << ",\"" #name "\":" << sensor.name
  FIELD(pixel_size_mm); FIELD(frame_width); FIELD(frame_height); FIELD(frame_stride);
  FIELD(frame_offset); FIELD(extra_height); FIELD(out_scale); FIELD(registers_offset);
  FIELD(stats_offset); FIELD(hdr_offset); FIELD(exposure_time_min); FIELD(exposure_time_max);
  FIELD(dc_gain_factor); FIELD(dc_gain_min_weight); FIELD(dc_gain_max_weight);
  FIELD(dc_gain_on_grey); FIELD(dc_gain_off_grey); FIELD(ev_scale);
  FIELD(analog_gain_min_idx); FIELD(analog_gain_max_idx); FIELD(analog_gain_rec_idx);
  FIELD(analog_gain_cost_delta); FIELD(analog_gain_cost_low); FIELD(analog_gain_cost_high);
  FIELD(target_grey_factor); FIELD(min_ev); FIELD(max_ev); FIELD(data_word);
  FIELD(probe_reg_addr); FIELD(probe_expected_data); FIELD(bits_per_pixel);
  FIELD(bayer_pattern); FIELD(mipi_format); FIELD(mclk_frequency); FIELD(frame_data_type);
  FIELD(readout_time_ns); FIELD(black_level);
#undef FIELD
  std::cout << ",\"sensor_analog_gains\":";
  values(std::vector<float>(sensor.sensor_analog_gains,
                           sensor.sensor_analog_gains + sensor.analog_gain_max_idx + 1));
  std::cout << ",\"start_reg_array\":"; registers(sensor.start_reg_array);
  std::cout << ",\"init_reg_array\":"; registers(sensor.init_reg_array);
#define ARRAY(name) std::cout << ",\"" #name "\":"; values(sensor.name)
  ARRAY(color_correct_matrix); ARRAY(gamma_lut_rgb); ARRAY(linearization_lut);
  ARRAY(linearization_pts); ARRAY(vignetting_lut);
#undef ARRAY
  std::cout << '}';
}

int main() {
  std::cout << std::setprecision(17) << std::boolalpha;
  const AR0231 ar0231;
  const OX03C10 ox03c10;
  const OS04C10 os04c10;
  const std::array<const SensorInfo *, 3> sensors = {&ar0231, &ox03c10, &os04c10};
  std::string operation;
  int kind;
  while (std::cin >> operation >> kind) {
    if (kind < 1 || kind > 3) return 2;
    const auto &sensor = sensors[kind - 1];
    if (operation == "config") {
      configuration(*sensor);
    } else if (operation == "exposure") {
      int time, gain, dc_gain;
      if (!(std::cin >> time >> gain >> dc_gain)) return 3;
      registers(sensor->getExposureRegisters(time, gain, dc_gain));
    } else if (operation == "score") {
      float desired, gain;
      int time, gain_index, previous;
      if (!(std::cin >> desired >> time >> gain_index >> gain >> previous)) return 3;
      std::cout << sensor->getExposureScore(desired, time, gain_index, gain, previous);
    } else if (operation == "address") {
      int port;
      if (!(std::cin >> port)) return 3;
      std::cout << sensor->getSlaveAddress(port);
    } else {
      return 4;
    }
    std::cout << '\n';
  }
}
