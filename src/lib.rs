use std::f32::consts::TAU;

use bevy::prelude::*;
use bevy::{
	core_pipeline::core_3d::Transparent3d,
	ecs::{
		query::QueryItem,
		system::{lifetimeless::*, SystemParamItem},
	},
	pbr::{
		MeshPipeline, MeshPipelineKey, RenderMeshInstances, SetMeshBindGroup, SetMeshViewBindGroup,
	},
	prelude::*,
	render::{
		extract_component::{ExtractComponent, ExtractComponentPlugin},
		mesh::{GpuBufferInfo, MeshVertexBufferLayout},
		render_asset::RenderAssets,
		render_phase::{
			AddRenderCommand, DrawFunctions, PhaseItem, RenderCommand, RenderCommandResult, RenderPhase,
			SetItemPipeline, TrackedRenderPass,
		},
		render_resource::*,
		renderer::RenderDevice,
		view::{ExtractedView, NoFrustumCulling},
		Render, RenderApp, RenderSet,
	},
};
use bytemuck::{Pod, Zeroable};
use rand::Rng;

// primarily copied from 0.12.1 example:
// https://github.com/bevyengine/bevy/blob/22e39c4abf6e2fdf99ba0820b3c35db73be71347/examples/shader/shader_instancing.rs

#[derive(Resource, Clone, Copy)]
pub struct StarfieldPlugin {
	pub num: usize,
	pub distance: f32,
}

impl Default for StarfieldPlugin {
	fn default() -> Self {
		Self {
			num: 1000,
			distance: 1_000.0,
		}
	}
}

const STARFIELD_SHADER_HANDLE: Handle<Shader> = Handle::weak_from_u128(4913569193382690169);

impl Plugin for StarfieldPlugin {
	fn build(&self, app: &mut App) {
		app
			.add_systems(Startup, Self::setup)
			.add_plugins(CustomMaterialPlugin)
			.insert_resource(*self);
		bevy::asset::load_internal_asset!(
			app,
			STARFIELD_SHADER_HANDLE,
			"starfield_shader.wgsl",
			Shader::from_wgsl
		);
	}
}

impl StarfieldPlugin {
	fn setup(config: Res<Self>, mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
		commands.spawn((
			meshes.add(Mesh::from(shape::UVSphere {
				radius: 0.5,
				..default()
			})),
			SpatialBundle::INHERITED_IDENTITY,
			InstanceMaterialData(
				// (1..=10)
				//     .flat_map(|x| (1..=10).map(move |y| (x as f32 / 10.0, y as f32 / 10.0)))
				//     .map(|(x, y)| InstanceData {
				//         position: Vec3::new(x * 10.0 - 5.0, y * 10.0 - 5.0, 0.0),
				//         scale: 1.0,
				//         color: Color::hsla(x * 360., y, 0.5, 1.0).as_rgba_f32(),
				//     })
				//     .collect(),
				{
					let mut stars = Vec::with_capacity(config.num);
					for _ in 0..config.num {
						stars.push(InstanceData {
							position: gen_random_sphere_normal() * config.distance,
							scale: 1.0,
							color: Color::BLUE.into(),
						});
					}
					stars
				},
			),
			// NOTE: Frustum culling is done based on the Aabb of the Mesh and the GlobalTransform.
			// As the cube is at the origin, if its Aabb moves outside the view frustum, all the
			// instanced cubes will be culled.
			// The InstanceMaterialData contains the 'GlobalTransform' information for this custom
			// instancing, and that is not taken into account with the built-in frustum culling.
			// We must disable the built-in frustum culling by adding the `NoFrustumCulling` marker
			// component to avoid incorrect culling.
			NoFrustumCulling,
		));
	}
}

fn from_polar_normal(theta: f32, phi: f32) -> Vec3 {
	Vec3 {
		x: theta.sin() * phi.cos(),
		y: theta.sin() * phi.sin(),
		z: theta.cos(),
	}
}

fn gen_random_sphere_normal() -> Vec3 {
	let mut rng = rand::thread_rng();
	let phi = rng.gen_range(0. ..TAU);
	let z: f32 = rng.gen_range(-1. ..1.);
	let theta = z.acos();

	let ret = from_polar_normal(theta, phi);

	ret.normalize()
}

#[derive(Component, Deref)]
struct InstanceMaterialData(Vec<InstanceData>);

impl ExtractComponent for InstanceMaterialData {
	type Query = &'static InstanceMaterialData;
	type Filter = ();
	type Out = Self;

	fn extract_component(item: QueryItem<'_, Self::Query>) -> Option<Self> {
		Some(InstanceMaterialData(item.0.clone()))
	}
}

struct CustomMaterialPlugin;

impl Plugin for CustomMaterialPlugin {
	fn build(&self, app: &mut App) {
		app.add_plugins(ExtractComponentPlugin::<InstanceMaterialData>::default());
		app
			.sub_app_mut(RenderApp)
			.add_render_command::<Transparent3d, DrawCustom>()
			.init_resource::<SpecializedMeshPipelines<CustomPipeline>>()
			.add_systems(
				Render,
				(
					queue_custom.in_set(RenderSet::QueueMeshes),
					prepare_instance_buffers.in_set(RenderSet::PrepareResources),
				),
			);
	}

	fn finish(&self, app: &mut App) {
		app.sub_app_mut(RenderApp).init_resource::<CustomPipeline>();
	}
}

#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct InstanceData {
	position: Vec3,
	scale: f32,
	color: [f32; 4],
}

#[allow(clippy::too_many_arguments)]
fn queue_custom(
	transparent_3d_draw_functions: Res<DrawFunctions<Transparent3d>>,
	custom_pipeline: Res<CustomPipeline>,
	msaa: Res<Msaa>,
	mut pipelines: ResMut<SpecializedMeshPipelines<CustomPipeline>>,
	pipeline_cache: Res<PipelineCache>,
	meshes: Res<RenderAssets<Mesh>>,
	render_mesh_instances: Res<RenderMeshInstances>,
	material_meshes: Query<Entity, With<InstanceMaterialData>>,
	mut views: Query<(&ExtractedView, &mut RenderPhase<Transparent3d>)>,
) {
	let draw_custom = transparent_3d_draw_functions.read().id::<DrawCustom>();

	let msaa_key = MeshPipelineKey::from_msaa_samples(msaa.samples());

	for (view, mut transparent_phase) in &mut views {
		let view_key = msaa_key | MeshPipelineKey::from_hdr(view.hdr);
		let rangefinder = view.rangefinder3d();
		for entity in &material_meshes {
			let Some(mesh_instance) = render_mesh_instances.get(&entity) else {
				continue;
			};
			let Some(mesh) = meshes.get(mesh_instance.mesh_asset_id) else {
				continue;
			};
			let key = view_key | MeshPipelineKey::from_primitive_topology(mesh.primitive_topology);
			let pipeline = pipelines
				.specialize(&pipeline_cache, &custom_pipeline, key, &mesh.layout)
				.unwrap();
			transparent_phase.add(Transparent3d {
				entity,
				pipeline,
				draw_function: draw_custom,
				distance: rangefinder.distance_translation(&mesh_instance.transforms.transform.translation),
				batch_range: 0..1,
				dynamic_offset: None,
			});
		}
	}
}

#[derive(Component)]
struct InstanceBuffer {
	buffer: Buffer,
	length: usize,
}

fn prepare_instance_buffers(
	mut commands: Commands,
	query: Query<(Entity, &InstanceMaterialData)>,
	render_device: Res<RenderDevice>,
) {
	for (entity, instance_data) in &query {
		let buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
			label: Some("instance data buffer"),
			contents: bytemuck::cast_slice(instance_data.as_slice()),
			usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
		});
		commands.entity(entity).insert(InstanceBuffer {
			buffer,
			length: instance_data.len(),
		});
	}
}

#[derive(Resource)]
struct CustomPipeline {
	shader: Handle<Shader>,
	mesh_pipeline: MeshPipeline,
}

impl FromWorld for CustomPipeline {
	fn from_world(world: &mut World) -> Self {
		let asset_server = world.resource::<AssetServer>();
		let shader = STARFIELD_SHADER_HANDLE;

		let mesh_pipeline = world.resource::<MeshPipeline>();

		CustomPipeline {
			shader,
			mesh_pipeline: mesh_pipeline.clone(),
		}
	}
}

impl SpecializedMeshPipeline for CustomPipeline {
	type Key = MeshPipelineKey;

	fn specialize(
		&self,
		key: Self::Key,
		layout: &MeshVertexBufferLayout,
	) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
		let mut descriptor = self.mesh_pipeline.specialize(key, layout)?;

		// meshes typically live in bind group 2. because we are using bindgroup 1
		// we need to add MESH_BINDGROUP_1 shader def so that the bindings are correctly
		// linked in the shader
		descriptor
			.vertex
			.shader_defs
			.push("MESH_BINDGROUP_1".into());

		descriptor.vertex.shader = self.shader.clone();
		descriptor.vertex.buffers.push(VertexBufferLayout {
			array_stride: std::mem::size_of::<InstanceData>() as u64,
			step_mode: VertexStepMode::Instance,
			attributes: vec![
				VertexAttribute {
					format: VertexFormat::Float32x4,
					offset: 0,
					shader_location: 3, // shader locations 0-2 are taken up by Position, Normal and UV attributes
				},
				VertexAttribute {
					format: VertexFormat::Float32x4,
					offset: VertexFormat::Float32x4.size(),
					shader_location: 4,
				},
			],
		});
		descriptor.fragment.as_mut().unwrap().shader = self.shader.clone();
		Ok(descriptor)
	}
}

type DrawCustom = (
	SetItemPipeline,
	SetMeshViewBindGroup<0>,
	SetMeshBindGroup<1>,
	DrawMeshInstanced,
);

struct DrawMeshInstanced;

impl<P: PhaseItem> RenderCommand<P> for DrawMeshInstanced {
	type Param = (SRes<RenderAssets<Mesh>>, SRes<RenderMeshInstances>);
	type ViewWorldQuery = ();
	type ItemWorldQuery = Read<InstanceBuffer>;

	#[inline]
	fn render<'w>(
		item: &P,
		_view: (),
		instance_buffer: &'w InstanceBuffer,
		(meshes, render_mesh_instances): SystemParamItem<'w, '_, Self::Param>,
		pass: &mut TrackedRenderPass<'w>,
	) -> RenderCommandResult {
		let Some(mesh_instance) = render_mesh_instances.get(&item.entity()) else {
			return RenderCommandResult::Failure;
		};
		let gpu_mesh = match meshes.into_inner().get(mesh_instance.mesh_asset_id) {
			Some(gpu_mesh) => gpu_mesh,
			None => return RenderCommandResult::Failure,
		};

		pass.set_vertex_buffer(0, gpu_mesh.vertex_buffer.slice(..));
		pass.set_vertex_buffer(1, instance_buffer.buffer.slice(..));

		match &gpu_mesh.buffer_info {
			GpuBufferInfo::Indexed {
				buffer,
				index_format,
				count,
			} => {
				pass.set_index_buffer(buffer.slice(..), 0, *index_format);
				pass.draw_indexed(0..*count, 0, 0..instance_buffer.length as u32);
			}
			GpuBufferInfo::NonIndexed => {
				pass.draw(0..gpu_mesh.vertex_count, 0..instance_buffer.length as u32);
			}
		}
		RenderCommandResult::Success
	}
}
