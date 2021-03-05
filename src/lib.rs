use std::fmt;

use geo::{
	line_string,
	map_coords::MapCoordsInplace,
	prelude::{BoundingRect, Centroid, EuclideanLength, SimplifyVW},
	Line, LineString, MultiPolygon, Point, Polygon, Rect,
};
use geo_clipper::Clipper;
use itertools::Itertools;
use line_intersection::LineInterval;
use ordered_float::NotNan;

#[derive(Clone, Debug, Copy)]
pub enum LineSplitCheck {
	MulToX { k: f64, b: f64 },
	MulToY { k: f64, b: f64 },
}

impl LineSplitCheck {
	pub fn calc(line: Line<f64>) -> Self {
		let dx = line.dx();
		let dy = line.dy();
		if dy.abs() < dx.abs() {
			let k = dy / dx;
			let b = line.start_point().y() - k * line.start_point().x();
			LineSplitCheck::MulToX { k, b }
		} else {
			let k = dx / dy;
			let b = line.start_point().x() - k * line.start_point().y();
			LineSplitCheck::MulToY { k, b }
		}
	}

	pub fn less_count(&self, point: Point<f64>) -> (bool, f64) {
		let result = match self {
			LineSplitCheck::MulToX { k, b } => point.y() - (point.x() * k + b),
			LineSplitCheck::MulToY { k, b } => point.x() - (point.y() * k + b),
		};
		(result < 0., result)
	}

	pub fn is_less(&self, point: Point<f64>) -> bool {
		self.less_count(point).0
	}
}

#[derive(Clone, Debug, Copy)]
pub enum LineSplitCheckGeneralized {
	Less(LineSplitCheck),
	Greater(LineSplitCheck),
}

impl LineSplitCheckGeneralized {
	pub fn check(&self, point: Point<f64>) -> bool {
		use LineSplitCheckGeneralized::*;
		match self {
			Less(check) => check.is_less(point),
			Greater(check) => !check.is_less(point),
		}
	}
}

#[derive(Clone, Debug)]
pub enum PolygonFastPrecalculatorPart {
	LineSplit {
		check: LineSplitCheck,
		less: Box<PolygonFastPrecalculatorPart>,
		greater: Box<PolygonFastPrecalculatorPart>,
	},
	Triangle {
		checks: [LineSplitCheckGeneralized; 3],
	},
	None,
}

impl PolygonFastPrecalculatorPart {
	pub fn calc(mut polygon: MultiPolygon<f64>) -> Self {
		// Simplify figure
		polygon = polygon.simplifyvw(&0.0001);

		// Remove figures that easier than triangle
		polygon = MultiPolygon(
			polygon
				.0
				.into_iter()
				.filter(|poly| poly.exterior().points_iter().count() > 3)
				.collect(),
		);

		// There is no figures
		if polygon.0.len() == 0 {
			return Self::None;
		}

		// This is figure that easier than triangle
		if polygon
			.0
			.iter()
			.all(|poly| poly.exterior().points_iter().count() < 4)
		{
			return Self::None;
		}

		// This is triangle
		if polygon.0.len() == 1 && polygon.0[0].exterior().points_iter().count() == 4 {
			let center = polygon.centroid().unwrap();

			let checks = polygon
				.0
				.iter()
				.map(|poly| poly.exterior().lines())
				.flatten()
				.map(|line| LineSplitCheck::calc(line))
				.map(|check| {
					if check.is_less(center) {
						LineSplitCheckGeneralized::Less(check)
					} else {
						LineSplitCheckGeneralized::Greater(check)
					}
				})
				.collect::<Vec<_>>();
			assert_eq!(checks.len(), 3);
			return Self::Triangle {
				checks: [checks[0], checks[1], checks[2]],
			};
		}

		// This is more complex figure that should be reduced to triangle

		let br = polygon.bounding_rect().unwrap().to_polygon();

		let all_points = polygon
			.0
			.iter()
			.map(|poly| poly.exterior().points_iter())
			.flatten()
			.collect::<Vec<_>>();

		let all_lines = all_points
			.iter()
			.cartesian_product(all_points.iter())
			.map(|(start, end)| Line::new(*start, *end));

		// For all lines in current figure, find best line that cut current polygons into 2 equivalent figures.
		let best = all_lines
			.map(|line| {
				let interval = LineInterval::line(line);

				let mut polygon1 = Vec::new();
				let mut polygon2 = Vec::new();
				let mut intersection_count = 0;
				for segment in br.exterior().lines() {
					if let Some(point) = LineInterval::line_segment(segment)
						.relate(&interval)
						.unique_intersection()
					{
						if intersection_count == 0 {
							polygon1.push(point);
							polygon2.push(point);
							polygon2.push(segment.end_point());
							intersection_count = 1;
						} else if intersection_count == 1 {
							polygon2.push(point);
							polygon1.push(point);
							polygon1.push(segment.end_point());
							intersection_count = 2;
						}
					} else {
						if intersection_count == 0 || intersection_count == 2 {
							polygon1.push(segment.end_point());
						} else if intersection_count == 1 {
							polygon2.push(segment.end_point());
						}
					}
				}

				let polygon1 = Polygon::new(LineString::from(polygon1), vec![]);
				let polygon2 = Polygon::new(LineString::from(polygon2), vec![]);

				let result1 = polygon.intersection(&polygon1, 600000000.0);
				let result2 = polygon.intersection(&polygon2, 600000000.0);

				(line, result1, result2)
			})
			.filter(|(line, _, _)| {
				line_string![line.start_point().into(), line.end_point().into()].euclidean_length()
					> 0.0001
			})
			.filter_map(|(line, result1, result2)| {
				pub fn mymax(a: f64, b: f64) -> f64 {
					if a > b { a } else { b }
				}

				// Metric by points count (works good)
				let a1 = result1
					.iter()
					.map(|poly| poly.exterior().points_iter())
					.flatten()
					.count() as f64;
				let a2 = result2
					.iter()
					.map(|poly| poly.exterior().points_iter())
					.flatten()
					.count() as f64;

				// Metric by area (works bad)
				/*
				let a1 = result1.unsigned_area();
				let a2 = result2.unsigned_area();
				*/

				if a1 != 0.0 && a2 != 0.0 {
					let current_val = mymax(a1 as f64 / a2 as f64, a2 as f64 / a1 as f64);
					Some((line, result1, result2, current_val))
				} else {
					// For complicated triangles like Polygon::new(LineString::from(vec![Coordinate {x: 0.0, y: 0.6357827466666667, }, Coordinate {x: 0.7843839333333333, y: 0.38768687, }, Coordinate {x: 0.7245766583333333, y: 0.25446109, }, Coordinate {x: 1.0, y: 0.3194888166666667, }, Coordinate {x: 0.0, y: 0.6357827466666667, }, ]), vec![], );
					if (a1 != 0.0 || a2 != 0.0) && a1 + a2 < all_points.len() as f64 {
						Some((line, result1, result2, 1e100))
					} else {
						None
					}
				}
			})
			.min_by_key(|(_, _, _, val)| {
				NotNan::new(*val)
					.unwrap_or_else(|_| panic!("can't find delimiter line:\n{:#?}", polygon))
			});

		let mut best = best.unwrap_or_else(|| panic!("can't find delimiter line:\n{:#?}", polygon));

		// For safe best finding
		/*
		let mut best = if let Some(best) = best {
			best
		} else {
			return Self::None;
		};
		*/

		let check = LineSplitCheck::calc(best.0);

		// Find most far pont and check if this point is less than provided check, i.e. check is this polygon really lies on `less` part of this check
		let should_swap = best
			.1
			.0
			.iter()
			.map(|poly| poly.exterior().points_iter())
			.flatten()
			.map(|point| check.less_count(point))
			.max_by_key(|(_, val)| NotNan::new(val.abs()).unwrap())
			.map(|(result, _)| !result)
			.unwrap_or(false);

		if should_swap {
			std::mem::swap(&mut best.1, &mut best.2);
		}

		Self::LineSplit {
			check,
			less: Box::new(Self::calc(best.1)),
			greater: Box::new(Self::calc(best.2)),
		}
	}
}

#[derive(Clone, Debug)]
pub struct PolygonFastPrecalculator {
	pub name: String,
	pub bounding_rect: Rect<f64>,
	pub parts: PolygonFastPrecalculatorPart,
}

impl PolygonFastPrecalculator {
	pub fn calc(name: String, mut polygon: MultiPolygon<f64>) -> Self {
		let br = polygon.bounding_rect().unwrap();
		polygon.map_coords_inplace(|&(x, y)| {
			let r = fit_point_into_default_borders(Point::new(x, y), &br);
			(r.x(), r.y())
		});
		Self {
			name,
			bounding_rect: br,
			parts: PolygonFastPrecalculatorPart::calc(polygon),
		}
	}

	pub fn is_inside(&self, mut point: Point<f64>) -> bool {
		fn is_inside_inner(check: &PolygonFastPrecalculatorPart, point: Point<f64>) -> bool {
			use PolygonFastPrecalculatorPart::*;
			match check {
				LineSplit {
					check,
					less,
					greater,
				} => {
					if check.is_less(point) {
						is_inside_inner(&**less, point)
					} else {
						is_inside_inner(&**greater, point)
					}
				},
				Triangle { checks } => checks.iter().all(|c| c.check(point)),
				None => return false,
			}
		}

		point = fit_point_into_default_borders(point, &self.bounding_rect);

		if !(0. <= point.x() && point.x() <= 1.) {
			return false;
		}
		if !(0. <= point.y() && point.y() <= 1.) {
			return false;
		}

		is_inside_inner(&self.parts, point)
	}
}

pub fn fit_point_into_default_borders(
	mut point: Point<f64>,
	bounding_rect: &Rect<f64>,
) -> Point<f64> {
	point.set_x((point.x() - bounding_rect.min().x) / bounding_rect.width());
	point.set_y((point.y() - bounding_rect.min().y) / bounding_rect.height());
	point
}

impl fmt::Display for LineSplitCheck {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		use LineSplitCheck::*;
		match self {
			MulToX { k, b } => write!(f, "a.y < a.x * {:e} + ({:e})", k, b),
			MulToY { k, b } => write!(f, "a.x < a.y * {:e} + ({:e})", k, b),
		}
	}
}

impl fmt::Display for LineSplitCheckGeneralized {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		use LineSplitCheckGeneralized::*;
		match self {
			Less(check) => write!(f, "({})", check),
			Greater(check) => write!(f, "!({})", check),
		}
	}
}

impl fmt::Display for PolygonFastPrecalculator {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		#[derive(Clone, Copy)]
		struct Tab(pub i32);

		impl fmt::Display for Tab {
			fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
				for _ in 0..self.0 {
					write!(f, " ")?;
				}
				Ok(())
			}
		}

		fn print_inner(
			check: &PolygonFastPrecalculatorPart,
			mut deep: Tab,
			f: &mut fmt::Formatter<'_>,
		) -> fmt::Result {
			#[rustfmt::skip]
            macro_rules! out { ($($a:tt)*) => { write!(f, "{}", deep)?; writeln!(f, $($a)*)?; }; }
			#[rustfmt::skip]
            macro_rules! inner { ($($a:tt)*) => { deep.0 += 2; { $($a)* } deep.0 -= 2; }; }

			use PolygonFastPrecalculatorPart::*;
			match check {
				LineSplit {
					check,
					less,
					greater,
				} => {
					out!("if ({}) {{", check);
					inner! {
						print_inner(less, deep, f)?;
					}
					out!("}} else {{");
					inner! {
						print_inner(greater, deep, f)?;
					}
					out!("}}");
				},
				Triangle { checks } => {
					out!("return {} && {} && {};", checks[0], checks[1], checks[2]);
				},
				None => {
					out!("return false;");
				},
			}
			Ok(())
		}

		let mut deep = Tab(0);

		#[rustfmt::skip]
        macro_rules! out { ($($a:tt)*) => { write!(f, "{}", deep)?; writeln!(f, $($a)*)?; }; }
		#[rustfmt::skip]
        macro_rules! inner { ($($a:tt)*) => { deep.0 += 2; { $($a)* } deep.0 -= 2; }; }

		out!("bool is_inside_{}(vec2 a) {{", self.name);
		inner! {
			out!("a = (a - vec2({:e}, {:e})) / vec2({:e}, {:e});", self.bounding_rect.min().x, self.bounding_rect.min().y, self.bounding_rect.width(), self.bounding_rect.height());
			out!("if (0. <= a.x && a.x <= 1. && 0. <= a.y && a.y <= 1.) {{");
			inner! {
				print_inner(&self.parts, deep, f)?;
			}
			out!("}} else {{");
			inner! {
				out!("return false;");
			}
			out!("}}");
		}
		out!("}}");

		Ok(())
	}
}

pub fn vec_to_multipolygon(array: Vec<(f64, f64)>) -> MultiPolygon<f64> {
	MultiPolygon::from(vec![Polygon::new(LineString::from(array), vec![])])
}
