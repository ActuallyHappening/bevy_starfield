// copied from 0.12.1 example:
// https://github.com/bevyengine/bevy/blob/22e39c4abf6e2fdf99ba0820b3c35db73be71347/assets/shaders/instancing.wgsl

#import bevy_pbr::mesh_functions::{get_model_matrix, mesh_position_local_to_clip}

struct Vertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,

		@location(3) instance_position: vec3<f32>,
    // @location(3) i_pos_scale: vec4<f32>,
    // @location(4) i_color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    // @location(0) color: vec4<f32>,
};

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    // let position = vertex.position * vertex.i_pos_scale.w + vertex.i_pos_scale.xyz;
		let position = vertex.position + vertex.instance_position;
		
    var out: VertexOutput;
    // NOTE: Passing 0 as the instance_index to get_model_matrix() is a hack
    // for this example as the instance_index builtin would map to the wrong
    // index in the Mesh array. This index could be passed in via another
    // uniform instead but it's unnecessary for the example.
    out.clip_position = mesh_position_local_to_clip(
        get_model_matrix(0u),
        vec4<f32>(position, 1.0)
    );

    // out.color = vertex.i_color;
		// out.color = vec4<f32>(1.0, 0.0, 0.0, 1.0);

    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // return in.color;
		return vec4<f32>(1.0, 1.0, 1.0, 1.0);
}
