defmodule WtApi.Client do
  @moduledoc "Configuration for invoking the local `wt api` command."

  defstruct executable: "wt", env: [], transport: nil

  @type t :: %__MODULE__{
          executable: String.t(),
          transport: (String.t() -> {:ok, map()} | {:error, term()}) | nil,
          env: %{optional(String.t()) => String.t()} | [{String.t(), String.t()}]
        }

  @spec new(keyword()) :: t()
  def new(options \\ []) do
    struct!(__MODULE__, options)
  end
end
