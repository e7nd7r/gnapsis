//! Database-level migrations (global, run once per database).

mod m001_schema;

pub use m001_schema::M001Schema;

use crate::migrations::traits::{DbMigration, Register};

/// Create the database migrations register.
pub fn create_register() -> Register<dyn DbMigration> {
    Register::<dyn DbMigration>::new().register(M001Schema)
}
