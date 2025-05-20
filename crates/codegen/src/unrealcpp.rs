use std::ops::Deref;
use crate::Lang;
use spacetimedb_schema::def::{ModuleDef, TableDef, ReducerDef, TypeDef};
use crate::code_indenter::CodeIndenter;

pub struct UnrealCpp;

impl Lang for UnrealCpp {
    fn table_filename(&self, _module: &ModuleDef, table: &TableDef) -> String {
        format!("Tables/{}.generated.h", table.name.deref())
    }

    fn type_filename(&self, type_name: &spacetimedb_schema::def::ScopedTypeName) -> String {
        let name = type_name.name_segments().last().map(|id| id.deref()).unwrap_or("Unnamed");
        format!("Types/{}.generated.h", name)
    }

    fn reducer_filename(&self, reducer_name: &spacetimedb_schema::identifier::Identifier) -> String {
        format!("Reducers/{}.generated.h", reducer_name.deref())
    }

    fn generate_table(&self, _module: &ModuleDef, table: &TableDef) -> String {
        let mut output = CodeIndenter::new(String::new(), "    ");
        let struct_name = format!("F{}", table.name.deref());

        writeln!(output, "#pragma once");
        writeln!(output, "#include \"CoreMinimal.h\"");
        writeln!(output);
        writeln!(output, "USTRUCT(BlueprintType)");
        writeln!(output, "struct {}", struct_name);
        writeln!(output, "{{");
        writeln!(output, "    GENERATED_BODY()");
        writeln!(output);
        writeln!(output, "}};");

        output.into_inner()
    }

    fn generate_type(&self, module: &ModuleDef, typ: &TypeDef) -> String {
        let name = typ.name.name_segments().last().map(|id| id.deref()).unwrap_or("Unnamed");
        let struct_name = format!("F{}", name);
        let mut output = CodeIndenter::new(String::new(), "    ");

        writeln!(output, "#pragma once");
        writeln!(output, "#include \"CoreMinimal.h\"");
        writeln!(output);
        writeln!(output, "USTRUCT(BlueprintType)");
        writeln!(output, "struct {}", struct_name);
        writeln!(output, "{{");
        writeln!(output, "    GENERATED_BODY()");
        writeln!(output);

        let typespace = module.typespace_for_generate();
        if let Some(product) = typespace[typ.ty].as_product() {
            for (field, field_ty) in product {
                let ty_str = ty_fmt_cpp(field_ty).to_string();
                let field_name = field.deref();
                writeln!(output, "    UPROPERTY(BlueprintReadWrite)");
                writeln!(output, "    {} {};", ty_str, field_name);
            }
        } else {
            writeln!(output, "    // Unsupported type variant in codegen");
        }

        writeln!(output, "}};");

        output.into_inner()
    }

    fn generate_reducer(&self, _module: &ModuleDef, reducer: &ReducerDef) -> String {
        let mut output = CodeIndenter::new(String::new(), "    ");

        log_reducer_info(reducer);

        let reducer_name = reducer.name.deref();
        let class_name = "USpacetimeReducers";
        let func_name = format!("CallReducer_{}", reducer_name);

        writeln!(output, "#pragma once");
        writeln!(output, "#include \"CoreMinimal.h\"");
        writeln!(output);
        writeln!(output, "UCLASS()");
        writeln!(output, "class {} : public UObject", class_name);
        writeln!(output, "{{");
        writeln!(output, "    GENERATED_BODY()");
        writeln!(output);
        writeln!(output, "public:");
        writeln!(output, "    UFUNCTION(BlueprintCallable)");
        write!(output, "    void {}(", func_name);

        let mut first = true;
        for (param, ty) in &reducer.params_for_generate.elements {
            if !first {
                write!(output, ", ");
            }
            first = false;
            write!(output, "{} {}", ty_fmt_cpp(ty), param.deref());
        }
        writeln!(output, ");");
        writeln!(output, "}};");

        output.into_inner()
    }

    fn generate_globals(&self, module: &ModuleDef) -> Vec<(String, String)> {
        let mut files = vec![];

        let mut output = CodeIndenter::new(String::new(), "    ");
        writeln!(output, "#pragma once");
        writeln!(output, "#include \"CoreMinimal.h\"");
        writeln!(output);
        writeln!(output, "// Auto-generated SpacetimeDB client globals for Unreal Engine");
        writeln!(output);
        writeln!(output, "class FSpacetimeDBClientGlobals {{");
        writeln!(output, "public:");
        writeln!(output, "    static FString AuthTokenPath;");
        writeln!(output, "    static FString HostURL;");
        writeln!(output, "    static FString DbName;");
        writeln!(output, "}};");

        files.push(("SpacetimeDBClientGlobals.generated.h".to_owned(), output.into_inner()));

        for reducer in module.reducers() {
            let reducer_name = reducer.name.deref();
            let mut cpp = CodeIndenter::new(String::new(), "    ");
            writeln!(cpp, "#include \"Reducers/{}.generated.h\"", reducer_name);
            write!(cpp, "void USpacetimeReducers::CallReducer_{}(", reducer_name);
            let mut first = true;
            for (param, ty) in &reducer.params_for_generate.elements {
                if !first {
                    write!(cpp, ", ");
                }
                first = false;
                write!(cpp, "{} {}", ty_fmt_cpp(ty), param.deref());
            }
            writeln!(cpp, ")");
            writeln!(cpp, "{{");
            writeln!(cpp, "    // TODO: Implement reducer logic");
            writeln!(cpp, "}};");

            files.push((format!("Reducers/{}.generated.cpp", reducer_name), cpp.into_inner()));
        }

        files
    }
}

fn log_reducer_info(reducer: &ReducerDef) {
    println!("Reducer Name: {}", reducer.name);

    if let Some(lifecycle) = reducer.lifecycle {
        println!("Lifecycle: {:?}", lifecycle);
    } else {
        println!("Lifecycle: None");
    }

    println!("Params (for generate):");
    for (param_name, param_type) in &reducer.params_for_generate.elements {
        println!("  - {}: {:?}", param_name, param_type);
    }

    println!("Params (original ProductType):");
    for param in &reducer.params.elements {
        println!(
            "  - {}: {:?}",
            param.name.as_deref().unwrap_or("<unnamed>"),
            param.algebraic_type
        );
    }
}

fn ty_fmt_cpp(ty: &spacetimedb_schema::type_for_generate::AlgebraicTypeUse) -> &'static str {
    use spacetimedb_schema::type_for_generate::AlgebraicTypeUse::*;
    match ty {
        String => "FString",
        Array(_) => "TArray<FString>",
        Identity => "FString",
        Timestamp => "FDateTime",
        Option(_) => "FString",
        Primitive(p) => match p {
            spacetimedb_schema::type_for_generate::PrimitiveType::I32 => "int32",
            spacetimedb_schema::type_for_generate::PrimitiveType::U32 => "uint32",
            spacetimedb_schema::type_for_generate::PrimitiveType::Bool => "bool",
            _ => "FString",
        },
        _ => "FString",
    }
}
