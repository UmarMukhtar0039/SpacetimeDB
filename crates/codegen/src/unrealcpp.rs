use std::fmt::{self, Write};
use std::ops::Deref;
use convert_case::{Case, Casing}; // Ensure Casing trait is in scope for to_case()

use spacetimedb_primitives::ColId;
use spacetimedb_schema::def::{
    BTreeAlgorithm, IndexAlgorithm, ModuleDef, TableDef, TypeDef, ReducerDef, ScopedTypeName,
}; // Import ScopedTypeName
use spacetimedb_schema::def::Identifier;
use spacetimedb_schema::schema::{TableSchema};
use spacetimedb_schema::type_for_generate::{
    AlgebraicTypeDef, AlgebraicTypeUse, PlainEnumTypeDef, PrimitiveType, ProductTypeDef, SumTypeDef,
    TypespaceForGenerate, TypeRef,
};

use super::util::fmt_fn;
use super::code_indenter::CodeIndenter;
use super::Lang;
use crate::util::{
    collect_case, is_reducer_invokable, iter_indexes, iter_reducers, iter_tables,
    print_auto_generated_file_comment, type_ref_name,
};

// Define indentation string for C++
const INDENT: &str = "    ";

// Helper struct for C++ code generation with indentation
// Consolidated into util module below


// Helper function to format SpacetimeDB types to Unreal C++ types
fn cpp_ty_fmt<'a>(module: &'a ModuleDef, ty: &'a AlgebraicTypeUse) -> impl fmt::Display + 'a {
    fmt_fn(move |f| match ty {
        AlgebraicTypeUse::Identity => f.write_str("FSpacetimeDBIdentity"), // Assuming a USTRUCT/struct for Identity
        AlgebraicTypeUse::ConnectionId => f.write_str("FSpacetimeDBConnectionId"), // Assuming a USTRUCT/struct for ConnectionId
        AlgebraicTypeUse::ScheduleAt => f.write_str("FSpacetimeDBScheduleAt"), // Assuming a USTRUCT/struct
        AlgebraicTypeUse::Timestamp => f.write_str("FSpacetimeDBTimestamp"), // Assuming a USTRUCT/struct
        AlgebraicTypeUse::TimeDuration => f.write_str("FSpacetimeDBTimeDuration"), // Assuming a USTRUCT/struct
        AlgebraicTypeUse::Unit => f.write_str("FSpacetimeDBUnit"), // Assuming a USTRUCT/struct for Unit (or void/TSuccess?)
        AlgebraicTypeUse::Option(inner_ty) => {
            // Options can be represented as pointers, TObjects, or special wrapper structs
            // For primitives, a Tizen<Type> or nullable equivalent might be needed.
            // For USTRUCTs/UCLASSes, pointers are common for optionality.
            // Let's use TOptional for primitives and TSharedPtr for complex types as a general approach.
             match &**inner_ty {
                AlgebraicTypeUse::Primitive(_) => write!(f, "TOptional<{}>", cpp_ty_fmt(module, inner_ty)),
                _ => write!(f, "TSharedPtr<{}>", cpp_ty_fmt(module, inner_ty)),
            }
        }
        AlgebraicTypeUse::Array(elem_ty) => write!(f, "TArray<{}>", cpp_ty_fmt(module, elem_ty)),
        AlgebraicTypeUse::String => f.write_str("FString"),
        AlgebraicTypeUse::Ref(r) => {
            // Reference to another defined type (USTRUCT or UCLASS)
            let type_name = type_ref_name(module, *r); // type_ref_name now returns &ScopedTypeName
            let cpp_type_name = collect_case(Case::Pascal, type_name); // collect_case now accepts &ScopedTypeName
            // Decide if it's a USTRUCT or UCLASS based on the definition
            match &module.typespace_for_generate()[*r] {
                 AlgebraicTypeDef::Product(_) => write!(f, "F{}", cpp_type_name), // Assuming Product types become USTRUCTs
                 AlgebraicTypeDef::Sum(_) => write!(f, "F{}", cpp_type_name), // Assuming Sum types become USTRUCTs
                 AlgebraicTypeDef::PlainEnum(_) => write!(f, "E{}", cpp_type_name), // Assuming Plain Enums become UENUMs
            }
        }
        AlgebraicTypeUse::Primitive(prim) => f.write_str(match prim {
            PrimitiveType::Bool => "bool",
            PrimitiveType::I8 => "int8",
            PrimitiveType::U8 => "uint8",
            PrimitiveType::I16 => "int16",
            PrimitiveType::U16 => "uint16",
            PrimitiveType::I32 => "int32",
            PrimitiveType::U32 => "uint32",
            PrimitiveType::I64 => "int64",
            PrimitiveType::U64 => "uint64",
            PrimitiveType::F32 => "float",
            PrimitiveType::F64 => "double",
            // Unreal Engine does not have native 128/256 bit integer types.
            // These would need custom implementation or a third-party library.
            // Placeholder names are used here.
            PrimitiveType::I128 => "FSpacetimeDBInt128",
            PrimitiveType::U128 => "FSpacetimeDBUInt128",
            PrimitiveType::I256 => "FSpacetimeDBInt256",
            PrimitiveType::U256 => "FSpacetimeDBUInt256",
        }),
        AlgebraicTypeUse::Never => unimplemented!("never types are not yet supported in C++ output"),
    })
}

// Helper function to format a field name to PascalCase for C++ properties/methods
fn cpp_field_name_pascal(name: &Identifier) -> String {
    name.deref().to_case(Case::Pascal)
}

// Helper function to format a variable name to camelCase for C++ parameters
fn cpp_var_name_camel(name: &Identifier) -> String {
    name.deref().to_case(Case::Camel)
}

// Helper function to generate a C++ struct definition
fn autogen_cpp_struct(
    module: &ModuleDef,
    name: String,
    product_type: &ProductTypeDef,
    output: &mut CodeIndenter<String>,
) -> fmt::Result { // Return fmt::Result
    writeln!(output, "USTRUCT(BlueprintType)")?;
    writeln!(output, "struct F{}", name)?;
    writeln!(output, "{{")?; // Corrected format string
    output.indent(1);
    writeln!(output, "GENERATED_BODY()")?;
    writeln!(output)?;

    for (orig_name, ty) in product_type.into_iter() {
        let field_name = cpp_field_name_pascal(orig_name);
        let ty_str = cpp_ty_fmt(module, ty).to_string();
        writeln!(output, "UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = \"SpacetimeDB\")")?;
        writeln!(output, "{} {};", ty_str, field_name)?;
        writeln!(output)?;
    }

    // Add default constructor (optional, GENERATED_BODY provides one)
    // writeln!(output, "F{}();", name);

    output.dedent(1);
    writeln!(output, "}}") // Corrected format string
}

// Helper function to generate a C++ enum definition
fn autogen_cpp_enum(name: String, enum_type: &PlainEnumTypeDef, output: &mut CodeIndenter<String>) -> fmt::Result { // Return fmt::Result
    writeln!(output, "UENUM(BlueprintType)")?;
    writeln!(output, "enum class E{} : uint8", name)?; // Use uint8 as the base type for UENUM
    writeln!(output, "{{")?; // Corrected format string
    output.indent(1);

    for variant in &*enum_type.variants {
        // Format Identifier by dereferencing to String
        writeln!(output, "{},", variant.deref())?;
    }

    output.dedent(1);
    writeln!(output, "}}") // Corrected format string
}

// Helper function to generate a C++ sum type definition (using enum + data)
fn autogen_cpp_sum(
    module: &ModuleDef,
    name: String,
    sum_type: &SumTypeDef,
    output: &mut CodeIndenter<String>,
) -> fmt::Result { // Return fmt::Result
    // Generate an enum for the variants
    writeln!(output, "UENUM(BlueprintType)")?;
    writeln!(output, "enum class E{}Variant : uint8", name)?;
    writeln!(output, "{{")?; // Corrected format string
    output.indent(1);
    for (variant_name, _) in sum_type.variants.iter() {
        writeln!(output, "VE_{},", variant_name.deref().to_case(Case::Pascal))?;
    }
    output.dedent(1);
    writeln!(output, "}}")?; // Corrected format string
    writeln!(output)?;

    // Generate a struct to hold the enum and potential data
    writeln!(output, "USTRUCT(BlueprintType)")?;
    writeln!(output, "struct F{}", name)?;
    writeln!(output, "{{")?; // Corrected format string
    output.indent(1);
    writeln!(output, "GENERATED_BODY()")?;
    writeln!(output)?;

    writeln!(output, "UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = \"SpacetimeDB\")")?;
    writeln!(output, "E{}Variant Variant;", name)?;
    writeln!(output)?;

    // Add optional fields for each variant's data
    // This is a simplified approach; a more robust solution might use TUnion or similar.
    for (variant_name, variant_ty) in sum_type.variants.iter() {
         let variant_field_name = format!("As{}", variant_name.deref().to_case(Case::Pascal));
         let variant_ty_str = cpp_ty_fmt(module, variant_ty).to_string();
         // Using Tizen for optional data within the struct
         writeln!(output, "UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = \"SpacetimeDB\")")?;
         writeln!(output, "TOptional<{}> {};", variant_ty_str, variant_field_name)?;
         writeln!(output)?;
    }

    output.dedent(1);
    writeln!(output, "}}") // Corrected format string
}


// Generator struct for Unreal C++
// Marked as pub to be accessible from lib.rs
pub struct UnrealCpp<'opts> {
    pub module_name: &'opts str, // Name for the Unreal Engine module/plugin
}

impl<'opts> UnrealCpp<'opts> {
    // Helper to generate the header file for a type
    fn generate_type_header(&self, module: &ModuleDef, typ: &TypeDef) -> Result<(String, String), fmt::Error> { // Return Result
            let type_name = collect_case(Case::Pascal, type_ref_name(module, typ.ty)); // Use type_ref_name which returns &ScopedTypeName
            let filename = format!("Public/SpacetimeDB/{}/Types/F{}.h", self.module_name, type_name);
            let mut output = CodeIndenter::new(String::new(), INDENT);

            print_auto_generated_file_comment(&mut output)?; // Use ? operator

            // Add standard Unreal Engine includes using the CppAutogen logic
            writeln!(output, "#pragma once")?;
            writeln!(output)?;
            writeln!(output, "#include \"CoreMinimal.h\"")?;
            writeln!(output, "#include \"UObject/ObjectMacros.h\"")?;
            writeln!(output, "#include \"UObject/GeneratedCppIncludes.h\"")?; // Required for USTRUCT/UCLASS
            writeln!(output)?;

            // Add includes for the underlying SpacetimeDB C++ client library
            writeln!(output, "// Assume these headers exist in your SpacetimeDB C++ client library")?;
            writeln!(output, "#include \"SpacetimeDBClientCore.h\"")?; // Core client types (Identity, ConnectionId, Status, etc.)
            // writeln!(output, "#include \"SpacetimeDBClientTables.h\""); // Base table handling - included in table headers
            // writeln!(output, "#include \"SpacetimeDBClientReducers.h\""); // Base reducer handling - included in reducer headers
            // writeln!(output, "#include \"SpacetimeDBClientSubscriptions.h\""); // Base subscription handling
            writeln!(output)?;

            // Add include for the generated header itself (for GENERATED_BODY)
            writeln!(output, "#include \"F{}.generated.h\"", type_name)?;
            writeln!(output)?;

           match &module.typespace_for_generate()[typ.ty] {
                AlgebraicTypeDef::Sum(sum) => autogen_cpp_sum(module, type_name.clone(), sum, &mut output)?,
                AlgebraicTypeDef::Product(prod) => autogen_cpp_struct(module, type_name.clone(), prod, &mut output)?,
                AlgebraicTypeDef::PlainEnum(plain_enum) => autogen_cpp_enum(type_name.clone(), plain_enum, &mut output)?,
            }

            // Return the result explicitly
            Ok((filename, output.into_inner()))
        }

    pub fn type_ref_name(module: &ModuleDef, type_ref: TypeRef) -> impl Iterator<Item = String> + '_ {
        module.typespace_for_generate().names[&type_ref].name_segments()
    }

    // Helper to generate the header file for a table
    fn generate_table_header(&self, module: &ModuleDef, table: &TableDef) -> Result<(String, String), fmt::Error> { // Return Result
        let table_name_pascal = table.name.deref().to_case(Case::Pascal);
        let filename = format!("Public/SpacetimeDB/{}/Tables/U{}.h", self.module_name, table_name_pascal);
        let mut output = CodeIndenter::new(String::new(), INDENT);

        print_auto_generated_file_comment(&mut output)?; // Use ? operator

        // Add standard Unreal Engine includes using the CppAutogen logic
        writeln!(output, "#pragma once")?;
        writeln!(output)?;
        writeln!(output, "#include \"CoreMinimal.h\"")?;
        writeln!(output, "#include \"UObject/ObjectMacros.h\"")?;
        writeln!(output, "#include \"UObject/GeneratedCppIncludes.h\"")?; // Required for USTRUCT/UCLASS
        writeln!(output)?;

        // Add includes for the underlying SpacetimeDB C++ client library
        writeln!(output, "// Assume these headers exist in your SpacetimeDB C++ client library")?;
        writeln!(output, "#include \"SpacetimeDBClientCore.h\"")?; // Core client types (Identity, ConnectionId, Status, etc.)
        writeln!(output, "#include \"SpacetimeDBClientTables.h\"")?; // Base table handling
        // writeln!(output, "#include \"SpacetimeDBClientReducers.h\""); // Base reducer handling
        // writeln!(output, "#include \"SpacetimeDBClientSubscriptions.h\""); // Base subscription handling
        writeln!(output)?;


        // Include the header for the table's data structure
        writeln!(
                output,
                "#include \"SpacetimeDB/{}/Types/F{}.h\"",
                self.module_name,
                collect_case(Case::Pascal, type_ref_name(module, table.product_type_ref))
            )?;


        // Include event types
        writeln!(output, "#include \"SpacetimeDB/{}/SpacetimeDBTypes.h\"", self.module_name)?; // Use ?
        writeln!(output)?;

        // Add include for the generated header itself
        writeln!(output, "#include \"U{}.generated.h\"", table_name_pascal)?; // Use ?
        writeln!(output)?;

        let type_ref = AlgebraicTypeUse::Ref(table.product_type_ref);
        let table_data_type = cpp_ty_fmt(module, &type_ref).to_string();

        writeln!(output, "UCLASS()")?;
        writeln!(output, "class SPACETIMEDBUNREALSDK_API U{} : public USpacetimeDBTableHandle<{}>", table_name_pascal, table_data_type)?;
        writeln!(output, "{{")?; // Corrected format string
        output.indent(1);
        writeln!(output, "GENERATED_BODY()")?;
        writeln!(output)?;

        writeln!(output, "public:")?;
        writeln!(output, "U{}(const FObjectInitializer& ObjectInitializer = FObjectInitializer::Get());", table_name_pascal)?; // Use ?
        writeln!(output)?;

        // Add index accessors
        for idx in iter_indexes(table) {
            let Some(accessor_name) = idx.accessor_name.as_ref() else {
                continue;
            };
            let cpp_accessor_name = cpp_field_name_pascal(accessor_name);

            match &idx.algorithm {
    IndexAlgorithm::BTree(BTreeAlgorithm { columns }) => {
        // Get typespace and product type in advance to avoid temporary borrow issues
        let typespace = module.typespace_for_generate();
        let product = match &typespace[table.product_type_ref] {
            AlgebraicTypeDef::Product(prod) => prod,
            _ => panic!("Expected product type"),
        };

        let schema = TableSchema::<(), ColId>::from_module_def(module, table, (), ColId(0))
            .validated()
            .expect("Failed to get schema for index");

        let is_unique = schema.is_unique(columns);

        let mut key_params = Vec::new();
        for col_pos in columns.iter() {
            let (field_name, field_type) = &product.elements[col_pos.idx()];
            let cpp_field_type = cpp_ty_fmt(module, field_type).to_string();
            let cpp_param_name = cpp_var_name_camel(field_name);
            key_params.push(format!("const {}& {}", cpp_field_type, cpp_param_name));
        }
        let key_params_str = key_params.join(", ");

        if is_unique {
            writeln!(output, "/** Unique index accessor for {} */", accessor_name)?;
            writeln!(output, "UFUNCTION(BlueprintPure, Category = \"SpacetimeDB|Tables|{}\")", table_name_pascal)?;
            writeln!(output, "{}* GetBy{}({});", table_data_type, cpp_accessor_name, key_params_str)?;
            writeln!(output)?;
        } else {
            writeln!(output, "/** Index accessor for {} */", accessor_name)?;
            writeln!(output, "UFUNCTION(BlueprintPure, Category = \"SpacetimeDB|Tables|{}\")", table_name_pascal)?;
            writeln!(output, "TArray<{}*> GetBy{}({});", table_data_type, cpp_accessor_name, key_params_str)?;
            writeln!(output)?;
        }
    }
    _ => {
        writeln!(output, "// Index algorithm not yet supported in C++ generator for accessor: {}", accessor_name)?;
        writeln!(output)?;
    }
}

        }


        // Add primary key accessor if it exists
       let schema = TableSchema::<(), usize>::from_module_def(module, table, (), 0)
        .validated()
        .expect("Failed to get schema for PK");

        if let Some(primary_col_index) = schema.pk() {
            let col_id = primary_col_index; // ColId

            // 🛠️ FIX: Bind to typespace first to extend lifetime
            let typespace = module.typespace_for_generate();
            let algebraic_def = &typespace[table.product_type_ref];
            let product = match algebraic_def {
                AlgebraicTypeDef::Product(prod) => prod,
                _ => panic!("Expected Product type for table {}", table.name),
            };

            let (col_name, col_ty) = &product.elements[col_id.idx()];
            let pk_col_name = col_name.deref().to_case(Case::Pascal);
            let pk_col_type = cpp_ty_fmt(module, col_ty).to_string();

            writeln!(output, "/** Primary key accessor */")?;
            writeln!(
                output,
                "UFUNCTION(BlueprintPure, Category = \"SpacetimeDB|Tables|{}\")",
                table_name_pascal
            )?;
            writeln!(
                output,
                "{}* GetByPrimaryKey(const {}& PrimaryKey);",
                table_data_type, pk_col_type
            )?;
            writeln!(output)?;
        }

        // Add table event delegates
        writeln!(output, "/** Delegate for when a row is added to this table */")?; // Use ?
        writeln!(output, "DECLARE_DYNAMIC_MULTICAST_DELEGATE_OneParam(FOn{}RowAdded, const {}&, Row);", table_name_pascal, table_data_type)?; // Use ?
        writeln!(output, "UPROPERTY(BlueprintAssignable, Category = \"SpacetimeDB|Events|Tables\")")?; // Use ?
        writeln!(output, "FOn{}RowAdded OnRowAdded;", table_name_pascal)?; // Use ?
        writeln!(output)?;

        writeln!(output, "/** Delegate for when a row in this table is updated */")?; // Use ?
        writeln!(output, "DECLARE_DYNAMIC_MULTICAST_DELEGATE_TwoParams(FOn{}RowUpdated, const {}&, OldRow, const {}&, NewRow);", table_name_pascal, table_data_type, table_data_type)?; // Use ?
        writeln!(output, "UPROPERTY(BlueprintAssignable, Category = \"SpacetimeDB|Events|Tables\")")?; // Use ?
        writeln!(output, "FOn{}RowUpdated OnRowUpdated;", table_name_pascal)?; // Use ?
        writeln!(output)?;

        writeln!(output, "/** Delegate for when a row is removed from this table */")?; // Use ?
        writeln!(output, "DECLARE_DYNAMIC_MULTICAST_DELEGATE_OneParam(FOn{}RowRemoved, const {}&, Row);", table_name_pascal, table_data_type)?; // Use ?
        writeln!(output, "UPROPERTY(BlueprintAssignable, Category = \"SpacetimeDB|Events|Tables\")")?; // Use ?
        writeln!(output, "FOn{}RowRemoved OnRowRemoved;", table_name_pascal)?; // Use ?
        writeln!(output)?;


        output.dedent(1);
        // Return the result explicitly
        Ok((filename, output.into_inner()))
    }

     // Helper to generate the header file for a reducer
     fn generate_reducer_header(&self, module: &ModuleDef, reducer: &ReducerDef) -> Result<(String, String), fmt::Error> { // Return Result
        let reducer_name_pascal = reducer.name.deref().to_case(Case::Pascal);
        let filename = format!("Public/SpacetimeDB/{}/Reducers/U{}.h", self.module_name, reducer_name_pascal);
        let mut output = CodeIndenter::new(String::new(), INDENT);

        print_auto_generated_file_comment(&mut output)?; // Use ? operator

        // Add standard Unreal Engine includes using the CppAutogen logic
        writeln!(output, "#pragma once")?;
        writeln!(output)?;
        writeln!(output, "#include \"CoreMinimal.h\"")?;
        writeln!(output, "#include \"UObject/ObjectMacros.h\"")?;
        writeln!(output, "#include \"UObject/GeneratedCppIncludes.h\"")?; // Required for USTRUCT/UCLASS
        writeln!(output)?;

        // Add includes for the underlying SpacetimeDB C++ client library
        writeln!(output, "// Assume these headers exist in your SpacetimeDB C++ client library")?;
        writeln!(output, "#include \"SpacetimeDBClientCore.h\"")?; // Core client types (Identity, ConnectionId, Status, etc.)
        // writeln!(output, "#include \"SpacetimeDBClientTables.h\""); // Base table handling
        writeln!(output, "#include \"SpacetimeDBClientReducers.h\"")?; // Base reducer handling
        // writeln!(output, "#include \"SpacetimeDBClientSubscriptions.h\""); // Base subscription handling
        writeln!(output)?;


        // Include event context structures
        writeln!(output, "#include \"SpacetimeDB/{}/SpacetimeDBTypes.h\"", self.module_name)?; // Use ?
        writeln!(output)?;

        // Add include for the generated header itself
        writeln!(output, "#include \"U{}.generated.h\"", reducer_name_pascal)?; // Use ?
        writeln!(output)?;

        // Generate the USTRUCT for reducer arguments
        let args_struct_name = format!("F{}Args", reducer_name_pascal);
        // Format Identifier by dereferencing to String
        writeln!(output, "// Arguments for the '{}' reducer", reducer.name.deref())?; // Use ?
        writeln!(output, "USTRUCT(BlueprintType)")?;
        writeln!(output, "struct {} : public SpacetimeDB::IReducerArgs", args_struct_name)?;
        writeln!(output, "{{")?; // Corrected format string
        output.indent(1);
        writeln!(output, "GENERATED_BODY()")?;
        writeln!(output)?;

        for (arg_name, arg_ty) in reducer.params_for_generate.into_iter() {
            let cpp_arg_name = cpp_field_name_pascal(arg_name);
            let cpp_arg_ty = cpp_ty_fmt(module, arg_ty).to_string();
            writeln!(output, "UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = \"SpacetimeDB|Reducers\")")?;
            writeln!(output, "{} {};", cpp_arg_ty, cpp_arg_name)?;
            writeln!(output)?;
        }

        // Implement the IReducerArgs interface method
        writeln!(output, "// SpacetimeDB::IReducerArgs implementation")?;
        // Format Identifier by dereferencing to String
        writeln!(output, "virtual FString GetReducerName() const override {{ return TEXT(\"{}\"); }}", reducer.name.deref())?; // Use ?
        writeln!(output)?;


        output.dedent(1);
        writeln!(output, "}}")?; // Corrected format string
        writeln!(output)?;


        // Generate the UCLASS for the reducer handler
        writeln!(output, "UCLASS()")?;
        writeln!(output, "class SPACETIMEDBUNREALSDK_API U{} : public USpacetimeDBReducerHandle", reducer_name_pascal)?;
        writeln!(output, "{{")?; // Corrected format string
        output.indent(1);
        writeln!(output, "GENERATED_BODY()")?;
        writeln!(output)?;

        writeln!(output, "public:")?;
        writeln!(output, "U{}(const FObjectInitializer& ObjectInitializer = FObjectInitializer::Get());", reducer_name_pascal)?; // Use ?
        writeln!(output)?;

        if is_reducer_invokable(reducer) {
            // Generate the method to call the reducer
            let mut func_params = Vec::new();
            for (arg_name, arg_ty) in reducer.params_for_generate.into_iter() {
                let cpp_arg_name = cpp_var_name_camel(arg_name);
                let cpp_arg_ty = cpp_ty_fmt(module, arg_ty).to_string();
                func_params.push(format!("const {}& {}", cpp_arg_ty, cpp_arg_name));
            }
            let func_params_str = func_params.join(", ");

            // Format Identifier by dereferencing to String
            writeln!(output, "/** Call the '{}' reducer */", reducer.name.deref())?; // Use ?
            writeln!(output, "UFUNCTION(BlueprintCallable, Category = \"SpacetimeDB|Reducers\")")?;
            writeln!(output, "void Call({});", func_params_str)?; // Use ?
            writeln!(output)?;

            // Format Identifier by dereferencing to String
            writeln!(output, "/** Set call flags for the '{}' reducer */", reducer.name.deref())?; // Use ?
            writeln!(output, "UFUNCTION(BlueprintCallable, Category = \"SpacetimeDB|Reducers\")")?;
            writeln!(output, "void SetCallFlags(const SpacetimeDB::FCallReducerFlags& Flags);")?; // Use ?
            writeln!(output)?;

            // Add internal storage for flags
            writeln!(output, "protected:")?;
            writeln!(output, "SpacetimeDB::FCallReducerFlags CallFlags;")?;
            writeln!(output)?;
        }

        // Generate the delegate for reducer completion
        // Format Identifier by dereferencing to String
        writeln!(output, "/** Delegate for when the '{}' reducer completes */", reducer.name.deref())?; // Use ?
        writeln!(output, "DECLARE_DYNAMIC_MULTICAST_DELEGATE_TwoParams(FOn{}Complete, const FSpacetimeDBReducerEventContext&, Context, const {}&, Args);", reducer_name_pascal, args_struct_name)?; // Use ?
        writeln!(output, "UPROPERTY(BlueprintAssignable, Category = \"SpacetimeDB|Events|Reducers\")")?; // Use ?
        writeln!(output, "FOn{}Complete OnComplete;", reducer_name_pascal)?; // Use ?
        writeln!(output)?;

        // Generate the delegate for reducer errors
        // Format Identifier by dereferencing to String
        writeln!(output, "/** Delegate for when the '{}' reducer fails */", reducer.name.deref())?; // Use ?
        writeln!(output, "DECLARE_DYNAMIC_MULTICAST_DELEGATE_TwoParams(FOn{}Error, const FSpacetimeDBReducerEventContext&, Context, const FString&, ErrorMessage);", reducer_name_pascal)?; // Or use FSpacetimeDBErrorContext? // Use ?
        writeln!(output, "UPROPERTY(BlueprintAssignable, Category = \"SpacetimeDB|Events|Reducers\")")?; // Use ?
        writeln!(output, "FOn{}Error OnError;", reducer_name_pascal)?; // Use ?
        writeln!(output)?;

        output.dedent(1);
        // Return the result explicitly
        Ok((filename, output.into_inner()))
    }

    // Helper to generate the main client header file (globals)
    fn generate_client_header(&self, module: &ModuleDef) -> Result<(String, String), fmt::Error> { // Return Result
        let filename = format!("Public/SpacetimeDB/{}/SpacetimeDBClient.h", self.module_name);
        let mut output = CodeIndenter::new(String::new(), INDENT);

        print_auto_generated_file_comment(&mut output)?; // Use ? operator

        // Add standard Unreal Engine includes using the CppAutogen logic
        writeln!(output, "#pragma once")?;
        writeln!(output)?;
        writeln!(output, "#include \"CoreMinimal.h\"")?;
        writeln!(output, "#include \"UObject/ObjectMacros.h\"")?;
        writeln!(output, "#include \"UObject/GeneratedCppIncludes.h\"")?; // Required for USTRUCT/UCLASS
        writeln!(output)?;

        // Add includes for the underlying SpacetimeDB C++ client library
        writeln!(output, "// Assume these headers exist in your SpacetimeDB C++ client library")?;
        writeln!(output, "#include \"SpacetimeDBClientCore.h\"")?; // Core client types (Identity, ConnectionId, Status, etc.)
        writeln!(output, "#include \"SpacetimeDBClientBase.h\"")?; // Base client class/interface
        // writeln!(output, "#include \"SpacetimeDBClientTables.h\""); // Base table handling
        // writeln!(output, "#include \"SpacetimeDBClientReducers.h\""); // Base reducer handling
        // writeln!(output, "#include \"SpacetimeDBClientSubscriptions.h\""); // Base subscription handling
        writeln!(output)?;


        // Include generated table and reducer headers
        writeln!(output, "#include \"SpacetimeDB/{}/Tables/USpacetimeDBTables.h\"", self.module_name)?; // Use ?
        writeln!(output, "#include \"SpacetimeDB/{}/Reducers/USpacetimeDBReducers.h\"", self.module_name)?; // Use ?
        writeln!(output, "#include \"SpacetimeDB/{}/SpacetimeDBTypes.h\"", self.module_name)?; // Event contexts, etc. // Use ?
        writeln!(output)?;

        // Add include for the generated header itself
        writeln!(output, "#include \"SpacetimeDBClient.generated.h\"")?; // Use ?
        writeln!(output)?;

        writeln!(output, "UCLASS(BlueprintType)")?;
        writeln!(output, "class SPACETIMEDBUNREALSDK_API USpacetimeDBClient : public USpacetimeDBClientBase")?; // Inherit from a base client class
        writeln!(output, "{{")?; // Corrected format string
        output.indent(1);
        writeln!(output, "GENERATED_BODY()")?;
        writeln!(output)?;

        writeln!(output, "public:")?;
        writeln!(output, "UUSpacetimeDBClient(const FObjectInitializer& ObjectInitializer = FObjectInitializer::Get());")?; // Use ?
        writeln!(output)?;

        // Add properties for generated tables and reducers access classes
        writeln!(output, "UPROPERTY(BlueprintReadOnly, Category = \"SpacetimeDB\")")?;
        writeln!(output, "USpacetimeDBTables* Db;")?;
        writeln!(output)?;

        writeln!(output, "UPROPERTY(BlueprintReadOnly, Category = \"SpacetimeDB\")")?;
        writeln!(output, "USpacetimeDBReducers* Reducers;")?;
        writeln!(output)?;

        // Add properties for connection and error delegates (defined in SpacetimeDBTypes.h)
        writeln!(output, "UPROPERTY(BlueprintAssignable, Category = \"SpacetimeDB|Events\")")?;
        writeln!(output, "FOnSpacetimeDBConnected OnConnected;")?;
        writeln!(output)?;

        writeln!(output, "UPROPERTY(BlueprintAssignable, Category = \"SpacetimeDB|Events\")")?;
        writeln!(output, "FOnSpacetimeDBDisconnected OnDisconnected;")?;
        writeln!(output)?;

        writeln!(output, "UPROPERTY(BlueprintAssignable, Category = \"SpacetimeDB|Events\")")?;
        writeln!(output, "FOnSpacetimeDBError OnError;")?;
        writeln!(output)?;

        writeln!(output, "UPROPERTY(BlueprintAssignable, Category = \"SpacetimeDB|Events|Reducers\")")?;
        writeln!(output, "FOnSpacetimeDBError OnUnhandledReducerError;")?;
        writeln!(output)?;


        // Implement base client methods (delegated to the underlying C++ client)
        writeln!(output, "// Implement USpacetimeDBClientBase interface")?;
        writeln!(output, "virtual void Connect(const FString& Host, const FString& Token = TEXT(\"\")) override;")?; // Use ?
        writeln!(output, "virtual void Disconnect() override;")?; // Use ?
        writeln!(output, "virtual bool IsConnected() const override;")?; // Use ?
        writeln!(output, "virtual USpacetimeDBSubscriptionBuilder* CreateSubscriptionBuilder() override;")?; // Use ?
        writeln!(output)?;

        writeln!(output, "// Internal dispatch method called by the underlying client")?;
        writeln!(output, "virtual bool DispatchReducerEvent(const FSpacetimeDBReducerEventContext& Context, const FString& ReducerName, const TArray<uint8>& ArgsData) override;")?; // Use ?
        writeln!(output)?;

        writeln!(output, "// Internal dispatch method called by the underlying client for table events")?;
        writeln!(output, "virtual void DispatchTableEvent(const FString& TableName, ESpacetimeDBTableEventType EventType, const TArray<uint8>& RowData, const TArray<uint8>& OldRowData) override;")?; // Use ?
        writeln!(output)?;


        output.dedent(1);
        // Return the result explicitly
        Ok((filename, output.into_inner()))
    }

    // Helper to generate the main tables access header file
    fn generate_tables_header(&self, module: &ModuleDef) -> Result<(String, String), fmt::Error> { // Return Result
        let filename = format!("Public/SpacetimeDB/{}/Tables/USpacetimeDBTables.h", self.module_name);
        let mut output = CodeIndenter::new(String::new(), INDENT);

        print_auto_generated_file_comment(&mut output)?; // Use ? operator

        // Add standard Unreal Engine includes using the CppAutogen logic
        writeln!(output, "#pragma once")?;
        writeln!(output)?;
        writeln!(output, "#include \"CoreMinimal.h\"")?;
        writeln!(output, "#include \"UObject/ObjectMacros.h\"")?;
        writeln!(output, "#include \"UObject/GeneratedCppIncludes.h\"")?; // Required for USTRUCT/UCLASS
        writeln!(output)?;

        // Add includes for the underlying SpacetimeDB C++ client library
        writeln!(output, "// Assume these headers exist in your SpacetimeDB C++ client library")?;
        // writeln!(output, "#include \"SpacetimeDBClientCore.h\""); // Core client types (Identity, ConnectionId, Status, etc.)
        // writeln!(output, "#include \"SpacetimeDBClientTables.h\""); // Base table handling
        // writeln!(output, "#include \"SpacetimeDBClientReducers.h\""); // Base reducer handling
        // writeln!(output, "#include \"SpacetimeDBClientSubscriptions.h\""); // Base subscription handling
        writeln!(output)?;


         // Include event types
        writeln!(output, "#include \"SpacetimeDB/{}/SpacetimeDBTypes.h\"", self.module_name)?; // Use ?
        writeln!(output)?;

        // Include headers for all generated tables
        for table in iter_tables(module) {
            let table_name_pascal = table.name.deref().to_case(Case::Pascal);
            writeln!(output, "#include \"U{}.h\"", table_name_pascal)?; // Use ?
        }
        writeln!(output)?;

        // Include the generated header itself
        writeln!(output, "#include \"USpacetimeDBTables.generated.h\"")?; // Use ?
        writeln!(output)?;

        writeln!(output, "UCLASS()")?;
        writeln!(output, "class SPACETIMEDBUNREALSDK_API USpacetimeDBTables : public UObject")?;
        writeln!(output, "{{")?; // Corrected format string
        output.indent(1);
        writeln!(output, "GENERATED_BODY()")?;
        writeln!(output)?;

        writeln!(output, "public:")?;
        writeln!(output, "UUSpacetimeDBTables(const FObjectInitializer& ObjectInitializer = FObjectInitializer::Get());")?; // Use ?
        writeln!(output)?;

        // Add properties for each generated table handler
        for table in iter_tables(module) {
            let table_name_pascal = table.name.deref().to_case(Case::Pascal);
            writeln!(output, "UPROPERTY(BlueprintReadOnly, Category = \"SpacetimeDB|Tables\")")?;
            writeln!(output, "U{}* {};", table_name_pascal, table_name_pascal)?; // Use ?
            writeln!(output)?;
        }

        output.dedent(1);
        // Return the result explicitly
        Ok((filename, output.into_inner()))
    }

     // Helper to generate the main reducers access header file
     fn generate_reducers_header(&self, module: &ModuleDef) -> Result<(String, String), fmt::Error> { // Return Result
        let filename = format!("Public/SpacetimeDB/{}/Reducers/USpacetimeDBReducers.h", self.module_name);
        let mut output = CodeIndenter::new(String::new(), INDENT);

        print_auto_generated_file_comment(&mut output)?; // Use ? operator

        // Add standard Unreal Engine includes using the CppAutogen logic
        writeln!(output, "#pragma once")?;
        writeln!(output)?;
        writeln!(output, "#include \"CoreMinimal.h\"")?;
        writeln!(output, "#include \"UObject/ObjectMacros.h\"")?;
        writeln!(output, "#include \"UObject/GeneratedCppIncludes.h\"")?; // Required for USTRUCT/UCLASS
        writeln!(output)?;

        // Add includes for the underlying SpacetimeDB C++ client library
        writeln!(output, "// Assume these headers exist in your SpacetimeDB C++ client library")?;
        // writeln!(output, "#include \"SpacetimeDBClientCore.h\""); // Core client types (Identity, ConnectionId, Status, etc.)
        // writeln!(output, "#include \"SpacetimeDBClientTables.h\""); // Base table handling
        // writeln!(output, "#include \"SpacetimeDBClientReducers.h\""); // Base reducer handling
        // writeln!(output, "#include \"SpacetimeDBClientSubscriptions.h\""); // Base subscription handling
        writeln!(output)?;

        // Include headers for all generated reducers
        for reducer in iter_reducers(module) {
             let reducer_name_pascal = reducer.name.deref().to_case(Case::Pascal);
             writeln!(output, "#include \"U{}.h\"", reducer_name_pascal)?; // Use ?
        }
        writeln!(output)?;

        // Include the generated header itself
        writeln!(output, "#include \"USpacetimeDBReducers.generated.h\"")?; // Use ?
        writeln!(output)?;

        writeln!(output, "UCLASS()")?;
        writeln!(output, "class SPACETIMEDBUNREALSDK_API USpacetimeDBReducers : public UObject")?;
        writeln!(output, "{{")?; // Corrected format string
        output.indent(1);
        writeln!(output, "GENERATED_BODY()")?;
        writeln!(output)?;

        writeln!(output, "public:")?;
        writeln!(output, "UUSpacetimeDBReducers(const FObjectInitializer& ObjectInitializer = FObjectInitializer::Get());")?; // Use ?
        writeln!(output)?;

        // Add properties for each generated reducer handler
        for reducer in iter_reducers(module) {
            let reducer_name_pascal = reducer.name.deref().to_case(Case::Pascal);
            writeln!(output, "UPROPERTY(BlueprintReadOnly, Category = \"SpacetimeDB|Reducers\")")?;
            writeln!(output, "U{}* {};", reducer_name_pascal, reducer_name_pascal)?; // Use ?
            writeln!(output)?;
        }

        output.dedent(1);
        // Return the result explicitly
        Ok((filename, output.into_inner()))
    }


     // Helper to generate the header for SetReducerFlags
     fn generate_set_reducer_flags_header(&self, module: &ModuleDef) -> Result<(String, String), fmt::Error> { // Return Result
        let filename = format!("Public/SpacetimeDB/{}/Reducers/USpacetimeDBSetReducerFlags.h", self.module_name);
        let mut output = CodeIndenter::new(String::new(), INDENT);

        print_auto_generated_file_comment(&mut output)?; // Use ? operator

        // Add standard Unreal Engine includes using the CppAutogen logic
        writeln!(output, "#pragma once")?;
        writeln!(output)?;
        writeln!(output, "#include \"CoreMinimal.h\"")?;
        writeln!(output, "#include \"UObject/ObjectMacros.h\"")?;
        writeln!(output, "#include \"UObject/GeneratedCppIncludes.h\"")?; // Required for USTRUCT/UCLASS
        writeln!(output)?;

        // Add includes for the underlying SpacetimeDB C++ client library
        writeln!(output, "// Assume these headers exist in your SpacetimeDB C++ client library")?;
        writeln!(output, "#include \"SpacetimeDBClientReducers.h\"")?; // For SpacetimeDB::FCallReducerFlags
        // writeln!(output, "#include \"SpacetimeDBClientCore.h\""); // Core client types (Identity, ConnectionId, Status, etc.)
        // writeln!(output, "#include \"SpacetimeDBClientTables.h\""); // Base table handling
        // writeln!(output, "#include \"SpacetimeDBClientSubscriptions.h\""); // Base subscription handling
        writeln!(output)?;


        // Include the generated header itself
        writeln!(output, "#include \"USpacetimeDBSetReducerFlags.generated.h\"")?; // Use ?
        writeln!(output)?;

        writeln!(output, "UCLASS()")?;
        writeln!(output, "class SPACETIMEDBUNREALSDK_API USpacetimeDBSetReducerFlags : public UObject")?;
        writeln!(output, "{{")?; // Corrected format string
        output.indent(1);
        writeln!(output, "GENERATED_BODY()")?;
        writeln!(output)?;

        writeln!(output, "public:").unwrap();
        writeln!(output, "UUSpacetimeDBSetReducerFlags(const FObjectInitializer& ObjectInitializer = FObjectInitializer::Get());")?; // Use ?
        writeln!(output)?;

        // Add properties for each reducer's flags
        for reducer in iter_reducers(module) {
            if is_reducer_invokable(reducer) {
                let reducer_name_pascal = reducer.name.deref().to_case(Case::Pascal);
                writeln!(output, "UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = \"SpacetimeDB|ReducerFlags\")")?;
                writeln!(output, "SpacetimeDB::FCallReducerFlags {}Flags;", reducer_name_pascal)?; // Use ?
                writeln!(output)?;
            }
        }

        output.dedent(1);
        // Return the result explicitly
        Ok((filename, output.into_inner()))
    }

     // Helper to generate the header for SubscriptionBuilder
     fn generate_subscription_builder_header(&self, module: &ModuleDef) -> Result<(String, String), fmt::Error> { // Return Result
        let filename = format!("Public/SpacetimeDB/{}/Subscriptions/USpacetimeDBSubscriptionBuilder.h", self.module_name);
        let mut output = CodeIndenter::new(String::new(), INDENT);

        print_auto_generated_file_comment(&mut output)?; // Use ? operator

        // Add standard Unreal Engine includes using the CppAutogen logic
        writeln!(output, "#pragma once")?;
        writeln!(output)?;
        writeln!(output, "#include \"CoreMinimal.h\"")?;
        writeln!(output, "#include \"UObject/ObjectMacros.h\"")?;
        writeln!(output, "#include \"UObject/GeneratedCppIncludes.h\"")?; // Required for USTRUCT/UCLASS
        writeln!(output)?;

        // Add includes for the underlying SpacetimeDB C++ client library
        writeln!(output, "// Assume these headers exist in your SpacetimeDB C++ client library")?;
        writeln!(output, "#include \"SpacetimeDBClientSubscriptions.h\"")?; // Base subscription builder class
        // writeln!(output, "#include \"SpacetimeDBClientCore.h\""); // Core client types (Identity, ConnectionId, Status, etc.)
        // writeln!(output, "#include \"SpacetimeDBClientTables.h\""); // Base table handling
        // writeln!(output, "#include \"SpacetimeDBClientReducers.h\""); // Base reducer handling
        writeln!(output)?;


        // Include event context structures and delegates
        writeln!(output, "#include \"SpacetimeDB/{}/SpacetimeDBTypes.h\"", self.module_name)?; // Use ?
        writeln!(output)?;

        // Add include for the generated header itself
        writeln!(output, "#include \"USpacetimeDBSubscriptionBuilder.generated.h\"")?; // Use ?
        writeln!(output).unwrap();

        writeln!(output, "UCLASS(BlueprintType)")?;
        writeln!(output, "class SPACETIMEDBUNREALSDK_API USpacetimeDBSubscriptionBuilder : public USpacetimeDBSubscriptionBuilderBase")?; // Inherit from a base builder class
        writeln!(output, "{{")?; // Corrected format string
        output.indent(1);
        writeln!(output, "GENERATED_BODY()")?;
        writeln!(output)?;

        writeln!(output, "public:")?;
        writeln!(output, "UUSpacetimeDBSubscriptionBuilder(const FObjectInitializer& ObjectInitializer = FObjectInitializer::Get());")?; // Use ?
        writeln!(output)?;

        // Add methods for setting callbacks
        writeln!(output, "/** Register a callback to run when the subscription is applied. */")?; // Use ?
        writeln!(output, "UFUNCTION(BlueprintCallable, Category = \"SpacetimeDB|Subscriptions\")")?;
        writeln!(output, "USpacetimeDBSubscriptionBuilder* OnApplied(FOnSubscriptionApplied Callback);")?; // Use ?
        writeln!(output)?;

        writeln!(output, "/** Register a callback to run when the subscription fails. */")?; // Use ?
        writeln!(output, "UFUNCTION(BlueprintCallable, Category = \"SpacetimeDB|Subscriptions\")")?;
        writeln!(output, "USpacetimeDBSubscriptionBuilder* OnError(FOnSubscriptionError Callback);")?; // Use ?
        writeln!(output)?;

        // Add methods for subscribing
        writeln!(output, "/** Subscribe to the following SQL queries. */")?; // Use ?
        writeln!(output, "UFUNCTION(BlueprintCallable, Category = \"SpacetimeDB|Subscriptions\")")?;
        writeln!(output, "USpacetimeDBSubscriptionHandle* Subscribe(const TArray<FString>& QuerySqls);")?; // Use ?
        writeln!(output)?;

        writeln!(output, "/** Subscribe to all rows from all tables. */")?; // Use ?
        writeln!(output, "UFUNCTION(BlueprintCallable, Category = \"SpacetimeDB|Subscriptions\")")?;
        writeln!(output, "void SubscribeToAllTables();")?; // Use ?
        writeln!(output)?;


        output.dedent(1);
        // Return the result explicitly
        Ok((filename, output.into_inner()))
    }

     // Helper to generate the header for SubscriptionHandle
     fn generate_subscription_handle_header(&self, module: &ModuleDef) -> Result<(String, String), fmt::Error> { // Return Result
        let filename = format!("Public/SpacetimeDB/{}/Subscriptions/USpacetimeDBSubscriptionHandle.h", self.module_name);
        let mut output = CodeIndenter::new(String::new(), INDENT);

        print_auto_generated_file_comment(&mut output)?; // Use ? operator

        // Add standard Unreal Engine includes using the CppAutogen logic
        writeln!(output, "#pragma once")?;
        writeln!(output)?;
        writeln!(output, "#include \"CoreMinimal.h\"")?;
        writeln!(output, "#include \"UObject/ObjectMacros.h\"")?;
        writeln!(output, "#include \"UObject/GeneratedCppIncludes.h\"")?; // Required for USTRUCT/UCLASS
        writeln!(output)?;

        // Add includes for the underlying SpacetimeDB C++ client library
        writeln!(output, "// Assume these headers exist in your SpacetimeDB C++ client library")?;
        writeln!(output, "#include \"SpacetimeDBClientSubscriptions.h\"")?; // Base subscription handle class
        // writeln!(output, "#include \"SpacetimeDBClientCore.h\""); // Core client types (Identity, ConnectionId, Status, etc.)
        // writeln!(output, "#include \"SpacetimeDBClientTables.h\""); // Base table handling
        // writeln!(output, "#include \"SpacetimeDBClientReducers.h\""); // Base reducer handling
        writeln!(output)?;


        // Include the generated header itself
        writeln!(output, "#include \"USpacetimeDBSubscriptionHandle.generated.h\"")?; // Use ?
        writeln!(output)?;

        writeln!(output, "UCLASS(BlueprintType)")?;
        writeln!(output, "class SPACETIMEDBUNREALSDK_API USpacetimeDBSubscriptionHandle : public USpacetimeDBSubscriptionHandleBase")?; // Inherit from a base handle class
        writeln!(output, "{{")?; // Corrected format string
        output.indent(1);
        writeln!(output, "GENERATED_BODY()")?;
        writeln!(output)?;

        writeln!(output, "public:")?;
        writeln!(output, "UUSpacetimeDBSubscriptionHandle(const FObjectInitializer& ObjectInitializer = FObjectInitializer::Get());")?; // Use ?
        writeln!(output)?;

        // Add methods to manage the subscription (e.g., unsubscribe)
        // writeln!(output, "/** Unsubscribe from this query. */")?; // Use ?
        // writeln!(output, "UFUNCTION(BlueprintCallable, Category = \"SpacetimeDB|Subscriptions\")")?;
        // writeln!(output, "void Unsubscribe();")?; // Use ?
        // writeln!(output)?;


        output.dedent(1);
        // Return the result explicitly
        Ok((filename, output.into_inner()))
    }

     // Helper to generate the core SpacetimeDBTypes header
     fn generate_core_types_header(&self, module: &ModuleDef) -> Result<(String, String), fmt::Error> { // Return Result
        let filename = format!("Public/SpacetimeDB/{}/SpacetimeDBTypes.h", self.module_name);
        let mut output = CodeIndenter::new(String::new(), INDENT);

        print_auto_generated_file_comment(&mut output)?; // Use ? operator

        // Add standard Unreal Engine includes using the CppAutogen logic
        writeln!(output, "#pragma once")?;
        writeln!(output)?;
        writeln!(output, "#include \"CoreMinimal.h\"")?;
        writeln!(output, "#include \"UObject/ObjectMacros.h\"")?;
        writeln!(output, "#include \"UObject/GeneratedCppIncludes.h\"")?; // Required for USTRUCT/UCLASS
        writeln!(output)?;

        // Add includes for the underlying SpacetimeDB C++ client library
        writeln!(output, "// Assume these headers exist in your SpacetimeDB C++ client library")?;
        writeln!(output, "#include \"SpacetimeDBClientCore.h\"")?; // For base types like Identity, ConnectionId, Status, etc.
        writeln!(output)?;


        // Include the generated header itself
        writeln!(output, "#include \"SpacetimeDBTypes.generated.h\"")?; // Use ?
        writeln!(output)?;

        // Add definitions for event context structs
        writeln!(output, "// --- Event Contexts ---")?;
        writeln!(output)?;

        writeln!(output, "USTRUCT(BlueprintType)")?;
        writeln!(output, "struct FSpacetimeDBEventContext")?;
        writeln!(output, "{{")?; // Corrected format string
        output.indent(1);
        writeln!(output, "GENERATED_BODY()")?;
        writeln!(output)?;
        // Add common context properties here (ConnectionId, Identity, IsActive etc.)
        writeln!(output, "UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = \"SpacetimeDB\")")?;
        writeln!(output, "FSpacetimeDBConnectionId ConnectionId;")?;
        writeln!(output)?;
        writeln!(output, "UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = \"SpacetimeDB\")")?;
        writeln!(output, "FSpacetimeDBIdentity Identity;")?;
        writeln!(output)?;
        writeln!(output, "UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = \"SpacetimeDB\")")?;
        writeln!(output, "bool bIsActive;")?;
        writeln!(output)?;

        output.dedent(1);
        writeln!(output, "}}")?; // Corrected format string
        writeln!(output)?;

        writeln!(output, "USTRUCT(BlueprintType)")?;
        writeln!(output, "struct FSpacetimeDBReducerEventContext : public FSpacetimeDBEventContext")?;
        writeln!(output, "{{")?; // Corrected format string
        output.indent(1);
        writeln!(output, "GENERATED_BODY()")?;
        writeln!(output)?;
        // Add reducer-specific properties (Event status, etc.)
        writeln!(output, "UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = \"SpacetimeDB\")")?;
        writeln!(output, "SpacetimeDB::Status Status;")?; // Assuming SpacetimeDB::Status enum exists
        writeln!(output)?;
        writeln!(output, "UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = \"SpacetimeDB\")")?;
        writeln!(output, "FString ErrorReason;")?; // If status is Failed
        writeln!(output)?;

        output.dedent(1);
        writeln!(output, "}}")?; // Corrected format string
        writeln!(output)?;

        writeln!(output, "USTRUCT(BlueprintType)")?;
        writeln!(output, "struct FSpacetimeDBErrorContext : public FSpacetimeDBEventContext")?;
        writeln!(output, "{{")?; // Corrected format string
        output.indent(1);
        writeln!(output, "GENERATED_BODY()")?;
        writeln!(output)?;
        // Add error-specific properties
        writeln!(output, "UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = \"SpacetimeDB\")")?;
        writeln!(output, "FString ErrorMessage;")?;
        writeln!(output)?;

        output.dedent(1);
        writeln!(output, "}}")?; // Corrected format string
        writeln!(output)?;

        writeln!(output, "USTRUCT(BlueprintType)")?;
        writeln!(output, "struct FSpacetimeDBSubscriptionEventContext : public FSpacetimeDBEventContext")?;
        writeln!(output, "{{")?; // Corrected format string
        output.indent(1);
        writeln!(output, "GENERATED_BODY()")?;
        writeln!(output)?;
        // Add subscription-specific properties if any
        output.dedent(1);
        writeln!(output, "}}")?; // Corrected format string
        writeln!(output)?;


        // Add definitions for core delegates
        writeln!(output, "// --- Delegates ---")?;
        writeln!(output)?;

        writeln!(output, "DECLARE_DYNAMIC_MULTICAST_DELEGATE_OneParam(FOnSpacetimeDBConnected, USpacetimeDBClient*, Client);")?;
        writeln!(output, "DECLARE_DYNAMIC_MULTICAST_DELEGATE_OneParam(FOnSpacetimeDBDisconnected, USpacetimeDBClient*, Client);")?;
        writeln!(output, "DECLARE_DYNAMIC_MULTICAST_DELEGATE_TwoParams(FOnSpacetimeDBError, USpacetimeDBClient*, Client, const FSpacetimeDBErrorContext&, ErrorContext);")?;
        writeln!(output)?;

        writeln!(output, "DECLARE_DYNAMIC_MULTICAST_DELEGATE_OneParam(FOnSubscriptionApplied, const FSpacetimeDBSubscriptionEventContext&, Context);")?;
        writeln!(output, "DECLARE_DYNAMIC_MULTICAST_DELEGATE_TwoParams(FOnSubscriptionError, const FSpacetimeDBErrorContext&, Context, const FString&, ErrorMessage);")?;
        writeln!(output)?;

        // Enum to represent table event types
        writeln!(output, "UENUM(BlueprintType)")?;
        writeln!(output, "enum class ESpacetimeDBTableEventType : uint8")?;
        writeln!(output, "{{")?; // Corrected format string
        output.indent(1);
        writeln!(output, "Added,")?;
        writeln!(output, "Updated,")?;
        writeln!(output, "Removed,")?;
        output.dedent(1);
        writeln!(output, "}}")?; // Corrected format string
        writeln!(output)?;


        // Return the result explicitly
        Ok((filename, output.into_inner()))
     }

    // Placeholder for generating .cpp files (implementation details)
    // This would involve iterating through generated headers and creating corresponding .cpp files
    // with constructor implementations, method bodies, etc.
    fn generate_cpp_files(&self, module: &ModuleDef) -> Vec<(String, String)> {
        let mut files = Vec::new();

        // Example: Generate .cpp for the main client class
        let client_cpp_filename = format!("Private/SpacetimeDB/{}/SpacetimeDBClient.cpp", self.module_name);
        let mut client_cpp_output = CodeIndenter::new(String::new(), INDENT);
        // Use ? operator for writeln calls within this function
        if writeln!(client_cpp_output, "// Generated file - do not modify").is_err() { return files; }
        if writeln!(client_cpp_output).is_err() { return files; }
        if writeln!(client_cpp_output, "#include \"SpacetimeDB/{}/SpacetimeDBClient.h\"", self.module_name).is_err() { return files; }
        // Include other generated headers as needed
        if writeln!(client_cpp_output, "#include \"SpacetimeDB/{}/Tables/USpacetimeDBTables.h\"", self.module_name).is_err() { return files; }
        if writeln!(client_cpp_output, "#include \"SpacetimeDB/{}/Reducers/USpacetimeDBReducers.h\"", self.module_name).is_err() { return files; }
        if writeln!(client_cpp_output, "#include \"SpacetimeDB/{}/Reducers/USpacetimeDBSetReducerFlags.h\"", self.module_name).is_err() { return files; }
        if writeln!(client_cpp_output, "#include \"SpacetimeDB/{}/Subscriptions/USpacetimeDBSubscriptionBuilder.h\"", self.module_name).is_err() { return files; }
        if writeln!(client_cpp_output, "#include \"SpacetimeDB/{}/Subscriptions/USpacetimeDBSubscriptionHandle.h\"", self.module_name).is_err() { return files; }
        if writeln!(client_cpp_output).is_err() { return files; }
        if writeln!(client_cpp_output, "// Include the actual SpacetimeDB C++ client library implementation headers").is_err() { return files; }
        if writeln!(client_cpp_output, "#include \"SpacetimeDBClientImpl.h\"").is_err() { return files; } // Assuming an implementation header
        if writeln!(client_cpp_output).is_err() { return files; }

        // Implement constructor
        if writeln!(client_cpp_output, "UUSpacetimeDBClient::UUSpacetimeDBClient(const FObjectInitializer& ObjectInitializer) : Super(ObjectInitializer)").is_err() { return files; }
        if writeln!(client_cpp_output, "{{").is_err() { return files; } // Corrected format string
        client_cpp_output.indent(1);
        if writeln!(client_cpp_output, "// Initialize generated sub-objects").is_err() { return files; }
        if writeln!(client_cpp_output, "Db = CreateDefaultSubobject<USpacetimeDBTables>(TEXT(\"DbTables\"));").is_err() { return files; }
        if writeln!(client_cpp_output, "Reducers = CreateDefaultSubobject<USpacetimeDBReducers>(TEXT(\"Reducers\"));").is_err() { return files; }
        // SetReducerFlags might be created differently depending on ownership
        // if writeln!(client_cpp_output, "SetReducerFlags = CreateDefaultSubobject<USpacetimeDBSetReducerFlags>(TEXT(\"SetReducerFlags\"));").is_err() { return files; }

        if writeln!(client_cpp_output, "// Initialize the underlying client instance").is_err() { return files; }
        // if writeln!(client_cpp_output, "// SpacetimeDBClientInstance = MakeUnique<SpacetimeDB::ClientImpl>(this);").is_err() { return files; } // Pass 'this' as IDbConnection

        if writeln!(client_cpp_output, "// Bind internal handlers to the underlying client's events").is_err() { return files; }
        // if writeln!(client_cpp_output, "// SpacetimeDBClientInstance->OnConnected.BindUObject(this, &USpacetimeDBClient::HandleConnected);").is_err() { return files; }
        // Add other bindings
        client_cpp_output.dedent(1);
        if writeln!(client_cpp_output, "}}").is_err() { return files; } // Corrected format string
        if writeln!(client_cpp_output).is_err() { return files; }

        // Implement base client methods
        if writeln!(client_cpp_output, "void UUSpacetimeDBClient::Connect(const FString& Host, const FString& Token)").is_err() { return files; }
        if writeln!(client_cpp_output, "{{").is_err() { return files; } // Corrected format string
        client_cpp_output.indent(1);
        // if writeln!(client_cpp_output, "// SpacetimeDBClientInstance->Connect(*Host, *Token);").is_err() { return files; }
        if writeln!(client_cpp_output, "UE_LOG(LogTemp, Warning, TEXT(\"Connecting to SpacetimeDB at %s\"), *Host);").is_err() { return files; }
        client_cpp_output.dedent(1);
        if writeln!(client_cpp_output, "}}").is_err() { return files; } // Corrected format string
        if writeln!(client_cpp_output).is_err() { return files; }

        // Add implementations for other methods (Disconnect, IsConnected, CreateSubscriptionBuilder, DispatchReducerEvent, DispatchTableEvent)
        // These would involve calling the corresponding methods on the SpacetimeDBClientInstance
        // and handling serialization/deserialization.

        files.push((client_cpp_filename, client_cpp_output.into_inner()));

        // Add .cpp generation for tables, reducers, types, etc.
        // This would follow a similar pattern: include corresponding header, implement constructor and methods.

        files
    }
}


// Main generation function
impl<'opts> Lang for UnrealCpp<'opts> {
    fn table_filename(&self, _module: &ModuleDef, table: &TableDef) -> String {
        format!("Public/SpacetimeDB/{}/Tables/U{}.h", self.module_name, table.name.deref().to_case(Case::Pascal))
    }

    fn type_filename(&self, type_name: &spacetimedb_schema::def::ScopedTypeName) -> String {
        format!("Public/SpacetimeDB/{}/Types/F{}.h", self.module_name, collect_case(Case::Pascal, type_name)) // Pass &ScopedTypeName
    }

    fn reducer_filename(&self, reducer_name: &Identifier) -> String {
        format!("Public/SpacetimeDB/{}/Reducers/U{}.h", self.module_name, reducer_name.deref().to_case(Case::Pascal))
    }

    // This function will generate multiple files (headers and source files)
    fn generate_module(&self, module: &ModuleDef) -> Vec<(String, String)> {
        let mut files = Vec::new();

        // Generate core types header
        if let Ok((filename, content)) = self.generate_core_types_header(module) { // Handle Result
            files.push((filename, content));
        }


        // Generate headers for each type
        for typ in module.types() {
            if let Ok((filename, content)) = self.generate_type_header(module, typ) { // Handle Result
                files.push((filename, content));
            }
        }

        // Generate headers for each table
        for table in iter_tables(module) {
            if let Ok((filename, content)) = self.generate_table_header(module, table) { // Handle Result
                files.push((filename, content));
            }
        }

        // Generate headers for each reducer
        for reducer in iter_reducers(module) {
            if let Ok((filename, content)) = self.generate_reducer_header(module, reducer) { // Handle Result
                files.push((filename, content));
            }
        }

        // Generate main tables access header
        if let Ok((filename, content)) = self.generate_tables_header(module) { // Handle Result
            files.push((filename, content));
        }

        // Generate main reducers access header
        if let Ok((filename, content)) = self.generate_reducers_header(module) { // Handle Result
            files.push((filename, content));
        }

        // Generate SetReducerFlags header
        if let Ok((filename, content)) = self.generate_set_reducer_flags_header(module) { // Handle Result
             files.push((filename, content));
        }

        // Generate SubscriptionBuilder header
        if let Ok((filename, content)) = self.generate_subscription_builder_header(module) { // Handle Result
            files.push((filename, content));
        }

         // Generate SubscriptionHandle header
        if let Ok((filename, content)) = self.generate_subscription_handle_header(module) { // Handle Result
            files.push((filename, content));
        }


        // Generate .cpp files (placeholder implementation)
        // This would need to be fully implemented based on the generated headers
        files.extend(self.generate_cpp_files(module));


        files
    }

    // The generate_table, generate_type, generate_reducer functions are not needed
    // in this implementation as generate_module handles all file generation.
    // However, the trait requires them, so we can provide dummy implementations or
    // adapt them to call the helper functions if the trait structure is fixed.
    fn generate_table(&self, _module: &ModuleDef, _table: &TableDef) -> String {
        unimplemented!("generate_module handles table generation")
    }

    fn generate_type(&self, _module: &ModuleDef, _typ: &TypeDef) -> String {
        unimplemented!("generate_module handles type generation")
    }

    fn generate_reducer(&self, _module: &ModuleDef, _reducer: &ReducerDef) -> String {
        unimplemented!("generate_module handles reducer generation")
    }

    // Implement the generate_globals method
    fn generate_globals(&self, module: &ModuleDef) -> Vec<(String, String)> {
        let mut files = Vec::new();

        // This method is now handled by the individual header generation helpers
        // called within generate_module. We can return an empty vector here or
        // potentially refactor generate_module to call this one method.
        // For now, returning empty to satisfy the trait.

        // If the Lang trait *must* return global files here, we would move the
        // calls to generate_client_header, generate_tables_header, etc., into this method.
        // Assuming the current structure where generate_module is the main entry point
        // and calls the specific header generators is intended:
        // This method's purpose in the original C# generator was to group global files.
        // In this C++ generator, we generate individual header files.
        // We could call the header generation helpers here and return the vector,
        // but that would duplicate calls from generate_module.
        // Let's assume generate_module is the orchestrator and this method
        // is either unused or intended for a different grouping.
        // For now, returning empty to satisfy the trait.

        files
    }
}
