#version 300 es
// Agentic Rigg, 3D-kortet paa Lys. Port av web/js/lys3d/gl/renderer.js (WebGL 2),
// uendret bortsett fra at renderer.rs setter inn lys.glsl ved include-linjen.
// Bare ASCII, uten apostrofer: noen GLSL-kompilatorer avviser andre tegn.
precision mediump float;
in vec2 v_c;
in vec3 v_col;
uniform float u_gain;
out vec4 o;
void main() {
  float d2 = dot(v_c, v_c);
  float a = max(exp(-d2 * 3.2) - 0.04, 0.0);
  vec3 c = 1.0 - exp(-v_col * a * u_gain);
  o = vec4(c, 0.0);
}
