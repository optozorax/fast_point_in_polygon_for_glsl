#version 100
precision lowp float;

struct Ray
{
    vec3 o; // origin
    vec3 d; // direction
};

vec3 normalize_normal(vec3 normal, Ray r) {
    normal = normalize(normal);
    if (dot(normal, r.d) > 0.) {
        normal *= -1.;
    }
    return normal;
}

vec3 mul_dir(mat4 matrix, vec3 vec) {
    return (matrix * vec4(vec, 0.)).xyz;
}

vec3 mul_pos(mat4 matrix, vec3 vec) {
    return (matrix * vec4(vec, 1.)).xyz;
}

vec3 color(float r, float g, float b) {
    return vec3(r*r, g*g, b*b);
}

vec3 add_normal_to_color(vec3 color, vec3 normal, vec3 direction) {
    const float not_dark_count = 0.4;
    color *= (abs(dot(normalize(direction), normalize(normal))) + not_dark_count) / (1. + not_dark_count);
    return color;
}

vec3 grid_color(vec3 start, vec2 uv) {
    uv = uv - vec2(0.125, 0.125);
    uv *= 1./4.;
    const float fr = 3.14159*8.0;
    vec3 col = start;
    col += 0.4*smoothstep(-0.01,0.01,cos(uv.x*fr*0.5)*cos(uv.y*fr*0.5)); 
    float wi = smoothstep(-1.0,-0.98,cos(uv.x*fr))*smoothstep(-1.0,-0.98,cos(uv.y*fr));
    col *= wi;
    
    return col;
}
