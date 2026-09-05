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
const direct_hoist_0 = new RefRuntype(undefined, "Request");
const direct_hoist_1 = new RefRuntype(undefined, "Response");
const direct_hoist_2 = new RefRuntype(undefined, "CreateWorldRequest");
const direct_hoist_3 = new RefRuntype(undefined, "DeleteWorldRequest");
const direct_hoist_4 = new RefRuntype(undefined, "StartCodexRequest");
const direct_hoist_5 = new RefRuntype(undefined, "InspectCodexRequest");
const direct_hoist_6 = new RefRuntype(undefined, "ResumeCodexRequest");
const direct_hoist_7 = new RefRuntype(undefined, "SendCodexMessageRequest");
const direct_hoist_8 = new RefRuntype(undefined, "SteerCodexRequest");
const direct_hoist_9 = new RefRuntype(undefined, "InterruptCodexRequest");
const direct_hoist_10 = new RefRuntype(undefined, "ReadWorldMailRequest");
const direct_hoist_11 = new RefRuntype(undefined, "SshAccess");
const direct_hoist_12 = new RefRuntype(undefined, "World");
const direct_hoist_13 = new RefRuntype(undefined, "WorldMail");
const direct_hoist_14 = new RefRuntype(undefined, "CreateWorldResult");
const direct_hoist_15 = new RefRuntype(undefined, "DeleteWorldResult");
const direct_hoist_16 = new RefRuntype(undefined, "StartCodexResult");
const direct_hoist_17 = new RefRuntype(undefined, "InspectCodexResult");
const direct_hoist_18 = new RefRuntype(undefined, "SendCodexMessageResult");
const direct_hoist_19 = new RefRuntype(undefined, "ReadWorldMailResult");
const direct_hoist_20 = new RefRuntype(undefined, "CapacityDetails");
const direct_hoist_21 = new RefRuntype(undefined, "Error");
const direct_hoist_22 = new RefRuntype(undefined, "SuccessResponse");
const direct_hoist_23 = new RefRuntype(undefined, "ErrorResponse");
const direct_hoist_24 = new ConstRuntype(undefined, "capacity");
const direct_hoist_25 = new RefRuntype(undefined, "UInt64");
const direct_hoist_26 = new AnyOfConstsRuntype(undefined, [
    "cpu",
    "disk",
    "memory"
]);
const direct_hoist_27 = new ObjectRuntype(undefined, {
    "kind": direct_hoist_24,
    "requested": direct_hoist_25,
    "reserved": direct_hoist_25,
    "resource": direct_hoist_26,
    "total": direct_hoist_25
}, []);
const direct_hoist_28 = new ConstRuntype(undefined, 1);
const direct_hoist_29 = new TypeofRuntype(undefined, "string");
const direct_hoist_30 = new RefRuntype(undefined, "Uuid");
const direct_hoist_31 = new ConstRuntype(undefined, "create_world");
const direct_hoist_32 = new RefRuntype(undefined, "UInt32");
const direct_hoist_33 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_28,
    "context": direct_hoist_29,
    "disk_gib": direct_hoist_25,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_30),
    "git_user_email": direct_hoist_29,
    "git_user_name": direct_hoist_29,
    "memory_mib": direct_hoist_25,
    "name": direct_hoist_29,
    "operation": direct_hoist_31,
    "request_id": direct_hoist_30,
    "vcpus": direct_hoist_32
}, []);
const direct_hoist_34 = new ObjectRuntype(undefined, {
    "world": direct_hoist_12
}, []);
const direct_hoist_35 = new ConstRuntype(undefined, "delete_world");
const direct_hoist_36 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_28,
    "context": direct_hoist_29,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_30),
    "operation": direct_hoist_35,
    "request_id": direct_hoist_30,
    "world_id": direct_hoist_30
}, []);
const direct_hoist_37 = new ObjectRuntype(undefined, {
    "world_id": direct_hoist_30
}, []);
const direct_hoist_38 = new TypeofRuntype(undefined, "boolean");
const direct_hoist_39 = new ObjectRuntype(undefined, {
    "code": direct_hoist_29,
    "details": new OptionalFieldRuntype(direct_hoist_20),
    "message": direct_hoist_29,
    "retryable": direct_hoist_38
}, []);
const direct_hoist_40 = new RefRuntype(undefined, "Int64");
const direct_hoist_41 = new ConstRuntype(undefined, "error");
const direct_hoist_42 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_28,
    "error": direct_hoist_21,
    "expires_at_unix_ms": new OptionalFieldRuntype(direct_hoist_40),
    "outcome": direct_hoist_41,
    "request_id": new OptionalFieldRuntype(direct_hoist_30),
    "server_id": new OptionalFieldRuntype(direct_hoist_30)
}, []);
const direct_hoist_43 = new ConstRuntype(undefined, "inspect_codex");
const direct_hoist_44 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_28,
    "context": direct_hoist_29,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_30),
    "operation": direct_hoist_43,
    "request_id": direct_hoist_30,
    "thread_id": direct_hoist_29,
    "world_id": direct_hoist_30
}, []);
const direct_hoist_45 = new AnyOfConstsRuntype(undefined, [
    "active",
    "error",
    "idle"
]);
const direct_hoist_46 = new ObjectRuntype(undefined, {
    "active_turn_id": new OptionalFieldRuntype(direct_hoist_29),
    "observed_at_unix_ms": direct_hoist_40,
    "pane_id": new OptionalFieldRuntype(direct_hoist_29),
    "screen": new OptionalFieldRuntype(direct_hoist_29),
    "status": direct_hoist_45,
    "thread_id": direct_hoist_29,
    "window_name": new OptionalFieldRuntype(direct_hoist_29)
}, []);
const direct_hoist_47 = new NumberWithFormatRuntype(undefined, [
    "Int64"
]);
const direct_hoist_48 = new ConstRuntype(undefined, "interrupt_codex");
const direct_hoist_49 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_28,
    "context": direct_hoist_29,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_30),
    "operation": direct_hoist_48,
    "request_id": direct_hoist_30,
    "thread_id": direct_hoist_29,
    "turn_id": direct_hoist_29,
    "world_id": direct_hoist_30
}, []);
const direct_hoist_50 = new ConstRuntype(undefined, "list_contexts");
const direct_hoist_51 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_28,
    "operation": direct_hoist_50,
    "request_id": direct_hoist_30
}, []);
const direct_hoist_52 = new ArrayRuntype(undefined, direct_hoist_29);
const direct_hoist_53 = new ObjectRuntype(undefined, {
    "contexts": direct_hoist_52
}, []);
const direct_hoist_54 = new ConstRuntype(undefined, "list_worlds");
const direct_hoist_55 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_28,
    "context": direct_hoist_29,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_30),
    "operation": direct_hoist_54,
    "request_id": direct_hoist_30
}, []);
const direct_hoist_56 = new ArrayRuntype(undefined, direct_hoist_12);
const direct_hoist_57 = new ObjectRuntype(undefined, {
    "worlds": direct_hoist_56
}, []);
const direct_hoist_58 = new ConstRuntype(undefined, "read_world_mail");
const direct_hoist_59 = new ObjectRuntype(undefined, {
    "after_message_id": direct_hoist_25,
    "api_version": direct_hoist_28,
    "context": direct_hoist_29,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_30),
    "limit": direct_hoist_32,
    "operation": direct_hoist_58,
    "request_id": direct_hoist_30,
    "world_id": direct_hoist_30
}, []);
const direct_hoist_60 = new ArrayRuntype(undefined, direct_hoist_13);
const direct_hoist_61 = new ObjectRuntype(undefined, {
    "high_water_message_id": direct_hoist_25,
    "messages": direct_hoist_60
}, []);
const direct_hoist_62 = new RefRuntype(undefined, "ListContextsRequest");
const direct_hoist_63 = new RefRuntype(undefined, "ListWorldsRequest");
const direct_hoist_64 = new AnyOfDiscriminatedRuntype(undefined, [
    direct_hoist_10,
    direct_hoist_2,
    direct_hoist_7,
    direct_hoist_4,
    direct_hoist_8,
    direct_hoist_3,
    direct_hoist_5,
    direct_hoist_9,
    direct_hoist_63,
    direct_hoist_6,
    direct_hoist_62
], "operation", {
    "create_world": direct_hoist_2,
    "delete_world": direct_hoist_3,
    "inspect_codex": direct_hoist_5,
    "interrupt_codex": direct_hoist_9,
    "list_contexts": direct_hoist_62,
    "list_worlds": direct_hoist_63,
    "read_world_mail": direct_hoist_10,
    "resume_codex": direct_hoist_6,
    "send_codex_message": direct_hoist_7,
    "start_codex": direct_hoist_4,
    "steer_codex": direct_hoist_8
}, {
    "create_world": direct_hoist_2,
    "delete_world": direct_hoist_3,
    "inspect_codex": direct_hoist_5,
    "interrupt_codex": direct_hoist_9,
    "list_contexts": direct_hoist_62,
    "list_worlds": direct_hoist_63,
    "read_world_mail": direct_hoist_10,
    "resume_codex": direct_hoist_6,
    "send_codex_message": direct_hoist_7,
    "start_codex": direct_hoist_4,
    "steer_codex": direct_hoist_8
});
const direct_hoist_65 = new AnyOfDiscriminatedRuntype(undefined, [
    direct_hoist_23,
    direct_hoist_22
], "outcome", {
    "error": direct_hoist_23,
    "ok": direct_hoist_22
}, {
    "error": direct_hoist_23,
    "ok": direct_hoist_22
});
const direct_hoist_66 = new RefRuntype(undefined, "ListContextsResult");
const direct_hoist_67 = new RefRuntype(undefined, "ListWorldsResult");
const direct_hoist_68 = new AnyOfRuntype(undefined, [
    direct_hoist_14,
    direct_hoist_15,
    direct_hoist_17,
    direct_hoist_66,
    direct_hoist_67,
    direct_hoist_19,
    direct_hoist_18,
    direct_hoist_16
]);
const direct_hoist_69 = new ConstRuntype(undefined, "resume_codex");
const direct_hoist_70 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_28,
    "context": direct_hoist_29,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_30),
    "operation": direct_hoist_69,
    "request_id": direct_hoist_30,
    "thread_id": direct_hoist_29,
    "world_id": direct_hoist_30
}, []);
const direct_hoist_71 = new ConstRuntype(undefined, "send_codex_message");
const direct_hoist_72 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_28,
    "context": direct_hoist_29,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_30),
    "message": direct_hoist_29,
    "operation": direct_hoist_71,
    "request_id": direct_hoist_30,
    "thread_id": direct_hoist_29,
    "world_id": direct_hoist_30
}, []);
const direct_hoist_73 = new AnyOfConstsRuntype(undefined, [
    "interrupt_requested",
    "started",
    "steered"
]);
const direct_hoist_74 = new ObjectRuntype(undefined, {
    "delivery": direct_hoist_73,
    "thread_id": direct_hoist_29,
    "turn_id": direct_hoist_29
}, []);
const direct_hoist_75 = new RefRuntype(undefined, "UInt16");
const direct_hoist_76 = new ObjectRuntype(undefined, {
    "host": direct_hoist_29,
    "host_keys": direct_hoist_52,
    "port": direct_hoist_75,
    "user": direct_hoist_29
}, []);
const direct_hoist_77 = new ConstRuntype(undefined, "start_codex");
const direct_hoist_78 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_28,
    "context": direct_hoist_29,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_30),
    "message": direct_hoist_29,
    "operation": direct_hoist_77,
    "request_id": direct_hoist_30,
    "world_id": direct_hoist_30
}, []);
const direct_hoist_79 = new ObjectRuntype(undefined, {
    "pane_id": new OptionalFieldRuntype(direct_hoist_29),
    "thread_id": direct_hoist_29,
    "turn_id": direct_hoist_29,
    "window_name": new OptionalFieldRuntype(direct_hoist_29)
}, []);
const direct_hoist_80 = new ConstRuntype(undefined, "steer_codex");
const direct_hoist_81 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_28,
    "context": direct_hoist_29,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_30),
    "message": direct_hoist_29,
    "operation": direct_hoist_80,
    "request_id": direct_hoist_30,
    "thread_id": direct_hoist_29,
    "turn_id": direct_hoist_29,
    "world_id": direct_hoist_30
}, []);
const direct_hoist_82 = new ConstRuntype(undefined, "ok");
const direct_hoist_83 = new RefRuntype(undefined, "Result");
const direct_hoist_84 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_28,
    "expires_at_unix_ms": new OptionalFieldRuntype(direct_hoist_40),
    "outcome": direct_hoist_82,
    "request_id": direct_hoist_30,
    "result": direct_hoist_83,
    "server_id": new OptionalFieldRuntype(direct_hoist_30)
}, []);
const direct_hoist_85 = new NumberWithFormatRuntype(undefined, [
    "UInt16"
]);
const direct_hoist_86 = new NumberWithFormatRuntype(undefined, [
    "UInt32"
]);
const direct_hoist_87 = new NumberWithFormatRuntype(undefined, [
    "UInt64"
]);
const direct_hoist_88 = new StringWithFormatRuntype(undefined, [
    "Uuid"
]);
const direct_hoist_89 = new AnyOfConstsRuntype(undefined, [
    "destroying",
    "error",
    "provisioning",
    "running",
    "stopped"
]);
const direct_hoist_90 = new ObjectRuntype(undefined, {
    "disk_gib": direct_hoist_25,
    "guest_ip": new OptionalFieldRuntype(direct_hoist_29),
    "last_error": new OptionalFieldRuntype(direct_hoist_29),
    "memory_mib": direct_hoist_25,
    "name": direct_hoist_29,
    "ssh": new OptionalFieldRuntype(direct_hoist_11),
    "status": direct_hoist_89,
    "vcpus": direct_hoist_32,
    "world_id": direct_hoist_30
}, []);
const direct_hoist_91 = new AnyOfConstsRuntype(undefined, [
    "completed",
    "failed",
    "message"
]);
const direct_hoist_92 = new ObjectRuntype(undefined, {
    "created_at_unix_ms": direct_hoist_40,
    "kind": direct_hoist_91,
    "message_id": direct_hoist_25,
    "pane_id": new OptionalFieldRuntype(direct_hoist_29),
    "text": direct_hoist_29,
    "thread_id": new OptionalFieldRuntype(direct_hoist_29),
    "turn_id": new OptionalFieldRuntype(direct_hoist_29),
    "world_id": direct_hoist_30
}, []);
const namedRuntypes = {
    "CapacityDetails": direct_hoist_27,
    "CreateWorldRequest": direct_hoist_33,
    "CreateWorldResult": direct_hoist_34,
    "DeleteWorldRequest": direct_hoist_36,
    "DeleteWorldResult": direct_hoist_37,
    "Error": direct_hoist_39,
    "ErrorResponse": direct_hoist_42,
    "InspectCodexRequest": direct_hoist_44,
    "InspectCodexResult": direct_hoist_46,
    "Int64": direct_hoist_47,
    "InterruptCodexRequest": direct_hoist_49,
    "ListContextsRequest": direct_hoist_51,
    "ListContextsResult": direct_hoist_53,
    "ListWorldsRequest": direct_hoist_55,
    "ListWorldsResult": direct_hoist_57,
    "ReadWorldMailRequest": direct_hoist_59,
    "ReadWorldMailResult": direct_hoist_61,
    "Request": direct_hoist_64,
    "Response": direct_hoist_65,
    "Result": direct_hoist_68,
    "ResumeCodexRequest": direct_hoist_70,
    "SendCodexMessageRequest": direct_hoist_72,
    "SendCodexMessageResult": direct_hoist_74,
    "SshAccess": direct_hoist_76,
    "StartCodexRequest": direct_hoist_78,
    "StartCodexResult": direct_hoist_79,
    "SteerCodexRequest": direct_hoist_81,
    "SuccessResponse": direct_hoist_84,
    "UInt16": direct_hoist_85,
    "UInt32": direct_hoist_86,
    "UInt64": direct_hoist_87,
    "Uuid": direct_hoist_88,
    "World": direct_hoist_90,
    "WorldMail": direct_hoist_92
};
const buildParsersInput = {
    "Request": direct_hoist_0,
    "Response": direct_hoist_1,
    "CreateWorldRequest": direct_hoist_2,
    "DeleteWorldRequest": direct_hoist_3,
    "StartCodexRequest": direct_hoist_4,
    "InspectCodexRequest": direct_hoist_5,
    "ResumeCodexRequest": direct_hoist_6,
    "SendCodexMessageRequest": direct_hoist_7,
    "SteerCodexRequest": direct_hoist_8,
    "InterruptCodexRequest": direct_hoist_9,
    "ReadWorldMailRequest": direct_hoist_10,
    "SshAccess": direct_hoist_11,
    "World": direct_hoist_12,
    "WorldMail": direct_hoist_13,
    "CreateWorldResult": direct_hoist_14,
    "DeleteWorldResult": direct_hoist_15,
    "StartCodexResult": direct_hoist_16,
    "InspectCodexResult": direct_hoist_17,
    "SendCodexMessageResult": direct_hoist_18,
    "ReadWorldMailResult": direct_hoist_19,
    "CapacityDetails": direct_hoist_20,
    "Error": direct_hoist_21,
    "SuccessResponse": direct_hoist_22,
    "ErrorResponse": direct_hoist_23
};

export default { buildParsers };