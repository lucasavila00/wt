defmodule WtApi.TransportError do
  @moduledoc "A failure to start or communicate with the `wt` process."

  defexception [:message, :exit_status, :stderr]

  @type t :: %__MODULE__{
          message: String.t(),
          exit_status: non_neg_integer() | nil,
          stderr: String.t()
        }
end

defmodule WtApi.Success do
  @moduledoc "A validated successful WT API response and its typed operation result."

  @enforce_keys [:request_id, :server_id, :result]
  defstruct [:request_id, :server_id, :expires_at_unix_ms, :result]

  @type t(result) :: %__MODULE__{
          request_id: String.t(),
          server_id: String.t(),
          expires_at_unix_ms: integer() | nil,
          result: result
        }
end

defmodule WtApi.ProtocolError do
  @moduledoc "A response that does not satisfy the WT API v1 protocol."

  defexception [:message]

  @type t :: %__MODULE__{message: String.t()}
end

defmodule WtApi.ServerError do
  @moduledoc "A structured error returned by WT."

  defexception [
    :code,
    :message,
    :details,
    :request_id,
    :server_id,
    :expires_at_unix_ms,
    :exit_status,
    :stderr,
    retryable: false
  ]

  @type t :: %__MODULE__{
          code: String.t(),
          message: String.t(),
          retryable: boolean(),
          details: WtApi.CapacityDetails.t() | nil,
          request_id: String.t(),
          server_id: String.t() | nil,
          expires_at_unix_ms: integer() | nil,
          exit_status: non_neg_integer(),
          stderr: String.t()
        }
end
