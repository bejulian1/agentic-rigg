#version 300 es
// Agentic Rigg, 3D-kortet paa Lys. Port av web/js/lys3d/gl/renderer.js (WebGL 2),
// uendret bortsett fra at renderer.rs setter inn lys.glsl ved include-linjen.
// Bare ASCII, uten apostrofer: noen GLSL-kompilatorer avviser andre tegn.
precision highp float;
in vec3 v_w;
uniform vec3 u_center;
uniform float u_size;
uniform float u_alpha;
#include lys.glsl
out vec4 o;
void main() {
  float s = shadowAt(v_ls, 4.0);
  float r = length(v_w.xz - u_center.xz) / u_size;
  float fade = 1.0 - smoothstep(0.18, 0.95, r);
  vec3 led = ledLights(v_w, vec3(0.0, 1.0, 0.0), vec3(0.35), vec3(0.04), 8.0, vec3(0.0, 1.0, 0.0));
  led = 1.0 - exp(-led * 1.4);
  float a = fade * u_alpha;
  // Premultiplisert: bare skyggen gjor gulvet morkere, og LED-lyset gir en
  // svak farget flekk. Utenfor skyggen er gulvet lik bakgrunnen.
  float dark = (1.0 - s) * 0.42 * a;
  o = vec4(pow(led, vec3(1.0 / 2.2)) * 0.22 * a, dark);
}
