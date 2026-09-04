# Generated from wt_api.schema.json by generate.mjs. Do not edit.

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

  defp decode_value(value, :uint16) when is_integer(value) and value in 0..65_535,
    do: {:ok, value}

  defp decode_value(value, :uint32) when is_integer(value) and value in 0..4_294_967_295,
    do: {:ok, value}

  defp decode_value(value, :uint64)
       when is_integer(value) and value in 0..18_446_744_073_709_551_615, do: {:ok, value}

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

defmodule WtApi.Request.CreateWorld do
  @moduledoc false
  alias WtApi.Generated.Decoder

  @enforce_keys [
    :context,
    :disk_gib,
    :git_user_email,
    :git_user_name,
    :memory_mib,
    :name,
    :request_id,
    :vcpus
  ]
  defstruct api_version: 1,
            context: nil,
            disk_gib: nil,
            expected_server_id: nil,
            git_user_email: nil,
            git_user_name: nil,
            memory_mib: nil,
            name: nil,
            operation: "create_world",
            request_id: nil,
            vcpus: nil

  @type t :: %__MODULE__{
          api_version: number(),
          context: String.t(),
          disk_gib: non_neg_integer(),
          expected_server_id: String.t() | nil,
          git_user_email: String.t(),
          git_user_name: String.t(),
          memory_mib: non_neg_integer(),
          name: String.t(),
          operation: String.t(),
          request_id: String.t(),
          vcpus: non_neg_integer()
        }

  @schema %{
    api_version: {:required, {:const, 1}},
    context: {:required, :string},
    disk_gib: {:required, :uint64},
    expected_server_id: {:optional, :uuid},
    git_user_email: {:required, :string},
    git_user_name: {:required, :string},
    memory_mib: {:required, :uint64},
    name: {:required, :string},
    operation: {:required, {:const, "create_world"}},
    request_id: {:required, :uuid},
    vcpus: {:required, :uint32}
  }

  @spec decode(map()) :: {:ok, t()} | {:error, String.t()}
  def decode(value), do: Decoder.decode_struct(value, __MODULE__, @schema)
end

defmodule WtApi.Request.DeleteWorld do
  @moduledoc false
  alias WtApi.Generated.Decoder

  @enforce_keys [:context, :request_id, :world_id]
  defstruct api_version: 1,
            context: nil,
            expected_server_id: nil,
            operation: "delete_world",
            request_id: nil,
            world_id: nil

  @type t :: %__MODULE__{
          api_version: number(),
          context: String.t(),
          expected_server_id: String.t() | nil,
          operation: String.t(),
          request_id: String.t(),
          world_id: String.t()
        }

  @schema %{
    api_version: {:required, {:const, 1}},
    context: {:required, :string},
    expected_server_id: {:optional, :uuid},
    operation: {:required, {:const, "delete_world"}},
    request_id: {:required, :uuid},
    world_id: {:required, :uuid}
  }

  @spec decode(map()) :: {:ok, t()} | {:error, String.t()}
  def decode(value), do: Decoder.decode_struct(value, __MODULE__, @schema)
end

defmodule WtApi.Request.StartCodex do
  @moduledoc false
  alias WtApi.Generated.Decoder

  @enforce_keys [:context, :message, :request_id, :world_id]
  defstruct api_version: 1,
            context: nil,
            expected_server_id: nil,
            message: nil,
            operation: "start_codex",
            request_id: nil,
            world_id: nil

  @type t :: %__MODULE__{
          api_version: number(),
          context: String.t(),
          expected_server_id: String.t() | nil,
          message: String.t(),
          operation: String.t(),
          request_id: String.t(),
          world_id: String.t()
        }

  @schema %{
    api_version: {:required, {:const, 1}},
    context: {:required, :string},
    expected_server_id: {:optional, :uuid},
    message: {:required, :string},
    operation: {:required, {:const, "start_codex"}},
    request_id: {:required, :uuid},
    world_id: {:required, :uuid}
  }

  @spec decode(map()) :: {:ok, t()} | {:error, String.t()}
  def decode(value), do: Decoder.decode_struct(value, __MODULE__, @schema)
end

defmodule WtApi.Request.InspectCodex do
  @moduledoc false
  alias WtApi.Generated.Decoder

  @enforce_keys [:context, :request_id, :thread_id, :world_id]
  defstruct api_version: 1,
            context: nil,
            expected_server_id: nil,
            operation: "inspect_codex",
            request_id: nil,
            thread_id: nil,
            world_id: nil

  @type t :: %__MODULE__{
          api_version: number(),
          context: String.t(),
          expected_server_id: String.t() | nil,
          operation: String.t(),
          request_id: String.t(),
          thread_id: String.t(),
          world_id: String.t()
        }

  @schema %{
    api_version: {:required, {:const, 1}},
    context: {:required, :string},
    expected_server_id: {:optional, :uuid},
    operation: {:required, {:const, "inspect_codex"}},
    request_id: {:required, :uuid},
    thread_id: {:required, :string},
    world_id: {:required, :uuid}
  }

  @spec decode(map()) :: {:ok, t()} | {:error, String.t()}
  def decode(value), do: Decoder.decode_struct(value, __MODULE__, @schema)
end

defmodule WtApi.Request.SendCodexMessage do
  @moduledoc false
  alias WtApi.Generated.Decoder

  @enforce_keys [:context, :message, :request_id, :thread_id, :world_id]
  defstruct api_version: 1,
            context: nil,
            expected_server_id: nil,
            message: nil,
            operation: "send_codex_message",
            request_id: nil,
            thread_id: nil,
            world_id: nil

  @type t :: %__MODULE__{
          api_version: number(),
          context: String.t(),
          expected_server_id: String.t() | nil,
          message: String.t(),
          operation: String.t(),
          request_id: String.t(),
          thread_id: String.t(),
          world_id: String.t()
        }

  @schema %{
    api_version: {:required, {:const, 1}},
    context: {:required, :string},
    expected_server_id: {:optional, :uuid},
    message: {:required, :string},
    operation: {:required, {:const, "send_codex_message"}},
    request_id: {:required, :uuid},
    thread_id: {:required, :string},
    world_id: {:required, :uuid}
  }

  @spec decode(map()) :: {:ok, t()} | {:error, String.t()}
  def decode(value), do: Decoder.decode_struct(value, __MODULE__, @schema)
end

defmodule WtApi.Request.ReadWorldMail do
  @moduledoc false
  alias WtApi.Generated.Decoder

  @enforce_keys [:after_message_id, :context, :limit, :request_id, :world_id]
  defstruct after_message_id: nil,
            api_version: 1,
            context: nil,
            expected_server_id: nil,
            limit: nil,
            operation: "read_world_mail",
            request_id: nil,
            world_id: nil

  @type t :: %__MODULE__{
          after_message_id: non_neg_integer(),
          api_version: number(),
          context: String.t(),
          expected_server_id: String.t() | nil,
          limit: non_neg_integer(),
          operation: String.t(),
          request_id: String.t(),
          world_id: String.t()
        }

  @schema %{
    after_message_id: {:required, :uint64},
    api_version: {:required, {:const, 1}},
    context: {:required, :string},
    expected_server_id: {:optional, :uuid},
    limit: {:required, :uint32},
    operation: {:required, {:const, "read_world_mail"}},
    request_id: {:required, :uuid},
    world_id: {:required, :uuid}
  }

  @spec decode(map()) :: {:ok, t()} | {:error, String.t()}
  def decode(value), do: Decoder.decode_struct(value, __MODULE__, @schema)
end

defmodule WtApi.SshAccess do
  @moduledoc false
  alias WtApi.Generated.Decoder

  @enforce_keys [:host, :host_keys, :port, :user]
  defstruct host: nil, host_keys: nil, port: nil, user: nil

  @type t :: %__MODULE__{
          host: String.t(),
          host_keys: [String.t()],
          port: non_neg_integer(),
          user: String.t()
        }

  @schema %{
    host: {:required, :string},
    host_keys: {:required, {:list, :string}},
    port: {:required, :uint16},
    user: {:required, :string}
  }

  @spec decode(map()) :: {:ok, t()} | {:error, String.t()}
  def decode(value), do: Decoder.decode_struct(value, __MODULE__, @schema)
end

defmodule WtApi.World do
  @moduledoc false
  alias WtApi.Generated.Decoder

  @enforce_keys [:disk_gib, :memory_mib, :name, :status, :vcpus, :world_id]
  defstruct disk_gib: nil,
            guest_ip: nil,
            last_error: nil,
            memory_mib: nil,
            name: nil,
            ssh: nil,
            status: nil,
            vcpus: nil,
            world_id: nil

  @type t :: %__MODULE__{
          disk_gib: non_neg_integer(),
          guest_ip: String.t() | nil,
          last_error: String.t() | nil,
          memory_mib: non_neg_integer(),
          name: String.t(),
          ssh: WtApi.SshAccess.t() | nil,
          status: String.t(),
          vcpus: non_neg_integer(),
          world_id: String.t()
        }

  @schema %{
    disk_gib: {:required, :uint64},
    guest_ip: {:optional, :string},
    last_error: {:optional, :string},
    memory_mib: {:required, :uint64},
    name: {:required, :string},
    ssh: {:optional, {:struct, WtApi.SshAccess}},
    status: {:required, {:enum, ["destroying", "error", "provisioning", "running", "stopped"]}},
    vcpus: {:required, :uint32},
    world_id: {:required, :uuid}
  }

  @spec decode(map()) :: {:ok, t()} | {:error, String.t()}
  def decode(value), do: Decoder.decode_struct(value, __MODULE__, @schema)
end

defmodule WtApi.WorldMail do
  @moduledoc false
  alias WtApi.Generated.Decoder

  @enforce_keys [:created_at_unix_ms, :kind, :message_id, :text, :world_id]
  defstruct created_at_unix_ms: nil,
            kind: nil,
            message_id: nil,
            pane_id: nil,
            text: nil,
            thread_id: nil,
            turn_id: nil,
            world_id: nil

  @type t :: %__MODULE__{
          created_at_unix_ms: integer(),
          kind: String.t(),
          message_id: non_neg_integer(),
          pane_id: String.t() | nil,
          text: String.t(),
          thread_id: String.t() | nil,
          turn_id: String.t() | nil,
          world_id: String.t()
        }

  @schema %{
    created_at_unix_ms: {:required, :integer},
    kind: {:required, {:enum, ["completed", "failed", "message"]}},
    message_id: {:required, :uint64},
    pane_id: {:optional, :string},
    text: {:required, :string},
    thread_id: {:optional, :string},
    turn_id: {:optional, :string},
    world_id: {:required, :uuid}
  }

  @spec decode(map()) :: {:ok, t()} | {:error, String.t()}
  def decode(value), do: Decoder.decode_struct(value, __MODULE__, @schema)
end

defmodule WtApi.Result.CreateWorld do
  @moduledoc false
  alias WtApi.Generated.Decoder

  @enforce_keys [:world]
  defstruct world: nil

  @type t :: %__MODULE__{
          world: WtApi.World.t()
        }

  @schema %{
    world: {:required, {:struct, WtApi.World}}
  }

  @spec decode(map()) :: {:ok, t()} | {:error, String.t()}
  def decode(value), do: Decoder.decode_struct(value, __MODULE__, @schema)
end

defmodule WtApi.Result.DeleteWorld do
  @moduledoc false
  alias WtApi.Generated.Decoder

  @enforce_keys [:world_id]
  defstruct world_id: nil

  @type t :: %__MODULE__{
          world_id: String.t()
        }

  @schema %{
    world_id: {:required, :uuid}
  }

  @spec decode(map()) :: {:ok, t()} | {:error, String.t()}
  def decode(value), do: Decoder.decode_struct(value, __MODULE__, @schema)
end

defmodule WtApi.Result.StartCodex do
  @moduledoc false
  alias WtApi.Generated.Decoder

  @enforce_keys [:pane_id, :thread_id, :turn_id, :window_name]
  defstruct pane_id: nil, thread_id: nil, turn_id: nil, window_name: nil

  @type t :: %__MODULE__{
          pane_id: String.t(),
          thread_id: String.t(),
          turn_id: String.t(),
          window_name: String.t()
        }

  @schema %{
    pane_id: {:required, :string},
    thread_id: {:required, :string},
    turn_id: {:required, :string},
    window_name: {:required, :string}
  }

  @spec decode(map()) :: {:ok, t()} | {:error, String.t()}
  def decode(value), do: Decoder.decode_struct(value, __MODULE__, @schema)
end

defmodule WtApi.Result.InspectCodex do
  @moduledoc false
  alias WtApi.Generated.Decoder

  @enforce_keys [:observed_at_unix_ms, :pane_id, :screen, :status, :thread_id, :window_name]
  defstruct active_turn_id: nil,
            observed_at_unix_ms: nil,
            pane_id: nil,
            screen: nil,
            status: nil,
            thread_id: nil,
            window_name: nil

  @type t :: %__MODULE__{
          active_turn_id: String.t() | nil,
          observed_at_unix_ms: integer(),
          pane_id: String.t(),
          screen: String.t(),
          status: String.t(),
          thread_id: String.t(),
          window_name: String.t()
        }

  @schema %{
    active_turn_id: {:optional, :string},
    observed_at_unix_ms: {:required, :integer},
    pane_id: {:required, :string},
    screen: {:required, :string},
    status: {:required, {:enum, ["active", "error", "idle"]}},
    thread_id: {:required, :string},
    window_name: {:required, :string}
  }

  @spec decode(map()) :: {:ok, t()} | {:error, String.t()}
  def decode(value), do: Decoder.decode_struct(value, __MODULE__, @schema)
end

defmodule WtApi.Result.SendCodexMessage do
  @moduledoc false
  alias WtApi.Generated.Decoder

  @enforce_keys [:delivery, :thread_id, :turn_id]
  defstruct delivery: nil, thread_id: nil, turn_id: nil

  @type t :: %__MODULE__{
          delivery: String.t(),
          thread_id: String.t(),
          turn_id: String.t()
        }

  @schema %{
    delivery: {:required, {:enum, ["started", "steered"]}},
    thread_id: {:required, :string},
    turn_id: {:required, :string}
  }

  @spec decode(map()) :: {:ok, t()} | {:error, String.t()}
  def decode(value), do: Decoder.decode_struct(value, __MODULE__, @schema)
end

defmodule WtApi.Result.ReadWorldMail do
  @moduledoc false
  alias WtApi.Generated.Decoder

  @enforce_keys [:high_water_message_id, :messages]
  defstruct high_water_message_id: nil, messages: nil

  @type t :: %__MODULE__{
          high_water_message_id: non_neg_integer(),
          messages: [WtApi.WorldMail.t()]
        }

  @schema %{
    high_water_message_id: {:required, :uint64},
    messages: {:required, {:list, {:struct, WtApi.WorldMail}}}
  }

  @spec decode(map()) :: {:ok, t()} | {:error, String.t()}
  def decode(value), do: Decoder.decode_struct(value, __MODULE__, @schema)
end

defmodule WtApi.CapacityDetails do
  @moduledoc false
  alias WtApi.Generated.Decoder

  @enforce_keys [:requested, :reserved, :resource, :total]
  defstruct kind: "capacity", requested: nil, reserved: nil, resource: nil, total: nil

  @type t :: %__MODULE__{
          kind: String.t(),
          requested: non_neg_integer(),
          reserved: non_neg_integer(),
          resource: String.t(),
          total: non_neg_integer()
        }

  @schema %{
    kind: {:required, {:const, "capacity"}},
    requested: {:required, :uint64},
    reserved: {:required, :uint64},
    resource: {:required, {:enum, ["cpu", "disk", "memory"]}},
    total: {:required, :uint64}
  }

  @spec decode(map()) :: {:ok, t()} | {:error, String.t()}
  def decode(value), do: Decoder.decode_struct(value, __MODULE__, @schema)
end

defmodule WtApi.ErrorData do
  @moduledoc false
  alias WtApi.Generated.Decoder

  @enforce_keys [:code, :message, :retryable]
  defstruct code: nil, details: nil, message: nil, retryable: nil

  @type t :: %__MODULE__{
          code: String.t(),
          details: WtApi.CapacityDetails.t() | nil,
          message: String.t(),
          retryable: boolean()
        }

  @schema %{
    code: {:required, :string},
    details: {:optional, {:struct, WtApi.CapacityDetails}},
    message: {:required, :string},
    retryable: {:required, :boolean}
  }

  @spec decode(map()) :: {:ok, t()} | {:error, String.t()}
  def decode(value), do: Decoder.decode_struct(value, __MODULE__, @schema)
end
