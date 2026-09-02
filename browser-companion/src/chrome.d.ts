declare const chrome: {
  runtime: {
    onInstalled: {
      addListener(callback: () => void): void;
    };
  };
};
