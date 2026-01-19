//! Camera orbit, pan, and zoom systems.

use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;

use crate::visualization::resources::CameraOrbit;

/// Calculate camera position from orbit parameters.
pub fn calculate_camera_position(orbit: &CameraOrbit) -> Vec3 {
    let x = orbit.distance * orbit.pitch.cos() * orbit.yaw.sin();
    let y = orbit.distance * orbit.pitch.sin();
    let z = orbit.distance * orbit.pitch.cos() * orbit.yaw.cos();
    orbit.target + Vec3::new(x, y, z)
}

/// Camera orbit control system.
///
/// Controls:
/// - Right-click drag: Orbit around target
/// - Middle-click drag (or Alt+right-click): Pan
/// - Scroll wheel: Zoom
/// - WASD: Pan horizontally
/// - Q/E: Pan vertically
/// - R: Reset view
pub fn camera_orbit_system(
    mut orbit: ResMut<CameraOrbit>,
    mut camera_query: Query<&mut Transform, With<Camera3d>>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut mouse_motion: EventReader<MouseMotion>,
    mut scroll: EventReader<MouseWheel>,
) {
    // Orbit on right-click drag (left-click is for node dragging)
    // Skip if Alt/Option is held (that's for panning)
    let alt_held = keyboard.pressed(KeyCode::AltLeft) || keyboard.pressed(KeyCode::AltRight);
    if mouse_button.pressed(MouseButton::Right) && !alt_held {
        for ev in mouse_motion.read() {
            orbit.yaw -= ev.delta.x * 0.01;
            orbit.pitch += ev.delta.y * 0.01;
            orbit.pitch = orbit.pitch.clamp(-1.5, 1.5);
        }
    }

    // Pan with middle-click drag OR Option/Alt + right-click (for Mac trackpads)
    let is_panning = mouse_button.pressed(MouseButton::Middle)
        || (mouse_button.pressed(MouseButton::Right) && alt_held);

    if is_panning {
        for ev in mouse_motion.read() {
            // Pan perpendicular to view direction
            let right = Vec3::new(orbit.yaw.cos(), 0.0, -orbit.yaw.sin());
            let up = Vec3::Y;
            orbit.target += right * ev.delta.x * 0.02;
            orbit.target -= up * ev.delta.y * 0.02;
        }
    }

    // Zoom on scroll
    for ev in scroll.read() {
        orbit.distance -= ev.y * 1.0;
        orbit.distance = orbit.distance.clamp(2.0, 100.0);
    }

    // WASD for panning
    let pan_speed = 0.2;
    let forward = Vec3::new(orbit.yaw.sin(), 0.0, orbit.yaw.cos());
    let right = Vec3::new(orbit.yaw.cos(), 0.0, -orbit.yaw.sin());

    if keyboard.pressed(KeyCode::KeyW) {
        orbit.target += forward * pan_speed;
    }
    if keyboard.pressed(KeyCode::KeyS) {
        orbit.target -= forward * pan_speed;
    }
    if keyboard.pressed(KeyCode::KeyA) {
        orbit.target -= right * pan_speed;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        orbit.target += right * pan_speed;
    }
    if keyboard.pressed(KeyCode::KeyQ) {
        orbit.target.y -= pan_speed;
    }
    if keyboard.pressed(KeyCode::KeyE) {
        orbit.target.y += pan_speed;
    }

    // Reset view with R
    if keyboard.just_pressed(KeyCode::KeyR) {
        orbit.yaw = 0.0;
        orbit.pitch = 0.3;
        orbit.distance = 25.0;
        orbit.target = Vec3::ZERO;
    }

    // Update camera transform
    if let Ok(mut transform) = camera_query.get_single_mut() {
        let pos = calculate_camera_position(&orbit);
        *transform = Transform::from_translation(pos).looking_at(orbit.target, Vec3::Y);
    }
}
