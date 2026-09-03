defmodule WtApi.Client do
  @moduledoc "Configuration for invoking the local `wt api` command."

  @enforce_keys [:executable]
  defstruct executable: "wt", env: []

  @type t :: %__MODULE__{
          executable: String.t(),
          env: %{optional(String.t()) => String.t()} | [{String.t(), String.t()}]
        }

  @spec new(keyword()) :: t()
  def new(options \\ []) do
    struct!(__MODULE__, options)
  end
end
