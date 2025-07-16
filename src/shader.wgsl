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
var<uniform> font_config: vec3<f32>;


struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct InstanceInput {
    @location(5) position: vec2<f32>,
    @location(6) scale: vec2<f32>,
    @location(7) bg: vec4<f32>,
    @location(8) fg: vec4<f32>,
    @location(9) tex_offset: vec2<f32>,
	@location(10) tex_scale: vec2<f32>,
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
    //out.tex_coords = (input.tex_coords * instance.tex_scale + instance.tex_offset) ;
    out.clip_position = vec4<f32>(
        (input.position * instance.scale + instance.position) * global_transform.scale + global_transform.translate, 0.0, 1.0
    );
	out.tex_coords = (input.tex_coords * instance.tex_scale) + instance.tex_offset;
    out.bg = instance.bg;
    out.fg = instance.fg;
    return out;
}

fn median(r: f32, g: f32, b: f32) -> f32 {
    return max(min(r, g), min(max(r, g), b));
}

const aa_threshold : f32 = 0.02;

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
	let s =  textureSample(r_color, r_sampler, input.tex_coords);
	return mix(input.fg, input.bg, vec4<f32>(s.r - 0.5) / aa_threshold);
}
