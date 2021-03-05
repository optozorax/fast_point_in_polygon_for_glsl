
uniform mat4 camera;
uniform int show_grid;

uniform vec2 offset;
uniform vec2 size;

varying vec2 uv;
varying vec2 uv_screen;

void main() {
    vec3 o = mul_pos(camera, vec3(0.));
    vec3 d = normalize(mul_dir(camera, vec3(uv_screen.x, uv_screen.y, 1.)));
    Ray r = Ray(o, d);

    gl_FragColor = vec4(0.8, 0.8, 0.8, 1.);        

    float t = -r.o.z/r.d.z;
    if (t > 0.) {
        vec3 pos = r.o + r.d * t;
        float prop = max(size.x, size.y);
        vec2 coord = (pos.xy + vec2(0.5) * size / prop) * prop + offset;
        if (is_inside_polygon(coord)) {
            vec3 clr = add_normal_to_color(color(0.4, 0.4, 0.4), vec3(0., 0., 1.), r.d);

            if (show_grid == 1) {
                clr = grid_color(clr, coord);
            }

            gl_FragColor = vec4(sqrt(clr), 1.);
        }
    }
}