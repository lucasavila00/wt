defmodule Agapi do
  @moduledoc """
  agapi v1 JSON client. The transport is a one-argument function exchanging
  JSON bytes; it returns stdout, stderr and exit_status without interpreting them.
  Transport errors never trigger automatic retries.
  """

  defstruct [:transport]

  def new(transport) when is_function(transport, 1), do: %__MODULE__{transport: transport}

  def call(%__MODULE__{transport: transport}, request) when is_map(request) do
    request = Map.put(request, "api_version", 1)

    with {:ok, execution} <- transport.(Jason.encode!(request)),
         {:ok, response} <- Jason.decode(execution.stdout) do
      decode(response, execution, request)
    else
      {:error, error} -> {:error, %Agapi.Error{message: inspect(error), kind: :transport}}
    end
  rescue
    error -> {:error, %Agapi.Error{message: Exception.message(error), kind: :transport}}
  end

  defp decode(%{"api_version" => 1, "request_id" => id} = response, execution, %{
         "request_id" => id
       }) do
    case {response, execution.exit_status} do
      {%{"outcome" => "ok", "result" => result}, 0} when is_map(result) ->
        {:ok, result}

      {%{"outcome" => "error", "error" => %{"message" => message}}, status}
      when is_integer(status) and status != 0 ->
        {:error, %Agapi.Error{message: message, kind: :api}}

      _ ->
        {:error, %Agapi.Error{message: "invalid agapi outcome", kind: :protocol}}
    end
  end

  defp decode(_, _, _),
    do: {:error, %Agapi.Error{message: "invalid agapi response identity", kind: :protocol}}
end
