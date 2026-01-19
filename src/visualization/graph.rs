//! Force-directed graph layout algorithm.

use bevy::math::Vec3;
use std::collections::HashMap;

use crate::models::{CompositionGraph, Subgraph, SubgraphNode};

/// Physics constants for force-directed layout.
const REPULSION_STRENGTH: f32 = 8000.0; // Close-range repulsion
const SPRING_STRENGTH: f32 = 80.0; // Edge springs
const IDEAL_LENGTH: f32 = 8.0; // Preferred edge length
const CENTERING_STRENGTH: f32 = 0.3; // Pull toward center
const DAMPING: f32 = 0.5; // Friction (lower = faster settling)
const MIN_DISTANCE: f32 = 0.5;
const MIN_MASS: f32 = 1.0; // Minimum mass per node
const MASS_PER_CONNECTION: f32 = 1.5; // Additional mass per connection

/// Node type for visualization coloring.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NodeType {
    /// Entity node (blue sphere).
    Entity,
    /// Document reference node (green cube).
    DocumentReference,
    /// Starting/root node (gold, larger).
    StartNode,
}

/// A node in the layout with position and velocity.
#[derive(Debug, Clone)]
pub struct LayoutNode {
    /// Node ID.
    pub id: String,
    /// Display label.
    pub label: String,
    /// Current position in 3D space.
    pub position: Vec3,
    /// Current velocity.
    pub velocity: Vec3,
    /// Node type for rendering.
    pub node_type: NodeType,
    /// Whether this is the starting node.
    pub is_start: bool,
    /// Mass (affects size and inertia).
    pub mass: f32,
}

/// An edge in the layout.
#[derive(Debug, Clone)]
pub struct LayoutEdge {
    /// Source node index.
    pub from_idx: usize,
    /// Target node index.
    pub to_idx: usize,
    /// Edge label (relationship type).
    pub label: String,
    /// Optional note on the relationship.
    pub note: Option<String>,
}

/// Graph layout with nodes and edges.
#[derive(Clone)]
pub struct GraphLayout {
    /// Nodes with positions.
    pub nodes: Vec<LayoutNode>,
    /// Edges connecting nodes.
    pub edges: Vec<LayoutEdge>,
    /// Whether the layout has stabilized.
    pub stable: bool,
}

impl GraphLayout {
    /// Create layout from a Subgraph.
    pub fn from_subgraph(subgraph: &Subgraph, start_id: &str) -> Self {
        let mut nodes = Vec::new();
        let mut id_to_idx = HashMap::new();

        // Create nodes with random initial positions (mass set later)
        for (i, node) in subgraph.nodes.iter().enumerate() {
            let (id, label, node_type) = match node {
                SubgraphNode::Entity {
                    id, name, distance, ..
                } => {
                    let nt = if *distance == 0 {
                        NodeType::StartNode
                    } else {
                        NodeType::Entity
                    };
                    (id.clone(), name.clone(), nt)
                }
                SubgraphNode::DocumentReference {
                    id,
                    document_path,
                    start_line,
                    end_line,
                    ..
                } => {
                    let label = format!("{}:{}-{}", document_path, start_line, end_line);
                    (id.clone(), label, NodeType::DocumentReference)
                }
            };

            let is_start = id == start_id;
            let position = random_position(i);

            id_to_idx.insert(id.clone(), nodes.len());
            nodes.push(LayoutNode {
                id,
                label,
                position,
                velocity: Vec3::ZERO,
                node_type,
                is_start,
                mass: MIN_MASS, // Temporary, will be recalculated
            });
        }

        // Create edges
        let edges: Vec<LayoutEdge> = subgraph
            .edges
            .iter()
            .filter_map(|e| {
                let from_idx = id_to_idx.get(&e.from_id)?;
                let to_idx = id_to_idx.get(&e.to_id)?;
                Some(LayoutEdge {
                    from_idx: *from_idx,
                    to_idx: *to_idx,
                    label: e.relationship.clone(),
                    note: e.note.clone(),
                })
            })
            .collect();

        // Count connections per node and distribute mass
        distribute_mass(&mut nodes, &edges);

        Self {
            nodes,
            edges,
            stable: false,
        }
    }

    /// Create layout from a CompositionGraph.
    pub fn from_composition(graph: &CompositionGraph) -> Self {
        let mut nodes = Vec::new();
        let mut id_to_idx = HashMap::new();
        let mut edges = Vec::new();

        // Add root entity
        let root_id = graph.entity.id.clone();
        id_to_idx.insert(root_id.clone(), 0);
        nodes.push(LayoutNode {
            id: root_id.clone(),
            label: graph.entity.name.clone(),
            position: Vec3::ZERO,
            velocity: Vec3::ZERO,
            node_type: NodeType::StartNode,
            is_start: true,
            mass: MIN_MASS,
        });

        // Add ancestors (positioned above)
        for (i, ancestor) in graph.ancestors.iter().enumerate() {
            let idx = nodes.len();
            id_to_idx.insert(ancestor.id.clone(), idx);
            nodes.push(LayoutNode {
                id: ancestor.id.clone(),
                label: ancestor.name.clone(),
                position: Vec3::new(
                    (i as f32 - graph.ancestors.len() as f32 / 2.0) * IDEAL_LENGTH,
                    ancestor.depth as f32 * IDEAL_LENGTH,
                    0.0,
                ),
                velocity: Vec3::ZERO,
                node_type: NodeType::Entity,
                is_start: false,
                mass: MIN_MASS,
            });

            // Edge from child to parent (BELONGS_TO)
            edges.push(LayoutEdge {
                from_idx: 0,
                to_idx: idx,
                label: "BELONGS_TO".to_string(),
                note: None,
            });
        }

        // Add descendants (positioned below)
        for (i, descendant) in graph.descendants.iter().enumerate() {
            let idx = nodes.len();
            id_to_idx.insert(descendant.id.clone(), idx);
            nodes.push(LayoutNode {
                id: descendant.id.clone(),
                label: descendant.name.clone(),
                position: Vec3::new(
                    (i as f32 - graph.descendants.len() as f32 / 2.0) * IDEAL_LENGTH,
                    descendant.depth as f32 * -IDEAL_LENGTH,
                    0.0,
                ),
                velocity: Vec3::ZERO,
                node_type: NodeType::Entity,
                is_start: false,
                mass: MIN_MASS,
            });

            // Edge from descendant to root (BELONGS_TO)
            edges.push(LayoutEdge {
                from_idx: idx,
                to_idx: 0,
                label: "BELONGS_TO".to_string(),
                note: None,
            });
        }

        // Distribute mass based on connections
        distribute_mass(&mut nodes, &edges);

        Self {
            nodes,
            edges,
            stable: false,
        }
    }

    /// Run one step of the force-directed layout algorithm.
    /// Always runs - forces are continuously calculated.
    pub fn update_physics(&mut self, dt: f32) {
        let n = self.nodes.len();
        if n == 0 {
            return;
        }

        // Calculate center of mass
        let center: Vec3 = self.nodes.iter().map(|n| n.position).sum::<Vec3>() / n as f32;

        // Pre-compute masses to avoid borrow issues
        let masses: Vec<f32> = self.nodes.iter().map(|n| n.mass).collect();

        // Calculate repulsion forces between all node pairs
        // F = ma, so a = F/m - heavier nodes accelerate less
        for i in 0..n {
            for j in (i + 1)..n {
                let delta = self.nodes[i].position - self.nodes[j].position;
                let dist = delta.length().max(MIN_DISTANCE);
                // Repulsion falls off with distance squared
                let force = REPULSION_STRENGTH / (dist * dist);
                let dir = delta.normalize_or_zero();

                // Divide by mass: heavier nodes accelerate less
                self.nodes[i].velocity += dir * force * dt / masses[i];
                self.nodes[j].velocity -= dir * force * dt / masses[j];
            }
        }

        // Calculate spring forces along edges (Hooke's law)
        for edge in &self.edges {
            let delta = self.nodes[edge.to_idx].position - self.nodes[edge.from_idx].position;
            let dist = delta.length();
            // Spring force: F = k * (x - rest_length)
            let displacement = dist - IDEAL_LENGTH;
            let force = displacement * SPRING_STRENGTH;
            let dir = delta.normalize_or_zero();

            // Divide by mass: heavier nodes accelerate less
            self.nodes[edge.from_idx].velocity += dir * force * dt / masses[edge.from_idx];
            self.nodes[edge.to_idx].velocity -= dir * force * dt / masses[edge.to_idx];
        }

        // Apply centering force (pulls nodes toward center of mass)
        for (i, node) in self.nodes.iter_mut().enumerate() {
            let to_center = center - node.position;
            node.velocity += to_center * CENTERING_STRENGTH / masses[i];
        }

        // Apply damping and update positions
        const MAX_VELOCITY: f32 = 200.0;
        const SETTLE_THRESHOLD: f32 = 2.0; // Below this, apply progressive damping
        for node in &mut self.nodes {
            node.velocity *= DAMPING;
            let speed = node.velocity.length();

            // Progressive damping: smoothly reduce velocity as it gets smaller
            if speed < SETTLE_THRESHOLD && speed > 0.001 {
                let t = speed / SETTLE_THRESHOLD; // 0 to 1
                node.velocity *= t * t; // Quadratic falloff for smooth settling
            } else if speed <= 0.001 {
                node.velocity = Vec3::ZERO;
            } else if speed > MAX_VELOCITY {
                node.velocity = node.velocity.normalize() * MAX_VELOCITY;
            }
            node.position += node.velocity * dt;
        }
    }

    /// Run the layout for a number of iterations to stabilize.
    pub fn stabilize(&mut self, iterations: usize) {
        let dt = 0.016; // ~60fps timestep
        for _ in 0..iterations {
            self.update_physics(dt);
        }
    }
}

/// Distribute mass among nodes based on connection count (arity).
/// Nodes with the same number of connections get identical mass.
fn distribute_mass(nodes: &mut [LayoutNode], edges: &[LayoutEdge]) {
    let n = nodes.len();
    if n == 0 {
        return;
    }

    // Count connections per node
    let mut connection_counts = vec![0usize; n];
    for edge in edges {
        connection_counts[edge.from_idx] += 1;
        connection_counts[edge.to_idx] += 1;
    }

    // Assign mass directly based on arity: mass = MIN_MASS + connections * MASS_PER_CONNECTION
    // This ensures all nodes with same connection count have identical mass
    for i in 0..n {
        nodes[i].mass = MIN_MASS + connection_counts[i] as f32 * MASS_PER_CONNECTION;
    }
}

/// Generate a random initial position in 3D space based on index.
/// Uses Fibonacci sphere distribution for even spacing.
fn random_position(i: usize) -> Vec3 {
    let golden_ratio = (1.0 + 5.0_f32.sqrt()) / 2.0;
    let idx = i as f32 + 0.5;

    // Fibonacci sphere - gives evenly distributed points on a sphere
    let theta = 2.0 * std::f32::consts::PI * idx / golden_ratio;
    let phi = (1.0 - 2.0 * idx / 20.0).acos(); // Assume max ~20 nodes for distribution

    // Vary radius so nodes aren't all on same sphere
    let radius = 3.0 + (i as f32 * 1.618).sin() * 2.0;

    Vec3::new(
        radius * phi.sin() * theta.cos(),
        radius * phi.cos(),
        radius * phi.sin() * theta.sin(),
    )
}
