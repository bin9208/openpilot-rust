use openpilot_cluster::{
    renderer::world::{helpers, obj::ObjMesh},
    scene::{CameraSpec, Vec3},
};

#[test]
fn obj_fans_preserve_negative_indices_normals_and_material_colors(
) -> Result<(), Box<dyn std::error::Error>> {
    let mesh = ObjMesh::parse(
        b"v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvn 0 0 -2\nusemtl body\nf -4//1 -3//1 -2//1 -1//1\n",
    )?;
    assert_eq!(
        mesh.vertices,
        vec![0., 0., 0., 1., 0., 0., 1., 1., 0., 0., 0., 0., 1., 1., 0., 0., 1., 0.]
    );
    assert_eq!(mesh.normals, [0., 0., -2.].repeat(6));
    assert_eq!(mesh.colors, [156, 166, 172, 255].repeat(6));
    Ok(())
}

#[test]
fn obj_ignores_invalid_utf8_and_uses_source_degenerate_normal(
) -> Result<(), Box<dyn std::error::Error>> {
    let mesh = ObjMesh::parse(b"v 0 \xff0 0\nv 0 0 0\nv 0 0 0\nusemtl unknown\nf 1 2 3\n")?;
    assert_eq!(mesh.normals, [0., 0., 1.].repeat(3));
    assert_eq!(mesh.colors, [142, 150, 156, 255].repeat(3));
    assert!(ObjMesh::parse(b"v 0 0 0\nf 1 2 3\n").is_err());
    Ok(())
}

#[test]
fn camera_depth_rejects_invalid_forward_vector() {
    let camera = CameraSpec {
        position: Vec3::new(0., -2., 0.),
        target: Vec3::new(0., 0., 0.),
        fovy_deg: 45.,
    };
    assert_eq!(
        helpers::camera_forward(&camera),
        Some(Vec3::new(0., 1., 0.))
    );
    assert_eq!(
        helpers::camera_depth_m(Vec3::new(2., 4., 3.), &camera),
        Some(6.)
    );
    let degenerate = CameraSpec {
        target: camera.position,
        ..camera
    };
    assert_eq!(helpers::camera_forward(&degenerate), None);
    assert_eq!(
        helpers::camera_depth_m(Vec3::new(f64::NAN, 0., 0.), &camera),
        None
    );
    assert_eq!(helpers::world_label_scale(18.).to_bits(), 1.0_f64.to_bits());
    assert_eq!(
        helpers::world_label_scale(180.).to_bits(),
        0.56_f64.to_bits()
    );
}

#[test]
fn label_rectangle_bounds_preserve_strict_overlap() {
    use openpilot_cluster::renderer::{world::helpers::Bounds, Rect};
    assert!(!helpers::rectangles_overlap(
        Rect::new(0., 0., 2., 2.),
        Rect::new(2., 0., 2., 2.)
    ));
    assert!(helpers::rectangles_overlap(
        Rect::new(0., 0., 2., 2.),
        Rect::new(1., 0., 2., 2.)
    ));
    assert!(helpers::label_rect_inside_bounds(
        Rect::new(0., 0., 2., 2.),
        Bounds::new(0., 0., 2., 2.)
    ));
    assert!(!helpers::label_rect_inside_bounds(
        Rect::new(f64::NAN, 0., 2., 2.),
        Bounds::new(0., 0., 2., 2.)
    ));
}
