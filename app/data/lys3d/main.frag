#version 300 es
// Agentic Rigg, 3D-kortet paa Lys. Port av web/js/lys3d/gl/renderer.js (WebGL 2),
// uendret bortsett fra at renderer.rs setter inn lys.glsl ved include-linjen.
// Bare ASCII, uten apostrofer: noen GLSL-kompilatorer avviser andre tegn.
precision highp float;
in vec3 v_w;
in vec3 v_n;
in vec2 v_uv;
in vec3 v_l;
flat in int v_led;
uniform vec3 u_color;
uniform float u_rough;
uniform float u_metal;
uniform float u_alpha;
uniform sampler2D u_map;
uniform float u_useMap;
uniform float u_alphaTest;
uniform float u_double;
uniform vec4 u_holes[2];
uniform float u_highlight;
uniform vec3 u_accent;
uniform vec3 u_led[56];
uniform vec3 u_zoneGain;
uniform vec3 u_cam;
uniform vec3 u_keyDir;
uniform vec3 u_keyCol;
uniform vec3 u_fillDir;
uniform vec3 u_fillCol;
uniform vec3 u_rimDir;
uniform vec3 u_rimCol;
uniform vec3 u_sky;
uniform vec3 u_ground;
uniform float u_exposure;
uniform float u_ledDiffuse;
uniform float u_env;
#include lys.glsl
out vec4 o;

vec3 envLight(vec3 R, float r) {
  float y = R.y;
  vec3 c = mix(u_ground, u_sky, smoothstep(-0.3, 0.5, y));
  float w = 0.03 + r * 0.55;
  c += u_keyCol * 0.55 * smoothstep(1.0 - w, 1.0 - w * 0.2, max(dot(R, u_keyDir), 0.0));
  c += u_fillCol * 0.9 * smoothstep(1.0 - w * 1.6, 1.0 - w * 0.3, max(dot(R, u_fillDir), 0.0));
  c += u_rimCol * 0.6 * smoothstep(1.0 - w * 1.6, 1.0 - w * 0.3, max(dot(R, u_rimDir), 0.0));
  c += vec3(0.07) * exp(-pow((y - 0.08) / (0.05 + r * 0.4), 2.0));
  return c;
}
vec3 aces(vec3 x) {
  return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
}
vec3 dirLight(vec3 L, vec3 C, vec3 N, vec3 V, vec3 diff, vec3 F0, float shin) {
  float ndl = max(dot(N, L), 0.0);
  vec3 H = normalize(L + V);
  float spec = (shin + 8.0) / 25.0 * pow(max(dot(N, H), 0.0), shin);
  vec3 F = F0 + (1.0 - F0) * pow(1.0 - max(dot(H, V), 0.0), 5.0);
  return C * ndl * (diff + F * spec);
}
void main() {
  for (int i = 0; i < 2; i++) {
    if (u_holes[i].w > 0.5 && distance(v_l.xy, u_holes[i].xy) < u_holes[i].z) discard;
  }
  vec4 tex = u_useMap > 0.5 ? texture(u_map, v_uv) : vec4(1.0);
  if (tex.a < u_alphaTest) discard;
  vec3 albedo = u_color * pow(tex.rgb, vec3(2.2));
  vec3 N = normalize(v_n);
  if (u_double > 0.5 && !gl_FrontFacing) N = -N;
  vec3 V = normalize(u_cam - v_w);
  float rough = clamp(u_rough, 0.05, 1.0);
  float r4 = rough * rough * rough * rough;
  float shin = clamp(2.0 / (r4 + 1e-4) - 2.0, 1.0, 2048.0);
  vec3 F0 = mix(vec3(0.04), albedo, u_metal);
  vec3 diff = albedo * (1.0 - u_metal);
  float sh = shadowAt(v_ls, 1.25);
  vec3 col = diff * mix(u_ground, u_sky, N.y * 0.5 + 0.5) * 1.6;
  col += dirLight(u_keyDir, u_keyCol * sh, N, V, diff, F0, shin);
  col += dirLight(u_fillDir, u_fillCol, N, V, diff, F0, shin);
  col += dirLight(u_rimDir, u_rimCol, N, V, diff, F0, shin);
  float ndv = max(dot(N, V), 0.0);
  vec3 Fe = F0 + (max(vec3(1.0 - rough), F0) - F0) * pow(1.0 - ndv, 5.0);
  col += envLight(reflect(-V, N), rough) * Fe * mix(1.0, 0.6, rough) * mix(0.6, 1.0, sh) * u_env;
  col += ledLights(v_w, N, diff, F0, shin, V);
  if (v_led >= 0) {
    float g = v_led < 20 ? u_zoneGain.x : (v_led < 38 ? u_zoneGain.y : u_zoneGain.z);
    vec3 e = u_led[v_led] * g;
    // Diffusoren: fargen slaar igjennom, med en lysere kjerne naar den er sterk.
    col = col * (1.0 - u_ledDiffuse * 0.6) + e * 2.4 + vec3(dot(e, vec3(0.3))) * 0.35;
  }
  col += u_highlight * u_accent * (0.10 + 0.55 * pow(1.0 - ndv, 2.0));
  col = aces(col * u_exposure);
  o = vec4(pow(col, vec3(1.0 / 2.2)), u_alpha);
}
