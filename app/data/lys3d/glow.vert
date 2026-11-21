#version 300 es
// Agentic Rigg, 3D-kortet paa Lys. Port av web/js/lys3d/gl/renderer.js (WebGL 2),
// uendret bortsett fra at renderer.rs setter inn lys.glsl ved include-linjen.
// Bare ASCII, uten apostrofer: noen GLSL-kompilatorer avviser andre tegn.
layout(location=0) in vec2 a_c;
layout(location=1) in vec3 a_p;
layout(location=2) in vec3 a_col;
uniform mat4 u_view;
uniform mat4 u_proj;
uniform float u_size;
out vec2 v_c;
out vec3 v_col;
void main() {
  vec4 vp = u_view * vec4(a_p, 1.0);
  vp.xy += a_c * u_size;
  vp.z += 3.0;
  v_c = a_c;
  v_col = a_col;
  gl_Position = u_proj * vp;
}
