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
// Original CPU has no SQRT intrinsic. Its pow(x,0.5) expands through xlog2
// and xexp2; using the host square-root instruction changes observable bits.
static inline float native_log2(float d) {
  if(d!=d || d<0.0f) return native_float_bits(2143289344u);
  if(d==0.0f) return native_float_bits(4286578688u);
  if(d==native_float_bits(2139095040u)) return d;
  int denormal=d<1.0e-4f;
  float a=denormal ? d*18446744073709551616.0f : d;
  union { unsigned int u; float f; } bits;
  bits.f=a*1.3333333730697632f;
  int exponent=(int)((bits.u>>23)&255u)-127;
  bits.f=a;
  bits.u-=((unsigned int)exponent)<<23;
  float m=bits.f;
  float e=(float)(exponent-(denormal ? 64 : 0));
  float x=(m-1.0f)/(m+1.0f), x2=x*x;
  float t=__builtin_fmaf(0.43745502829551697f,x2,0.5764790177345276f);
  t=__builtin_fmaf(t,x2,0.961801290512085f);
  float r=__builtin_fmaf(t,x*x2,e);
  r=__builtin_fmaf(x,2.885390043258667f,r);
  return __builtin_fmaf(x,3.2734472910078694e-08f,r);
}
// Original contracted CPU LLVM FMA selects the first multiplicand's NaN,
// then the second, then the addend. Register allocation for C fmaf may swap
// multiplicands; explicit operand selection preserves raw payload/quieting.
// Held IMAGE residual is added to the convolution accumulator in the
// original final kernel. Its first-operand NaN priority is observable.
static inline float native_image_residual_add(float convolution,float residual) {
  if(convolution!=convolution) return convolution+convolution;
  if(residual!=residual) return residual+residual;
  return convolution+residual;
}
static inline float native_contracted_fma(float a,float b,float c) {
  if(a!=a) return a+a;
  if(b!=b) return b+b;
  if(c!=c) return c+c;
  return __builtin_fmaf(a,b,c);
}
static inline float native_sqrt(float x) {
  if(x<0.0f) return native_float_bits(2143289344u);
  return native_exp2(0.5f*native_log2(x));
}

"#;
