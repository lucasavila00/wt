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
const RequiredNumberFormats = ["Integer","UnsignedInteger"];
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
const direct_hoist_19 = new ConstRuntype(undefined, "capacity");
const direct_hoist_20 = new RefRuntype(undefined, "UnsignedInteger");
const direct_hoist_21 = new AnyOfConstsRuntype(undefined, [
    "cpu",
    "disk",
    "memory"
]);
const direct_hoist_22 = new ObjectRuntype(undefined, {
    "kind": direct_hoist_19,
    "requested": direct_hoist_20,
    "reserved": direct_hoist_20,
    "resource": direct_hoist_21,
    "total": direct_hoist_20
}, []);
const direct_hoist_23 = new ConstRuntype(undefined, 1);
const direct_hoist_24 = new TypeofRuntype(undefined, "string");
const direct_hoist_25 = new RefRuntype(undefined, "Uuid");
const direct_hoist_26 = new ConstRuntype(undefined, "create_world");
const direct_hoist_27 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_23,
    "context": direct_hoist_24,
    "disk_gib": direct_hoist_20,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_25),
    "git_user_email": direct_hoist_24,
    "git_user_name": direct_hoist_24,
    "memory_mib": direct_hoist_20,
    "name": direct_hoist_24,
    "operation": direct_hoist_26,
    "request_id": direct_hoist_25,
    "vcpus": direct_hoist_20
}, []);
const direct_hoist_28 = new ObjectRuntype(undefined, {
    "world": direct_hoist_9
}, []);
const direct_hoist_29 = new ConstRuntype(undefined, "delete_world");
const direct_hoist_30 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_23,
    "context": direct_hoist_24,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_25),
    "operation": direct_hoist_29,
    "request_id": direct_hoist_25,
    "world_id": direct_hoist_25
}, []);
const direct_hoist_31 = new ObjectRuntype(undefined, {
    "world_id": direct_hoist_25
}, []);
const direct_hoist_32 = new TypeofRuntype(undefined, "boolean");
const direct_hoist_33 = new ObjectRuntype(undefined, {
    "code": direct_hoist_24,
    "details": new OptionalFieldRuntype(direct_hoist_17),
    "message": direct_hoist_24,
    "retryable": direct_hoist_32
}, []);
const direct_hoist_34 = new ConstRuntype(undefined, "inspect_codex");
const direct_hoist_35 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_23,
    "context": direct_hoist_24,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_25),
    "operation": direct_hoist_34,
    "request_id": direct_hoist_25,
    "thread_id": direct_hoist_24,
    "world_id": direct_hoist_25
}, []);
const direct_hoist_36 = new RefRuntype(undefined, "Integer");
const direct_hoist_37 = new AnyOfConstsRuntype(undefined, [
    "active",
    "error",
    "idle"
]);
const direct_hoist_38 = new ObjectRuntype(undefined, {
    "active_turn_id": new OptionalFieldRuntype(direct_hoist_24),
    "observed_at_unix_ms": direct_hoist_36,
    "pane_id": direct_hoist_24,
    "screen": direct_hoist_24,
    "status": direct_hoist_37,
    "thread_id": direct_hoist_24,
    "window_name": direct_hoist_24
}, []);
const direct_hoist_39 = new NumberWithFormatRuntype(undefined, [
    "Integer"
]);
const direct_hoist_40 = new ConstRuntype(undefined, "read_world_mail");
const direct_hoist_41 = new ObjectRuntype(undefined, {
    "after_message_id": direct_hoist_20,
    "api_version": direct_hoist_23,
    "context": direct_hoist_24,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_25),
    "limit": direct_hoist_20,
    "operation": direct_hoist_40,
    "request_id": direct_hoist_25,
    "world_id": direct_hoist_25
}, []);
const direct_hoist_42 = new ArrayRuntype(undefined, direct_hoist_10);
const direct_hoist_43 = new ObjectRuntype(undefined, {
    "high_water_message_id": direct_hoist_20,
    "messages": direct_hoist_42
}, []);
const direct_hoist_44 = new AnyOfDiscriminatedRuntype(undefined, [
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
const direct_hoist_45 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_23,
    "expires_at_unix_ms": new OptionalFieldRuntype(direct_hoist_36),
    "request_id": new OptionalFieldRuntype(direct_hoist_25),
    "server_id": new OptionalFieldRuntype(direct_hoist_25)
}, []);
const direct_hoist_46 = new ConstRuntype(undefined, "error");
const direct_hoist_47 = new ObjectRuntype(undefined, {
    "error": direct_hoist_18,
    "outcome": direct_hoist_46
}, []);
const direct_hoist_48 = new ConstRuntype(undefined, "ok");
const direct_hoist_49 = new RefRuntype(undefined, "Result");
const direct_hoist_50 = new ObjectRuntype(undefined, {
    "outcome": direct_hoist_48,
    "result": direct_hoist_49
}, []);
const direct_hoist_51 = new AnyOfDiscriminatedRuntype(undefined, [
    direct_hoist_47,
    direct_hoist_50
], "outcome", {
    "error": direct_hoist_47,
    "ok": direct_hoist_50
}, {
    "error": direct_hoist_47,
    "ok": direct_hoist_50
});
const direct_hoist_52 = new AllOfRuntype(undefined, [
    direct_hoist_45,
    direct_hoist_51
]);
const direct_hoist_53 = new AnyOfRuntype(undefined, [
    direct_hoist_11,
    direct_hoist_12,
    direct_hoist_14,
    direct_hoist_16,
    direct_hoist_15,
    direct_hoist_13
]);
const direct_hoist_54 = new ConstRuntype(undefined, "send_codex_message");
const direct_hoist_55 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_23,
    "context": direct_hoist_24,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_25),
    "message": direct_hoist_24,
    "operation": direct_hoist_54,
    "request_id": direct_hoist_25,
    "thread_id": direct_hoist_24,
    "world_id": direct_hoist_25
}, []);
const direct_hoist_56 = new AnyOfConstsRuntype(undefined, [
    "started",
    "steered"
]);
const direct_hoist_57 = new ObjectRuntype(undefined, {
    "delivery": direct_hoist_56,
    "thread_id": direct_hoist_24,
    "turn_id": direct_hoist_24
}, []);
const direct_hoist_58 = new ArrayRuntype(undefined, direct_hoist_24);
const direct_hoist_59 = new ObjectRuntype(undefined, {
    "host": direct_hoist_24,
    "host_keys": direct_hoist_58,
    "port": direct_hoist_20,
    "user": direct_hoist_24
}, []);
const direct_hoist_60 = new ConstRuntype(undefined, "start_codex");
const direct_hoist_61 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_23,
    "context": direct_hoist_24,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_25),
    "message": direct_hoist_24,
    "operation": direct_hoist_60,
    "request_id": direct_hoist_25,
    "world_id": direct_hoist_25
}, []);
const direct_hoist_62 = new ObjectRuntype(undefined, {
    "pane_id": direct_hoist_24,
    "thread_id": direct_hoist_24,
    "turn_id": direct_hoist_24,
    "window_name": direct_hoist_24
}, []);
const direct_hoist_63 = new NumberWithFormatRuntype(undefined, [
    "UnsignedInteger"
]);
const direct_hoist_64 = new StringWithFormatRuntype(undefined, [
    "Uuid"
]);
const direct_hoist_65 = new AnyOfConstsRuntype(undefined, [
    "destroying",
    "error",
    "provisioning",
    "running",
    "stopped"
]);
const direct_hoist_66 = new ObjectRuntype(undefined, {
    "disk_gib": direct_hoist_20,
    "guest_ip": new OptionalFieldRuntype(direct_hoist_24),
    "last_error": new OptionalFieldRuntype(direct_hoist_24),
    "memory_mib": direct_hoist_20,
    "name": direct_hoist_24,
    "ssh": new OptionalFieldRuntype(direct_hoist_8),
    "status": direct_hoist_65,
    "vcpus": direct_hoist_20,
    "world_id": direct_hoist_25
}, []);
const direct_hoist_67 = new AnyOfConstsRuntype(undefined, [
    "completed",
    "failed",
    "message"
]);
const direct_hoist_68 = new ObjectRuntype(undefined, {
    "created_at_unix_ms": direct_hoist_36,
    "kind": direct_hoist_67,
    "message_id": direct_hoist_20,
    "pane_id": new OptionalFieldRuntype(direct_hoist_24),
    "text": direct_hoist_24,
    "thread_id": new OptionalFieldRuntype(direct_hoist_24),
    "turn_id": new OptionalFieldRuntype(direct_hoist_24),
    "world_id": direct_hoist_25
}, []);
const namedRuntypes = {
    "CapacityDetails": direct_hoist_22,
    "CreateWorldRequest": direct_hoist_27,
    "CreateWorldResult": direct_hoist_28,
    "DeleteWorldRequest": direct_hoist_30,
    "DeleteWorldResult": direct_hoist_31,
    "Error": direct_hoist_33,
    "InspectCodexRequest": direct_hoist_35,
    "InspectCodexResult": direct_hoist_38,
    "Integer": direct_hoist_39,
    "ReadWorldMailRequest": direct_hoist_41,
    "ReadWorldMailResult": direct_hoist_43,
    "Request": direct_hoist_44,
    "Response": direct_hoist_52,
    "Result": direct_hoist_53,
    "SendCodexMessageRequest": direct_hoist_55,
    "SendCodexMessageResult": direct_hoist_57,
    "SshAccess": direct_hoist_59,
    "StartCodexRequest": direct_hoist_61,
    "StartCodexResult": direct_hoist_62,
    "UnsignedInteger": direct_hoist_63,
    "Uuid": direct_hoist_64,
    "World": direct_hoist_66,
    "WorldMail": direct_hoist_68
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
    "Error": direct_hoist_18
};

export default { buildParsers };