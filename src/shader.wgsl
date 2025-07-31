struct GlobalTransformUniform {
    scale: vec2<f32>,
    translate: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> global_transform: GlobalTransformUniform;

@group(0) @binding(1)
var r_sampler: sampler;

@group(0) @binding(2)
var<storage, read> line_points: array<f32>;

@group(0) @binding(3)
var<storage, read> quadratic_points: array<f32>;

@group(0) @binding(4)
var<storage, read> cubic_points: array<f32>;


struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct InstanceInput {
    @location(2) position: vec2<f32>,
    @location(3) scale: vec2<f32>,
    @location(4) bg: vec4<f32>,
    @location(5) fg: vec4<f32>,

	/// x is the offset, y is the length
    @location(6) lines_off: vec2<u32>,
    @location(7) quadratic_off: vec2<u32>,
    @location(8) cubic_off: vec2<u32>,
}


struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) bg: vec4<f32>,
    @location(3) fg: vec4<f32>,
    @location(4) lines_off: vec2<u32>,
    @location(5) quadratic_off: vec2<u32>,
    @location(6) cubic_off: vec2<u32>,
}

@vertex
fn vs_main(input: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    out.tex_coords = input.tex_coords ;
    out.clip_position = vec4<f32>(
        (input.position * instance.scale + instance.position) * global_transform.scale + global_transform.translate, 0., 1.
    );
    out.bg = instance.bg;
    out.fg = instance.fg;
    out.lines_off = instance.lines_off;
    out.quadratic_off = instance.quadratic_off;
    out.cubic_off = instance.cubic_off;
    return out;
}



fn sdLine(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    var s = 1.;

    if ba.x * ba.y < 0.0 {s = -1.0;};
    let h = clamp((pa.y + s * pa.x) / (ba.y + s * ba.x), 0.0, 1.0);
    let q = abs(pa - h * ba);
    return max(q.x, q.y);
}

fn sdQuadratic(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>, c: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = c - a;
    var s = 1.;

    if ba.x * ba.y < 0.0 {s = -1.0;};
    let h = clamp((pa.y + s * pa.x) / (ba.y + s * ba.x), 0.0, 1.0);
    let q = abs(pa - h * ba);
    return max(q.x, q.y);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    //var winding = 0;
    var min_dist = 9999.;

	if (input.quadratic_off.y  > u32( 500 )) {
		return vec4<f32>(0.5);
	}

	/// Remeber, x is offset, y is length
    for (var i = input.lines_off.x; i < input.lines_off.x + input.lines_off.y; i++) {
        let idx = i * u32(4);

        let x0 = line_points[idx] ;
        let y0 = line_points[idx+u32(1)];
        let p0 = vec2<f32>(x0, y0);

        let x1 = line_points[idx + u32(2)] ;
        let y1 = line_points[idx + u32(3)];
        let p1 = vec2<f32>(x1, y1);


        min_dist = min(min_dist, sdLine(input.tex_coords.xy, p0, p1));
/*
        let m = (p1.y - p0.y) / (p1.x - p1.x);

        let intersecting_x = (input.tex_coords.y - p0.y) / m + p0.x;
        if intersecting_x < input.tex_coords.x {
			continue;
        }

		/// (1 - t) * p0 + t * p1 = p
		/// t = (p - p0) / (p1 - p0)
		/// Since there is no such thing as division of vectors, i am considering the y component
        var t = (input.tex_coords.y - p0.y) / (p1.y - p0.y);
        if abs(p1.y - p0.y) < 0.00001 {
            t = (input.tex_coords.x - p0.x) / (p1.x - p0.x);
        }
        if t > 1. || t < 0. {
			continue;
        }

        if m > 0. {
            winding += 1;
        } else {
            winding -= 1;
        }
		*/
    }

    for (var i = input.quadratic_off.x; i < input.quadratic_off.x + input.quadratic_off.y; i++) {
        let idx = i * u32(6);

        let x0 = quadratic_points[idx] ;
        let y0 = quadratic_points[idx+u32(1)];
        let p0 = vec2<f32>(x0, y0);

        let x1 = quadratic_points[idx + u32(2)] ;
        let y1 = quadratic_points[idx + u32(3)];
        let p1 = vec2<f32>(x1, y1);

        let x2 = quadratic_points[idx + u32(4)] ;
        let y2 = quadratic_points[idx + u32(5)];
        let p2 = vec2<f32>(x2, y2);

        min_dist = min(min_dist, sdQuadratic(input.tex_coords.xy, p0, p1, p2));
	}

    if min_dist < 0.01 {
        return input.fg;
    }
	/*
    if winding == 0 {
        return input.fg;
    }
*/
    return input.bg;
}
