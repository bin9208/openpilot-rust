#include <stdint.h>

void op_kernel_0(void **buffers, const int32_t *scalars) {
  float *state = buffers[0];
  const float *input = buffers[1];
  int32_t i = scalars[0];
  state[i] = state[i] * 2.0f + input[i];
}

void op_kernel_1(void **buffers, const int32_t *scalars) {
  float *output = buffers[0];
  const float *state = buffers[1];
  output[0] = state[0] + state[1];
}
