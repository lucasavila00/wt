defmodule WtApi do
  @moduledoc "Typed Elixir client for the version 1 `wt api` JSON protocol."

  alias WtApi.Request
  alias WtApi.Result
  alias WtApi.{Client, ErrorData, ProtocolError, ServerError, Success, TransportError}

  @type error :: TransportError.t() | ProtocolError.t() | ServerError.t()

  @spec create_world(Request.CreateWorld.t()) ::
          {:ok, Success.t(Result.CreateWorld.t())} | {:error, error()}
  def create_world(request), do: create_world(Client.new(), request)

  @spec create_world(Client.t(), Request.CreateWorld.t()) ::
          {:ok, Success.t(Result.CreateWorld.t())} | {:error, error()}
  def create_world(client, %Request.CreateWorld{} = request) do
    call(client, request, Result.CreateWorld, &world_identity(&1, request))
  end

  @spec delete_world(Request.DeleteWorld.t()) ::
          {:ok, Success.t(Result.DeleteWorld.t())} | {:error, error()}
  def delete_world(request), do: delete_world(Client.new(), request)

  @spec delete_world(Client.t(), Request.DeleteWorld.t()) ::
          {:ok, Success.t(Result.DeleteWorld.t())} | {:error, error()}
  def delete_world(client, %Request.DeleteWorld{} = request) do
    call(
      client,
      request,
      Result.DeleteWorld,
      &equal_identity(&1.world_id, request.world_id, "world ID")
    )
  end

  @spec start_codex(Request.StartCodex.t()) ::
          {:ok, Success.t(Result.StartCodex.t())} | {:error, error()}
  def start_codex(request), do: start_codex(Client.new(), request)

  @spec start_codex(Client.t(), Request.StartCodex.t()) ::
          {:ok, Success.t(Result.StartCodex.t())} | {:error, error()}
  def start_codex(client, %Request.StartCodex{} = request) do
    call(client, request, Result.StartCodex, fn _result -> :ok end)
  end

  @spec inspect_codex(Request.InspectCodex.t()) ::
          {:ok, Success.t(Result.InspectCodex.t())} | {:error, error()}
  def inspect_codex(request), do: inspect_codex(Client.new(), request)

  @spec inspect_codex(Client.t(), Request.InspectCodex.t()) ::
          {:ok, Success.t(Result.InspectCodex.t())} | {:error, error()}
  def inspect_codex(client, %Request.InspectCodex{} = request) do
    call(
      client,
      request,
      Result.InspectCodex,
      &equal_identity(&1.thread_id, request.thread_id, "thread ID")
    )
  end

  @doc "Resume a persisted thread and restore its visible window without starting a turn."
  @spec resume_codex(Request.ResumeCodex.t()) ::
          {:ok, Success.t(Result.InspectCodex.t())} | {:error, error()}
  def resume_codex(request), do: resume_codex(Client.new(), request)

  @spec resume_codex(Client.t(), Request.ResumeCodex.t()) ::
          {:ok, Success.t(Result.InspectCodex.t())} | {:error, error()}
  def resume_codex(client, %Request.ResumeCodex{} = request) do
    call(
      client,
      request,
      Result.InspectCodex,
      &equal_identity(&1.thread_id, request.thread_id, "thread ID")
    )
  end

  @spec send_codex_message(Request.SendCodexMessage.t()) ::
          {:ok, Success.t(Result.SendCodexMessage.t())} | {:error, error()}
  def send_codex_message(request), do: send_codex_message(Client.new(), request)

  @spec send_codex_message(Client.t(), Request.SendCodexMessage.t()) ::
          {:ok, Success.t(Result.SendCodexMessage.t())} | {:error, error()}
  def send_codex_message(client, %Request.SendCodexMessage{} = request) do
    call(
      client,
      request,
      Result.SendCodexMessage,
      &equal_identity(&1.thread_id, request.thread_id, "thread ID")
    )
  end

  def steer_codex(request), do: steer_codex(Client.new(), request)

  def steer_codex(client, %Request.SteerCodex{} = request) do
    call(client, request, Result.SendCodexMessage, fn result ->
      with :ok <- equal_identity(result.thread_id, request.thread_id, "thread ID"),
           :ok <- equal_identity(result.turn_id, request.turn_id, "turn ID"),
           do: :ok
    end)
  end

  def interrupt_codex(request), do: interrupt_codex(Client.new(), request)

  def interrupt_codex(client, %Request.InterruptCodex{} = request) do
    call(client, request, Result.SendCodexMessage, fn result ->
      with :ok <- equal_identity(result.thread_id, request.thread_id, "thread ID"),
           :ok <- equal_identity(result.turn_id, request.turn_id, "turn ID"),
           do: :ok
    end)
  end

  @spec read_world_mail(Request.ReadWorldMail.t()) ::
          {:ok, Success.t(Result.ReadWorldMail.t())} | {:error, error()}
  def read_world_mail(request), do: read_world_mail(Client.new(), request)

  @spec read_world_mail(Client.t(), Request.ReadWorldMail.t()) ::
          {:ok, Success.t(Result.ReadWorldMail.t())} | {:error, error()}
  def read_world_mail(client, %Request.ReadWorldMail{} = request) do
    call(client, request, Result.ReadWorldMail, &mail_identity(&1, request.world_id))
  end

  defp call(%Client{} = client, request, result_module, validate_identity) do
    request_map = encode_request(request)

    with {:ok, _validated_request} <- protocol_decode(request.__struct__, request_map),
         {:ok, execution} <- execute(client, Jason.encode!(request_map)),
         {:ok, response} <- decode_response(execution),
         :ok <- validate_metadata(response, request),
         {:ok, result} <- decode_outcome(response, execution, result_module),
         :ok <- validate_identity.(result) do
      {:ok,
       %Success{
         request_id: response["request_id"],
         server_id: response["server_id"],
         expires_at_unix_ms: response["expires_at_unix_ms"],
         result: result
       }}
    end
  end

  defp encode_request(request) do
    request
    |> Map.from_struct()
    |> Enum.reject(fn {_key, value} -> is_nil(value) end)
    |> Map.new(fn {key, value} -> {Atom.to_string(key), value} end)
  end

  defp execute(%Client{} = client, input) do
    try do
      result =
        Exile.stream([client.executable, "api"],
          input: [input],
          stderr: :consume,
          env: client.env
        )
        |> Enum.reduce(%{stdout: [], stderr: [], exit_status: nil}, fn
          {:stdout, data}, output -> %{output | stdout: [data | output.stdout]}
          {:stderr, data}, output -> %{output | stderr: [data | output.stderr]}
          {:exit, {:status, status}}, output -> %{output | exit_status: status}
        end)

      {:ok,
       %{
         stdout: result.stdout |> Enum.reverse() |> IO.iodata_to_binary(),
         stderr: result.stderr |> Enum.reverse() |> IO.iodata_to_binary(),
         exit_status: result.exit_status
       }}
    rescue
      error ->
        {:error,
         %TransportError{
           message: Exception.message(error),
           exit_status: nil,
           stderr: ""
         }}
    catch
      :exit, reason ->
        {:error,
         %TransportError{
           message: "wt process exited unexpectedly: #{inspect(reason)}",
           exit_status: nil,
           stderr: ""
         }}
    end
  end

  defp decode_response(%{stdout: stdout, stderr: stderr, exit_status: status}) do
    case Jason.decode(stdout) do
      {:ok, response} when is_map(response) ->
        {:ok, response}

      {:ok, _response} ->
        {:error, %ProtocolError{message: "WT API response is not an object"}}

      {:error, error} ->
        {:error,
         %TransportError{
           message: "WT API returned invalid JSON: #{Exception.message(error)}",
           exit_status: status,
           stderr: stderr
         }}
    end
  end

  defp validate_metadata(response, request) do
    with :ok <- equal_identity(response["api_version"], 1, "API version"),
         :ok <- equal_identity(response["request_id"], request.request_id, "request ID"),
         :ok <- valid_uuid(response["server_id"], response["outcome"] == "ok", "server ID"),
         :ok <- expected_server(response["server_id"], request.expected_server_id) do
      :ok
    end
  end

  defp decode_outcome(%{"outcome" => "ok", "result" => value}, %{exit_status: 0}, module) do
    protocol_decode(module, value)
  end

  defp decode_outcome(%{"outcome" => "ok"}, %{exit_status: status}, _module) do
    protocol_error("WT returned a successful response with exit status #{inspect(status)}")
  end

  defp decode_outcome(
         %{"outcome" => "error", "error" => value} = response,
         %{exit_status: status} = execution,
         _module
       )
       when is_integer(status) and status != 0 do
    with {:ok, error} <- protocol_decode(ErrorData, value) do
      {:error,
       %ServerError{
         code: error.code,
         message: error.message,
         retryable: error.retryable,
         details: error.details,
         request_id: response["request_id"],
         server_id: response["server_id"],
         expires_at_unix_ms: response["expires_at_unix_ms"],
         exit_status: status,
         stderr: execution.stderr
       }}
    end
  end

  defp decode_outcome(%{"outcome" => "error"}, %{exit_status: status}, _module) do
    protocol_error("WT returned an error response with exit status #{inspect(status)}")
  end

  defp decode_outcome(_response, _execution, _module),
    do: protocol_error("invalid WT API outcome")

  defp protocol_decode(module, value) do
    case module.decode(value) do
      {:ok, decoded} -> {:ok, decoded}
      {:error, message} -> protocol_error(message)
    end
  end

  defp world_identity(result, request) do
    equal_identity(result.world.name, request.name, "world name")
  end

  defp mail_identity(result, world_id) do
    if Enum.all?(result.messages, &(&1.world_id == world_id)) do
      :ok
    else
      protocol_error("WT returned mail for a different world ID")
    end
  end

  defp expected_server(_actual, nil), do: :ok
  defp expected_server(actual, expected), do: equal_identity(actual, expected, "server ID")

  defp valid_uuid(nil, false, _identity), do: :ok

  defp valid_uuid(value, _required, identity) when is_binary(value) do
    case WtApi.Generated.Decoder.decode_uuid(value) do
      {:ok, _value} -> :ok
      {:error, _reason} -> protocol_error("WT returned an invalid #{identity}")
    end
  end

  defp valid_uuid(_value, _required, identity), do: protocol_error("WT omitted #{identity}")

  defp equal_identity(value, value, _identity), do: :ok

  defp equal_identity(_actual, _expected, identity),
    do: protocol_error("WT returned a different #{identity}")

  defp protocol_error(message), do: {:error, %ProtocolError{message: message}}
end
