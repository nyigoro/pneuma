(() => {
  if (typeof globalThis.AbortController === "undefined") {
    class AbortSignal {
      constructor() {
        this.aborted = false;
        this.onabort = null;
      }
    }

    class AbortController {
      constructor() {
        this.signal = new AbortSignal();
      }

      abort() {
        this.signal.aborted = true;
        if (typeof this.signal.onabort === "function") {
          this.signal.onabort();
        }
      }
    }

    globalThis.AbortController = AbortController;
    globalThis.AbortSignal = AbortSignal;
  }
})();
