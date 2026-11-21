#version 300 es
// Agentic Rigg, 3D-kortet paa Lys. Port av web/js/lys3d/gl/renderer.js (WebGL 2),
// uendret bortsett fra at renderer.rs setter inn lys.glsl ved include-linjen.
// Bare ASCII, uten apostrofer: noen GLSL-kompilatorer avviser andre tegn.
layout(location=0) in vec3 a_pos;
layout(location=2) in vec2 a_uv;
uniform mat4 u_model;
uniform mat4 u_vp;
out vec3 v_l;
out vec2 v_uv;
void main() {
  v_l = a_pos;
  v_uv = a_uv;
  gl_Position = u_vp * u_model * vec4(a_pos, 1.0);
}
