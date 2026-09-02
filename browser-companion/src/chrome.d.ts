declare const chrome: {
  runtime: {
    onInstalled: {
      addListener(callback: () => void): void;
    };
  };
  action?: {
    setBadgeText(details: { text: string }): void;
    setBadgeBackgroundColor(details: { color: string }): void;
    setTitle(details: { title: string }): void;
  };
};
