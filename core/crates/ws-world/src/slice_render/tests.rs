use super::*;
use proptest::prelude::*;

fn point(x: i64, y: i64) -> GeoPoint {
    GeoPoint {
        lon_pdeg: x,
        lat_pdeg: y,
    }
}
fn twice_area(p: &[GeoPoint]) -> i128 {
    p.windows(2)
        .map(|w| {
            i128::from(w[0].lon_pdeg) * i128::from(w[1].lat_pdeg)
                - i128::from(w[1].lon_pdeg) * i128::from(w[0].lat_pdeg)
        })
        .sum::<i128>()
        .abs()
}

#[test]
fn concave_roofs_use_source_order_ears_and_preserve_exact_area() {
    let p = [
        point(0, 0),
        point(4, 0),
        point(4, 4),
        point(2, 2),
        point(0, 4),
        point(0, 0),
    ];
    let t = triangulate(&p).unwrap();
    assert_eq!(t, [[1, 2, 3], [0, 1, 3], [0, 3, 4]]);
    assert_eq!(
        t.iter()
            .map(|t| cross(p[t[0]], p[t[1]], p[t[2]]))
            .sum::<i128>(),
        twice_area(&p)
    );
    let mut reversed = p;
    reversed.reverse();
    let t = triangulate(&reversed).unwrap();
    assert_eq!(
        t.iter()
            .map(|t| cross(reversed[t[0]], reversed[t[1]], reversed[t[2]]))
            .sum::<i128>(),
        twice_area(&p)
    );
}

#[test]
fn invalid_topology_is_rejected_and_collinear_roof_edges_keep_area() {
    for p in [
        vec![],
        vec![point(0, 0); 4],
        vec![point(0, 0), point(1, 0), point(1, 1), point(0, 1)],
        vec![
            point(0, 0),
            point(4, 4),
            point(0, 4),
            point(4, 0),
            point(0, 0),
        ],
        vec![
            point(0, 0),
            point(4, 0),
            point(4, 4),
            point(4, 0),
            point(0, 0),
        ],
    ] {
        assert_eq!(triangulate(&p), Err(RenderError::Geometry));
    }
    let p = [
        point(0, 0),
        point(2, 0),
        point(4, 0),
        point(4, 4),
        point(0, 4),
        point(0, 0),
    ];
    let t = triangulate(&p).unwrap();
    assert_eq!(t.len(), 2);
    assert_eq!(
        t.iter()
            .map(|t| cross(p[t[0]], p[t[1]], p[t[2]]))
            .sum::<i128>(),
        twice_area(&p)
    );
}

#[test]
fn all_source_footprints_have_complete_bounded_roof_and_wall_coverage() {
    let world = SliceWorld::new(71449).unwrap();
    let before = world.clone();
    let g = Geometry::new(&world).unwrap();
    assert_eq!(world, before);
    assert_eq!(
        (g.buildings.len(), g.vertices.len(), g.indices.len()),
        (58, 2245, 3693)
    );
    assert_eq!(g.buildings.iter().filter(|b| b.entity.is_some()).count(), 4);
    assert!(g.buildings.windows(2).all(|p| p[0].bin < p[1].bin));
    let (mut vertex_end, mut index_end) = (0, 0);
    for row in &g.buildings {
        let source = BLOCK_BUILDINGS
            .iter()
            .find(|b| u64::from(b.bin) == row.bin)
            .unwrap();
        let ring = source.footprint.polygons[0].rings[0].points;
        let n = ring.len() - 1;
        assert_eq!(row.entity.is_some(), source.featured.is_some());
        assert_eq!((row.first_vertex, row.first_index), (vertex_end, index_end));
        assert_eq!(
            (row.vertex_count, row.index_count),
            (n as u32 * 5, (n as u32 - 2) * 3 + n as u32 * 6)
        );
        assert_eq!(row.height_m, f64::from(source.height_mm) as f32 / 1000.0);
        let triangles = triangulate(ring).unwrap();
        assert_eq!(
            triangles
                .iter()
                .map(|t| cross(ring[t[0]], ring[t[1]], ring[t[2]]))
                .sum::<i128>(),
            twice_area(ring)
        );
        for (i, &p) in ring[..n].iter().enumerate() {
            let expected = geo_to_render(p, f64::from(source.height_mm) / 1000.0).unwrap();
            let actual = g.vertices[row.first_vertex as usize + i].position;
            assert!(
                actual
                    .into_iter()
                    .zip(expected)
                    .all(|(a, b)| (f64::from(a) - b).abs() < 0.0001)
            );
            // A roof uses source above-ground height, not the unrelated NAVD88 ground number.
            let ground = geo_to_render(p, 0.0).unwrap();
            let delta = subtract(expected, ground);
            let length = delta.iter().map(|v| v * v).sum::<f64>().sqrt();
            assert!((length - f64::from(source.height_mm) / 1000.0).abs() < 0.000001);
        }
        for &i in &g.indices[row.first_index as usize..(row.first_index + row.index_count) as usize]
        {
            assert!((row.first_vertex..row.first_vertex + row.vertex_count).contains(&i));
        }
        vertex_end += row.vertex_count;
        index_end += row.index_count;
    }
    assert_eq!(
        (vertex_end as usize, index_end as usize),
        (g.vertices.len(), g.indices.len())
    );
    for v in &g.vertices {
        assert!(v.position.iter().chain(&v.normal).all(|v| v.is_finite()));
        assert!(
            (v.normal
                .iter()
                .map(|v| f64::from(*v) * f64::from(*v))
                .sum::<f64>()
                - 1.0)
                .abs()
                < 0.000001
        );
        for i in 0..3 {
            assert!((g.bounds[i]..=g.bounds[i + 3]).contains(&v.position[i]));
        }
    }
}

#[test]
fn every_native_and_axis_swapped_triangle_agrees_with_its_outward_normal() {
    let g = Geometry::new(&SliceWorld::new(42).unwrap()).unwrap();
    for (triangle, t) in g.indices.chunks_exact(3).enumerate() {
        let [a, b, c] = [t[0], t[1], t[2]].map(|i| g.vertices[i as usize]);
        let xyz = |v: [f32; 3]| v.map(f64::from);
        let ab = subtract(xyz(b.position), xyz(a.position));
        let ac = subtract(xyz(c.position), xyz(a.position));
        let n = normal(ab, ac).unwrap();
        assert!(
            n.iter().zip(a.normal).map(|(x, y)| x * y).sum::<f32>() > 0.99,
            "triangle {triangle} indices {t:?}: {a:?} {b:?} {c:?}; normal {n:?}"
        );
        // Unity axis reflection needs exactly one index reversal.
        let swap = |v: [f32; 3]| [f64::from(v[0]), f64::from(v[2]), f64::from(v[1])];
        let n = normal(
            subtract(swap(c.position), swap(a.position)),
            subtract(swap(b.position), swap(a.position)),
        )
        .unwrap();
        let expected = [a.normal[0], a.normal[2], a.normal[1]];
        assert!(n.iter().zip(expected).map(|(x, y)| x * y).sum::<f32>() > 0.99);
    }
}

#[test]
fn roof_topology_version1_has_an_exact_cross_platform_golden() {
    let mut hash = ws_rng::SipHasher24::new(0, 0x5753_524f_4f46_0001);
    hash.write_words(&[u64::from(VERSION), crate::slice::DATASET_VERSION]);
    for b in BLOCK_BUILDINGS {
        let ring = b.footprint.polygons[0].rings[0].points;
        let triangles = triangulate(ring).unwrap();
        hash.write_words(&[u64::from(b.bin), ring.len() as u64, triangles.len() as u64]);
        for t in triangles {
            hash.write_words(&t.map(|i| i as u64));
        }
    }
    assert_eq!(hash.finish(), 855_534_150_767_469_492); // New version1 source-topology golden.
}

#[test]
fn render_inverse_rejects_nonfinite_and_excessive_inputs() {
    for p in [
        [f64::NAN, 0.0, 0.0],
        [0.0, f64::INFINITY, 0.0],
        [0.0, 0.0, f64::NEG_INFINITY],
        [1_000_001.0, 0.0, 0.0],
    ] {
        assert_eq!(render_to_geo(p), Err(RenderError::Coordinates));
    }
    assert_eq!(
        geo_to_render(point(181_000_000_000_000, 0), 0.0),
        Err(RenderError::Coordinates)
    );
    assert_eq!(heading([0.0; 3], [0.0; 3]).unwrap().to_bits(), 0);
    assert_eq!(heading([0.0; 3], [1.0, 0.0, 0.0]), Ok(0.0));
    assert!(
        (heading([0.0; 3], [0.0, 1.0, 0.0]).unwrap() - core::f32::consts::FRAC_PI_2).abs() < 1e-7
    );
    assert_eq!(
        heading([f64::NAN; 3], [0.0; 3]),
        Err(RenderError::Coordinates)
    );
}

proptest! {
    #[test]
    fn translated_scaled_concave_roofs_conserve_exact_area(x in -1_000_000_000i64..1_000_000_000,y in -1_000_000_000i64..1_000_000_000,s in 1i64..100_000) {
        let p=[point(x,y),point(x+4*s,y),point(x+4*s,y+4*s),point(x+2*s,y+2*s),point(x,y+4*s),point(x,y)];
        let t=triangulate(&p).unwrap();
        prop_assert_eq!(t.iter().map(|t|cross(p[t[0]],p[t[1]],p[t[2]])).sum::<i128>(),twice_area(&p));
    }
    #[test]
    fn block_geodetic_positions_round_trip_without_navigation_axis_shortcuts(index in 0usize..58,height in 0u32..100_000) {
        let p=BLOCK_BUILDINGS[index].centroid;let render=geo_to_render(p,f64::from(height)/1000.0).unwrap();let back=render_to_geo(render).unwrap();
        prop_assert!((back.lon_pdeg-p.lon_pdeg).abs()<100);
        prop_assert!((back.lat_pdeg-p.lat_pdeg).abs()<100);
    }
}
