#version 300 es
// Agentic Rigg, 3D-kortet paa Lys. Port av web/js/lys3d/gl/renderer.js (WebGL 2),
// uendret bortsett fra at renderer.rs setter inn lys.glsl ved include-linjen.
// Bare ASCII, uten apostrofer: noen GLSL-kompilatorer avviser andre tegn.
layout(location=0) in vec2 a_c;
uniform mat4 u_vp;
uniform mat4 u_lvp;
uniform vec3 u_center;
uniform float u_size;
out vec3 v_w;
out vec4 v_ls;
void main() {
  vec3 w = u_center + vec3(a_c.x * u_size, 0.0, a_c.y * u_size);
  v_w = w;
  v_ls = u_lvp * vec4(w, 1.0);
  gl_Position = u_vp * vec4(w, 1.0);
}
