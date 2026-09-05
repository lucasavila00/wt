defmodule Agapi.LocalTest do
  use ExUnit.Case, async: true

  test "independent JSON client crosses the real executable boundary" do
    state = Path.join(System.tmp_dir!(), "agapi-client-#{System.unique_integer([:positive])}")
    on_exit(fn -> File.rm_rf!(state) end)
    executable = System.fetch_env!("AGAPI_TEST_BINARY")
    client = Agapi.Local.client(executable: executable, state_dir: state)

    request = %{
      "request_id" => "11111111-1111-4111-8111-111111111111",
      "operation" => "read_events",
      "after" => 0
    }

    assert {:ok, %{"events" => [], "high_water" => 0}} = Agapi.call(client, request)

    assert {:error, %Agapi.Error{kind: :api}} =
             Agapi.call(client, Map.put(request, "operation", "unknown"))
  end
end
