#version 300 es
// Agentic HW Monitor, 3D-kortet paa Lys. Port av web/js/lys3d/gl/renderer.js (WebGL 2),
// uendret bortsett fra at renderer.rs setter inn lys.glsl ved include-linjen.
// Bare ASCII, uten apostrofer: noen GLSL-kompilatorer avviser andre tegn.
precision mediump float;
in vec2 v_uv;
uniform vec2 u_c;
uniform vec2 u_r;
uniform vec3 u_col;
uniform float u_alpha;
out vec4 o;
void main() {
  vec2 d = (v_uv - u_c) / u_r;
  float a = exp(-dot(d, d) * 2.2) * u_alpha;
  o = vec4(u_col * a, 0.0);
}
