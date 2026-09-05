defmodule Agapi.Local do
  @moduledoc "Direct process transport. Runs with host permissions; provides no isolation."

  def client(options) do
    executable = Keyword.get(options, :executable, "agapi")
    state_dir = Keyword.fetch!(options, :state_dir)
    env = Keyword.get(options, :env, [])

    Agapi.new(fn input ->
      output =
        Exile.stream([executable, "--state-dir", state_dir, "api"],
          input: [input],
          env: env,
          stderr: :consume
        )
        |> Enum.reduce(%{stdout: [], stderr: [], exit_status: nil}, fn
          {:stdout, bytes}, acc -> %{acc | stdout: [acc.stdout, bytes]}
          {:stderr, bytes}, acc -> %{acc | stderr: [acc.stderr, bytes]}
          {:exit, {:status, status}}, acc -> %{acc | exit_status: status}
        end)

      {:ok, %{output | stdout: IO.iodata_to_binary(output.stdout), stderr: IO.iodata_to_binary(output.stderr)}}
    end)
  end
end
