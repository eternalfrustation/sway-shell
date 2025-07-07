struct GlobalTransformUniform {
    scale: vec2<f32>,
    translate: vec2<f32>,
};

@group(0) @binding(0) // 1.
var<uniform> global_transform: GlobalTransformUniform;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct InstanceInput {
    @location(5) position: vec2<f32>,
    @location(6) scale: vec2<f32>,
    @location(7) bg: u32,
    @location(8) fg: u32,
}


struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) bg: vec4<f32>,
    @location(2) fg: vec4<f32>,
}

@vertex
fn vs_main(input: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    out.tex_coords = input.tex_coords;
    out.clip_position = vec4<f32>(
        (input.position * instance.scale + instance.position) * global_transform.scale + global_transform.translate, 0.0, 1.0
    );
    out.bg = rgba8tovec4(instance.bg);
    out.fg = rgba8tovec4(instance.fg);
    return out;
}

fn rgba8tovec4(color: u32) -> vec4<f32> {
    let r: f32 = f32((color & u32(0xff000000)) >> u32(24)) / f32(255);
    let g: f32 = f32((color & u32(0x00ff0000)) >> u32(16)) / f32(255);
    let b: f32 = f32((color & u32(0x0000ff00)) >> u32(8)) / f32(255);
    let a: f32 = f32(color & u32(0x000000ff)) / f32(255);
    return vec4<f32>(r, g, b, a);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.fg;
}
