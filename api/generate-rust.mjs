import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { SchemaPrintingContext } from "@beff/client";
import { Codecs } from "./parser.ts";

const root = dirname(fileURLToPath(import.meta.url));
const context = new SchemaPrintingContext({
  refPathTemplate: "#/$defs/{name}",
  definitionContainerKey: "$defs",
});
Codecs.Request.schemaWithContext(context);
Codecs.Response.schemaWithContext(context);
const definitions = context.exportDefinitions().$defs;

const refName = (value) => value.replace("#/$defs/", "");
const pascal = (value) =>
  value
    .split("_")
    .map((part) => part[0].toUpperCase() + part.slice(1))
    .join("");

const primitiveRefs = {
  Uuid: "String",
  Int64: "i64",
  UInt16: "u16",
  UInt32: "u32",
  UInt64: "u64",
  Result: "ApiResult",
};

const objectNames = {
  SshAccess: "ApiSshAccess",
  World: "ApiWorld",
  CapacityDetails: "ApiCapacityDetails",
  Error: "ApiError",
};

const enumNames = {
  "World.status": "ApiWorldStatus",
  "CapacityDetails.resource": "ApiCapacityResource",
};

const rustType = (fieldSchema, owner, field) => {
  if (fieldSchema.$ref) {
    const name = refName(fieldSchema.$ref);
    return (
      primitiveRefs[name] ??
      objectNames[name] ??
      (() => {
        throw new Error(`unsupported ref ${name}`);
      })()
    );
  }
  if (fieldSchema.enum) {
    if (fieldSchema.enum.length === 1 && typeof fieldSchema.enum[0] === "number") return "u32";
    const name = enumNames[`${owner}.${field}`];
    if (name) return name;
    if (fieldSchema.enum.length === 1 && typeof fieldSchema.enum[0] === "string") return "String";
    throw new Error(`unnamed enum ${owner}.${field}`);
  }
  if (fieldSchema.type === "string") return "String";
  if (fieldSchema.type === "boolean") return "bool";
  if (fieldSchema.type === "array") return `Vec<${rustType(fieldSchema.items, owner, field)}>`;
  throw new Error(`unsupported type ${owner}.${field}: ${JSON.stringify(fieldSchema)}`);
};

const fields = (name, omit = []) => {
  const typeSchema = definitions[name];
  if (typeSchema?.type !== "object") throw new Error(`${name} is not an object schema`);
  const required = new Set(typeSchema.required ?? []);
  return Object.entries(typeSchema.properties)
    .filter(([field]) => !omit.includes(field))
    .map(([field, fieldSchema]) => ({
      field,
      fieldSchema,
      optional: !required.has(field),
      type: rustType(fieldSchema, name, field),
    }));
};

const emitFields = (items, visibility) =>
  items
    .map(({ field, optional, type }) => {
      const attribute = optional
        ? '        #[serde(default, skip_serializing_if = "Option::is_none")]\n'
        : "";
      return `${attribute}        ${visibility}${field}: ${optional ? `Option<${type}>` : type},`;
    })
    .join("\n");

const emitEnum = (name, values) => `#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ${name} {
${values.map((value) => `    ${pascal(value)},`).join("\n")}
}
`;

const enumDefinitions = Object.entries(enumNames).map(([path, name]) => {
  const [owner, field] = path.split(".");
  return emitEnum(name, definitions[owner].properties[field].enum);
});

const requestMapping = definitions.Request.discriminator.mapping;
const requestVariants = Object.entries(requestMapping).map(([operation, ref]) => {
  const name = refName(ref);
  return `    #[serde(rename = ${JSON.stringify(operation)})]
    ${pascal(operation)} {
${emitFields(fields(name, ["operation"]), "")}
    },`;
});

const request = `#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "operation", deny_unknown_fields)]
pub(super) enum Request {
${requestVariants.join("\n")}
}
`;

const resultVariants = definitions.Result.anyOf.map(({ $ref }) => {
  const name = refName($ref);
  const variant = name.slice(0, -"Result".length);
  return `    ${variant} {
${emitFields(fields(name), "")}
    },`;
});

const result = `#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(super) enum ApiResult {
${resultVariants.join("\n")}
}
`;

const responseMapping = definitions.Response.discriminator.mapping;
const responseVariants = Object.entries(responseMapping).map(([outcome, ref]) => {
  const name = refName(ref);
  return `    #[serde(rename = ${JSON.stringify(outcome)})]
    ${pascal(outcome)} {
${emitFields(fields(name, ["outcome"]), "")}
    },`;
});

const response = `#[derive(Debug, Serialize)]
#[serde(tag = "outcome")]
pub(super) enum ApiResponse {
${responseVariants.join("\n")}
}
`;

const structs = Object.entries(objectNames).map(
  ([source, name]) => `#[derive(Debug, Serialize)]
pub(super) struct ${name} {
${emitFields(fields(source), "pub(super) ")}
}
`,
);

const output = `// Generated from api/api.ts. Do not edit.

use serde::{Deserialize, Serialize};

${request}
${response}
${result}
${enumDefinitions.join("\n")}
${structs.join("\n")}`;

const destination = join(root, "../crates/products/wt/client/src/api/generated.rs");
writeFileSync(destination, output);
const formatted = spawnSync("rustfmt", ["--edition", "2021", destination], { encoding: "utf8" });
if (formatted.status !== 0) {
  process.stderr.write(formatted.stderr);
  process.exit(formatted.status ?? 1);
}
