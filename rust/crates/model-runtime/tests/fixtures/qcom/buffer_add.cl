__kernel void buffer_add(__global float *out, __global const float *inp) { int i=get_global_id(0); out[i]=inp[i]+1.0f; }
