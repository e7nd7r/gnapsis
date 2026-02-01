//! Force-directed graph layout algorithm.

use bevy::math::Vec3;
use std::collections::{HashMap, HashSet, VecDeque};

use crate::models::{QueryGraph, QueryGraphNode};

/// Physics constants for force-directed layout.
const REPULSION_STRENGTH: f32 = 200.0; // Base repulsion (no degree scaling)
const DAMPING: f32 = 0.6; // Velocity friction per step
const MIN_DISTANCE: f32 = 0.5;
const MIN_MASS: f32 = 1.0; // Minimum mass per node
const MASS_PER_CONNECTION: f32 = 1.5; // Additional mass per connection

// Per-relationship-type spring parameters (stiffness, rest_length)
// Stiffness: how strongly the log spring pulls toward rest length
// Rest length: distance where spring force is zero
// Equilibrium: stiffness * ln(d/rest) = REPULSION / d²
// With these values, BELONGS_TO equilibrium ≈ 4.8, RELATED_TO ≈ 12
const SPRING_BELONGS_TO: (f32, f32) = (50.0, 4.0); // Tight parent-child
const SPRING_CALLS: (f32, f32) = (20.0, 7.0); // Code links moderate
const SPRING_IMPORTS: (f32, f32) = (20.0, 7.0);
const SPRING_IMPLEMENTS: (f32, f32) = (20.0, 7.0);
const SPRING_INSTANTIATES: (f32, f32) = (20.0, 7.0);
const SPRING_RELATED_TO: (f32, f32) = (10.0, 10.0); // Loose semantic
const SPRING_DEFAULT: (f32, f32) = (15.0, 8.0);

/// Node type for visualization coloring.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NodeType {
    /// Entity node (blue sphere).
    Entity,
    /// Starting/root node (gold, larger).
    StartNode,
}

/// A document reference attached to an entity (shown in info panel, not as a graph node).
#[derive(Debug, Clone)]
pub struct ReferenceInfo {
    /// File path (relative to project root).
    pub path: String,
    /// Starting line number.
    pub start_line: u32,
    /// Ending line number.
    pub end_line: u32,
    /// Description of what this reference points to.
    pub description: String,
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
    /// Scope level (Domain, Feature, Namespace, Component, Unit) or None for references.
    pub scope: Option<String>,
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
    /// Log-spring stiffness for this edge type.
    pub stiffness: f32,
    /// Rest length where spring force is zero.
    pub rest_length: f32,
}

/// Graph layout with nodes and edges.
#[derive(Clone)]
pub struct GraphLayout {
    /// Nodes with positions.
    pub nodes: Vec<LayoutNode>,
    /// Edges connecting nodes.
    pub edges: Vec<LayoutEdge>,
    /// Document references per entity ID (shown in info panel).
    pub entity_references: HashMap<String, Vec<ReferenceInfo>>,
}

impl GraphLayout {
    /// Create layout from a QueryGraph (semantic query result).
    ///
    /// Reference nodes are not included in the layout — they are stored
    /// in `entity_references` for display in the info panel.
    pub fn from_query_graph(graph: &QueryGraph) -> Self {
        let mut nodes = Vec::new();
        let mut id_to_idx = HashMap::new();

        // Collect reference info keyed by reference ID
        let mut ref_by_id: HashMap<String, ReferenceInfo> = HashMap::new();

        // Create entity nodes only (skip references)
        let mut entity_count = 0;
        for node in &graph.nodes {
            match node {
                QueryGraphNode::Entity {
                    id,
                    name,
                    scope,
                    relevance: _,
                    ..
                } => {
                    let node_type = if id == &graph.root_entity.id {
                        NodeType::StartNode
                    } else {
                        NodeType::Entity
                    };
                    let is_start = matches!(node_type, NodeType::StartNode);
                    let position = random_position(entity_count, graph.nodes.len());
                    entity_count += 1;

                    id_to_idx.insert(id.clone(), nodes.len());
                    nodes.push(LayoutNode {
                        id: id.clone(),
                        label: name.clone(),
                        position,
                        velocity: Vec3::ZERO,
                        node_type,
                        is_start,
                        mass: MIN_MASS,
                        scope: scope.clone(),
                    });
                }
                QueryGraphNode::Reference {
                    id,
                    document_path,
                    start_line,
                    end_line,
                    description,
                    ..
                } => {
                    ref_by_id.insert(
                        id.clone(),
                        ReferenceInfo {
                            path: document_path.clone(),
                            start_line: *start_line,
                            end_line: *end_line,
                            description: description.clone(),
                        },
                    );
                }
            }
        }

        // Build entity_references from HAS_REFERENCE edges, and layout edges from the rest
        let mut entity_references: HashMap<String, Vec<ReferenceInfo>> = HashMap::new();
        let mut edges = Vec::new();

        for e in &graph.edges {
            if e.relationship == "HAS_REFERENCE" {
                // Map HAS_REFERENCE edge to entity_references.
                // from_id is the entity, to_id is the reference.
                if let Some(ref_info) = ref_by_id.get(&e.to_id) {
                    entity_references
                        .entry(e.from_id.clone())
                        .or_default()
                        .push(ref_info.clone());
                }
            } else if let (Some(&from_idx), Some(&to_idx)) =
                (id_to_idx.get(&e.from_id), id_to_idx.get(&e.to_id))
            {
                let (stiffness, rest_length) = spring_params(&e.relationship);
                edges.push(LayoutEdge {
                    from_idx,
                    to_idx,
                    label: e.relationship.clone(),
                    note: e.note.clone(),
                    stiffness,
                    rest_length,
                });
            }
        }

        // Distribute mass based on connections
        distribute_mass(&mut nodes, &edges);

        Self {
            nodes,
            edges,
            entity_references,
        }
    }

    /// Run one step of the force-directed layout algorithm.
    ///
    /// Uses a modified Eades model:
    /// - Repulsion: FA2-style degree-weighted inverse-square between all pairs
    /// - Attraction: Logarithmic springs with per-edge-type stiffness and rest length
    /// - Centering: D3-style pure translation (no force, just recenters)
    /// - Cooling: Alpha decay reduces forces over time for convergence
    pub fn update_physics(&mut self, dt: f32) {
        let n = self.nodes.len();
        if n == 0 {
            return;
        }

        // Pre-compute masses to avoid borrow issues
        let masses: Vec<f32> = self.nodes.iter().map(|n| n.mass).collect();

        // --- Repulsion: inverse-square between all pairs ---
        // F_r = K / d²
        // Simple Coulomb-style repulsion. Mass handles inertia (heavier = slower).
        for i in 0..n {
            for j in (i + 1)..n {
                let delta = self.nodes[i].position - self.nodes[j].position;
                let dist = delta.length().max(MIN_DISTANCE);
                let force = REPULSION_STRENGTH / (dist * dist);
                let dir = delta.normalize_or_zero();

                self.nodes[i].velocity += dir * force * dt / masses[i];
                self.nodes[j].velocity -= dir * force * dt / masses[j];
            }
        }

        // --- Attraction: Eades logarithmic springs ---
        // F_a = stiffness * ln(d / rest_length)
        // Zero force at rest_length, gentle pull beyond, push below.
        // Logarithmic growth prevents violent yanking of distant nodes.
        for edge in &self.edges {
            let delta = self.nodes[edge.to_idx].position - self.nodes[edge.from_idx].position;
            let dist = delta.length().max(MIN_DISTANCE);
            let force = edge.stiffness * (dist / edge.rest_length).ln();
            let dir = delta.normalize_or_zero();

            self.nodes[edge.from_idx].velocity += dir * force * dt / masses[edge.from_idx];
            self.nodes[edge.to_idx].velocity -= dir * force * dt / masses[edge.to_idx];
        }

        // --- Centering: D3-style pure translation (no force) ---
        // Translate all nodes so centroid is at origin. Prevents drift
        // without adding energy or distorting the layout.
        let centroid: Vec3 = self.nodes.iter().map(|n| n.position).sum::<Vec3>() / n as f32;
        for node in &mut self.nodes {
            node.position -= centroid;
        }

        // --- Damping and integration ---
        const MAX_VELOCITY: f32 = 200.0;
        for node in &mut self.nodes {
            node.velocity *= DAMPING;
            let speed = node.velocity.length();
            if speed > MAX_VELOCITY {
                node.velocity = node.velocity.normalize() * MAX_VELOCITY;
            } else if speed < 0.001 {
                node.velocity = Vec3::ZERO;
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

    /// Calculate the bounding sphere radius that encompasses all nodes.
    /// Returns (center, radius) where center is the centroid of all nodes.
    pub fn bounding_sphere(&self) -> (Vec3, f32) {
        if self.nodes.is_empty() {
            return (Vec3::ZERO, 1.0);
        }

        // Calculate centroid
        let center: Vec3 =
            self.nodes.iter().map(|n| n.position).sum::<Vec3>() / self.nodes.len() as f32;

        // Find maximum distance from centroid
        let max_dist = self
            .nodes
            .iter()
            .map(|n| (n.position - center).length())
            .fold(0.0_f32, |a, b| a.max(b));

        // Add some padding
        (center, max_dist + 2.0)
    }

    /// Collect all nodes and edges within `hops` hops of `start` via BFS.
    /// BELONGS_TO edges are only traversed toward children (parent → child).
    /// All other edges are traversed bidirectionally.
    /// Returns (node indices, edge pairs) in the neighborhood.
    pub fn collect_n_hop_neighborhood(
        &self,
        start: usize,
        hops: usize,
    ) -> (HashSet<usize>, HashSet<(usize, usize)>) {
        let mut visited_nodes = HashSet::new();
        let mut visited_edges = HashSet::new();
        let mut queue = VecDeque::new();

        visited_nodes.insert(start);
        queue.push_back((start, 0));

        while let Some((current, depth)) = queue.pop_front() {
            if depth >= hops {
                continue;
            }
            for edge in &self.edges {
                // BELONGS_TO: child --BELONGS_TO--> parent
                // Only traverse parent → child (edge.to_idx is parent, from_idx is child)
                let neighbor = if edge.label == "BELONGS_TO" {
                    if edge.to_idx == current {
                        Some(edge.from_idx) // current is parent, go to child
                    } else {
                        None // don't go upward
                    }
                } else if edge.from_idx == current {
                    Some(edge.to_idx)
                } else if edge.to_idx == current {
                    Some(edge.from_idx)
                } else {
                    None
                };
                if let Some(n) = neighbor {
                    visited_edges.insert((edge.from_idx, edge.to_idx));
                    if visited_nodes.insert(n) {
                        queue.push_back((n, depth + 1));
                    }
                }
            }
        }

        (visited_nodes, visited_edges)
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

/// Get spring parameters (stiffness, rest_length) for a relationship type.
/// Hierarchical edges cluster tighter; semantic edges stay loose.
fn spring_params(relationship: &str) -> (f32, f32) {
    match relationship {
        "BELONGS_TO" => SPRING_BELONGS_TO,
        "CALLS" => SPRING_CALLS,
        "IMPORTS" => SPRING_IMPORTS,
        "IMPLEMENTS" => SPRING_IMPLEMENTS,
        "INSTANTIATES" => SPRING_INSTANTIATES,
        "RELATED_TO" => SPRING_RELATED_TO,
        _ => SPRING_DEFAULT,
    }
}

/// Generate a random initial position in 3D space based on index.
/// Uses Fibonacci sphere distribution for even spacing.
fn random_position(i: usize, total_nodes: usize) -> Vec3 {
    let golden_ratio = (1.0 + 5.0_f32.sqrt()) / 2.0;
    let idx = i as f32 + 0.5;
    let n = total_nodes.max(1) as f32;

    // Fibonacci sphere - gives evenly distributed points on a sphere
    let theta = 2.0 * std::f32::consts::PI * idx / golden_ratio;
    // Use actual node count instead of hardcoded 20
    let phi = (1.0 - 2.0 * idx / n).acos();

    // Vary radius based on node count - larger graphs need more space
    let base_radius = 3.0 + (n / 10.0).sqrt() * 2.0;
    let radius = base_radius + (i as f32 * 1.618).sin() * 2.0;

    Vec3::new(
        radius * phi.sin() * theta.cos(),
        radius * phi.cos(),
        radius * phi.sin() * theta.sin(),
    )
}
