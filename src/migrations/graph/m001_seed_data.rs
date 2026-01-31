//! Seed data migration - scopes and default categories.

use futures::future::BoxFuture;
use futures::FutureExt;

use crate::error::AppError;
use crate::graph::Query;
use crate::migrations::{GraphMigration, GraphMigrationContext, Migration};
use crate::models::{generate_ulid, Scope};

pub struct M001SeedData {
    graph_name: String,
}

impl M001SeedData {
    pub fn new(graph_name: &str) -> Self {
        Self {
            graph_name: graph_name.to_string(),
        }
    }

    async fn create_scopes(
        &self,
        ctx: &(dyn GraphMigrationContext + Sync),
    ) -> Result<(), AppError> {
        for scope in Scope::all() {
            Query::new(
                ctx,
                "MERGE (s:Scope {name: $name})
                 SET s.depth = $depth, s.description = $description",
            )
            .param("name", scope.to_string())
            .param("depth", scope.depth() as i64)
            .param("description", scope.description())
            .run()
            .await?;
        }

        Query::new(
            ctx,
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

    async fn create_default_categories(
        &self,
        ctx: &(dyn GraphMigrationContext + Sync),
    ) -> Result<(), AppError> {
        let categories = [
            ("core", "Domain", "Core business logic"),
            ("infrastructure", "Domain", "Infrastructure and utilities"),
            ("functional", "Feature", "Functional capabilities"),
            ("non-functional", "Feature", "Cross-cutting concerns"),
            ("technical", "Feature", "Technical implementation details"),
            ("module", "Namespace", "Code module"),
            ("library", "Namespace", "External library"),
            ("class", "Component", "Object-oriented class"),
            ("struct", "Component", "Data structure"),
            ("trait", "Component", "Trait/interface definition"),
            ("interface", "Component", "Interface definition"),
            ("enum", "Component", "Enumeration type"),
            ("function", "Unit", "Standalone function"),
            ("method", "Unit", "Class/struct method"),
            ("property", "Unit", "Property accessor"),
            ("field", "Unit", "Data field"),
            ("constant", "Unit", "Constant value"),
        ];

        let now = chrono::Utc::now().to_rfc3339();

        for (name, scope, description) in categories {
            Query::new(
                ctx,
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

impl Migration for M001SeedData {
    type Context = dyn GraphMigrationContext + Sync;

    fn id(&self) -> &'static str {
        "graph001_seed_data"
    }
    fn version(&self) -> u32 {
        1
    }
    fn description(&self) -> &'static str {
        "Seed data (scopes and default categories)"
    }

    fn up<'a>(&'a self, ctx: &'a Self::Context) -> BoxFuture<'a, Result<(), AppError>> {
        async move {
            self.create_scopes(ctx).await?;
            self.create_default_categories(ctx).await?;
            Ok(())
        }
        .boxed()
    }
}

impl GraphMigration for M001SeedData {
    fn graph_name(&self) -> &str {
        &self.graph_name
    }
}
