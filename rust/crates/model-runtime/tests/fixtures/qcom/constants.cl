__constant float coeffs[4]={1.25f,2.5f,3.75f,4.0f}; __kernel void constants(__global float *out, __global const float *inp) { int i=get_global_id(0); out[i]=inp[i]*coeffs[i&3]+0.75f; }
