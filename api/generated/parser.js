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
const direct_hoist_6 = new RefRuntype(undefined, "SshAccess");
const direct_hoist_7 = new RefRuntype(undefined, "World");
const direct_hoist_8 = new RefRuntype(undefined, "CreateWorldResult");
const direct_hoist_9 = new RefRuntype(undefined, "DeleteWorldResult");
const direct_hoist_10 = new RefRuntype(undefined, "CapacityDetails");
const direct_hoist_11 = new RefRuntype(undefined, "Error");
const direct_hoist_12 = new RefRuntype(undefined, "SuccessResponse");
const direct_hoist_13 = new RefRuntype(undefined, "ErrorResponse");
const direct_hoist_14 = new ConstRuntype(undefined, "capacity");
const direct_hoist_15 = new RefRuntype(undefined, "UInt64");
const direct_hoist_16 = new AnyOfConstsRuntype(undefined, [
    "cpu",
    "disk",
    "memory"
]);
const direct_hoist_17 = new ObjectRuntype(undefined, {
    "kind": direct_hoist_14,
    "requested": direct_hoist_15,
    "reserved": direct_hoist_15,
    "resource": direct_hoist_16,
    "total": direct_hoist_15
}, []);
const direct_hoist_18 = new ConstRuntype(undefined, 1);
const direct_hoist_19 = new TypeofRuntype(undefined, "string");
const direct_hoist_20 = new RefRuntype(undefined, "Uuid");
const direct_hoist_21 = new ConstRuntype(undefined, "create_world");
const direct_hoist_22 = new RefRuntype(undefined, "UInt32");
const direct_hoist_23 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_18,
    "context": direct_hoist_19,
    "disk_gib": direct_hoist_15,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_20),
    "git_user_email": direct_hoist_19,
    "git_user_name": direct_hoist_19,
    "memory_mib": direct_hoist_15,
    "name": direct_hoist_19,
    "operation": direct_hoist_21,
    "request_id": direct_hoist_20,
    "vcpus": direct_hoist_22
}, []);
const direct_hoist_24 = new ObjectRuntype(undefined, {
    "world": direct_hoist_7
}, []);
const direct_hoist_25 = new ConstRuntype(undefined, "delete_world");
const direct_hoist_26 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_18,
    "context": direct_hoist_19,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_20),
    "operation": direct_hoist_25,
    "request_id": direct_hoist_20,
    "world_id": direct_hoist_20
}, []);
const direct_hoist_27 = new ObjectRuntype(undefined, {
    "world_id": direct_hoist_20
}, []);
const direct_hoist_28 = new TypeofRuntype(undefined, "boolean");
const direct_hoist_29 = new ObjectRuntype(undefined, {
    "code": direct_hoist_19,
    "details": new OptionalFieldRuntype(direct_hoist_10),
    "message": direct_hoist_19,
    "retryable": direct_hoist_28
}, []);
const direct_hoist_30 = new RefRuntype(undefined, "Int64");
const direct_hoist_31 = new ConstRuntype(undefined, "error");
const direct_hoist_32 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_18,
    "error": direct_hoist_11,
    "expires_at_unix_ms": new OptionalFieldRuntype(direct_hoist_30),
    "outcome": direct_hoist_31,
    "request_id": new OptionalFieldRuntype(direct_hoist_20),
    "server_id": new OptionalFieldRuntype(direct_hoist_20)
}, []);
const direct_hoist_33 = new ArrayRuntype(undefined, direct_hoist_19);
const direct_hoist_34 = new ConstRuntype(undefined, "exec_world");
const direct_hoist_35 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_18,
    "args": direct_hoist_33,
    "context": direct_hoist_19,
    "executable": direct_hoist_19,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_20),
    "operation": direct_hoist_34,
    "request_id": direct_hoist_20,
    "stdin": direct_hoist_19,
    "world_id": direct_hoist_20
}, []);
const direct_hoist_36 = new ObjectRuntype(undefined, {
    "exit_status": direct_hoist_30,
    "stderr": direct_hoist_19,
    "stdout": direct_hoist_19
}, []);
const direct_hoist_37 = new NumberWithFormatRuntype(undefined, [
    "Int64"
]);
const direct_hoist_38 = new ConstRuntype(undefined, "list_contexts");
const direct_hoist_39 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_18,
    "operation": direct_hoist_38,
    "request_id": direct_hoist_20
}, []);
const direct_hoist_40 = new ObjectRuntype(undefined, {
    "contexts": direct_hoist_33
}, []);
const direct_hoist_41 = new ConstRuntype(undefined, "list_worlds");
const direct_hoist_42 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_18,
    "context": direct_hoist_19,
    "expected_server_id": new OptionalFieldRuntype(direct_hoist_20),
    "operation": direct_hoist_41,
    "request_id": direct_hoist_20
}, []);
const direct_hoist_43 = new ArrayRuntype(undefined, direct_hoist_7);
const direct_hoist_44 = new ObjectRuntype(undefined, {
    "worlds": direct_hoist_43
}, []);
const direct_hoist_45 = new RefRuntype(undefined, "ListContextsRequest");
const direct_hoist_46 = new RefRuntype(undefined, "ListWorldsRequest");
const direct_hoist_47 = new AnyOfDiscriminatedRuntype(undefined, [
    direct_hoist_0,
    direct_hoist_4,
    direct_hoist_5,
    direct_hoist_46,
    direct_hoist_45
], "operation", {
    "create_world": direct_hoist_4,
    "delete_world": direct_hoist_5,
    "exec_world": direct_hoist_0,
    "list_contexts": direct_hoist_45,
    "list_worlds": direct_hoist_46
}, {
    "create_world": direct_hoist_4,
    "delete_world": direct_hoist_5,
    "exec_world": direct_hoist_0,
    "list_contexts": direct_hoist_45,
    "list_worlds": direct_hoist_46
});
const direct_hoist_48 = new AnyOfDiscriminatedRuntype(undefined, [
    direct_hoist_13,
    direct_hoist_12
], "outcome", {
    "error": direct_hoist_13,
    "ok": direct_hoist_12
}, {
    "error": direct_hoist_13,
    "ok": direct_hoist_12
});
const direct_hoist_49 = new RefRuntype(undefined, "ListContextsResult");
const direct_hoist_50 = new RefRuntype(undefined, "ListWorldsResult");
const direct_hoist_51 = new AnyOfRuntype(undefined, [
    direct_hoist_8,
    direct_hoist_9,
    direct_hoist_1,
    direct_hoist_49,
    direct_hoist_50
]);
const direct_hoist_52 = new RefRuntype(undefined, "UInt16");
const direct_hoist_53 = new ObjectRuntype(undefined, {
    "host": direct_hoist_19,
    "host_keys": direct_hoist_33,
    "port": direct_hoist_52,
    "user": direct_hoist_19
}, []);
const direct_hoist_54 = new ConstRuntype(undefined, "ok");
const direct_hoist_55 = new RefRuntype(undefined, "Result");
const direct_hoist_56 = new ObjectRuntype(undefined, {
    "api_version": direct_hoist_18,
    "expires_at_unix_ms": new OptionalFieldRuntype(direct_hoist_30),
    "outcome": direct_hoist_54,
    "request_id": direct_hoist_20,
    "result": direct_hoist_55,
    "server_id": new OptionalFieldRuntype(direct_hoist_20)
}, []);
const direct_hoist_57 = new NumberWithFormatRuntype(undefined, [
    "UInt16"
]);
const direct_hoist_58 = new NumberWithFormatRuntype(undefined, [
    "UInt32"
]);
const direct_hoist_59 = new NumberWithFormatRuntype(undefined, [
    "UInt64"
]);
const direct_hoist_60 = new StringWithFormatRuntype(undefined, [
    "Uuid"
]);
const direct_hoist_61 = new AnyOfConstsRuntype(undefined, [
    "destroying",
    "error",
    "provisioning",
    "running",
    "stopped"
]);
const direct_hoist_62 = new ObjectRuntype(undefined, {
    "disk_gib": direct_hoist_15,
    "guest_ip": new OptionalFieldRuntype(direct_hoist_19),
    "last_error": new OptionalFieldRuntype(direct_hoist_19),
    "memory_mib": direct_hoist_15,
    "name": direct_hoist_19,
    "ssh": new OptionalFieldRuntype(direct_hoist_6),
    "status": direct_hoist_61,
    "vcpus": direct_hoist_22,
    "world_id": direct_hoist_20
}, []);
const namedRuntypes = {
    "CapacityDetails": direct_hoist_17,
    "CreateWorldRequest": direct_hoist_23,
    "CreateWorldResult": direct_hoist_24,
    "DeleteWorldRequest": direct_hoist_26,
    "DeleteWorldResult": direct_hoist_27,
    "Error": direct_hoist_29,
    "ErrorResponse": direct_hoist_32,
    "ExecWorldRequest": direct_hoist_35,
    "ExecWorldResult": direct_hoist_36,
    "Int64": direct_hoist_37,
    "ListContextsRequest": direct_hoist_39,
    "ListContextsResult": direct_hoist_40,
    "ListWorldsRequest": direct_hoist_42,
    "ListWorldsResult": direct_hoist_44,
    "Request": direct_hoist_47,
    "Response": direct_hoist_48,
    "Result": direct_hoist_51,
    "SshAccess": direct_hoist_53,
    "SuccessResponse": direct_hoist_56,
    "UInt16": direct_hoist_57,
    "UInt32": direct_hoist_58,
    "UInt64": direct_hoist_59,
    "Uuid": direct_hoist_60,
    "World": direct_hoist_62
};
const buildParsersInput = {
    "ExecWorldRequest": direct_hoist_0,
    "ExecWorldResult": direct_hoist_1,
    "Request": direct_hoist_2,
    "Response": direct_hoist_3,
    "CreateWorldRequest": direct_hoist_4,
    "DeleteWorldRequest": direct_hoist_5,
    "SshAccess": direct_hoist_6,
    "World": direct_hoist_7,
    "CreateWorldResult": direct_hoist_8,
    "DeleteWorldResult": direct_hoist_9,
    "CapacityDetails": direct_hoist_10,
    "Error": direct_hoist_11,
    "SuccessResponse": direct_hoist_12,
    "ErrorResponse": direct_hoist_13
};

export default { buildParsers };