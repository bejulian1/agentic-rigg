#version 300 es
// Agentic Rigg, 3D-kortet paa Lys. Port av web/js/lys3d/gl/renderer.js (WebGL 2),
// uendret bortsett fra at renderer.rs setter inn lys.glsl ved include-linjen.
// Bare ASCII, uten apostrofer: noen GLSL-kompilatorer avviser andre tegn.
layout(location=0) in vec2 a_c;
out vec2 v_uv;
void main() {
  v_uv = a_c * 0.5 + 0.5;
  gl_Position = vec4(a_c, 0.0, 1.0);
}
