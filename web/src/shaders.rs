pub const PAGE_VERTEX_SHADER: &'static str = "
attribute vec2 position;
attribute float color;
attribute float mask;

varying float v_color;
varying float v_mask;
varying vec2 v_position;

void main () {
  v_color = color;
  v_mask = mask;
  v_position = vec2(position.x, 199.0 - position.y)  * vec2(1.0/319.0, 1.0/199.0);
  gl_Position = vec4((position * vec2(2.0/319.0, -2.0/199.0)) + vec2(-1.0, 1.0), 1.0, 1.0);
}
";

pub const PAGE_FRAGMENT_SHADER: &'static str = "
precision mediump float;

varying float v_color;
varying float v_mask;
varying vec2 v_position;

uniform sampler2D u_page_zero;
uniform sampler2D u_page_self;

void main () {
  int mask = int(v_mask);
  if (mask != 0) {
    int color = int(texture2D(u_page_self, v_position).r * 255.0);
    if (color < mask) {
       color = color + mask;
    }

    gl_FragColor = vec4(float(color) / 255.0);
  } else if (int(v_color) > 15) {
    gl_FragColor = vec4(texture2D(u_page_zero, v_position).r);
  } else {
    gl_FragColor = vec4(v_color / 255.0);
  }
}
";
pub const FRAME_VERTEX_SHADER: &'static str = "
attribute vec2 position;

varying vec2 v_position;

void main () {
  v_position = (position + vec2(1.0)) / vec2(2.0);
  gl_Position = vec4(position, 1.0, 1.0);
}
";

pub const FRAME_FRAGMENT_SHADER: &'static str = "
precision mediump float;

varying vec2 v_position;

uniform sampler2D u_palette;
uniform sampler2D u_page;

void main () {
  float color_index = texture2D(u_page, v_position).r * 255.0;
  gl_FragColor = vec4(texture2D(u_palette, vec2(color_index / 15.0, 0.0)).rgb, 1.0);
}
";

pub const COPY_VERTEX_SHADER: &'static str = "
attribute vec2 position;

varying vec2 v_position;

void main () {
  v_position = (position + vec2(1.0)) / vec2(2.0);
  gl_Position = vec4(position, 1.0, 1.0);
}
";

pub const COPY_FRAGMENT_SHADER: &'static str = "
precision mediump float;

varying vec2 v_position;

uniform sampler2D u_page;
uniform int u_fill;
uniform int u_scroll;

void main () {
  if (u_fill > 15) {
    float scroll = float(u_scroll) / 200.0;
    gl_FragColor = vec4(texture2D(u_page, v_position.xy + vec2(0.0, scroll)).r);
  } else {
    gl_FragColor = vec4(float(u_fill) / 255.0);
  }
}
";

pub const FONT_VERTEX_SHADER: &'static str = "
attribute vec2 position;
attribute vec2 uv;

varying vec2 v_position;
varying vec2 v_uv;

void main () {
  v_position = vec2(position.x, 199.0 - position.y)  * vec2(1.0/319.0, 1.0/199.0);
  v_uv = uv;
  gl_Position = vec4((position * vec2(2.0/319.0, -2.0/199.0)) + vec2(-1.0, 1.0), 1.0, 1.0);
}
";

pub const FONT_FRAGMENT_SHADER: &'static str = "
precision mediump float;

varying vec2 v_position;
varying vec2 v_uv;

uniform sampler2D u_font_atlas;
uniform int u_color;

void main () {
  float pixel = texture2D(u_font_atlas, v_uv.xy).a;
  if (pixel > 0.5) {
    gl_FragColor = vec4(float(u_color) / 255.0);
  } else {
    discard;
  }
}
";
