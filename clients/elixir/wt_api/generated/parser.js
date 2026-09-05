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
const direct_hoist_6 = new RefRuntype(undefined, "ReadWorldMailRequest");
const direct_hoist_7 = new RefRuntype(undefined, "SshAccess");
const direct_hoist_8 = new RefRuntype(undefined, "World");
const direct_hoist_9 = new RefRuntype(undefined, "WorldMail");
const direct_hoist_10 = new RefRuntype(undefined, "CreateWorldResult");
const direct_hoist_11 = new RefRuntype(undefined, "DeleteWorldResult");
const direct_hoist_12 = new RefRuntype(undefined, "ReadWorldMailResult");
const direct_hoist_13 = new RefRuntype(undefined, "CapacityDetails");
const direct_hoist_14 = new RefRuntype(undefined, "Error");
const direct_hoist_15 = new RefRuntype(undefined, "SuccessResponse");
const direct_hoist_16 = new RefRuntype(undefined, "ErrorResponse");
const direct_hoist_17 = new ConstRuntype(undefined, "capacity");
const direct_hoist_18 = new RefRuntype(undefined, "UInt64");
const direct_hoist_19 = new AnyOfConstsRuntype(undefined, [
    "cpu",
    "disk",
    "memory"
]);
const direct_hoist_20 = new ObjectRuntype(undefined, {
    "kind": direct_hoist_17,
    "requested": direct_hoist_18,
    "reserved": direct_hoist_18,
    "resource": direct_hoist_19,
    "total": direct_hoist_18
}, []);
const direct_hoist_21 = new ConstRuntype(undefined, 1);
const direct_hoist_22 = new TypeofRuntype(undefined, "string");
const direct_hoist_23 = new RefRuntype(undefined, "Uuid");
const direct_hoist_24 = new ConstRuntype(undefined, "create_world");
const direct_hoist_25 = new RefRuntype(undefined, "UInt32");
const direct_hoist_26 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_21,
    "context": direct_hoist_22,
    "disk_gib": direct_hoist_18,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_23),
    "git_user_email": direct_hoist_22,
    "git_user_name": direct_hoist_22,
    "memory_mib": direct_hoist_18,
    "name": direct_hoist_22,
    "operation": direct_hoist_24,
    "request_id": direct_hoist_23,
    "vcpus": direct_hoist_25
}, []);
const direct_hoist_27 = new ObjectRuntype(undefined, {
    "world": direct_hoist_8
}, []);
const direct_hoist_28 = new ConstRuntype(undefined, "delete_world");
const direct_hoist_29 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_21,
    "context": direct_hoist_22,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_23),
    "operation": direct_hoist_28,
    "request_id": direct_hoist_23,
    "world_id": direct_hoist_23
}, []);
const direct_hoist_30 = new ObjectRuntype(undefined, {
    "world_id": direct_hoist_23
}, []);
const direct_hoist_31 = new TypeofRuntype(undefined, "boolean");
const direct_hoist_32 = new ObjectRuntype(undefined, {
    "code": direct_hoist_22,
    "details": new OptionalFieldRuntype(direct_hoist_13),
    "message": direct_hoist_22,
    "retryable": direct_hoist_31
}, []);
const direct_hoist_33 = new RefRuntype(undefined, "Int64");
const direct_hoist_34 = new ConstRuntype(undefined, "error");
const direct_hoist_35 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_21,
    "error": direct_hoist_14,
    "expires_at_unix_ms": new OptionalFieldRuntype(direct_hoist_33),
    "outcome": direct_hoist_34,
    "request_id": new OptionalFieldRuntype(direct_hoist_23),
    "server_id": new OptionalFieldRuntype(direct_hoist_23)
}, []);
const direct_hoist_36 = new ArrayRuntype(undefined, direct_hoist_22);
const direct_hoist_37 = new ConstRuntype(undefined, "exec_world");
const direct_hoist_38 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_21,
    "args": direct_hoist_36,
    "context": direct_hoist_22,
    "executable": direct_hoist_22,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_23),
    "operation": direct_hoist_37,
    "request_id": direct_hoist_23,
    "stdin": direct_hoist_22,
    "world_id": direct_hoist_23
}, []);
const direct_hoist_39 = new ObjectRuntype(undefined, {
    "exit_status": direct_hoist_33,
    "stderr": direct_hoist_22,
    "stdout": direct_hoist_22
}, []);
const direct_hoist_40 = new NumberWithFormatRuntype(undefined, [
    "Int64"
]);
const direct_hoist_41 = new ConstRuntype(undefined, "list_contexts");
const direct_hoist_42 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_21,
    "operation": direct_hoist_41,
    "request_id": direct_hoist_23
}, []);
const direct_hoist_43 = new ObjectRuntype(undefined, {
    "contexts": direct_hoist_36
}, []);
const direct_hoist_44 = new ConstRuntype(undefined, "list_worlds");
const direct_hoist_45 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_21,
    "context": direct_hoist_22,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_23),
    "operation": direct_hoist_44,
    "request_id": direct_hoist_23
}, []);
const direct_hoist_46 = new ArrayRuntype(undefined, direct_hoist_8);
const direct_hoist_47 = new ObjectRuntype(undefined, {
    "worlds": direct_hoist_46
}, []);
const direct_hoist_48 = new ConstRuntype(undefined, "read_world_mail");
const direct_hoist_49 = new ObjectRuntype(undefined, {
    "after_message_id": direct_hoist_18,
    "api_version": direct_hoist_21,
    "context": direct_hoist_22,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_23),
    "limit": direct_hoist_25,
    "operation": direct_hoist_48,
    "request_id": direct_hoist_23,
    "world_id": direct_hoist_23
}, []);
const direct_hoist_50 = new ArrayRuntype(undefined, direct_hoist_9);
const direct_hoist_51 = new ObjectRuntype(undefined, {
    "high_water_message_id": direct_hoist_18,
    "messages": direct_hoist_50
}, []);
const direct_hoist_52 = new RefRuntype(undefined, "ListContextsRequest");
const direct_hoist_53 = new RefRuntype(undefined, "ListWorldsRequest");
const direct_hoist_54 = new AnyOfDiscriminatedRuntype(undefined, [
    direct_hoist_6,
    direct_hoist_0,
    direct_hoist_4,
    direct_hoist_5,
    direct_hoist_53,
    direct_hoist_52
], "operation", {
    "create_world": direct_hoist_4,
    "delete_world": direct_hoist_5,
    "exec_world": direct_hoist_0,
    "list_contexts": direct_hoist_52,
    "list_worlds": direct_hoist_53,
    "read_world_mail": direct_hoist_6
}, {
    "create_world": direct_hoist_4,
    "delete_world": direct_hoist_5,
    "exec_world": direct_hoist_0,
    "list_contexts": direct_hoist_52,
    "list_worlds": direct_hoist_53,
    "read_world_mail": direct_hoist_6
});
const direct_hoist_55 = new AnyOfDiscriminatedRuntype(undefined, [
    direct_hoist_16,
    direct_hoist_15
], "outcome", {
    "error": direct_hoist_16,
    "ok": direct_hoist_15
}, {
    "error": direct_hoist_16,
    "ok": direct_hoist_15
});
const direct_hoist_56 = new RefRuntype(undefined, "ListContextsResult");
const direct_hoist_57 = new RefRuntype(undefined, "ListWorldsResult");
const direct_hoist_58 = new AnyOfRuntype(undefined, [
    direct_hoist_10,
    direct_hoist_11,
    direct_hoist_1,
    direct_hoist_56,
    direct_hoist_57,
    direct_hoist_12
]);
const direct_hoist_59 = new RefRuntype(undefined, "UInt16");
const direct_hoist_60 = new ObjectRuntype(undefined, {
    "host": direct_hoist_22,
    "host_keys": direct_hoist_36,
    "port": direct_hoist_59,
    "user": direct_hoist_22
}, []);
const direct_hoist_61 = new ConstRuntype(undefined, "ok");
const direct_hoist_62 = new RefRuntype(undefined, "Result");
const direct_hoist_63 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_21,
    "expires_at_unix_ms": new OptionalFieldRuntype(direct_hoist_33),
    "outcome": direct_hoist_61,
    "request_id": direct_hoist_23,
    "result": direct_hoist_62,
    "server_id": new OptionalFieldRuntype(direct_hoist_23)
}, []);
const direct_hoist_64 = new NumberWithFormatRuntype(undefined, [
    "UInt16"
]);
const direct_hoist_65 = new NumberWithFormatRuntype(undefined, [
    "UInt32"
]);
const direct_hoist_66 = new NumberWithFormatRuntype(undefined, [
    "UInt64"
]);
const direct_hoist_67 = new StringWithFormatRuntype(undefined, [
    "Uuid"
]);
const direct_hoist_68 = new AnyOfConstsRuntype(undefined, [
    "destroying",
    "error",
    "provisioning",
    "running",
    "stopped"
]);
const direct_hoist_69 = new ObjectRuntype(undefined, {
    "disk_gib": direct_hoist_18,
    "guest_ip": new OptionalFieldRuntype(direct_hoist_22),
    "last_error": new OptionalFieldRuntype(direct_hoist_22),
    "memory_mib": direct_hoist_18,
    "name": direct_hoist_22,
    "ssh": new OptionalFieldRuntype(direct_hoist_7),
    "status": direct_hoist_68,
    "vcpus": direct_hoist_25,
    "world_id": direct_hoist_23
}, []);
const direct_hoist_70 = new AnyOfConstsRuntype(undefined, [
    "completed",
    "failed",
    "message"
]);
const direct_hoist_71 = new ObjectRuntype(undefined, {
    "created_at_unix_ms": direct_hoist_33,
    "kind": direct_hoist_70,
    "message_id": direct_hoist_18,
    "pane_id": new OptionalFieldRuntype(direct_hoist_22),
    "text": direct_hoist_22,
    "thread_id": new OptionalFieldRuntype(direct_hoist_22),
    "turn_id": new OptionalFieldRuntype(direct_hoist_22),
    "world_id": direct_hoist_23
}, []);
const namedRuntypes = {
    "CapacityDetails": direct_hoist_20,
    "CreateWorldRequest": direct_hoist_26,
    "CreateWorldResult": direct_hoist_27,
    "DeleteWorldRequest": direct_hoist_29,
    "DeleteWorldResult": direct_hoist_30,
    "Error": direct_hoist_32,
    "ErrorResponse": direct_hoist_35,
    "ExecWorldRequest": direct_hoist_38,
    "ExecWorldResult": direct_hoist_39,
    "Int64": direct_hoist_40,
    "ListContextsRequest": direct_hoist_42,
    "ListContextsResult": direct_hoist_43,
    "ListWorldsRequest": direct_hoist_45,
    "ListWorldsResult": direct_hoist_47,
    "ReadWorldMailRequest": direct_hoist_49,
    "ReadWorldMailResult": direct_hoist_51,
    "Request": direct_hoist_54,
    "Response": direct_hoist_55,
    "Result": direct_hoist_58,
    "SshAccess": direct_hoist_60,
    "SuccessResponse": direct_hoist_63,
    "UInt16": direct_hoist_64,
    "UInt32": direct_hoist_65,
    "UInt64": direct_hoist_66,
    "Uuid": direct_hoist_67,
    "World": direct_hoist_69,
    "WorldMail": direct_hoist_71
};
const buildParsersInput = {
    "ExecWorldRequest": direct_hoist_0,
    "ExecWorldResult": direct_hoist_1,
    "Request": direct_hoist_2,
    "Response": direct_hoist_3,
    "CreateWorldRequest": direct_hoist_4,
    "DeleteWorldRequest": direct_hoist_5,
    "ReadWorldMailRequest": direct_hoist_6,
    "SshAccess": direct_hoist_7,
    "World": direct_hoist_8,
    "WorldMail": direct_hoist_9,
    "CreateWorldResult": direct_hoist_10,
    "DeleteWorldResult": direct_hoist_11,
    "ReadWorldMailResult": direct_hoist_12,
    "CapacityDetails": direct_hoist_13,
    "Error": direct_hoist_14,
    "SuccessResponse": direct_hoist_15,
    "ErrorResponse": direct_hoist_16
};

export default { buildParsers };