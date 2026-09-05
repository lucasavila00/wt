import parse from "./generated/parser.js";
import type * as Api from "./api.js";

export const Codecs = parse.buildParsers<{
  ExecWorldRequest: Api.ExecWorldRequest;
  ExecWorldResult: Api.ExecWorldResult;
  Request: Api.Request;
  Response: Api.Response;
  CreateWorldRequest: Api.CreateWorldRequest;
  DeleteWorldRequest: Api.DeleteWorldRequest;
  ReadWorldMailRequest: Api.ReadWorldMailRequest;
  SshAccess: Api.SshAccess;
  World: Api.World;
  WorldMail: Api.WorldMail;
  CreateWorldResult: Api.CreateWorldResult;
  DeleteWorldResult: Api.DeleteWorldResult;
  ReadWorldMailResult: Api.ReadWorldMailResult;
  CapacityDetails: Api.CapacityDetails;
  Error: Api.Error;
  SuccessResponse: Api.SuccessResponse;
  ErrorResponse: Api.ErrorResponse;
}>({
  stringFormats: {
    Uuid: {
      validator: (value) =>
        /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(value),
      jsonSchemaFormat: "uuid",
    },
  },
  numberFormats: {
    Int64: {
      validator: Number.isSafeInteger,
      jsonSchemaFormat: "int64",
    },
    UInt16: {
      validator: (value) => Number.isSafeInteger(value) && value >= 0 && value <= 65_535,
      jsonSchemaFormat: "uint16",
    },
    UInt32: {
      validator: (value) => Number.isSafeInteger(value) && value >= 0 && value <= 4_294_967_295,
      jsonSchemaFormat: "uint32",
    },
    UInt64: {
      validator: (value) => Number.isSafeInteger(value) && value >= 0,
      jsonSchemaFormat: "uint64",
    },
  },
});
