// Agentic Rigg, 3D-kortet paa Lys. Port av web/js/lys3d/gl/renderer.js (WebGL 2),
// uendret bortsett fra at renderer.rs setter inn lys.glsl ved include-linjen.
// Bare ASCII, uten apostrofer: noen GLSL-kompilatorer avviser andre tegn.
uniform vec3 u_barA;
uniform vec3 u_barB;
uniform vec3 u_barCol;
uniform vec3 u_f1Pos;
uniform vec3 u_f1Col;
uniform vec3 u_f2Pos;
uniform vec3 u_f2Col;
uniform highp sampler2DShadow u_shadow;
uniform float u_shadowTexel;
in vec4 v_ls;

float shadowAt(vec4 ls, float soft) {
  vec3 p = ls.xyz / ls.w * 0.5 + 0.5;
  if (p.x < 0.0 || p.x > 1.0 || p.y < 0.0 || p.y > 1.0 || p.z > 1.0) return 1.0;
  float s = 0.0;
  float t = u_shadowTexel * soft;
  for (int i = -1; i <= 1; i++)
    for (int j = -1; j <= 1; j++)
      s += texture(u_shadow, vec3(p.xy + vec2(float(i), float(j)) * t, p.z - 0.0006));
  return s / 9.0;
}
vec3 pointLight(vec3 pos, vec3 col, float range, vec3 P, vec3 N, vec3 diff, vec3 F0, float shin, vec3 V) {
  vec3 d = pos - P;
  float dist = max(length(d), 1.0);
  vec3 L = d / dist;
  float att = 1.0 / (1.0 + dist * dist / (range * range));
  att *= 1.0 - smoothstep(range * 2.5, range * 4.0, dist);
  float ndl = max(dot(N, L), 0.0);
  vec3 H = normalize(L + V);
  float spec = (shin + 8.0) / 25.0 * pow(max(dot(N, H), 0.0), shin);
  return col * att * ndl * (diff + F0 * spec);
}
vec3 ledLights(vec3 P, vec3 N, vec3 diff, vec3 F0, float shin, vec3 V) {
  vec3 ab = u_barB - u_barA;
  float t = clamp(dot(P - u_barA, ab) / max(dot(ab, ab), 1e-3), 0.0, 1.0);
  vec3 c = pointLight(u_barA + ab * t, u_barCol, 26.0, P, N, diff, F0, shin, V);
  c += pointLight(u_f1Pos, u_f1Col, 40.0, P, N, diff, F0, shin, V);
  c += pointLight(u_f2Pos, u_f2Col, 40.0, P, N, diff, F0, shin, V);
  return c;
}
