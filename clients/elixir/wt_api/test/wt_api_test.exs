defmodule WtApiTest do
  use ExUnit.Case, async: true

  alias WtApi.Request
  alias WtApi.Result
  alias WtApi.{Client, ProtocolError, ServerError, Success, TransportError}

  @request_id "11111111-1111-4111-8111-111111111111"
  @server_id "22222222-2222-4222-8222-222222222222"
  @world_id "00000000-0000-4000-8000-000000000001"

  setup do
    root = Path.join(System.tmp_dir!(), "wt-api-elixir-#{System.unique_integer([:positive])}")
    bin = Path.join(root, "bin")
    File.mkdir_p!(Path.join(root, ".wt"))
    File.mkdir_p!(bin)

    File.write!(
      Path.join(root, ".wt/config.toml"),
      "version = 1\n[[contexts]]\nname = \"ars\"\nkind = \"bare_metal_local\"\n"
    )

    fake_wts = Path.join(bin, "wts")
    File.write!(fake_wts, fake_wts_script())
    File.chmod!(fake_wts, 0o755)
    on_exit(fn -> File.rm_rf!(root) end)

    wt =
      System.get_env("WT_API_TEST_WT", Path.expand("../../../../target/debug/wt", __DIR__))

    client = %Client{
      executable: wt,
      env: %{"HOME" => root, "PATH" => "#{bin}:#{System.get_env("PATH", "")}"}
    }

    %{client: client}
  end

  test "uses wt from PATH by default" do
    assert %Client{executable: "wt", env: []} = Client.new()
  end

  test "all v1 operations cross the real wt api binary", %{client: client} do
    assert {:ok,
            %Success{
              request_id: @request_id,
              server_id: @server_id,
              result: %Result.CreateWorld{
                world: %WtApi.World{
                  world_id: @world_id,
                  name: "agent-1",
                  status: "running",
                  ssh: %WtApi.SshAccess{user: "wt"}
                }
              }
            }} =
             WtApi.create_world(client, %Request.CreateWorld{
               request_id: @request_id,
               expected_server_id: @server_id,
               context: "ars",
               name: "agent-1",
               vcpus: 2,
               memory_mib: 4096,
               disk_gib: 32,
               git_user_name: "Ada Lovelace",
               git_user_email: "ada@example.com"
             })

    assert {:ok, %Success{result: %Result.DeleteWorld{world_id: @world_id}}} =
             WtApi.delete_world(client, %Request.DeleteWorld{
               request_id: @request_id,
               context: "ars",
               world_id: @world_id
             })

    assert {:ok,
            %Success{
              result: %Result.StartCodex{thread_id: "thread-123", turn_id: "turn-456"}
            }} =
             WtApi.start_codex(client, %Request.StartCodex{
               request_id: @request_id,
               context: "ars",
               world_id: @world_id,
               message: "review this"
             })

    assert {:ok,
            %Success{result: %Result.InspectCodex{thread_id: "thread-123", status: "active"}}} =
             WtApi.inspect_codex(client, %Request.InspectCodex{
               request_id: @request_id,
               context: "ars",
               world_id: @world_id,
               thread_id: "thread-123"
             })

    assert {:ok,
            %Success{result: %Result.InspectCodex{thread_id: "thread-123", status: "active"}}} =
             WtApi.resume_codex(client, %Request.ResumeCodex{
               request_id: @request_id,
               context: "ars",
               world_id: @world_id,
               thread_id: "thread-123"
             })

    assert {:ok,
            %Success{
              result: %Result.SendCodexMessage{
                thread_id: "thread-123",
                turn_id: "turn-789",
                delivery: "steered"
              }
            }} =
             WtApi.send_codex_message(client, %Request.SendCodexMessage{
               request_id: @request_id,
               context: "ars",
               world_id: @world_id,
               thread_id: "thread-123",
               message: "continue"
             })

    assert {:ok,
            %Success{
              result: %Result.ReadWorldMail{
                messages: [%WtApi.WorldMail{world_id: @world_id, text: "done"}],
                high_water_message_id: 7
              }
            }} =
             WtApi.read_world_mail(client, %Request.ReadWorldMail{
               request_id: @request_id,
               context: "ars",
               world_id: @world_id,
               after_message_id: 0,
               limit: 10
             })
  end

  test "returns structured server errors without mixing stderr into JSON", %{client: client} do
    assert {:error,
            %ServerError{
              code: "capacity",
              retryable: true,
              exit_status: 1,
              stderr: stderr,
              details: %WtApi.CapacityDetails{resource: "cpu", requested: 2}
            }} =
             WtApi.create_world(client, %Request.CreateWorld{
               request_id: @request_id,
               context: "ars",
               name: "capacity",
               vcpus: 2,
               memory_mib: 4096,
               disk_gib: 32,
               git_user_name: "Ada Lovelace",
               git_user_email: "ada@example.com"
             })

    assert stderr =~ "wt api: request failed"
  end

  test "rejects changed response identities", %{client: client} do
    assert {:error, %ProtocolError{message: "WT returned a different world ID"}} =
             WtApi.delete_world(client, %Request.DeleteWorld{
               request_id: @request_id,
               context: "ars",
               world_id: "00000000-0000-4000-8000-000000000003"
             })

    assert {:error, %ProtocolError{message: "WT returned mail for a different world ID"}} =
             WtApi.read_world_mail(client, %Request.ReadWorldMail{
               request_id: @request_id,
               context: "ars",
               world_id: "00000000-0000-4000-8000-000000000004",
               after_message_id: 0,
               limit: 10
             })

    assert {:error, %ProtocolError{message: "WT returned a different world name"}} =
             WtApi.create_world(client, %Request.CreateWorld{
               request_id: @request_id,
               context: "ars",
               name: "different-name",
               vcpus: 2,
               memory_mib: 4096,
               disk_gib: 32,
               git_user_name: "Ada Lovelace",
               git_user_email: "ada@example.com"
             })

    assert {:error, %ProtocolError{message: "WT returned a different thread ID"}} =
             WtApi.inspect_codex(client, %Request.InspectCodex{
               request_id: @request_id,
               context: "ars",
               world_id: @world_id,
               thread_id: "different-thread"
             })

    assert {:error, %ProtocolError{message: "WT returned a different server ID"}} =
             WtApi.delete_world(client, %Request.DeleteWorld{
               request_id: @request_id,
               expected_server_id: "33333333-3333-4333-8333-333333333333",
               context: "ars",
               world_id: @world_id
             })
  end

  test "validates request metadata before invoking wt", %{client: client} do
    client = %{client | executable: "/does/not/exist"}

    assert {:error, %ProtocolError{message: "invalid api_version: unexpected type"}} =
             WtApi.delete_world(client, %Request.DeleteWorld{
               api_version: 2,
               request_id: @request_id,
               context: "ars",
               world_id: @world_id
             })

    assert {:error, %ProtocolError{message: "invalid request_id: expected UUID"}} =
             WtApi.delete_world(client, %Request.DeleteWorld{
               request_id: "not-a-uuid",
               context: "ars",
               world_id: @world_id
             })
  end

  test "reports process startup failures as transport errors" do
    assert {:error, %TransportError{message: message}} =
             WtApi.delete_world(%Client{executable: "/does/not/exist"}, %Request.DeleteWorld{
               request_id: @request_id,
               context: "ars",
               world_id: @world_id
             })

    assert message =~ "command not found"
  end

  test "generated UUID validation accepts RFC 9562 UUIDv7" do
    assert {:ok, %Request.DeleteWorld{request_id: "01941f29-7c00-7a2a-9c2c-4f5f8cc45d9a"}} =
             Request.DeleteWorld.decode(%{
               "api_version" => 1,
               "request_id" => "01941f29-7c00-7a2a-9c2c-4f5f8cc45d9a",
               "context" => "ars",
               "operation" => "delete_world",
               "world_id" => @world_id
             })
  end

  test "generated integer decoders preserve wire widths" do
    request = %{
      "api_version" => 1,
      "request_id" => @request_id,
      "context" => "ars",
      "operation" => "create_world",
      "name" => "agent-1",
      "vcpus" => 4_294_967_295,
      "memory_mib" => 18_446_744_073_709_551_615,
      "disk_gib" => 18_446_744_073_709_551_615,
      "git_user_name" => "Ada Lovelace",
      "git_user_email" => "ada@example.com"
    }

    assert {:ok, %Request.CreateWorld{}} = Request.CreateWorld.decode(request)

    assert {:error, "invalid vcpus: unexpected type"} =
             Request.CreateWorld.decode(%{request | "vcpus" => 4_294_967_296})

    assert {:error, "invalid memory_mib: unexpected type"} =
             Request.CreateWorld.decode(%{
               request
               | "memory_mib" => 18_446_744_073_709_551_616
             })

    ssh = %{"host" => "192.0.2.2", "host_keys" => [], "port" => 65_535, "user" => "wt"}
    assert {:ok, %WtApi.SshAccess{port: 65_535}} = WtApi.SshAccess.decode(ssh)

    assert {:error, "invalid port: unexpected type"} =
             WtApi.SshAccess.decode(%{ssh | "port" => 65_536})
  end

  defp fake_wts_script do
    ~S"""
    #!/bin/sh
    set -eu
    request=$(cat)
    case "$request" in
      *'"request_id":"11111111-1111-4111-8111-111111111111"'*) ;;
      *) exit 3 ;;
    esac
    case "$request" in
      *'"expected_server_id":"22222222-2222-4222-8222-222222222222"'*'"name":"agent-1"'*) ;;
      *'"name":"agent-1"'*) exit 4 ;;
      *) ;;
    esac
    case "$request" in
      *'"name":"capacity"'*)
        printf '%s\n' '{"protocol_version":20,"request_id":"11111111-1111-4111-8111-111111111111","server_id":"22222222-2222-4222-8222-222222222222","outcome":"error","error":{"code":"capacity","message":"world CPU capacity is full","retryable":true,"capacity":{"resource":"cpu","total":4,"reserved":4,"requested":2}}}'
        ;;
      *'"operation":"create_world"'*)
        printf '%s\n' '{"protocol_version":20,"event":"progress","message":"creating disk"}'
        printf '%s\n' '{"protocol_version":20,"request_id":"11111111-1111-4111-8111-111111111111","server_id":"22222222-2222-4222-8222-222222222222","expires_at_unix_ms":2592000100,"outcome":"ok","response":{"response":"world","world":{"world_id":"00000000-0000-4000-8000-000000000001","name":"agent-1","owner":"tester","status":"running","vcpus":2,"memory_mib":4096,"disk_gib":32,"guest_ip":"192.0.2.2","ssh":{"user":"wt","host":"192.0.2.2","port":22,"host_keys":["ssh-ed25519 AAAATEST guest"],"future_ssh_field":true},"future_world_field":true}},"future_response_field":true}'
        ;;
      *'"operation":"start_codex"'*)
        printf '%s\n' '{"protocol_version":20,"request_id":"11111111-1111-4111-8111-111111111111","server_id":"22222222-2222-4222-8222-222222222222","outcome":"ok","response":{"response":"codex_started","thread_id":"thread-123","turn_id":"turn-456","pane_id":"%7","window_name":"codex-thread-123"}}'
        ;;
      *'"operation":"inspect_codex"'*|*'"operation":"resume_codex"'*)
        printf '%s\n' '{"protocol_version":20,"request_id":"11111111-1111-4111-8111-111111111111","server_id":"22222222-2222-4222-8222-222222222222","outcome":"ok","response":{"response":"codex_inspection","thread_id":"thread-123","status":"active","active_turn_id":"turn-456","pane_id":"%7","window_name":"codex-thread-123","screen":"Codex is working","observed_at_unix_ms":1800000000000}}'
        ;;
      *'"operation":"send_codex_message"'*)
        printf '%s\n' '{"protocol_version":20,"request_id":"11111111-1111-4111-8111-111111111111","server_id":"22222222-2222-4222-8222-222222222222","outcome":"ok","response":{"response":"codex_message_sent","thread_id":"thread-123","turn_id":"turn-789","delivery":"steered"}}'
        ;;
      *'"operation":"list_world_mail"'*)
        printf '%s\n' '{"protocol_version":20,"request_id":"11111111-1111-4111-8111-111111111111","server_id":"22222222-2222-4222-8222-222222222222","outcome":"ok","response":{"response":"world_mail","messages":[{"id":7,"world_id":"00000000-0000-4000-8000-000000000001","thread_id":"thread-123","turn_id":"turn-456","pane_id":"%7","created_at_unix_ms":1800000000000,"kind":"completed","message":"done"}],"high_water_id":7}}'
        ;;
      *'"operation":"delete_world"'*)
        printf '%s\n' '{"protocol_version":20,"request_id":"11111111-1111-4111-8111-111111111111","server_id":"22222222-2222-4222-8222-222222222222","expires_at_unix_ms":2592000100,"outcome":"ok","response":{"response":"world_deleted","world_id":"00000000-0000-4000-8000-000000000001"}}'
        ;;
      *) exit 2 ;;
    esac
    """
  end
end
