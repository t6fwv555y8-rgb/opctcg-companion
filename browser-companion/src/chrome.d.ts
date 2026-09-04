declare const chrome: {
  runtime: {
    onInstalled: {
      addListener(callback: () => void): void;
    };
    onMessage: {
      addListener(
        callback: (
          message: any,
          sender: unknown,
          sendResponse: (response?: unknown) => void,
        ) => boolean | void,
      ): void;
    };
    sendMessage?: (
      message: unknown,
      responseCallback?: (response: unknown) => void,
    ) => void;
  };
  action?: {
    setBadgeText(details: { text: string }): void | Promise<void>;
    setBadgeBackgroundColor(details: { color: string }): void | Promise<void>;
    setTitle(details: { title: string }): void | Promise<void>;
  };
};
