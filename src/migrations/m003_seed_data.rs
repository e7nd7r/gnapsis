//! Seed data migration - scopes and default categories.

use async_trait::async_trait;
use neo4rs::{query, Txn};

use crate::error::AppError;
use crate::models::{generate_ulid, Scope};

use super::Migration;

/// Seed data migration (scopes and categories).
pub struct M003SeedData;

#[async_trait(?Send)]
impl Migration for M003SeedData {
    fn id(&self) -> &'static str {
        "m003_seed_data"
    }

    fn version(&self) -> u32 {
        3
    }

    fn description(&self) -> &'static str {
        "Seed data (scopes and default categories)"
    }

    async fn up(&self, txn: &mut Txn) -> Result<(), AppError> {
        self.create_scopes(txn).await?;
        self.create_default_categories(txn).await?;
        Ok(())
    }
}

impl M003SeedData {
    /// Create Scope nodes with the fixed hierarchy.
    async fn create_scopes(&self, txn: &mut Txn) -> Result<(), AppError> {
        // Create each scope node
        for scope in Scope::all() {
            txn.run(
                query(
                    "MERGE (s:Scope {name: $name})
                     SET s.depth = $depth, s.description = $description",
                )
                .param("name", scope.to_string())
                .param("depth", scope.depth() as i64)
                .param("description", scope.description()),
            )
            .await?;
        }

        // Create hierarchy: Domain -> Feature -> Namespace -> Component -> Unit
        txn.run(query(
            "MATCH (domain:Scope {name: 'Domain'})
             MATCH (feature:Scope {name: 'Feature'})
             MATCH (namespace:Scope {name: 'Namespace'})
             MATCH (component:Scope {name: 'Component'})
             MATCH (unit:Scope {name: 'Unit'})
             MERGE (domain)-[:COMPOSES]->(feature)
             MERGE (feature)-[:COMPOSES]->(namespace)
             MERGE (namespace)-[:COMPOSES]->(component)
             MERGE (component)-[:COMPOSES]->(unit)",
        ))
        .await?;

        Ok(())
    }

    /// Create default categories for each scope.
    async fn create_default_categories(&self, txn: &mut Txn) -> Result<(), AppError> {
        let categories = [
            // Domain
            ("core", "Domain", "Core business logic"),
            ("infrastructure", "Domain", "Infrastructure and utilities"),
            // Feature
            ("functional", "Feature", "Functional capabilities"),
            ("non-functional", "Feature", "Cross-cutting concerns"),
            ("technical", "Feature", "Technical implementation details"),
            // Namespace
            ("module", "Namespace", "Code module"),
            ("library", "Namespace", "External library"),
            // Component
            ("class", "Component", "Object-oriented class"),
            ("struct", "Component", "Data structure"),
            ("trait", "Component", "Trait/interface definition"),
            ("interface", "Component", "Interface definition"),
            ("enum", "Component", "Enumeration type"),
            // Unit
            ("function", "Unit", "Standalone function"),
            ("method", "Unit", "Class/struct method"),
            ("property", "Unit", "Property accessor"),
            ("field", "Unit", "Data field"),
            ("constant", "Unit", "Constant value"),
        ];

        for (name, scope, description) in categories {
            txn.run(
                query(
                    "MATCH (s:Scope {name: $scope})
                     MERGE (c:Category {name: $name})-[:IN_SCOPE]->(s)
                     SET c.id = coalesce(c.id, $id),
                         c.description = $description,
                         c.created_at = coalesce(c.created_at, datetime())",
                )
                .param("name", name)
                .param("scope", scope)
                .param("description", description)
                .param("id", generate_ulid()),
            )
            .await?;
        }
        Ok(())
    }
}
