(() => {
  if (typeof globalThis.SpeechRecognition !== "undefined") {
    return;
  }

  class SpeechRecognition {
    start() {}
    stop() {}
  }

  globalThis.SpeechRecognition = SpeechRecognition;
  globalThis.webkitSpeechRecognition = SpeechRecognition;
})();
