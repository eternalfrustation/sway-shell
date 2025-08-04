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

fn cross_f(a: vec2<f32>, b: vec2<f32>) -> f32 {
	return a.x * b.y - a.y * b.x;
}

fn sdLine(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
	let ba = b - a;
	let unclamped_t = dot(p - a, ba) / (dot2(ba));
	let t = clamp(unclamped_t, 0., 1. ) ;
	let q = (1. - t) * a + t * b;
	let pq = p - q;
	let dist = length(p - q) * sign(cross_f(ba , q - p));
	var orthogonality = 9999.;
	if (abs(dist) > 0. && (unclamped_t > 1. || unclamped_t < 0.)) {
        orthogonality = cross_f(normalize(ba), normalize(p - q));
	}
	return vec2<f32>(dist, orthogonality);
}

fn dot2(v: vec2<f32>) -> f32 {
    return dot(v, v);
}


fn sdQuadratic(pos: vec2<f32>, A: vec2<f32>, B: vec2<f32>, C: vec2<f32>) -> vec2<f32> {
    let a = B - A;
    let b = A - 2.0 * B + C;
    let c = a * 2.0;
    let d = A - pos;
    let kk = 1.0 / dot(b, b);
    let kx = kk * dot(a, b);
    let ky = kk * (2.0 * dot(a, a) + dot(d, b)) / 3.0;
    let kz = kk * dot(d, a);
    var res = 0.0;
    let p = ky - kx * kx;
    let p3 = p * p * p;
    let q = kx * (2.0 * kx * kx - 3.0 * ky) + kz;
    var h = q * q + 4.0 * p3;
    var sign = 0;

	var orthogonality = 9999.;

    if h >= 0.0 {
        h = sqrt(h);
        let x = (vec2(h, -h) - q) / 2.0;
        let uv = sign(x) * pow(abs(x), vec2(1.0 / 3.0));
		let unclamped_t = uv.x + uv.y - kx;
        let t = clamp(unclamped_t, 0.0, 1.0);
        let q = (d + (c + b * t) * t);
		let tangent = 2. * (1. - t) * (B - A) + 2. * t * (C - B) ;
		let lq = length(q);
		if (lq > 0. && (unclamped_t > 1. || unclamped_t < 0.)) {
			orthogonality = cross_f(normalize(tangent), normalize(q));
		}
		res = length(q) * sign(cross_f(tangent, q));
    } else {
        let z = sqrt(-p);
        let v = acos(q / (p * z * 2.0)) / 3.0;
        let m = cos(v);
        let n = sin(v) * 1.732050808;
        let  unclamped_t = vec3<f32>(m + m, -n - m, n - m) * z - kx;
        let  t = clamp(unclamped_t, vec3<f32>(0.0), vec3<f32>(1.0));
		let q1 = d + (c + b * t.x) * t.x;
		let q2 = d + (c + b * t.y) * t.y;
		let q1d = dot2(q1);
		let q2d = dot2(q2);
		if q1d < q2d {
			let tangent = 2. * (1. - t.x) * (B - A) + 2. * t.x * (C - B) ;
			if (q1d > 0. && (unclamped_t.x > 1. || unclamped_t.x < 0.)) {
				orthogonality = cross_f(normalize(tangent), normalize(q1));
			}
			res = sqrt(q1d) * sign(cross_f(tangent, q1)) ;
		} else {
			let tangent = 2. * (1. - t.y) * (B - A) + 2. * t.y * (C - B) ;
			if (q2d > 0. && (unclamped_t.y > 1. || unclamped_t.y < 0.)) {
				orthogonality = cross_f(normalize(tangent), normalize(q2));
			}
			res = sqrt(q2d) * sign(cross_f(tangent, q2)) ;
		}
    }
    return vec2<f32>(res, orthogonality);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    //var winding = 0;
	// x has the distance, y has the orthogonality
    var min_dist = vec2<f32>(999., -999.);

    if input.quadratic_off.y > u32(500) {
        return vec4<f32>(0.5);
    }

	/// Remeber, x is offset, y is length
    for (var i = input.lines_off.x; i < input.lines_off.x + input.lines_off.y; i++) {
        let idx = i * u32(4);

        let x0 = line_points[idx] ;
        let y0 = line_points[idx + u32(1)];
        let p0 = vec2<f32>(x0, y0);

        let x1 = line_points[idx + u32(2)] ;
        let y1 = line_points[idx + u32(3)];
        let p1 = vec2<f32>(x1, y1);

        let d = sdLine(input.tex_coords.xy, p0, p1);

		if abs(abs(d.x) - abs(min_dist.x)) < 0.0001  {
			if abs(min_dist.y) <= abs(d.y) {
				min_dist = d;
			}
		} else {
			if abs(d.x) < abs( min_dist.x ) {
				min_dist = d;
			}
		}


		//winding += intersectingLine(input.tex_coords.xy, p0, p1);

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
        let y0 = quadratic_points[idx + u32(1)];
        let p0 = vec2<f32>(x0, y0);

        let x1 = quadratic_points[idx + u32(2)] ;
        let y1 = quadratic_points[idx + u32(3)];
        let p1 = vec2<f32>(x1, y1);

        let x2 = quadratic_points[idx + u32(4)] ;
        let y2 = quadratic_points[idx + u32(5)];
        let p2 = vec2<f32>(x2, y2);
        let d = sdQuadratic(input.tex_coords.xy, p0, p1, p2);

		if abs(abs(d.x) - abs(min_dist.x)) < 0.0001  {
			if abs(min_dist.y) <= abs(d.y) {
				min_dist = d;
			}
		} else {
			if abs(d.x) < abs( min_dist.x ) {
				min_dist = d;
			}
		}
		/*
        if abs(d.x) < abs(min_dist.x) {
            min_dist = d;
        }
		*/

        //min_dist = min(min_dist, sdQuadratic(input.tex_coords.xy, p0, p1, p2));
    }
/*
    if winding != 0 {
        return input.fg;
    }
*/

	
    return mix(input.fg, input.bg, ( min_dist.x * 100. ) );
}
