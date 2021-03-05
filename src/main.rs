use std::f32::consts::PI;

use fast_point_in_polygon_for_glsl::*;
use macroquad::prelude::*;
use megaui_macroquad::{
	draw_megaui, draw_window,
	megaui::{self, hash},
	WindowParams,
};

use crate::{megaui::Vector2, miniquad::ShaderError};

struct RotateAroundCam {
	alpha: f32,
	beta: f32,
	r: f32,
	previous_mouse: Vec2,
}

impl RotateAroundCam {
	const BETA_MAX: f32 = PI - 0.01;
	const BETA_MIN: f32 = 0.01;
	const MOUSE_SENSITIVITY: f32 = 1.2;
	const SCALE_FACTOR: f32 = 1.1;
	const VIEW_ANGLE: f32 = 80. / 180. * PI;

	fn new() -> Self {
		Self {
			alpha: PI / 2.,
			beta: PI / 2.,
			r: 0.8,
			previous_mouse: Vec2::default(),
		}
	}

	fn process_mouse_and_keys(&mut self) -> bool {
		let mut is_something_changed = false;

		let mouse_pos: Vec2 = mouse_position_local();

		if is_mouse_button_down(MouseButton::Left) {
			let dalpha = (mouse_pos.x - self.previous_mouse.x) * Self::MOUSE_SENSITIVITY;
			let dbeta = (mouse_pos.y - self.previous_mouse.y) * Self::MOUSE_SENSITIVITY;

			self.alpha += dalpha;
			self.beta = clamp(self.beta + dbeta, Self::BETA_MIN, Self::BETA_MAX);

			is_something_changed = true;
		}

		let wheel_value = mouse_wheel().1;
		if wheel_value > 0. {
			self.r *= 1.0 / Self::SCALE_FACTOR;
			is_something_changed = true;
		} else if wheel_value < 0. {
			self.r *= Self::SCALE_FACTOR;
			is_something_changed = true;
		}

		self.previous_mouse = mouse_pos;

		return is_something_changed;
	}

	fn get_matrix(&self) -> Mat4 {
		let pos = Vec3::new(
			-self.beta.sin() * self.alpha.cos(),
			self.beta.cos(),
			-self.beta.sin() * self.alpha.sin(),
		) * self.r;
		let look_at = Vec3::new(0., 0., 0.);

		let h = (Self::VIEW_ANGLE / 2.).tan();

		let k = (look_at - pos).normalize();
		let i = k.cross(Vec3::new(0., 1., 0.)).normalize() * h;
		let j = k.cross(i).normalize() * h;

		Mat4::from_cols(
			Vec4::new(i.x, i.y, i.z, 0.),
			Vec4::new(j.x, j.y, j.z, 0.),
			Vec4::new(k.x, k.y, k.z, 0.),
			Vec4::new(pos.x, pos.y, pos.z, 1.),
		)
	}
}

struct PolygonShader {
	points: String,
	update_points: bool,
	show_grid: bool,
	error: Option<String>,
	text: String,
	material: Material,
	offset: (f32, f32),
	size: (f32, f32),
}

impl PolygonShader {
	const FRAGMENT_SHADER_AFTER: &'static str = include_str!("frag_after.glsl");
	const FRAGMENT_SHADER_BEFORE: &'static str = include_str!("frag_before.glsl");
	const VERTEX_SHADER: &'static str = include_str!("vertex.glsl");

	fn new(init: Vec<(f64, f64)>) -> Self {
		let points = init
			.iter()
			.map(|(x, y)| format!("{} {}", x, y))
			.collect::<Vec<String>>()
			.join("\n");
		let (material, offset, size, text) = Self::calc_material(init).unwrap_or_else(|err| {
			if let miniquad::graphics::ShaderError::CompilationError { error_message, .. } = err {
				println!("Fragment shader compilation error:\n{}", error_message);
			} else {
				println!("Other material error:\n{:#?}", err);
			}
			std::process::exit(1)
		});
		Self {
			points,
			update_points: false,
			show_grid: false,
			error: None,
			text,
			material,
			size,
			offset,
		}
	}

	fn parse_points(s: &str) -> Result<Vec<(f64, f64)>, String> {
		let mut result = Vec::new();
		let mut pos = 0;
		for line in s.split('\n').map(|x| x.trim()).filter(|x| !x.is_empty()) {
			let mut tokens = line.split(' ').map(|x| x.trim()).filter(|x| !x.is_empty());
			let a = tokens
				.next()
				.ok_or_else(|| format!("Can't find first number on line {}", pos))?;
			let b = tokens
				.next()
				.ok_or_else(|| format!("Can't find second number on line {}", pos))?;

			if tokens.next().is_some() {
				return Err(format!(
					"There is something after two numbers on line {}",
					pos
				));
			}

			let a: f64 = a.parse().map_err(|err| {
				format!(
					"Can't parse first number: {:#?}\nNumber trying to parse: `{}`\nLine: {}",
					err, a, pos
				)
			})?;
			let b: f64 = b.parse().map_err(|err| {
				format!(
					"Can't parse second number: {:#?}\nNumber trying to parse: `{}`\nLine: {}",
					err, b, pos
				)
			})?;

			result.push((a, b));

			pos += 1;
		}
		Ok(result)
	}

	fn calc_material(
		vec: Vec<(f64, f64)>,
	) -> Result<(Material, (f32, f32), (f32, f32), String), ShaderError> {
		let calculated =
			PolygonFastPrecalculator::calc("polygon".to_owned(), vec_to_multipolygon(vec));
		let offset = (
			calculated.bounding_rect.min().x as f32,
			calculated.bounding_rect.min().y as f32,
		);
		let size = (
			calculated.bounding_rect.width() as f32,
			calculated.bounding_rect.height() as f32,
		);
		let text = format!("{}", calculated);
		let material = load_material(
			Self::VERTEX_SHADER,
			&format!(
				"{}\n{}\n{}",
				Self::FRAGMENT_SHADER_BEFORE,
				text,
				Self::FRAGMENT_SHADER_AFTER
			),
			MaterialParams {
				uniforms: vec![
					("camera".to_owned(), UniformType::Mat4),
					("resolution".to_owned(), UniformType::Float2),
					("offset".to_owned(), UniformType::Float2),
					("size".to_owned(), UniformType::Float2),
					("show_grid".to_owned(), UniformType::Int1),
				],
				..Default::default()
			},
		)?;
		Ok((material, offset, size, text))
	}

	fn update(&mut self) {
		if self.update_points {
			match Self::parse_points(&self.points) {
				Ok(vec) => {
					let (material, offset, size, text) = Self::calc_material(vec).unwrap();
					self.material = material;
					self.offset = offset;
					self.size = size;
					self.text = text;
					self.error = None;
				},
				Err(message) => {
					self.error = Some(message);
				},
			}
			self.update_points = false;
		}
	}
}

fn window_conf() -> Conf {
	Conf {
		window_title: "Portal visualization".to_owned(),
		high_dpi: true,
		..Default::default()
	}
}

#[macroquad::main(window_conf)]
async fn main() {
	let mut cam = RotateAroundCam::new();

	let init = vec![
		(3.1, 3.4),
		(29.0, 10.0),
		(0.0, 19.9),
		(16.5, 31.3),
		(27.1, 22.6),
		(17.7, 0.0),
		(16.7, 24.0),
		(5.5, 10.8),
		(19.4, 11.3),
	];

	let mut shader = PolygonShader::new(init);
	let mut closed = false;

	loop {
		let mut mouse_over_canvas = true;
		draw_window(
			hash!(),
			vec2(20., 20.),
			vec2(150., 270.),
			WindowParams {
				label: "Coordinates".to_string(),
				close_button: false,
				..Default::default()
			},
			|ui| {
				mouse_over_canvas &=
					!ui.is_mouse_over(Vector2::new(mouse_position().0, mouse_position().1));

				ui.editbox(
					hash!(),
					megaui::Vector2::new(140., 200.),
					&mut shader.points,
				);
				if ui.button(None, "Update") {
					shader.update_points = true;
					closed = false;
				}
				ui.same_line(0.0);
				if ui.button(None, "Show grid") {
					shader.show_grid = !shader.show_grid;
				}

				if let Some(mut error) = shader.error.clone() {
					if !closed {
						closed = !draw_window(
							hash!(),
							vec2(200., 200.),
							vec2(400., 100.),
							WindowParams {
								label: "Error message".to_string(),
								close_button: true,
								..Default::default()
							},
							|ui| {
								mouse_over_canvas &= !ui.is_mouse_over(Vector2::new(
									mouse_position().0,
									mouse_position().1,
								));

								ui.editbox(hash!(), megaui::Vector2::new(495., 280.), &mut error);
							},
						);
					}
				} else {
					closed = false;
				}
			},
		);

		draw_window(
			hash!(),
			vec2(20., 310.),
			vec2(500., 300.),
			WindowParams {
				label: "Shader code".to_string(),
				close_button: false,
				..Default::default()
			},
			|ui| {
				mouse_over_canvas &=
					!ui.is_mouse_over(Vector2::new(mouse_position().0, mouse_position().1));

				ui.editbox(hash!(), megaui::Vector2::new(495., 280.), &mut shader.text);
			},
		);

		if mouse_over_canvas {
			cam.process_mouse_and_keys();
		}

		shader
			.material
			.set_uniform("resolution", (screen_width(), screen_height()));
		shader.material.set_uniform("camera", cam.get_matrix());
		shader.material.set_uniform("offset", shader.offset);
		shader.material.set_uniform("size", shader.size);
		shader
			.material
			.set_uniform("show_grid", shader.show_grid as i32);

		clear_background(BLACK);
		gl_use_material(shader.material);
		draw_rectangle(0., 0., screen_width(), screen_height(), WHITE);
		gl_use_default_material();

		shader.update();

		draw_megaui();

		next_frame().await;
	}
}
