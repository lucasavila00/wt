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
const direct_hoist_6 = new RefRuntype(undefined, "SendCodexMessageRequest");
const direct_hoist_7 = new RefRuntype(undefined, "ReadWorldMailRequest");
const direct_hoist_8 = new RefRuntype(undefined, "SshAccess");
const direct_hoist_9 = new RefRuntype(undefined, "World");
const direct_hoist_10 = new RefRuntype(undefined, "WorldMail");
const direct_hoist_11 = new RefRuntype(undefined, "CreateWorldResult");
const direct_hoist_12 = new RefRuntype(undefined, "DeleteWorldResult");
const direct_hoist_13 = new RefRuntype(undefined, "StartCodexResult");
const direct_hoist_14 = new RefRuntype(undefined, "InspectCodexResult");
const direct_hoist_15 = new RefRuntype(undefined, "SendCodexMessageResult");
const direct_hoist_16 = new RefRuntype(undefined, "ReadWorldMailResult");
const direct_hoist_17 = new RefRuntype(undefined, "CapacityDetails");
const direct_hoist_18 = new RefRuntype(undefined, "Error");
const direct_hoist_19 = new RefRuntype(undefined, "SuccessResponse");
const direct_hoist_20 = new RefRuntype(undefined, "ErrorResponse");
const direct_hoist_21 = new ConstRuntype(undefined, "capacity");
const direct_hoist_22 = new RefRuntype(undefined, "UInt64");
const direct_hoist_23 = new AnyOfConstsRuntype(undefined, [
    "cpu",
    "disk",
    "memory"
]);
const direct_hoist_24 = new ObjectRuntype(undefined, {
    "kind": direct_hoist_21,
    "requested": direct_hoist_22,
    "reserved": direct_hoist_22,
    "resource": direct_hoist_23,
    "total": direct_hoist_22
}, []);
const direct_hoist_25 = new ConstRuntype(undefined, 1);
const direct_hoist_26 = new TypeofRuntype(undefined, "string");
const direct_hoist_27 = new RefRuntype(undefined, "Uuid");
const direct_hoist_28 = new ConstRuntype(undefined, "create_world");
const direct_hoist_29 = new RefRuntype(undefined, "UInt32");
const direct_hoist_30 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_25,
    "context": direct_hoist_26,
    "disk_gib": direct_hoist_22,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_27),
    "git_user_email": direct_hoist_26,
    "git_user_name": direct_hoist_26,
    "memory_mib": direct_hoist_22,
    "name": direct_hoist_26,
    "operation": direct_hoist_28,
    "request_id": direct_hoist_27,
    "vcpus": direct_hoist_29
}, []);
const direct_hoist_31 = new ObjectRuntype(undefined, {
    "world": direct_hoist_9
}, []);
const direct_hoist_32 = new ConstRuntype(undefined, "delete_world");
const direct_hoist_33 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_25,
    "context": direct_hoist_26,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_27),
    "operation": direct_hoist_32,
    "request_id": direct_hoist_27,
    "world_id": direct_hoist_27
}, []);
const direct_hoist_34 = new ObjectRuntype(undefined, {
    "world_id": direct_hoist_27
}, []);
const direct_hoist_35 = new TypeofRuntype(undefined, "boolean");
const direct_hoist_36 = new ObjectRuntype(undefined, {
    "code": direct_hoist_26,
    "details": new OptionalFieldRuntype(direct_hoist_17),
    "message": direct_hoist_26,
    "retryable": direct_hoist_35
}, []);
const direct_hoist_37 = new RefRuntype(undefined, "Int64");
const direct_hoist_38 = new ConstRuntype(undefined, "error");
const direct_hoist_39 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_25,
    "error": direct_hoist_18,
    "expires_at_unix_ms": new OptionalFieldRuntype(direct_hoist_37),
    "outcome": direct_hoist_38,
    "request_id": new OptionalFieldRuntype(direct_hoist_27),
    "server_id": new OptionalFieldRuntype(direct_hoist_27)
}, []);
const direct_hoist_40 = new ConstRuntype(undefined, "inspect_codex");
const direct_hoist_41 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_25,
    "context": direct_hoist_26,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_27),
    "operation": direct_hoist_40,
    "request_id": direct_hoist_27,
    "thread_id": direct_hoist_26,
    "world_id": direct_hoist_27
}, []);
const direct_hoist_42 = new AnyOfConstsRuntype(undefined, [
    "active",
    "error",
    "idle"
]);
const direct_hoist_43 = new ObjectRuntype(undefined, {
    "active_turn_id": new OptionalFieldRuntype(direct_hoist_26),
    "observed_at_unix_ms": direct_hoist_37,
    "pane_id": direct_hoist_26,
    "screen": direct_hoist_26,
    "status": direct_hoist_42,
    "thread_id": direct_hoist_26,
    "window_name": direct_hoist_26
}, []);
const direct_hoist_44 = new NumberWithFormatRuntype(undefined, [
    "Int64"
]);
const direct_hoist_45 = new ConstRuntype(undefined, "read_world_mail");
const direct_hoist_46 = new ObjectRuntype(undefined, {
    "after_message_id": direct_hoist_22,
    "api_version": direct_hoist_25,
    "context": direct_hoist_26,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_27),
    "limit": direct_hoist_29,
    "operation": direct_hoist_45,
    "request_id": direct_hoist_27,
    "world_id": direct_hoist_27
}, []);
const direct_hoist_47 = new ArrayRuntype(undefined, direct_hoist_10);
const direct_hoist_48 = new ObjectRuntype(undefined, {
    "high_water_message_id": direct_hoist_22,
    "messages": direct_hoist_47
}, []);
const direct_hoist_49 = new AnyOfDiscriminatedRuntype(undefined, [
    direct_hoist_7,
    direct_hoist_2,
    direct_hoist_6,
    direct_hoist_4,
    direct_hoist_3,
    direct_hoist_5
], "operation", {
    "create_world": direct_hoist_2,
    "delete_world": direct_hoist_3,
    "inspect_codex": direct_hoist_5,
    "read_world_mail": direct_hoist_7,
    "send_codex_message": direct_hoist_6,
    "start_codex": direct_hoist_4
}, {
    "create_world": direct_hoist_2,
    "delete_world": direct_hoist_3,
    "inspect_codex": direct_hoist_5,
    "read_world_mail": direct_hoist_7,
    "send_codex_message": direct_hoist_6,
    "start_codex": direct_hoist_4
});
const direct_hoist_50 = new AnyOfDiscriminatedRuntype(undefined, [
    direct_hoist_20,
    direct_hoist_19
], "outcome", {
    "error": direct_hoist_20,
    "ok": direct_hoist_19
}, {
    "error": direct_hoist_20,
    "ok": direct_hoist_19
});
const direct_hoist_51 = new AnyOfRuntype(undefined, [
    direct_hoist_11,
    direct_hoist_12,
    direct_hoist_14,
    direct_hoist_16,
    direct_hoist_15,
    direct_hoist_13
]);
const direct_hoist_52 = new ConstRuntype(undefined, "send_codex_message");
const direct_hoist_53 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_25,
    "context": direct_hoist_26,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_27),
    "message": direct_hoist_26,
    "operation": direct_hoist_52,
    "request_id": direct_hoist_27,
    "thread_id": direct_hoist_26,
    "world_id": direct_hoist_27
}, []);
const direct_hoist_54 = new AnyOfConstsRuntype(undefined, [
    "started",
    "steered"
]);
const direct_hoist_55 = new ObjectRuntype(undefined, {
    "delivery": direct_hoist_54,
    "thread_id": direct_hoist_26,
    "turn_id": direct_hoist_26
}, []);
const direct_hoist_56 = new ArrayRuntype(undefined, direct_hoist_26);
const direct_hoist_57 = new RefRuntype(undefined, "UInt16");
const direct_hoist_58 = new ObjectRuntype(undefined, {
    "host": direct_hoist_26,
    "host_keys": direct_hoist_56,
    "port": direct_hoist_57,
    "user": direct_hoist_26
}, []);
const direct_hoist_59 = new ConstRuntype(undefined, "start_codex");
const direct_hoist_60 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_25,
    "context": direct_hoist_26,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_27),
    "message": direct_hoist_26,
    "operation": direct_hoist_59,
    "request_id": direct_hoist_27,
    "world_id": direct_hoist_27
}, []);
const direct_hoist_61 = new ObjectRuntype(undefined, {
    "pane_id": direct_hoist_26,
    "thread_id": direct_hoist_26,
    "turn_id": direct_hoist_26,
    "window_name": direct_hoist_26
}, []);
const direct_hoist_62 = new ConstRuntype(undefined, "ok");
const direct_hoist_63 = new RefRuntype(undefined, "Result");
const direct_hoist_64 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_25,
    "expires_at_unix_ms": new OptionalFieldRuntype(direct_hoist_37),
    "outcome": direct_hoist_62,
    "request_id": direct_hoist_27,
    "result": direct_hoist_63,
    "server_id": direct_hoist_27
}, []);
const direct_hoist_65 = new NumberWithFormatRuntype(undefined, [
    "UInt16"
]);
const direct_hoist_66 = new NumberWithFormatRuntype(undefined, [
    "UInt32"
]);
const direct_hoist_67 = new NumberWithFormatRuntype(undefined, [
    "UInt64"
]);
const direct_hoist_68 = new StringWithFormatRuntype(undefined, [
    "Uuid"
]);
const direct_hoist_69 = new AnyOfConstsRuntype(undefined, [
    "destroying",
    "error",
    "provisioning",
    "running",
    "stopped"
]);
const direct_hoist_70 = new ObjectRuntype(undefined, {
    "disk_gib": direct_hoist_22,
    "guest_ip": new OptionalFieldRuntype(direct_hoist_26),
    "last_error": new OptionalFieldRuntype(direct_hoist_26),
    "memory_mib": direct_hoist_22,
    "name": direct_hoist_26,
    "ssh": new OptionalFieldRuntype(direct_hoist_8),
    "status": direct_hoist_69,
    "vcpus": direct_hoist_29,
    "world_id": direct_hoist_27
}, []);
const direct_hoist_71 = new AnyOfConstsRuntype(undefined, [
    "completed",
    "failed",
    "message"
]);
const direct_hoist_72 = new ObjectRuntype(undefined, {
    "created_at_unix_ms": direct_hoist_37,
    "kind": direct_hoist_71,
    "message_id": direct_hoist_22,
    "pane_id": new OptionalFieldRuntype(direct_hoist_26),
    "text": direct_hoist_26,
    "thread_id": new OptionalFieldRuntype(direct_hoist_26),
    "turn_id": new OptionalFieldRuntype(direct_hoist_26),
    "world_id": direct_hoist_27
}, []);
const namedRuntypes = {
    "CapacityDetails": direct_hoist_24,
    "CreateWorldRequest": direct_hoist_30,
    "CreateWorldResult": direct_hoist_31,
    "DeleteWorldRequest": direct_hoist_33,
    "DeleteWorldResult": direct_hoist_34,
    "Error": direct_hoist_36,
    "ErrorResponse": direct_hoist_39,
    "InspectCodexRequest": direct_hoist_41,
    "InspectCodexResult": direct_hoist_43,
    "Int64": direct_hoist_44,
    "ReadWorldMailRequest": direct_hoist_46,
    "ReadWorldMailResult": direct_hoist_48,
    "Request": direct_hoist_49,
    "Response": direct_hoist_50,
    "Result": direct_hoist_51,
    "SendCodexMessageRequest": direct_hoist_53,
    "SendCodexMessageResult": direct_hoist_55,
    "SshAccess": direct_hoist_58,
    "StartCodexRequest": direct_hoist_60,
    "StartCodexResult": direct_hoist_61,
    "SuccessResponse": direct_hoist_64,
    "UInt16": direct_hoist_65,
    "UInt32": direct_hoist_66,
    "UInt64": direct_hoist_67,
    "Uuid": direct_hoist_68,
    "World": direct_hoist_70,
    "WorldMail": direct_hoist_72
};
const buildParsersInput = {
    "Request": direct_hoist_0,
    "Response": direct_hoist_1,
    "CreateWorldRequest": direct_hoist_2,
    "DeleteWorldRequest": direct_hoist_3,
    "StartCodexRequest": direct_hoist_4,
    "InspectCodexRequest": direct_hoist_5,
    "SendCodexMessageRequest": direct_hoist_6,
    "ReadWorldMailRequest": direct_hoist_7,
    "SshAccess": direct_hoist_8,
    "World": direct_hoist_9,
    "WorldMail": direct_hoist_10,
    "CreateWorldResult": direct_hoist_11,
    "DeleteWorldResult": direct_hoist_12,
    "StartCodexResult": direct_hoist_13,
    "InspectCodexResult": direct_hoist_14,
    "SendCodexMessageResult": direct_hoist_15,
    "ReadWorldMailResult": direct_hoist_16,
    "CapacityDetails": direct_hoist_17,
    "Error": direct_hoist_18,
    "SuccessResponse": direct_hoist_19,
    "ErrorResponse": direct_hoist_20
};

export default { buildParsers };