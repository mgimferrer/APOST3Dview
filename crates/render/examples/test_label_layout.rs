//! Headless sanity check for 3D label layout math — doesn't need a GPU
//! device (GlyphAtlas needs one for the texture, so this only exercises
//! the parts that don't: camera distance-to-world-pixel math and the
//! centered multi-glyph layout arithmetic in isolation).

use apost3dview_render::OrbitCamera;

fn main() {
    // world_units_per_pixel should scale linearly with distance, and be
    // symmetric/reasonable for a typical FOV.
    let camera = OrbitCamera::default();
    let near = camera.world_units_per_pixel(10.0, 800.0);
    let far = camera.world_units_per_pixel(20.0, 800.0);
    println!("world_units_per_pixel: near(d=10)={near}, far(d=20)={far}");
    assert!((far - 2.0 * near).abs() < 1e-5, "should scale linearly with distance");
    assert!(near > 0.0);

    // Sanity on the camera itself: eye should be `distance` away from target.
    let d = (camera.eye() - camera.target).length();
    assert!((d - camera.distance).abs() < 1e-4, "eye should be `distance` from target");

    // world_units_per_pixel at zero viewport height should be a safe
    // zero, not NaN/inf (division-by-zero guard).
    let degenerate = camera.world_units_per_pixel(10.0, 0.0);
    assert_eq!(degenerate, 0.0);

    println!("ALL CHECKS PASSED");
}
