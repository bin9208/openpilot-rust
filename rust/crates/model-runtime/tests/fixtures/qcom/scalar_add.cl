__kernel void scalar_add(__global float *out, __global const float *inp, int offset) { int i=get_global_id(0); out[i]=inp[i]+offset; }
