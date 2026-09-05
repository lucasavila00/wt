import type { NumberFormat, StringFormat } from "@beff/client";

export type Uuid = StringFormat<"Uuid">;
export type Int64 = NumberFormat<"Int64">;
export type UInt16 = NumberFormat<"UInt16">;
export type UInt32 = NumberFormat<"UInt32">;
export type UInt64 = NumberFormat<"UInt64">;

export type CreateWorldRequest = {
  api_version: 1;
  request_id: Uuid;
  expected_server_id?: Uuid;
  context: string;
  operation: "create_world";
  name: string;
  vcpus: UInt32;
  memory_mib: UInt64;
  disk_gib: UInt64;
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

export type ResumeCodexRequest = {
  api_version: 1;
  request_id: Uuid;
  expected_server_id?: Uuid;
  context: string;
  operation: "resume_codex";
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
  after_message_id: UInt64;
  limit: UInt32;
};

export type SteerCodexRequest = {
  api_version: 1;
  request_id: Uuid;
  expected_server_id?: Uuid;
  context: string;
  operation: "steer_codex";
  world_id: Uuid;
  thread_id: string;
  turn_id: string;
  message: string;
};

export type InterruptCodexRequest = {
  api_version: 1;
  request_id: Uuid;
  expected_server_id?: Uuid;
  context: string;
  operation: "interrupt_codex";
  world_id: Uuid;
  thread_id: string;
  turn_id: string;
};

export type Request =
  | CreateWorldRequest
  | DeleteWorldRequest
  | StartCodexRequest
  | InspectCodexRequest
  | ResumeCodexRequest
  | SendCodexMessageRequest
  | SteerCodexRequest
  | InterruptCodexRequest
  | ReadWorldMailRequest;

export type SshAccess = {
  user: string;
  host: string;
  port: UInt16;
  host_keys: string[];
};

export type World = {
  world_id: Uuid;
  name: string;
  status: "provisioning" | "running" | "stopped" | "destroying" | "error";
  vcpus: UInt32;
  memory_mib: UInt64;
  disk_gib: UInt64;
  guest_ip?: string;
  last_error?: string;
  ssh?: SshAccess;
};

export type WorldMail = {
  message_id: UInt64;
  world_id: Uuid;
  thread_id?: string;
  turn_id?: string;
  pane_id?: string;
  created_at_unix_ms: Int64;
  kind: "message" | "completed" | "failed";
  text: string;
};

export type CreateWorldResult = { world: World };
export type DeleteWorldResult = { world_id: Uuid };

export type StartCodexResult = {
  thread_id: string;
  turn_id: string;
  pane_id?: string;
  window_name?: string;
};

export type InspectCodexResult = {
  thread_id: string;
  status: "active" | "idle" | "error";
  active_turn_id?: string;
  pane_id?: string;
  window_name?: string;
  screen?: string;
  observed_at_unix_ms: Int64;
};

export type SendCodexMessageResult = {
  thread_id: string;
  turn_id: string;
  delivery: "steered" | "started" | "interrupt_requested";
};

export type ReadWorldMailResult = {
  messages: WorldMail[];
  high_water_message_id: UInt64;
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
  total: UInt64;
  reserved: UInt64;
  requested: UInt64;
};

export type Error = {
  code: string;
  message: string;
  retryable: boolean;
  details?: CapacityDetails;
};

export type SuccessResponse = {
  api_version: 1;
  request_id: Uuid;
  server_id: Uuid;
  expires_at_unix_ms?: Int64;
  outcome: "ok";
  result: Result;
};

export type ErrorResponse = {
  api_version: 1;
  request_id?: Uuid;
  server_id?: Uuid;
  expires_at_unix_ms?: Int64;
  outcome: "error";
  error: Error;
};

export type Response = SuccessResponse | ErrorResponse;
