import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { SchemaPrintingContext } from "@beff/client";
import { Codecs } from "./parser.ts";

const root = dirname(fileURLToPath(import.meta.url));
const context = new SchemaPrintingContext({
  refPathTemplate: "#/$defs/{name}",
  definitionContainerKey: "$defs",
});
const request = Codecs.Request.schemaWithContext(context);
const response = Codecs.Response.schemaWithContext(context);
const definitions = context.exportDefinitions().$defs;
const schema = {
  $schema: "https://json-schema.org/draft/2020-12/schema",
  title: "WT API v1",
  oneOf: [request, response],
  $defs: definitions,
};

writeFileSync(join(root, "wt_api.schema.json"), `${JSON.stringify(schema, null, 2)}\n`);

const publicTypes = [
  "CreateWorldRequest",
  "DeleteWorldRequest",
  "StartCodexRequest",
  "InspectCodexRequest",
  "SendCodexMessageRequest",
  "ReadWorldMailRequest",
  "SshAccess",
  "World",
  "WorldMail",
  "CreateWorldResult",
  "DeleteWorldResult",
  "StartCodexResult",
  "InspectCodexResult",
  "SendCodexMessageResult",
  "ReadWorldMailResult",
  "CapacityDetails",
  "Error",
];

const moduleName = (name) => {
  if (name.endsWith("Request")) return `WtApi.Request.${name.slice(0, -7)}`;
  if (name.endsWith("Result")) return `WtApi.Result.${name.slice(0, -6)}`;
  if (name === "Error") return "WtApi.ErrorData";
  return `WtApi.${name}`;
};

const refName = (value) => value.replace("#/$defs/", "");

const descriptor = (fieldSchema) => {
  if (fieldSchema.$ref) {
    const name = refName(fieldSchema.$ref);
    if (name === "Uuid") return ":uuid";
    if (name === "Int64") return ":integer";
    if (["UInt16", "UInt32", "UInt64"].includes(name)) return `:${name.toLowerCase()}`;
    return `{:struct, ${moduleName(name)}}`;
  }
  if (fieldSchema.enum?.length === 1) return `{:const, ${JSON.stringify(fieldSchema.enum[0])}}`;
  if (fieldSchema.enum) return `{:enum, ${inspectList(fieldSchema.enum)}}`;
  if (fieldSchema.type === "string") return ":string";
  if (fieldSchema.format === "int64") return ":integer";
  if (["uint16", "uint32", "uint64"].includes(fieldSchema.format)) return `:${fieldSchema.format}`;
  if (fieldSchema.type === "number") return ":number";
  if (fieldSchema.type === "boolean") return ":boolean";
  if (fieldSchema.type === "array") return `{:list, ${descriptor(fieldSchema.items)}}`;
  throw new Error(`unsupported schema: ${JSON.stringify(fieldSchema)}`);
};

const inspectList = (values) => `[${values.map((value) => JSON.stringify(value)).join(", ")}]`;

const typeSpec = (fieldSchema) => {
  if (fieldSchema.$ref) {
    const name = refName(fieldSchema.$ref);
    if (name === "Uuid") return "String.t()";
    if (name === "Int64") return "integer()";
    if (["UInt16", "UInt32", "UInt64"].includes(name)) return "non_neg_integer()";
    return `${moduleName(name)}.t()`;
  }
  if (fieldSchema.type === "string") return "String.t()";
  if (fieldSchema.format === "int64") return "integer()";
  if (["uint16", "uint32", "uint64"].includes(fieldSchema.format)) return "non_neg_integer()";
  if (fieldSchema.type === "number") return "number()";
  if (fieldSchema.type === "boolean") return "boolean()";
  if (fieldSchema.type === "array") return `[${typeSpec(fieldSchema.items)}]`;
  throw new Error(`unsupported schema: ${JSON.stringify(fieldSchema)}`);
};

const emitModule = (name) => {
  const typeSchema = definitions[name];
  if (typeSchema?.type !== "object") throw new Error(`${name} is not an object schema`);
  const required = new Set(typeSchema.required ?? []);
  const fields = Object.entries(typeSchema.properties);
  const enforced = fields
    .filter(([field, fieldSchema]) => required.has(field) && fieldSchema.enum?.length !== 1)
    .map(([field]) => `:${field}`);
  const structFields = fields.map(([field, fieldSchema]) => {
    const defaultValue =
      fieldSchema.enum?.length === 1 ? JSON.stringify(fieldSchema.enum[0]) : "nil";
    return `${field}: ${defaultValue}`;
  });
  const fieldSpecs = fields.map(([field, fieldSchema]) => {
    const optional = required.has(field) ? "" : " | nil";
    return `          ${field}: ${typeSpec(fieldSchema)}${optional}`;
  });
  const descriptors = fields.map(([field, fieldSchema]) => {
    const presence = required.has(field) ? ":required" : ":optional";
    return `    ${field}: {${presence}, ${descriptor(fieldSchema)}}`;
  });
  return `defmodule ${moduleName(name)} do
  @moduledoc false
  alias WtApi.Generated.Decoder

  @enforce_keys [${enforced.join(", ")}]
  defstruct [${structFields.join(", ")}]

  @type t :: %__MODULE__{
${fieldSpecs.join(",\n")}
        }

  @schema %{
${descriptors.join(",\n")}
  }

  @spec decode(map()) :: {:ok, t()} | {:error, String.t()}
  def decode(value), do: Decoder.decode_struct(value, __MODULE__, @schema)
end
`;
};

const generated = `# Generated from wt_api.schema.json by generate.mjs. Do not edit.

defmodule WtApi.Generated.Decoder do
  @moduledoc false

  @uuid ~r/^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i

  def decode_uuid(value), do: decode_value(value, :uuid)

  def decode_struct(value, module, schema) when is_map(value) do
    Enum.reduce_while(schema, {:ok, []}, fn {field, {presence, type}}, {:ok, fields} ->
      key = Atom.to_string(field)

      case Map.fetch(value, key) do
        :error when presence == :optional -> {:cont, {:ok, [{field, nil} | fields]}}
        :error -> {:halt, {:error, "missing field #{key}"}}
        {:ok, item} -> decode_field(field, item, type, fields)
      end
    end)
    |> case do
      {:ok, fields} -> {:ok, struct!(module, fields)}
      error -> error
    end
  end

  def decode_struct(_value, _module, _schema), do: {:error, "expected object"}

  defp decode_field(field, value, type, fields) do
    case decode_value(value, type) do
      {:ok, decoded} -> {:cont, {:ok, [{field, decoded} | fields]}}
      {:error, reason} -> {:halt, {:error, "invalid #{field}: #{reason}"}}
    end
  end

  defp decode_value(value, :uuid) when is_binary(value) do
    if Regex.match?(@uuid, value), do: {:ok, value}, else: {:error, "expected UUID"}
  end

  defp decode_value(value, :string) when is_binary(value), do: {:ok, value}
  defp decode_value(value, :integer) when is_integer(value), do: {:ok, value}
  defp decode_value(value, :uint16) when is_integer(value) and value in 0..65_535, do: {:ok, value}
  defp decode_value(value, :uint32) when is_integer(value) and value in 0..4_294_967_295, do: {:ok, value}
  defp decode_value(value, :uint64) when is_integer(value) and value in 0..18_446_744_073_709_551_615, do: {:ok, value}
  defp decode_value(value, :number) when is_number(value), do: {:ok, value}
  defp decode_value(value, :boolean) when is_boolean(value), do: {:ok, value}
  defp decode_value(value, {:const, expected}) when value == expected, do: {:ok, value}

  defp decode_value(value, {:enum, values}) when is_binary(value) do
    if value in values, do: {:ok, value}, else: {:error, "unexpected value"}
  end

  defp decode_value(value, {:struct, module}), do: module.decode(value)

  defp decode_value(value, {:list, type}) when is_list(value) do
    Enum.reduce_while(value, {:ok, []}, fn item, {:ok, items} ->
      case decode_value(item, type) do
        {:ok, decoded} -> {:cont, {:ok, [decoded | items]}}
        error -> {:halt, error}
      end
    end)
    |> case do
      {:ok, items} -> {:ok, Enum.reverse(items)}
      error -> error
    end
  end

  defp decode_value(_value, _type), do: {:error, "unexpected type"}
end

${publicTypes.map(emitModule).join("\n")}`;

writeFileSync(join(root, "lib/wt_api/generated.ex"), generated);

const formatted = spawnSync("mix", ["format", "lib/wt_api/generated.ex"], {
  cwd: root,
  encoding: "utf8",
});

if (formatted.status !== 0) {
  process.stderr.write(formatted.stderr);
  process.exit(formatted.status ?? 1);
}
