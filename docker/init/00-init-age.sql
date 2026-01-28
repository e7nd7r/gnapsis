-- Initialize Apache AGE and pgvector extensions

-- Load the AGE extension
CREATE EXTENSION IF NOT EXISTS age;

-- Load the pgvector extension for embeddings
CREATE EXTENSION IF NOT EXISTS vector;

-- Set search path for AGE
LOAD 'age';
SET search_path = ag_catalog, public;

-- Create the knowledge graph
SELECT create_graph('knowledge_graph');
