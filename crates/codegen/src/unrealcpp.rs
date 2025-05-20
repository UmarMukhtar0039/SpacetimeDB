use crate::Lang;
use spacetimedb_schema::def::{ModuleDef, TableDef, ReducerDef, ScopedTypeName, TypeDef};

pub struct UnrealCpp;

impl Lang for UnrealCpp {
    fn generate_type(&self, _module: &ModuleDef, _typ: &TypeDef) -> String {
        "// UnrealCpp type generation not implemented yet".to_string()
    }

    fn generate_table(&self, _module: &ModuleDef, _table: &TableDef) -> String {
        "// UnrealCpp table generation not implemented yet".to_string()
    }

    fn generate_reducer(&self, _module: &ModuleDef, _reducer: &ReducerDef) -> String {
        "// UnrealCpp reducer generation not implemented yet".to_string()
    }

    fn generate_globals(&self, _module: &ModuleDef) -> Vec<(String, String)> {
        vec![(
            "SpacetimeDBClient.gen.cpp".to_string(),
            "// UnrealCpp globals not ismplemented yet".to_string(),
        )]
    }

    fn type_filename(&self, type_name: &ScopedTypeName) -> String {
        format!("Types/{}.gen.h", type_name.to_string())
    }

    fn table_filename(&self, _module: &ModuleDef, table: &TableDef) -> String {
        format!("Tables/{}.gen.h", table.name.to_string())
    }

    fn reducer_filename(&self, reducer_name: &spacetimedb_schema::identifier::Identifier) -> String {
        format!("Reducers/{}.gen.h", reducer_name.to_string())
    }
}