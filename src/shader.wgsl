struct GlobalTransformUniform {
    scale: vec2<f32>,
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
}


struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@vertex
fn vs_main(input: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    out.tex_coords = input.tex_coords;
    out.clip_position = vec4<f32>((input.position * instance.scale + instance.position) * global_transform.scale, 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 1.0, 0.0, 1.0);
}
