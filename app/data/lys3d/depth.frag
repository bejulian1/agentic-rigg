#version 300 es
// Agentic Rigg, 3D-kortet paa Lys. Port av web/js/lys3d/gl/renderer.js (WebGL 2),
// uendret bortsett fra at renderer.rs setter inn lys.glsl ved include-linjen.
// Bare ASCII, uten apostrofer: noen GLSL-kompilatorer avviser andre tegn.
precision mediump float;
in vec3 v_l;
in vec2 v_uv;
uniform vec4 u_holes[2];
uniform sampler2D u_map;
uniform float u_useMap;
uniform float u_alphaTest;
uniform vec3 u_id;
out vec4 o;
void main() {
  for (int i = 0; i < 2; i++) {
    if (u_holes[i].w > 0.5 && distance(v_l.xy, u_holes[i].xy) < u_holes[i].z) discard;
  }
  if (u_useMap > 0.5 && texture(u_map, v_uv).a < u_alphaTest) discard;
  o = vec4(u_id, 1.0);
}
