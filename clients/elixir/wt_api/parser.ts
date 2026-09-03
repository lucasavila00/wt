import parse from "./generated/parser.js";
import type * as Api from "./api.js";

export const Codecs = parse.buildParsers<{
  Request: Api.Request;
  Response: Api.Response;
  CreateWorldRequest: Api.CreateWorldRequest;
  DeleteWorldRequest: Api.DeleteWorldRequest;
  StartCodexRequest: Api.StartCodexRequest;
  InspectCodexRequest: Api.InspectCodexRequest;
  SendCodexMessageRequest: Api.SendCodexMessageRequest;
  ReadWorldMailRequest: Api.ReadWorldMailRequest;
  SshAccess: Api.SshAccess;
  World: Api.World;
  WorldMail: Api.WorldMail;
  CreateWorldResult: Api.CreateWorldResult;
  DeleteWorldResult: Api.DeleteWorldResult;
  StartCodexResult: Api.StartCodexResult;
  InspectCodexResult: Api.InspectCodexResult;
  SendCodexMessageResult: Api.SendCodexMessageResult;
  ReadWorldMailResult: Api.ReadWorldMailResult;
  CapacityDetails: Api.CapacityDetails;
  Error: Api.Error;
}>({
  stringFormats: {
    Uuid: {
      validator: (value) =>
        /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(value),
      jsonSchemaFormat: "uuid",
    },
  },
  numberFormats: {
    Integer: {
      validator: Number.isSafeInteger,
      jsonSchemaFormat: "integer",
    },
    UnsignedInteger: {
      validator: (value) => Number.isSafeInteger(value) && value >= 0,
      jsonSchemaFormat: "unsigned-integer",
    },
  },
});
