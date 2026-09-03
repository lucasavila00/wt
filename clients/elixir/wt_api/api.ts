import type { NumberFormat, StringFormat } from "@beff/client";

export type Uuid = StringFormat<"Uuid">;
export type Integer = NumberFormat<"Integer">;
export type UnsignedInteger = NumberFormat<"UnsignedInteger">;

export type CreateWorldRequest = {
  api_version: 1;
  request_id: Uuid;
  expected_server_id?: Uuid;
  context: string;
  operation: "create_world";
  name: string;
  vcpus: UnsignedInteger;
  memory_mib: UnsignedInteger;
  disk_gib: UnsignedInteger;
  git_user_name: string;
  git_user_email: string;
};

export type DeleteWorldRequest = {
  api_version: 1;
  request_id: Uuid;
  expected_server_id?: Uuid;
  context: string;
  operation: "delete_world";
  world_id: Uuid;
};

export type StartCodexRequest = {
  api_version: 1;
  request_id: Uuid;
  expected_server_id?: Uuid;
  context: string;
  operation: "start_codex";
  world_id: Uuid;
  message: string;
};

export type InspectCodexRequest = {
  api_version: 1;
  request_id: Uuid;
  expected_server_id?: Uuid;
  context: string;
  operation: "inspect_codex";
  world_id: Uuid;
  thread_id: string;
};

export type SendCodexMessageRequest = {
  api_version: 1;
  request_id: Uuid;
  expected_server_id?: Uuid;
  context: string;
  operation: "send_codex_message";
  world_id: Uuid;
  thread_id: string;
  message: string;
};

export type ReadWorldMailRequest = {
  api_version: 1;
  request_id: Uuid;
  expected_server_id?: Uuid;
  context: string;
  operation: "read_world_mail";
  world_id: Uuid;
  after_message_id: UnsignedInteger;
  limit: UnsignedInteger;
};

export type Request =
  | CreateWorldRequest
  | DeleteWorldRequest
  | StartCodexRequest
  | InspectCodexRequest
  | SendCodexMessageRequest
  | ReadWorldMailRequest;

export type SshAccess = {
  user: string;
  host: string;
  port: UnsignedInteger;
  host_keys: string[];
};

export type World = {
  world_id: Uuid;
  name: string;
  status: "provisioning" | "running" | "stopped" | "destroying" | "error";
  vcpus: UnsignedInteger;
  memory_mib: UnsignedInteger;
  disk_gib: UnsignedInteger;
  guest_ip?: string;
  last_error?: string;
  ssh?: SshAccess;
};

export type WorldMail = {
  message_id: UnsignedInteger;
  world_id: Uuid;
  thread_id?: string;
  turn_id?: string;
  pane_id?: string;
  created_at_unix_ms: Integer;
  kind: "message" | "completed" | "failed";
  text: string;
};

export type CreateWorldResult = { world: World };
export type DeleteWorldResult = { world_id: Uuid };

export type StartCodexResult = {
  thread_id: string;
  turn_id: string;
  pane_id: string;
  window_name: string;
};

export type InspectCodexResult = {
  thread_id: string;
  status: "active" | "idle" | "error";
  active_turn_id?: string;
  pane_id: string;
  window_name: string;
  screen: string;
  observed_at_unix_ms: Integer;
};

export type SendCodexMessageResult = {
  thread_id: string;
  turn_id: string;
  delivery: "steered" | "started";
};

export type ReadWorldMailResult = {
  messages: WorldMail[];
  high_water_message_id: UnsignedInteger;
};

export type Result =
  | CreateWorldResult
  | DeleteWorldResult
  | StartCodexResult
  | InspectCodexResult
  | SendCodexMessageResult
  | ReadWorldMailResult;

export type CapacityDetails = {
  kind: "capacity";
  resource: "cpu" | "memory" | "disk";
  total: UnsignedInteger;
  reserved: UnsignedInteger;
  requested: UnsignedInteger;
};

export type Error = {
  code: string;
  message: string;
  retryable: boolean;
  details?: CapacityDetails;
};

export type Response = {
  api_version: 1;
  request_id?: Uuid;
  server_id?: Uuid;
  expires_at_unix_ms?: Integer;
} & ({ outcome: "ok"; result: Result } | { outcome: "error"; error: Error });
