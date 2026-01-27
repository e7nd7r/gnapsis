-- Initialize Apache AGE extension and create the knowledge graph

-- Load the AGE extension
CREATE EXTENSION IF NOT EXISTS age;

-- Set search path for AGE
LOAD 'age';
SET search_path = ag_catalog, public;

-- Create the knowledge graph
SELECT create_graph('knowledge_graph');
