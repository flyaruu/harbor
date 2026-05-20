use geo::{Coord, LineString, MultiPolygon, Polygon};

pub(crate) fn decode_feature_polygons(
    commands: &[u32],
) -> std::result::Result<MultiPolygon<f64>, String> {
    let rings = decode_polygon_rings(commands)?;
    group_rings_into_multipolygon(&rings)
}

fn read_point_delta(
    commands: &[u32],
    cursor: &mut usize,
) -> std::result::Result<(i32, i32), String> {
    if *cursor + 1 >= commands.len() {
        return Err("truncated point delta".to_string());
    }

    let dx = zigzag_decode(commands[*cursor]);
    let dy = zigzag_decode(commands[*cursor + 1]);
    *cursor += 2;
    Ok((dx, dy))
}

fn zigzag_decode(value: u32) -> i32 {
    ((value >> 1) as i32) ^ (-((value & 1) as i32))
}

fn decode_polygon_rings(commands: &[u32]) -> std::result::Result<Vec<Vec<Coord<f64>>>, String> {
    let mut cursor = 0usize;
    let mut x = 0i32;
    let mut y = 0i32;
    let mut current_ring: Vec<Coord<f64>> = Vec::new();
    let mut rings = Vec::new();

    while cursor < commands.len() {
        let command = commands[cursor];
        cursor += 1;

        let command_id = command & 0x7;
        let count = command >> 3;

        match command_id {
            1 => {
                if !current_ring.is_empty() {
                    return Err("encountered MoveTo before closing polygon ring".to_string());
                }
                if count != 1 {
                    return Err(format!("polygon MoveTo expected count=1, found {count}"));
                }

                let (dx, dy) = read_point_delta(commands, &mut cursor)?;
                x = x.wrapping_add(dx);
                y = y.wrapping_add(dy);
                current_ring.push(Coord {
                    x: x as f64,
                    y: y as f64,
                });
            }
            2 => {
                if current_ring.is_empty() {
                    return Err("LineTo encountered before polygon MoveTo".to_string());
                }

                for _ in 0..count {
                    let (dx, dy) = read_point_delta(commands, &mut cursor)?;
                    x = x.wrapping_add(dx);
                    y = y.wrapping_add(dy);
                    current_ring.push(Coord {
                        x: x as f64,
                        y: y as f64,
                    });
                }
            }
            7 => {
                if current_ring.len() < 3 {
                    return Err("polygon ring contained fewer than 3 vertices".to_string());
                }
                close_ring(&mut current_ring);
                rings.push(current_ring);
                current_ring = Vec::new();
            }
            other => return Err(format!("unsupported geometry command {other}")),
        }
    }

    if !current_ring.is_empty() {
        return Err("polygon ring missing ClosePath".to_string());
    }

    Ok(rings)
}

fn close_ring(ring: &mut Vec<Coord<f64>>) {
    if ring.first() != ring.last() {
        ring.push(ring[0]);
    }
}

fn group_rings_into_multipolygon(
    rings: &[Vec<Coord<f64>>],
) -> std::result::Result<MultiPolygon<f64>, String> {
    let mut polygons = Vec::new();
    let mut exterior_sign = 0.0f64;
    let mut current_exterior: Option<Vec<Coord<f64>>> = None;
    let mut current_holes: Vec<Vec<Coord<f64>>> = Vec::new();

    for ring in rings {
        let area = ring_signed_area(ring);
        if area.abs() < f64::EPSILON {
            continue;
        }

        let sign = area.signum();
        if exterior_sign == 0.0 {
            exterior_sign = sign;
        }

        if sign == exterior_sign {
            if let Some(exterior) = current_exterior.replace(ring.clone()) {
                polygons.push(build_polygon(exterior, std::mem::take(&mut current_holes))?);
            }
        } else if current_exterior.is_some() {
            current_holes.push(ring.clone());
        } else {
            return Err("polygon hole encountered before exterior ring".to_string());
        }
    }

    if let Some(exterior) = current_exterior {
        polygons.push(build_polygon(exterior, current_holes)?);
    }

    Ok(MultiPolygon(polygons))
}

fn build_polygon(
    exterior: Vec<Coord<f64>>,
    holes: Vec<Vec<Coord<f64>>>,
) -> std::result::Result<Polygon<f64>, String> {
    if exterior.len() < 4 {
        return Err("polygon exterior ring contained too few vertices".to_string());
    }

    let exterior = LineString::new(exterior);
    let interiors = holes
        .into_iter()
        .filter(|ring| ring.len() >= 4)
        .map(LineString::new)
        .collect();
    Ok(Polygon::new(exterior, interiors))
}

fn ring_signed_area(ring: &[Coord<f64>]) -> f64 {
    ring.windows(2)
        .map(|segment| segment[0].x * segment[1].y - segment[1].x * segment[0].y)
        .sum::<f64>()
        * 0.5
}
