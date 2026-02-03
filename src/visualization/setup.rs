//! Scene setup and material creation for the graph visualization.

use bevy::prelude::*;
use bevy::ui::PositionType;
use std::collections::{HashMap, HashSet};

use crate::visualization::components::{
    EdgeArrow, EdgeHotspot, GraphEdge, GraphNode, InfoPanel, InfoPanelText, NodeLabel,
};
use crate::visualization::constants::{
    edge_color_for_relationship, node_color_for_scope, BASE_NODE_RADIUS, COLOR_EDGE_DEFAULT,
    COLOR_NODE_DEFAULT, COLOR_START, MAX_NODE_RADIUS, MIN_NODE_RADIUS, SCOPE_NAMES,
};
use crate::visualization::graph::NodeType;
use crate::visualization::resources::{CameraOrbit, GraphLayoutRes, NodeMaterials};
use crate::visualization::systems::camera::calculate_camera_position;

/// Setup the scene with camera, lighting, and graph nodes.
pub fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    layout: Res<GraphLayoutRes>,
    orbit: Res<CameraOrbit>,
) {
    // Camera
    let camera_pos = calculate_camera_position(&orbit);
    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(camera_pos).looking_at(orbit.target, Vec3::Y),
    ));

    // Main directional light (sun-like)
    commands.spawn((
        DirectionalLight {
            illuminance: 20000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(10.0, 20.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Secondary fill light from opposite side
    commands.spawn((
        DirectionalLight {
            illuminance: 8000.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(-8.0, 10.0, -8.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Point lights around the scene for metallic reflections
    let point_light_positions = [
        Vec3::new(8.0, 5.0, 8.0),
        Vec3::new(-8.0, 5.0, 8.0),
        Vec3::new(8.0, 5.0, -8.0),
        Vec3::new(-8.0, 5.0, -8.0),
        Vec3::new(0.0, 12.0, 0.0),
    ];
    for pos in point_light_positions {
        commands.spawn((
            PointLight {
                intensity: 500000.0,
                color: Color::WHITE,
                shadows_enabled: false,
                range: 50.0,
                ..default()
            },
            Transform::from_translation(pos),
        ));
    }

    // Ambient light
    commands.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 300.0,
    });

    // Create scope-based node materials
    let mut scope_materials: HashMap<String, (Handle<StandardMaterial>, Handle<StandardMaterial>)> =
        HashMap::new();

    // Build materials for each scope + a default for nodes without scope
    let scope_entries: Vec<(&str, Color)> = SCOPE_NAMES
        .iter()
        .map(|&s| (s, node_color_for_scope(Some(s))))
        .chain(std::iter::once(("_DEFAULT", COLOR_NODE_DEFAULT)))
        .collect();

    for (scope_name, color) in &scope_entries {
        let [r, g, b] = color.to_srgba().to_f32_array_no_alpha();

        let normal = materials.add(StandardMaterial {
            base_color: *color,
            metallic: 0.3,
            perceptual_roughness: 0.5,
            reflectance: 0.3,
            emissive: LinearRgba::BLACK,
            ..default()
        });
        let glow = materials.add(StandardMaterial {
            base_color: *color,
            metallic: 0.5,
            perceptual_roughness: 0.3,
            reflectance: 0.5,
            emissive: LinearRgba::new(r * 1.5, g * 1.5, b * 1.5, 1.0),
            ..default()
        });
        scope_materials.insert(scope_name.to_string(), (normal, glow));
    }

    let start_normal = materials.add(StandardMaterial {
        base_color: COLOR_START,
        metallic: 0.5,
        perceptual_roughness: 0.4,
        reflectance: 0.5,
        emissive: LinearRgba::new(0.15, 0.15, 0.2, 1.0), // Slight glow to stand out
        ..default()
    });
    let start_glow = materials.add(StandardMaterial {
        base_color: COLOR_START,
        metallic: 0.6,
        perceptual_roughness: 0.2,
        reflectance: 0.6,
        emissive: LinearRgba::new(1.2, 1.2, 1.4, 1.0),
        ..default()
    });

    // Create edge materials for each relationship type
    let relationship_types = [
        "BELONGS_TO",
        "CALLS",
        "IMPORTS",
        "IMPLEMENTS",
        "INSTANTIATES",
        "RELATED_TO",
    ];

    let mut edge_materials = HashMap::new();
    for rel_type in relationship_types {
        let color = edge_color_for_relationship(rel_type);
        // Extract RGB components for emissive (same hue, just glowing)
        let [r, g, b] = color.to_srgba().to_f32_array_no_alpha();

        let normal = materials.add(StandardMaterial {
            base_color: color,
            metallic: 0.3,
            perceptual_roughness: 0.6,
            reflectance: 0.3,
            emissive: LinearRgba::BLACK,
            ..default()
        });
        // Glow version: same color with strong emissive
        let glow = materials.add(StandardMaterial {
            base_color: color,
            metallic: 0.4,
            perceptual_roughness: 0.4,
            reflectance: 0.4,
            emissive: LinearRgba::new(r * 2.5, g * 2.5, b * 2.5, 1.0),
            ..default()
        });
        edge_materials.insert(rel_type.to_string(), (normal, glow));
    }

    // Default edge material for unknown types
    let default_normal = materials.add(StandardMaterial {
        base_color: COLOR_EDGE_DEFAULT,
        metallic: 0.3,
        perceptual_roughness: 0.6,
        reflectance: 0.3,
        emissive: LinearRgba::BLACK,
        ..default()
    });
    let default_glow = materials.add(StandardMaterial {
        base_color: COLOR_EDGE_DEFAULT,
        metallic: 0.4,
        perceptual_roughness: 0.4,
        reflectance: 0.4,
        emissive: LinearRgba::new(1.2, 1.2, 1.2, 1.0),
        ..default()
    });
    edge_materials.insert("_DEFAULT".to_string(), (default_normal, default_glow));

    // Store materials as resource for glow updates
    commands.insert_resource(NodeMaterials {
        scope_materials: scope_materials.clone(),
        start_normal: start_normal.clone(),
        start_glow,
        edge_materials: edge_materials.clone(),
    });

    // Spawn nodes with labels
    let text_style = TextFont {
        font_size: 9.0,
        ..default()
    };
    let text_color = TextColor(Color::srgba(0.85, 0.85, 0.85, 0.7));

    for (idx, node) in layout.0.nodes.iter().enumerate() {
        // Calculate radius based on mass: r = base * sqrt(mass)
        // Using sqrt so area scales linearly with mass
        let radius = (BASE_NODE_RADIUS * node.mass.sqrt()).clamp(MIN_NODE_RADIUS, MAX_NODE_RADIUS);

        // Create mesh dynamically based on calculated radius
        let (mesh, material) = match node.node_type {
            NodeType::StartNode => {
                let mesh = meshes.add(Sphere::new(radius * 1.3).mesh().ico(5).unwrap());
                (mesh, start_normal.clone())
            }
            NodeType::Entity => {
                let mesh = meshes.add(Sphere::new(radius).mesh().ico(4).unwrap());
                let scope_key = node.scope.as_deref().unwrap_or("_DEFAULT");
                let mat = scope_materials
                    .get(scope_key)
                    .or_else(|| scope_materials.get("_DEFAULT"))
                    .map(|(normal, _)| normal.clone())
                    .unwrap();
                (mesh, mat)
            }
        };

        // Spawn node mesh
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::from_translation(node.position),
            GraphNode {
                id: node.id.clone(),
                node_idx: idx,
                radius,
            },
        ));

        // Spawn label as UI text (will be positioned in screen space)
        commands.spawn((
            Text::new(&node.label),
            text_style.clone(),
            text_color,
            bevy::ui::Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            NodeLabel { node_idx: idx },
        ));
    }

    // Spawn edges as thin cylinders with arrowheads showing direction
    let edge_mesh = meshes.add(Cylinder::new(0.05, 1.0));
    let arrow_mesh = meshes.add(Cone::new(0.12, 0.3));

    for edge in &layout.0.edges {
        let from_pos = layout.0.nodes[edge.from_idx].position;
        let to_pos = layout.0.nodes[edge.to_idx].position;

        let midpoint = (from_pos + to_pos) / 2.0;
        let direction = to_pos - from_pos;
        let length = direction.length();

        if length > 0.01 {
            let dir_norm = direction.normalize();
            let rotation = Quat::from_rotation_arc(Vec3::Y, dir_norm);

            // Get material for this relationship type
            let material = edge_materials
                .get(&edge.label)
                .or_else(|| edge_materials.get("_DEFAULT"))
                .map(|(normal, _)| normal.clone())
                .unwrap();

            commands.spawn((
                Mesh3d(edge_mesh.clone()),
                MeshMaterial3d(material.clone()),
                Transform::from_translation(midpoint)
                    .with_rotation(rotation)
                    .with_scale(Vec3::new(1.0, length, 1.0)),
                GraphEdge {
                    from_idx: edge.from_idx,
                    to_idx: edge.to_idx,
                    relationship: edge.label.clone(),
                },
            ));

            // Arrowhead cone at target end, offset by target node radius
            let target_node = &layout.0.nodes[edge.to_idx];
            let target_radius = (BASE_NODE_RADIUS * target_node.mass.sqrt())
                .clamp(MIN_NODE_RADIUS, MAX_NODE_RADIUS);
            let arrow_pos = to_pos - dir_norm * (target_radius + 0.2);

            commands.spawn((
                Mesh3d(arrow_mesh.clone()),
                MeshMaterial3d(material),
                Transform::from_translation(arrow_pos).with_rotation(rotation),
                EdgeArrow {
                    from_idx: edge.from_idx,
                    to_idx: edge.to_idx,
                },
            ));

            // Spawn invisible hotspot for click detection (positioned at edge midpoint in screen space)
            commands.spawn((
                bevy::ui::Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(30.0),
                    height: Val::Px(30.0),
                    ..default()
                },
                EdgeHotspot {
                    from_idx: edge.from_idx,
                    to_idx: edge.to_idx,
                    relationship: edge.label.clone(),
                    note: edge.note.clone(),
                },
            ));
        }
    }

    // Collect unique scopes present in the graph for node legend
    let mut node_scopes: Vec<&str> = layout
        .0
        .nodes
        .iter()
        .filter_map(|n| n.scope.as_deref())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    node_scopes.sort();

    // Spawn node scope legend
    commands
        .spawn((
            bevy::ui::Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(50.0),
                left: Val::Px(10.0),
                padding: UiRect::all(Val::Px(10.0)),
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(20.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.1, 0.1, 0.15, 0.85)),
            BorderRadius::all(Val::Px(6.0)),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Nodes:"),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::srgb(0.6, 0.6, 0.6)),
            ));
            for scope in node_scopes {
                let color = node_color_for_scope(Some(scope));
                parent
                    .spawn(bevy::ui::Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(6.0),
                        ..default()
                    })
                    .with_children(|item| {
                        item.spawn((
                            bevy::ui::Node {
                                width: Val::Px(12.0),
                                height: Val::Px(12.0),
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BackgroundColor(color),
                            BorderColor(Color::srgba(1.0, 1.0, 1.0, 0.3)),
                            BorderRadius::all(Val::Px(6.0)),
                        ));
                        item.spawn((
                            Text::new(scope),
                            TextFont {
                                font_size: 12.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.8, 0.8, 0.8)),
                        ));
                    });
            }
        });

    // Collect unique relationship types for edge legend
    let mut legend_types: Vec<&str> = layout
        .0
        .edges
        .iter()
        .map(|e| e.label.as_str())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    legend_types.sort();

    // Spawn edge legend panel at the bottom
    commands
        .spawn((
            bevy::ui::Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(10.0),
                left: Val::Px(10.0),
                padding: UiRect::all(Val::Px(10.0)),
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(20.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.1, 0.1, 0.15, 0.85)),
            BorderRadius::all(Val::Px(6.0)),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Edges:"),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::srgb(0.6, 0.6, 0.6)),
            ));
            for rel_type in legend_types {
                let color = edge_color_for_relationship(rel_type);
                // Legend item: colored box + label
                parent
                    .spawn(bevy::ui::Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(6.0),
                        ..default()
                    })
                    .with_children(|item| {
                        // Color swatch
                        item.spawn((
                            bevy::ui::Node {
                                width: Val::Px(14.0),
                                height: Val::Px(14.0),
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BackgroundColor(color),
                            BorderColor(Color::srgba(1.0, 1.0, 1.0, 0.3)),
                            BorderRadius::all(Val::Px(2.0)),
                        ));
                        // Label
                        item.spawn((
                            Text::new(rel_type),
                            TextFont {
                                font_size: 12.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.8, 0.8, 0.8)),
                        ));
                    });
            }
        });

    // Spawn info panel on the left
    commands
        .spawn((
            bevy::ui::Node {
                position_type: PositionType::Absolute,
                left: Val::Px(10.0),
                top: Val::Px(10.0),
                width: Val::Px(280.0),
                min_height: Val::Px(100.0),
                padding: UiRect::all(Val::Px(12.0)),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgba(0.1, 0.1, 0.15, 0.9)),
            BorderRadius::all(Val::Px(8.0)),
            InfoPanel,
        ))
        .with_children(|parent| {
            // Panel title
            parent.spawn((
                Text::new("Node Info"),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::srgb(0.9, 0.9, 0.9)),
            ));
            // Panel content (updated dynamically)
            parent.spawn((
                Text::new("Click a node to see details"),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(0.7, 0.7, 0.7)),
                InfoPanelText,
            ));
        });
}
