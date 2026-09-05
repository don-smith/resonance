export const conversationsErrorMessages = Object.freeze({
  "unavailable-capability": "Conversations are unavailable.",
  "invalid-request": "The conversations request is invalid.",
  "missing-channel": "That channel is unavailable.",
  "archived-channel": "That channel is archived.",
  unauthorized: "You cannot perform that conversation action.",
  "missing-epoch": "Conversation encryption is not ready yet.",
  "authoring-blocked":
    "Conversation authoring is waiting for membership to update.",
  "size-limit": "The conversations size limit was exceeded.",
  internal: "Conversations could not complete the request.",
});

export type ConversationsErrorCode = keyof typeof conversationsErrorMessages;
export type ConversationsError = Readonly<
  {
    [Code in ConversationsErrorCode]: {
      code: Code;
      message: (typeof conversationsErrorMessages)[Code];
    };
  }[ConversationsErrorCode]
>;
