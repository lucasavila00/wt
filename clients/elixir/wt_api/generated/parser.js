//@ts-nocheck

"use strict";

import {
  TypeofRuntype,
  AnyRuntype,
  NullishRuntype,
  NeverRuntype,
  ConstRuntype,
  RegexRuntype,
  DateRuntype,
  BigIntRuntype,
  StringWithFormatRuntype,
  NumberWithFormatRuntype,
  AnyOfConstsRuntype,
  TupleRuntype,
  AllOfRuntype,
  AnyOfRuntype,
  ArrayRuntype,
  AnyOfDiscriminatedRuntype,
  ObjectRuntype,
  OptionalFieldRuntype,
  BaseRefRuntype,
  registerStringFormatter,
  registerNumberFormatter,
  buildParserFromRuntype,
  generateHashFromString,
  TypedArrayRuntype,
  MapRuntype,
  SetRuntype,
} from "@beff/client/codegen-v2";

class RefRuntype extends BaseRefRuntype  {
  getNamedRuntypes() {
    return namedRuntypes;
  }
}

const buildParsers = (args) => {
  const stringFormats = args?.stringFormats ?? {};
  for (const k of RequiredStringFormats) {
    if (stringFormats[k] == null) {
      throw new Error(`Missing custom format ${k}`);
    }
  }
  Object.keys(stringFormats).forEach((k) => {
    const v = stringFormats[k];
    registerStringFormatter(k, v);
  });
  const numberFormats = args?.numberFormats ?? {};
  for (const k of RequiredNumberFormats) {
    if (numberFormats[k] == null) {
      throw new Error(`Missing custom format ${k}`);
    }
  }
  Object.keys(numberFormats).forEach((k) => {
    const v = numberFormats[k];
    registerNumberFormatter(k, v);
  });
  let acc = {};
  for (const k of Object.keys(buildParsersInput)) {
    const it = buildParserFromRuntype(buildParsersInput[k], k, false);
    acc[k] = it;
  }
  return acc;
};

const RequiredStringFormats = ["Uuid"];
const RequiredNumberFormats = ["Int64","UInt16","UInt32","UInt64"];
const direct_hoist_0 = new RefRuntype(undefined, "ExecWorldRequest");
const direct_hoist_1 = new RefRuntype(undefined, "ExecWorldResult");
const direct_hoist_2 = new RefRuntype(undefined, "Request");
const direct_hoist_3 = new RefRuntype(undefined, "Response");
const direct_hoist_4 = new RefRuntype(undefined, "CreateWorldRequest");
const direct_hoist_5 = new RefRuntype(undefined, "DeleteWorldRequest");
const direct_hoist_6 = new RefRuntype(undefined, "StartCodexRequest");
const direct_hoist_7 = new RefRuntype(undefined, "InspectCodexRequest");
const direct_hoist_8 = new RefRuntype(undefined, "ResumeCodexRequest");
const direct_hoist_9 = new RefRuntype(undefined, "SendCodexMessageRequest");
const direct_hoist_10 = new RefRuntype(undefined, "SteerCodexRequest");
const direct_hoist_11 = new RefRuntype(undefined, "InterruptCodexRequest");
const direct_hoist_12 = new RefRuntype(undefined, "ReadWorldMailRequest");
const direct_hoist_13 = new RefRuntype(undefined, "SshAccess");
const direct_hoist_14 = new RefRuntype(undefined, "World");
const direct_hoist_15 = new RefRuntype(undefined, "WorldMail");
const direct_hoist_16 = new RefRuntype(undefined, "CreateWorldResult");
const direct_hoist_17 = new RefRuntype(undefined, "DeleteWorldResult");
const direct_hoist_18 = new RefRuntype(undefined, "StartCodexResult");
const direct_hoist_19 = new RefRuntype(undefined, "InspectCodexResult");
const direct_hoist_20 = new RefRuntype(undefined, "SendCodexMessageResult");
const direct_hoist_21 = new RefRuntype(undefined, "ReadWorldMailResult");
const direct_hoist_22 = new RefRuntype(undefined, "CapacityDetails");
const direct_hoist_23 = new RefRuntype(undefined, "Error");
const direct_hoist_24 = new RefRuntype(undefined, "SuccessResponse");
const direct_hoist_25 = new RefRuntype(undefined, "ErrorResponse");
const direct_hoist_26 = new ConstRuntype(undefined, "capacity");
const direct_hoist_27 = new RefRuntype(undefined, "UInt64");
const direct_hoist_28 = new AnyOfConstsRuntype(undefined, [
    "cpu",
    "disk",
    "memory"
]);
const direct_hoist_29 = new ObjectRuntype(undefined, {
    "kind": direct_hoist_26,
    "requested": direct_hoist_27,
    "reserved": direct_hoist_27,
    "resource": direct_hoist_28,
    "total": direct_hoist_27
}, []);
const direct_hoist_30 = new ConstRuntype(undefined, 1);
const direct_hoist_31 = new TypeofRuntype(undefined, "string");
const direct_hoist_32 = new RefRuntype(undefined, "Uuid");
const direct_hoist_33 = new ConstRuntype(undefined, "create_world");
const direct_hoist_34 = new RefRuntype(undefined, "UInt32");
const direct_hoist_35 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_30,
    "context": direct_hoist_31,
    "disk_gib": direct_hoist_27,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_32),
    "git_user_email": direct_hoist_31,
    "git_user_name": direct_hoist_31,
    "memory_mib": direct_hoist_27,
    "name": direct_hoist_31,
    "operation": direct_hoist_33,
    "request_id": direct_hoist_32,
    "vcpus": direct_hoist_34
}, []);
const direct_hoist_36 = new ObjectRuntype(undefined, {
    "world": direct_hoist_14
}, []);
const direct_hoist_37 = new ConstRuntype(undefined, "delete_world");
const direct_hoist_38 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_30,
    "context": direct_hoist_31,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_32),
    "operation": direct_hoist_37,
    "request_id": direct_hoist_32,
    "world_id": direct_hoist_32
}, []);
const direct_hoist_39 = new ObjectRuntype(undefined, {
    "world_id": direct_hoist_32
}, []);
const direct_hoist_40 = new TypeofRuntype(undefined, "boolean");
const direct_hoist_41 = new ObjectRuntype(undefined, {
    "code": direct_hoist_31,
    "details": new OptionalFieldRuntype(direct_hoist_22),
    "message": direct_hoist_31,
    "retryable": direct_hoist_40
}, []);
const direct_hoist_42 = new RefRuntype(undefined, "Int64");
const direct_hoist_43 = new ConstRuntype(undefined, "error");
const direct_hoist_44 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_30,
    "error": direct_hoist_23,
    "expires_at_unix_ms": new OptionalFieldRuntype(direct_hoist_42),
    "outcome": direct_hoist_43,
    "request_id": new OptionalFieldRuntype(direct_hoist_32),
    "server_id": new OptionalFieldRuntype(direct_hoist_32)
}, []);
const direct_hoist_45 = new ArrayRuntype(undefined, direct_hoist_31);
const direct_hoist_46 = new ConstRuntype(undefined, "exec_world");
const direct_hoist_47 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_30,
    "args": direct_hoist_45,
    "context": direct_hoist_31,
    "executable": direct_hoist_31,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_32),
    "operation": direct_hoist_46,
    "request_id": direct_hoist_32,
    "stdin": direct_hoist_31,
    "world_id": direct_hoist_32
}, []);
const direct_hoist_48 = new ObjectRuntype(undefined, {
    "exit_status": direct_hoist_42,
    "stderr": direct_hoist_31,
    "stdout": direct_hoist_31
}, []);
const direct_hoist_49 = new ConstRuntype(undefined, "inspect_codex");
const direct_hoist_50 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_30,
    "context": direct_hoist_31,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_32),
    "operation": direct_hoist_49,
    "request_id": direct_hoist_32,
    "thread_id": direct_hoist_31,
    "world_id": direct_hoist_32
}, []);
const direct_hoist_51 = new AnyOfConstsRuntype(undefined, [
    "active",
    "error",
    "idle"
]);
const direct_hoist_52 = new ObjectRuntype(undefined, {
    "active_turn_id": new OptionalFieldRuntype(direct_hoist_31),
    "observed_at_unix_ms": direct_hoist_42,
    "pane_id": new OptionalFieldRuntype(direct_hoist_31),
    "screen": new OptionalFieldRuntype(direct_hoist_31),
    "status": direct_hoist_51,
    "thread_id": direct_hoist_31,
    "window_name": new OptionalFieldRuntype(direct_hoist_31)
}, []);
const direct_hoist_53 = new NumberWithFormatRuntype(undefined, [
    "Int64"
]);
const direct_hoist_54 = new ConstRuntype(undefined, "interrupt_codex");
const direct_hoist_55 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_30,
    "context": direct_hoist_31,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_32),
    "operation": direct_hoist_54,
    "request_id": direct_hoist_32,
    "thread_id": direct_hoist_31,
    "turn_id": direct_hoist_31,
    "world_id": direct_hoist_32
}, []);
const direct_hoist_56 = new ConstRuntype(undefined, "list_contexts");
const direct_hoist_57 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_30,
    "operation": direct_hoist_56,
    "request_id": direct_hoist_32
}, []);
const direct_hoist_58 = new ObjectRuntype(undefined, {
    "contexts": direct_hoist_45
}, []);
const direct_hoist_59 = new ConstRuntype(undefined, "list_worlds");
const direct_hoist_60 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_30,
    "context": direct_hoist_31,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_32),
    "operation": direct_hoist_59,
    "request_id": direct_hoist_32
}, []);
const direct_hoist_61 = new ArrayRuntype(undefined, direct_hoist_14);
const direct_hoist_62 = new ObjectRuntype(undefined, {
    "worlds": direct_hoist_61
}, []);
const direct_hoist_63 = new ConstRuntype(undefined, "read_world_mail");
const direct_hoist_64 = new ObjectRuntype(undefined, {
    "after_message_id": direct_hoist_27,
    "api_version": direct_hoist_30,
    "context": direct_hoist_31,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_32),
    "limit": direct_hoist_34,
    "operation": direct_hoist_63,
    "request_id": direct_hoist_32,
    "world_id": direct_hoist_32
}, []);
const direct_hoist_65 = new ArrayRuntype(undefined, direct_hoist_15);
const direct_hoist_66 = new ObjectRuntype(undefined, {
    "high_water_message_id": direct_hoist_27,
    "messages": direct_hoist_65
}, []);
const direct_hoist_67 = new RefRuntype(undefined, "ListContextsRequest");
const direct_hoist_68 = new RefRuntype(undefined, "ListWorldsRequest");
const direct_hoist_69 = new AnyOfDiscriminatedRuntype(undefined, [
    direct_hoist_12,
    direct_hoist_0,
    direct_hoist_4,
    direct_hoist_9,
    direct_hoist_6,
    direct_hoist_10,
    direct_hoist_5,
    direct_hoist_7,
    direct_hoist_11,
    direct_hoist_68,
    direct_hoist_8,
    direct_hoist_67
], "operation", {
    "create_world": direct_hoist_4,
    "delete_world": direct_hoist_5,
    "exec_world": direct_hoist_0,
    "inspect_codex": direct_hoist_7,
    "interrupt_codex": direct_hoist_11,
    "list_contexts": direct_hoist_67,
    "list_worlds": direct_hoist_68,
    "read_world_mail": direct_hoist_12,
    "resume_codex": direct_hoist_8,
    "send_codex_message": direct_hoist_9,
    "start_codex": direct_hoist_6,
    "steer_codex": direct_hoist_10
}, {
    "create_world": direct_hoist_4,
    "delete_world": direct_hoist_5,
    "exec_world": direct_hoist_0,
    "inspect_codex": direct_hoist_7,
    "interrupt_codex": direct_hoist_11,
    "list_contexts": direct_hoist_67,
    "list_worlds": direct_hoist_68,
    "read_world_mail": direct_hoist_12,
    "resume_codex": direct_hoist_8,
    "send_codex_message": direct_hoist_9,
    "start_codex": direct_hoist_6,
    "steer_codex": direct_hoist_10
});
const direct_hoist_70 = new AnyOfDiscriminatedRuntype(undefined, [
    direct_hoist_25,
    direct_hoist_24
], "outcome", {
    "error": direct_hoist_25,
    "ok": direct_hoist_24
}, {
    "error": direct_hoist_25,
    "ok": direct_hoist_24
});
const direct_hoist_71 = new RefRuntype(undefined, "ListContextsResult");
const direct_hoist_72 = new RefRuntype(undefined, "ListWorldsResult");
const direct_hoist_73 = new AnyOfRuntype(undefined, [
    direct_hoist_16,
    direct_hoist_17,
    direct_hoist_1,
    direct_hoist_19,
    direct_hoist_71,
    direct_hoist_72,
    direct_hoist_21,
    direct_hoist_20,
    direct_hoist_18
]);
const direct_hoist_74 = new ConstRuntype(undefined, "resume_codex");
const direct_hoist_75 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_30,
    "context": direct_hoist_31,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_32),
    "operation": direct_hoist_74,
    "request_id": direct_hoist_32,
    "thread_id": direct_hoist_31,
    "world_id": direct_hoist_32
}, []);
const direct_hoist_76 = new ConstRuntype(undefined, "send_codex_message");
const direct_hoist_77 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_30,
    "context": direct_hoist_31,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_32),
    "message": direct_hoist_31,
    "operation": direct_hoist_76,
    "request_id": direct_hoist_32,
    "thread_id": direct_hoist_31,
    "world_id": direct_hoist_32
}, []);
const direct_hoist_78 = new AnyOfConstsRuntype(undefined, [
    "interrupt_requested",
    "started",
    "steered"
]);
const direct_hoist_79 = new ObjectRuntype(undefined, {
    "delivery": direct_hoist_78,
    "thread_id": direct_hoist_31,
    "turn_id": direct_hoist_31
}, []);
const direct_hoist_80 = new RefRuntype(undefined, "UInt16");
const direct_hoist_81 = new ObjectRuntype(undefined, {
    "host": direct_hoist_31,
    "host_keys": direct_hoist_45,
    "port": direct_hoist_80,
    "user": direct_hoist_31
}, []);
const direct_hoist_82 = new ConstRuntype(undefined, "start_codex");
const direct_hoist_83 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_30,
    "context": direct_hoist_31,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_32),
    "message": direct_hoist_31,
    "operation": direct_hoist_82,
    "request_id": direct_hoist_32,
    "world_id": direct_hoist_32
}, []);
const direct_hoist_84 = new ObjectRuntype(undefined, {
    "pane_id": new OptionalFieldRuntype(direct_hoist_31),
    "thread_id": direct_hoist_31,
    "turn_id": direct_hoist_31,
    "window_name": new OptionalFieldRuntype(direct_hoist_31)
}, []);
const direct_hoist_85 = new ConstRuntype(undefined, "steer_codex");
const direct_hoist_86 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_30,
    "context": direct_hoist_31,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_32),
    "message": direct_hoist_31,
    "operation": direct_hoist_85,
    "request_id": direct_hoist_32,
    "thread_id": direct_hoist_31,
    "turn_id": direct_hoist_31,
    "world_id": direct_hoist_32
}, []);
const direct_hoist_87 = new ConstRuntype(undefined, "ok");
const direct_hoist_88 = new RefRuntype(undefined, "Result");
const direct_hoist_89 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_30,
    "expires_at_unix_ms": new OptionalFieldRuntype(direct_hoist_42),
    "outcome": direct_hoist_87,
    "request_id": direct_hoist_32,
    "result": direct_hoist_88,
    "server_id": new OptionalFieldRuntype(direct_hoist_32)
}, []);
const direct_hoist_90 = new NumberWithFormatRuntype(undefined, [
    "UInt16"
]);
const direct_hoist_91 = new NumberWithFormatRuntype(undefined, [
    "UInt32"
]);
const direct_hoist_92 = new NumberWithFormatRuntype(undefined, [
    "UInt64"
]);
const direct_hoist_93 = new StringWithFormatRuntype(undefined, [
    "Uuid"
]);
const direct_hoist_94 = new AnyOfConstsRuntype(undefined, [
    "destroying",
    "error",
    "provisioning",
    "running",
    "stopped"
]);
const direct_hoist_95 = new ObjectRuntype(undefined, {
    "disk_gib": direct_hoist_27,
    "guest_ip": new OptionalFieldRuntype(direct_hoist_31),
    "last_error": new OptionalFieldRuntype(direct_hoist_31),
    "memory_mib": direct_hoist_27,
    "name": direct_hoist_31,
    "ssh": new OptionalFieldRuntype(direct_hoist_13),
    "status": direct_hoist_94,
    "vcpus": direct_hoist_34,
    "world_id": direct_hoist_32
}, []);
const direct_hoist_96 = new AnyOfConstsRuntype(undefined, [
    "completed",
    "failed",
    "message"
]);
const direct_hoist_97 = new ObjectRuntype(undefined, {
    "created_at_unix_ms": direct_hoist_42,
    "kind": direct_hoist_96,
    "message_id": direct_hoist_27,
    "pane_id": new OptionalFieldRuntype(direct_hoist_31),
    "text": direct_hoist_31,
    "thread_id": new OptionalFieldRuntype(direct_hoist_31),
    "turn_id": new OptionalFieldRuntype(direct_hoist_31),
    "world_id": direct_hoist_32
}, []);
const namedRuntypes = {
    "CapacityDetails": direct_hoist_29,
    "CreateWorldRequest": direct_hoist_35,
    "CreateWorldResult": direct_hoist_36,
    "DeleteWorldRequest": direct_hoist_38,
    "DeleteWorldResult": direct_hoist_39,
    "Error": direct_hoist_41,
    "ErrorResponse": direct_hoist_44,
    "ExecWorldRequest": direct_hoist_47,
    "ExecWorldResult": direct_hoist_48,
    "InspectCodexRequest": direct_hoist_50,
    "InspectCodexResult": direct_hoist_52,
    "Int64": direct_hoist_53,
    "InterruptCodexRequest": direct_hoist_55,
    "ListContextsRequest": direct_hoist_57,
    "ListContextsResult": direct_hoist_58,
    "ListWorldsRequest": direct_hoist_60,
    "ListWorldsResult": direct_hoist_62,
    "ReadWorldMailRequest": direct_hoist_64,
    "ReadWorldMailResult": direct_hoist_66,
    "Request": direct_hoist_69,
    "Response": direct_hoist_70,
    "Result": direct_hoist_73,
    "ResumeCodexRequest": direct_hoist_75,
    "SendCodexMessageRequest": direct_hoist_77,
    "SendCodexMessageResult": direct_hoist_79,
    "SshAccess": direct_hoist_81,
    "StartCodexRequest": direct_hoist_83,
    "StartCodexResult": direct_hoist_84,
    "SteerCodexRequest": direct_hoist_86,
    "SuccessResponse": direct_hoist_89,
    "UInt16": direct_hoist_90,
    "UInt32": direct_hoist_91,
    "UInt64": direct_hoist_92,
    "Uuid": direct_hoist_93,
    "World": direct_hoist_95,
    "WorldMail": direct_hoist_97
};
const buildParsersInput = {
    "ExecWorldRequest": direct_hoist_0,
    "ExecWorldResult": direct_hoist_1,
    "Request": direct_hoist_2,
    "Response": direct_hoist_3,
    "CreateWorldRequest": direct_hoist_4,
    "DeleteWorldRequest": direct_hoist_5,
    "StartCodexRequest": direct_hoist_6,
    "InspectCodexRequest": direct_hoist_7,
    "ResumeCodexRequest": direct_hoist_8,
    "SendCodexMessageRequest": direct_hoist_9,
    "SteerCodexRequest": direct_hoist_10,
    "InterruptCodexRequest": direct_hoist_11,
    "ReadWorldMailRequest": direct_hoist_12,
    "SshAccess": direct_hoist_13,
    "World": direct_hoist_14,
    "WorldMail": direct_hoist_15,
    "CreateWorldResult": direct_hoist_16,
    "DeleteWorldResult": direct_hoist_17,
    "StartCodexResult": direct_hoist_18,
    "InspectCodexResult": direct_hoist_19,
    "SendCodexMessageResult": direct_hoist_20,
    "ReadWorldMailResult": direct_hoist_21,
    "CapacityDetails": direct_hoist_22,
    "Error": direct_hoist_23,
    "SuccessResponse": direct_hoist_24,
    "ErrorResponse": direct_hoist_25
};

export default { buildParsers };