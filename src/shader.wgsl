struct GlobalTransformUniform {
    scale: vec2<f32>,
    translate: vec2<f32>,
};

@group(0) @binding(0) // 1.
var<uniform> global_transform: GlobalTransformUniform;

@group(0) @binding(1)
var r_color: texture_2d<f32>;

@group(0) @binding(2)
var r_sampler: sampler;

@group(0) @binding(3)
var<storage, read> font_config: array<FontLocation>;

struct FontLocation {
	curve_type0: u32,
	curve_offset0: u32,
}


struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct InstanceInput {
    @location(5) position: vec2<f32>,
    @location(6) scale: vec2<f32>,
    @location(7) bg: vec4<f32>,
    @location(8) fg: vec4<f32>,
    @location(9) curve_offset: u32,
    @location(10) curve_len: u32,
}


struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(1) bg: vec4<f32>,
    @location(2) fg: vec4<f32>,
    @location(9) curve_offset: u32,
    @location(10) curve_len: u32,
}

@vertex
fn vs_main(input: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    //out.tex_coords = (input.tex_coords * instance.tex_scale + instance.tex_offset) ;
    out.clip_position = vec4<f32>(
        (input.position * instance.scale + instance.position) * global_transform.scale + global_transform.translate, 0.0, 1.0
    );
    out.bg = instance.bg;
    out.fg = instance.fg;
	out.curve_offset = instance.curve_offset;
	out.curve_len = instance.curve_len;
    return out;
}

fn median(r: f32, g: f32, b: f32) -> f32 {
    return max(min(r, g), min(max(r, g), b));
}

const aa_threshold : f32 = 0.02;

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
	
}
