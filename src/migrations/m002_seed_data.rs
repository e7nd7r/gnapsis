//! Seed data migration - scopes and default categories.

use crate::error::AppError;
use crate::graph::{CypherExecutor, QueryExt};
use crate::models::{generate_ulid, Scope};

/// Seed data migration (scopes and categories).
pub struct M002SeedData;

impl M002SeedData {
    /// Apply the migration.
    pub async fn up<T>(&self, txn: &T) -> Result<(), AppError>
    where
        T: CypherExecutor + Sync,
    {
        self.create_scopes(txn).await?;
        self.create_default_categories(txn).await?;
        Ok(())
    }

    /// Create Scope nodes with the fixed hierarchy.
    async fn create_scopes<T: CypherExecutor + Sync>(&self, txn: &T) -> Result<(), AppError> {
        // Create each scope node
        for scope in Scope::all() {
            txn.query(
                "MERGE (s:Scope {name: $name})
                 SET s.depth = $depth, s.description = $description",
            )
            .param("name", scope.to_string())
            .param("depth", scope.depth() as i64)
            .param("description", scope.description())
            .run()
            .await?;
        }

        // Create hierarchy: Domain -> Feature -> Namespace -> Component -> Unit
        txn.query(
            "MATCH (domain:Scope {name: 'Domain'})
             MATCH (feature:Scope {name: 'Feature'})
             MATCH (namespace:Scope {name: 'Namespace'})
             MATCH (component:Scope {name: 'Component'})
             MATCH (unit:Scope {name: 'Unit'})
             MERGE (domain)-[:COMPOSES]->(feature)
             MERGE (feature)-[:COMPOSES]->(namespace)
             MERGE (namespace)-[:COMPOSES]->(component)
             MERGE (component)-[:COMPOSES]->(unit)",
        )
        .run()
        .await?;

        Ok(())
    }

    /// Create default categories for each scope.
    async fn create_default_categories<T: CypherExecutor + Sync>(
        &self,
        txn: &T,
    ) -> Result<(), AppError> {
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

        // Get current timestamp as ISO 8601 string for AGE compatibility
        let now = chrono::Utc::now().to_rfc3339();

        for (name, scope, description) in categories {
            txn.query(
                "MATCH (s:Scope {name: $scope})
                 MERGE (c:Category {name: $name})-[:IN_SCOPE]->(s)
                 SET c.id = coalesce(c.id, $id),
                     c.description = $description,
                     c.created_at = coalesce(c.created_at, $created_at)",
            )
            .param("name", name)
            .param("scope", scope)
            .param("description", description)
            .param("id", generate_ulid())
            .param("created_at", &now)
            .run()
            .await?;
        }
        Ok(())
    }
}
