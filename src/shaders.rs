pub const PAGE_VERTEX_SHADER: &'static str = "
#version 420

in vec2 position;
in uint color;
in uint depth;
in uint mask;

out flat uint v_color;
out flat uint v_depth;
out flat uint v_mask;
out vec2 v_position;

void main () {
  v_color = color;
  v_depth = depth;
  v_mask = mask;
  v_position = vec2(position.x, 199 - position.y)  * vec2(1.0/319.0, 1.0/199.0);
  gl_Position = vec4((position * vec2(2.0/319.0, -2.0/199.0)) + vec2(-1.0, 1.0), 1.0, 1.0);
}
";

pub const PAGE_FRAGMENT_SHADER: &'static str = "
#version 420

in flat uint v_color;
in flat uint v_depth;
in flat uint v_mask;
in vec2 v_position;

uniform uint u_max_depth;
uniform usampler2D u_page_zero;
uniform usampler2D u_page_self;

out uint f_color;

void main () {
  if (v_mask != 0) {
    f_color = texture(u_page_self, v_position).r | v_mask;
  } else if (v_color > 15) {
    f_color = texture(u_page_zero, v_position).r;
  } else {
    f_color = v_color;
  }
  gl_FragDepth = float(v_depth) / float(u_max_depth);
}
";

pub const FRAME_VERTEX_SHADER: &'static str = "
#version 420

in vec2 position;

out vec2 v_position;

void main () {
  v_position = (position + vec2(1.0)) / vec2(2.0);
  gl_Position = vec4(position, 1.0, 1.0);
}
";

pub const FRAME_FRAGMENT_SHADER: &'static str = "
#version 420

in vec2 v_position;

uniform sampler2D u_palette;
uniform usampler2D u_page;

out vec4 f_color;

void main () {
  uint color_index = texture(u_page, v_position).r;
  f_color = vec4(texelFetch(u_palette, ivec2(color_index, 0), 0).rgb, 1.0);
}
";

pub const COPY_VERTEX_SHADER: &'static str = "
#version 420

in vec2 position;

out vec2 v_position;

void main () {
  v_position = (position + vec2(1.0)) / vec2(2.0);
  gl_Position = vec4(position, 1.0, 1.0);
}
";

pub const COPY_FRAGMENT_SHADER: &'static str = "
#version 420

in vec2 v_position;

uniform usampler2D u_page;
uniform uint u_fill;
uniform int u_scroll;

out uint f_color;

void main () {
  if (u_fill > 15) {
    float scroll = float(u_scroll) / 200.0;
    f_color = texture(u_page, v_position.xy + vec2(0.0, scroll)).r;
  } else {
    f_color = u_fill;
  }
}
";

pub const FONT_VERTEX_SHADER: &'static str = "
#version 420

in vec2 position;
in vec2 uv;

out vec2 v_position;
out vec2 v_uv;

void main () {
  v_position = vec2(position.x, 199 - position.y)  * vec2(1.0/319.0, 1.0/199.0);
  v_uv = uv;
  gl_Position = vec4((position * vec2(2.0/319.0, -2.0/199.0)) + vec2(-1.0, 1.0), 1.0, 1.0);
}
";

pub const FONT_FRAGMENT_SHADER: &'static str = "
#version 420

in vec2 v_position;
in vec2 v_uv;

uniform usampler2D u_font_atlas;
uniform uint u_color;

out uint f_color;

void main () {
  uint pixel = texture(u_font_atlas, v_uv.xy).r;
  if (pixel > 0) {
    f_color = u_color;
  } else {
    discard;
  }
}
";
