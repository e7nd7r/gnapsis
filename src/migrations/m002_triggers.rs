//! PostgreSQL triggers migration for domain constraint enforcement.
//!
//! AGE allows executing Cypher queries from SQL via the `cypher()` function.
//! We use this to implement validation triggers that use Cypher for graph traversal.
//!
//! Note: Label tables are created lazily by AGE, so we create the trigger
//! functions here and attach them to tables after seed data creates the labels.

use crate::error::AppError;
use crate::graph::{CypherExecutor, SqlExecutor};

/// PostgreSQL triggers migration.
pub struct M002Triggers;

impl M002Triggers {
    /// Apply the migration.
    pub async fn up<T>(&self, txn: &T) -> Result<(), AppError>
    where
        T: CypherExecutor + SqlExecutor + Sync,
    {
        self.create_trigger_functions(txn).await
    }

    /// Create PostgreSQL trigger functions using Cypher for validation.
    async fn create_trigger_functions<T: SqlExecutor + Sync>(
        &self,
        txn: &T,
    ) -> Result<(), AppError> {
        // Ensure AGE is loaded for trigger functions
        txn.execute_sql("LOAD 'age'").await?;
        txn.execute_sql("SET search_path = ag_catalog, public").await?;

        // Function to prevent deletion of entities with children
        // Note: Use $func$ for function body to allow $$ inside for Cypher queries
        txn.execute_sql(
            r#"
            CREATE OR REPLACE FUNCTION prevent_delete_with_children()
            RETURNS TRIGGER AS $func$
            DECLARE
                entity_id TEXT;
                child_count INTEGER;
            BEGIN
                -- Load AGE for this session
                LOAD 'age';
                SET search_path = ag_catalog, public;

                -- Extract entity id from AGE properties
                entity_id := agtype_access_operator(OLD.properties, '"id"')::text;
                -- Remove quotes from the extracted value
                entity_id := trim(both '"' from entity_id);

                -- Use Cypher to count children
                SELECT count INTO child_count FROM cypher('knowledge_graph', $$
                    MATCH (child:Entity)-[:BELONGS_TO]->(parent:Entity)
                    WHERE parent.id = $entity_id
                    RETURN count(child) as count
                $$, format('{"entity_id": "%s"}', entity_id)::agtype) as (count agtype);

                IF child_count > 0 THEN
                    RAISE EXCEPTION 'Cannot delete entity %: has % child(ren)', entity_id, child_count;
                END IF;

                RETURN OLD;
            END;
            $func$ LANGUAGE plpgsql;
            "#,
        )
        .await?;

        // Function to cascade delete references when entity is deleted
        txn.execute_sql(
            r#"
            CREATE OR REPLACE FUNCTION cascade_delete_entity_references()
            RETURNS TRIGGER AS $func$
            DECLARE
                entity_id TEXT;
            BEGIN
                LOAD 'age';
                SET search_path = ag_catalog, public;

                entity_id := agtype_access_operator(OLD.properties, '"id"')::text;
                entity_id := trim(both '"' from entity_id);

                -- Delete references via Cypher
                PERFORM * FROM cypher('knowledge_graph', $$
                    MATCH (e:Entity {id: $entity_id})-[:HAS_REFERENCE]->(r)
                    DETACH DELETE r
                $$, format('{"entity_id": "%s"}', entity_id)::agtype) as (result agtype);

                -- Clean up embeddings table
                DELETE FROM embeddings WHERE id = entity_id;

                RETURN OLD;
            END;
            $func$ LANGUAGE plpgsql;
            "#,
        )
        .await?;

        // Function to validate scope hierarchy on BELONGS_TO creation
        txn.execute_sql(
            r#"
            CREATE OR REPLACE FUNCTION validate_belongs_to_scope()
            RETURNS TRIGGER AS $func$
            DECLARE
                child_depth INTEGER;
                parent_depth INTEGER;
                child_name TEXT;
                parent_name TEXT;
                rec RECORD;
            BEGIN
                LOAD 'age';
                SET search_path = ag_catalog, public;

                -- Get child entity info using graph vertex id
                FOR rec IN
                    SELECT * FROM cypher('knowledge_graph', $$
                        MATCH (e:Entity)-[:CLASSIFIED_AS]->(:Category)-[:IN_SCOPE]->(s:Scope)
                        WHERE id(e) = $vertex_id
                        RETURN e.name as name, s.depth as depth
                    $$, format('{"vertex_id": %s}', NEW.start_id)::agtype) as (name agtype, depth agtype)
                LOOP
                    child_name := trim(both '"' from rec.name::text);
                    child_depth := rec.depth::text::integer;
                END LOOP;

                -- Get parent entity info
                FOR rec IN
                    SELECT * FROM cypher('knowledge_graph', $$
                        MATCH (e:Entity)-[:CLASSIFIED_AS]->(:Category)-[:IN_SCOPE]->(s:Scope)
                        WHERE id(e) = $vertex_id
                        RETURN e.name as name, s.depth as depth
                    $$, format('{"vertex_id": %s}', NEW.end_id)::agtype) as (name agtype, depth agtype)
                LOOP
                    parent_name := trim(both '"' from rec.name::text);
                    parent_depth := rec.depth::text::integer;
                END LOOP;

                -- Validate scope hierarchy
                IF child_depth IS NOT NULL AND parent_depth IS NOT NULL THEN
                    IF child_depth <= parent_depth THEN
                        RAISE EXCEPTION 'Invalid BELONGS_TO: % (depth %) cannot belong to % (depth %)',
                            child_name, child_depth, parent_name, parent_depth;
                    END IF;
                END IF;

                RETURN NEW;
            END;
            $func$ LANGUAGE plpgsql;
            "#,
        )
        .await?;

        // Helper function to attach triggers after label tables exist
        txn.execute_sql(
            r#"
            CREATE OR REPLACE FUNCTION attach_graph_triggers()
            RETURNS void AS $$
            BEGIN
                -- Entity triggers
                IF EXISTS (
                    SELECT 1 FROM information_schema.tables
                    WHERE table_schema = 'knowledge_graph' AND table_name = 'Entity'
                ) THEN
                    DROP TRIGGER IF EXISTS trg_prevent_delete_with_children ON knowledge_graph."Entity";
                    DROP TRIGGER IF EXISTS trg_cascade_delete_references ON knowledge_graph."Entity";

                    CREATE TRIGGER trg_prevent_delete_with_children
                        BEFORE DELETE ON knowledge_graph."Entity"
                        FOR EACH ROW EXECUTE FUNCTION prevent_delete_with_children();

                    CREATE TRIGGER trg_cascade_delete_references
                        AFTER DELETE ON knowledge_graph."Entity"
                        FOR EACH ROW EXECUTE FUNCTION cascade_delete_entity_references();

                    RAISE NOTICE 'Attached triggers to Entity table';
                END IF;

                -- BELONGS_TO edge triggers
                IF EXISTS (
                    SELECT 1 FROM information_schema.tables
                    WHERE table_schema = 'knowledge_graph' AND table_name = 'BELONGS_TO'
                ) THEN
                    DROP TRIGGER IF EXISTS trg_validate_belongs_to ON knowledge_graph."BELONGS_TO";

                    CREATE TRIGGER trg_validate_belongs_to
                        BEFORE INSERT ON knowledge_graph."BELONGS_TO"
                        FOR EACH ROW EXECUTE FUNCTION validate_belongs_to_scope();

                    RAISE NOTICE 'Attached trigger to BELONGS_TO edge table';
                END IF;
            END;
            $$ LANGUAGE plpgsql;
            "#,
        )
        .await?;

        tracing::info!("Created Cypher-based trigger functions for domain constraints");
        Ok(())
    }
}
