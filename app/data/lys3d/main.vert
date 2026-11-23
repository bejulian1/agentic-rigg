#version 300 es
// Agentic HW Monitor, 3D-kortet paa Lys. Port av web/js/lys3d/gl/renderer.js (WebGL 2),
// uendret bortsett fra at renderer.rs setter inn lys.glsl ved include-linjen.
// Bare ASCII, uten apostrofer: noen GLSL-kompilatorer avviser andre tegn.
layout(location=0) in vec3 a_pos;
layout(location=1) in vec3 a_nrm;
layout(location=2) in vec2 a_uv;
layout(location=3) in float a_led;
uniform mat4 u_model;
uniform mat3 u_nmat;
uniform mat4 u_vp;
uniform mat4 u_lvp;
out vec3 v_w;
out vec3 v_n;
out vec2 v_uv;
out vec3 v_l;
out vec4 v_ls;
flat out int v_led;
void main() {
  vec4 w = u_model * vec4(a_pos, 1.0);
  v_w = w.xyz;
  v_n = u_nmat * a_nrm;
  v_uv = a_uv;
  v_l = a_pos;
  v_ls = u_lvp * w;
  v_led = int(floor(a_led + 0.5));
  gl_Position = u_vp * w;
}
