use std::{collections::HashSet, fmt::Write};
use swc_common::{sync::Lrc, FileName, SourceMap};
use swc_ecma_ast::*;
use swc_ecma_parser::{parse_file_as_module, Syntax, TsSyntax};

pub fn generate(path: &str, source: String) -> Result<String, String> {
    let source_map: Lrc<SourceMap> = Default::default();
    let file = source_map.new_source_file(FileName::Custom(path.into()).into(), source);
    let mut recovered = Vec::new();
    let module = parse_file_as_module(
        &file,
        Syntax::Typescript(TsSyntax::default()),
        EsVersion::latest(),
        None,
        &mut recovered,
    )
    .map_err(|error| format!("{path}: TypeScript parse error: {error:?}"))?;
    if let Some(error) = recovered.first() {
        return Err(format!("{path}: TypeScript parse error: {error:?}"));
    }

    let aliases = module
        .body
        .iter()
        .map(|item| match item {
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(export)) => match &export.decl {
                Decl::TsTypeAlias(alias) => Ok(alias.as_ref()),
                _ => Err("only exported type aliases are supported"),
            },
            _ => Err("only exported type aliases are supported"),
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|message| format!("{path}: {message}"))?;

    let mut output = String::new();
    let mut names = HashSet::new();
    for alias in aliases {
        let name = alias.id.sym.as_str();
        if !rust_type_name(name) {
            return Err(format!(
                "{path}: type alias `{name}` cannot be represented as a Rust type name"
            ));
        }
        if alias.type_params.is_some() {
            return Err(format!(
                "{path}: generic type alias `{name}` is unsupported"
            ));
        }
        if !names.insert(name) {
            return Err(format!("{path}: duplicate type alias `{name}`"));
        }
        emit_alias(path, alias, &mut output)?;
    }
    Ok(output)
}

fn emit_alias(path: &str, alias: &TsTypeAliasDecl, output: &mut String) -> Result<(), String> {
    let name = alias.id.sym.as_str();
    match alias.type_ann.as_ref() {
        ty if string_union(ty).is_some() => {
            writeln!(
                output,
                "#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]"
            )
            .unwrap();
            writeln!(output, "pub enum {name} {{").unwrap();
            for value in string_union(ty).unwrap() {
                writeln!(
                    output,
                    "    #[serde(rename = {value:?})]\n    {},",
                    rust_variant(path, &value)?
                )
                .unwrap();
            }
            writeln!(output, "}}\n").unwrap();
        }
        ty if name == "GitHostingTarget" => emit_object(path, name, ty, output)?,
        ty if matches!(
            name,
            "GitHostingCommand" | "WtToolsFeedbackCommand" | "WtToolsWorldCommand"
        ) =>
        {
            emit_commands(path, name, ty, output)?
        }
        ty if name == "WtToolsCommand" => emit_envelope(path, ty, output)?,
        _ => return Err(format!("{path}: unsupported type alias `{name}`")),
    }
    Ok(())
}

fn emit_object(path: &str, name: &str, ty: &TsType, output: &mut String) -> Result<(), String> {
    let fields = object_fields(path, ty)?;
    writeln!(
        output,
        "#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]\n#[serde(deny_unknown_fields)]\npub struct {name} {{"
    )
    .unwrap();
    emit_fields(path, &fields, output, true)?;
    writeln!(output, "}}\n").unwrap();
    Ok(())
}

fn emit_envelope(path: &str, ty: &TsType, output: &mut String) -> Result<(), String> {
    let members = match ty {
        TsType::TsUnionOrIntersectionType(TsUnionOrIntersectionType::TsUnionType(union)) => {
            union.types.iter().map(|member| member.as_ref()).collect()
        }
        TsType::TsTypeLit(_) => vec![ty],
        _ => {
            return Err(format!("{path}: `WtToolsCommand` must be an object union"));
        }
    };
    if members.len() != 3 {
        return Err(format!("{path}: `WtToolsCommand` must have three members"));
    }
    writeln!(
        output,
        "#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]\n#[serde(untagged, deny_unknown_fields)]\npub enum WtToolsCommand {{"
    )
    .unwrap();
    for member in members {
        let fields = object_fields(path, member)?;
        let variant = if fields.iter().any(|field| field.0 == "target") {
            "GitHosting"
        } else if fields.iter().any(|field| matches!(field.2, TsType::TsTypeRef(reference) if reference.type_name.as_ident().is_some_and(|name| name.sym == *"WtToolsWorldCommand"))) {
            "World"
        } else {
            "Feedback"
        };
        writeln!(output, "    {variant} {{").unwrap();
        emit_fields(path, &fields, output, false)?;
        writeln!(output, "    }},").unwrap();
    }
    writeln!(output, "}}\n").unwrap();
    Ok(())
}

fn emit_fields(
    path: &str,
    fields: &[(String, bool, &TsType)],
    output: &mut String,
    public: bool,
) -> Result<(), String> {
    for (field_name, optional, field_type) in fields {
        let field_type = rust_type(path, field_name, field_type)?;
        let field_type = if *optional {
            format!("Option<{field_type}>")
        } else {
            field_type
        };
        let visibility = if public { "pub " } else { "" };
        writeln!(output, "        {visibility}{field_name}: {field_type},").unwrap();
    }
    Ok(())
}

fn string_union(ty: &TsType) -> Option<Vec<String>> {
    let TsType::TsUnionOrIntersectionType(TsUnionOrIntersectionType::TsUnionType(union)) = ty
    else {
        return None;
    };
    union
        .types
        .iter()
        .map(|ty| match ty.as_ref() {
            TsType::TsLitType(lit) => match &lit.lit {
                TsLit::Str(value) => Some(value.value.to_string_lossy().into_owned()),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

fn emit_commands(path: &str, name: &str, ty: &TsType, output: &mut String) -> Result<(), String> {
    let members = match ty {
        TsType::TsUnionOrIntersectionType(TsUnionOrIntersectionType::TsUnionType(union)) => {
            union.types.iter().map(|member| member.as_ref()).collect()
        }
        TsType::TsTypeLit(_) => vec![ty],
        _ => return Err(format!("{path}: `{name}` must be an object union")),
    };
    writeln!(
        output,
        "#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]"
    )
    .unwrap();
    writeln!(
        output,
        "#[serde(tag = \"action\", deny_unknown_fields)]\npub enum {name} {{"
    )
    .unwrap();
    let mut actions = HashSet::new();
    for member in members {
        let fields = object_fields(path, member)?;
        let mut action_fields = fields.iter().filter(|field| field.0 == "action");
        let action = action_fields
            .next()
            .ok_or_else(|| format!("{path}: command member has no `action`"))?;
        if action_fields.next().is_some() {
            return Err(format!(
                "{path}: command member has duplicate `action` fields"
            ));
        }
        let action_name = literal_string(action.2)
            .ok_or_else(|| format!("{path}: `action` must be a string literal"))?;
        if action.1 {
            return Err(format!("{path}: `action` cannot be optional"));
        }
        if !actions.insert(action_name.clone()) {
            return Err(format!("{path}: duplicate command action `{action_name}`"));
        }
        writeln!(
            output,
            "    #[serde(rename = {action_name:?})]\n    {} {{",
            rust_variant(path, &action_name)?
        )
        .unwrap();
        for (field_name, optional, field_type) in fields.iter().filter(|field| field.0 != "action")
        {
            let rust_type = rust_type(path, field_name, field_type)?;
            if *optional {
                writeln!(output, "        #[serde(default)]").unwrap();
            }
            let rust_type = if *optional && rust_type != "bool" {
                format!("Option<{rust_type}>")
            } else {
                rust_type
            };
            writeln!(output, "        {field_name}: {rust_type},").unwrap();
        }
        writeln!(output, "    }},").unwrap();
    }
    writeln!(output, "}}\n").unwrap();
    Ok(())
}

fn object_fields<'a>(
    path: &str,
    ty: &'a TsType,
) -> Result<Vec<(String, bool, &'a TsType)>, String> {
    let TsType::TsTypeLit(literal) = ty else {
        return Err(format!("{path}: command members must be object types"));
    };
    let mut names = HashSet::new();
    literal
        .members
        .iter()
        .map(|member| {
            let TsTypeElement::TsPropertySignature(property) = member else {
                return Err(format!("{path}: only object properties are supported"));
            };
            let Expr::Ident(name) = property.key.as_ref() else {
                return Err(format!("{path}: property names must be identifiers"));
            };
            if !rust_field_name(name.sym.as_str()) {
                return Err(format!(
                    "{path}: property `{}` cannot be represented as a Rust field name",
                    name.sym
                ));
            }
            if !names.insert(name.sym.as_str()) {
                return Err(format!("{path}: duplicate property `{}`", name.sym));
            }
            let ty = property
                .type_ann
                .as_ref()
                .ok_or_else(|| format!("{path}: property `{}` needs a type", name.sym))?;
            Ok((
                name.sym.to_string(),
                property.optional,
                ty.type_ann.as_ref(),
            ))
        })
        .collect()
}

fn rust_type(path: &str, field: &str, ty: &TsType) -> Result<String, String> {
    if field == "provider"
        && string_union(ty).as_deref() == Some(&["github".to_owned(), "gitlab".to_owned()])
    {
        return Ok("ProviderKind".into());
    }
    match ty {
        TsType::TsKeywordType(keyword) => match keyword.kind {
            TsKeywordTypeKind::TsStringKeyword => Ok("String".into()),
            TsKeywordTypeKind::TsNumberKeyword if field == "timeout_seconds" => Ok("u64".into()),
            TsKeywordTypeKind::TsNumberKeyword => Err(format!(
                "{path}: number is supported only for `timeout_seconds`"
            )),
            TsKeywordTypeKind::TsBooleanKeyword => Ok("bool".into()),
            _ => Err(format!("{path}: unsupported TypeScript keyword type")),
        },
        TsType::TsTypeRef(reference) => match &reference.type_name {
            TsEntityName::Ident(name)
                if reference.type_params.is_none() && rust_type_name(name.sym.as_str()) =>
            {
                Ok(name.sym.to_string())
            }
            _ => Err(format!(
                "{path}: generic or qualified types are unsupported"
            )),
        },
        TsType::TsArrayType(array) => Ok(format!(
            "Vec<{}>",
            rust_type(path, field, &array.elem_type)?
        )),
        _ => Err(format!("{path}: unsupported field type")),
    }
}

fn literal_string(ty: &TsType) -> Option<String> {
    match ty {
        TsType::TsLitType(lit) => match &lit.lit {
            TsLit::Str(value) => Some(value.value.to_string_lossy().into_owned()),
            _ => None,
        },
        _ => None,
    }
}

fn rust_variant(path: &str, value: &str) -> Result<String, String> {
    if value.is_empty()
        || !value
            .chars()
            .all(|character| character.is_ascii_lowercase() || character == '_')
        || value.split('_').any(str::is_empty)
    {
        return Err(format!(
            "{path}: string literal `{value}` must be lower_snake_case"
        ));
    }
    Ok(value
        .split('_')
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
                .unwrap_or_default()
        })
        .collect())
}

fn rust_type_name(value: &str) -> bool {
    value
        .strip_prefix(|character: char| character.is_ascii_uppercase())
        .is_some_and(|rest| {
            rest.chars()
                .all(|character| character.is_ascii_alphanumeric())
        })
}

fn rust_field_name(value: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn",
        "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
        "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe",
        "use", "where", "while", "async", "await", "dyn",
    ];
    !KEYWORDS.contains(&value)
        && value
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_lowercase())
        && value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
}
