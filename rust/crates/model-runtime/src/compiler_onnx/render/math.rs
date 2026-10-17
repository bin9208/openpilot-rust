//! CPU:LLVM float32 decomposition from the pinned original xexp2/rintk/ldexp2k.
//! Keep target polynomial/FMA and scaling order; libm exp2 is not equivalent.
pub(super) const CPU_MATH: &str = r#"static inline float native_float_bits(unsigned int bits) {
  union { unsigned int u; float f; } value; value.u=bits; return value.f;
}
static inline float native_sign(float x) { return x!=0.0f ? (x<0.0f ? -1.0f : 1.0f) : 0.0f; }
static inline float native_exp2(float x) {
  if(x!=x) return native_float_bits(2143289344u);
  if(x>=128.0f) return native_float_bits(2139095040u);
  if(x< -150.0f) return 0.0f;
  int q=(int)(x+(x<0.0f ? -0.5f : 0.5f));
  float s=x-(float)q;
  float u=0.1535920892e-3f;
  u=__builtin_fmaf(u,s,0.1339262701e-2f);
  u=__builtin_fmaf(u,s,0.9618384764e-2f);
  u=__builtin_fmaf(u,s,0.5550347269e-1f);
  u=__builtin_fmaf(u,s,0.2402264476e+0f);
  u=__builtin_fmaf(u,s,0.6931471825e+0f);
  u=__builtin_fmaf(u,s,1.0f);
  int lo=q>=0 ? q/2 : -((-q+1)/2);
  float first=native_float_bits((unsigned int)(lo+127)<<23);
  float second=native_float_bits((unsigned int)(q-lo+127)<<23);
  return (u*first)*second;
}
"#;
