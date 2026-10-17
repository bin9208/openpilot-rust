#version 300 es
precision highp float;

in vec2 fragTexCoord;
out vec4 finalColor;

uniform vec4 fillColor;

// Gradient line defined in *screen pixels*
uniform int useGradient;
uniform vec2 gradientStart;  // e.g. vec2(0, 0)
uniform vec2 gradientEnd;    // e.g. vec2(0, screenHeight)
uniform vec4 gradientColors[20];
uniform float gradientStops[20];
uniform int gradientColorCount;

vec4 getGradientColor(vec2 p) {
  // Compute t from screen-space position
  vec2 d = gradientStart - gradientEnd;
  float len2 = max(dot(d, d), 1e-6);
  float t = clamp(dot(p - gradientEnd, d) / len2, 0.0, 1.0);

  // Clamp to range
  float t0 = gradientStops[0];
  float tn = gradientStops[gradientColorCount-1];
  if (t <= t0) return gradientColors[0];
  if (t >= tn) return gradientColors[gradientColorCount-1];

  for (int i = 0; i < gradientColorCount - 1; i++) {
    float a = gradientStops[i];
    float b = gradientStops[i+1];
    if (t >= a && t <= b) {
      float k = (t - a) / max(b - a, 1e-6);
      return mix(gradientColors[i], gradientColors[i+1], k);
    }
  }

  return gradientColors[gradientColorCount-1];
}

void main() {
  // TODO: do proper antialiasing
  finalColor = useGradient == 1 ? getGradientColor(gl_FragCoord.xy) : fillColor;
}
