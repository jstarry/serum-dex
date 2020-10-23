use borsh::schema::{BorshSchema, BorshSchemaContainer};
use borsh::BorshSerialize;

use serum_registry::accounts::*;

fn main() -> std::io::Result<()> {
    let mut schema: BorshSchemaContainer = Registrar::schema_container();
    //    PoolRequest::add_definitions_recursively(&mut schema.definitions);
    schema.serialize(&mut std::io::stdout())
}
