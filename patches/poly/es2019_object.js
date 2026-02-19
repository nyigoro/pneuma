(() => {
  if (typeof Object.fromEntries !== "function") {
    Object.fromEntries = function fromEntries(iterable) {
      const out = {};
      for (const pair of iterable) {
        if (!pair || pair.length < 2) {
          continue;
        }
        out[pair[0]] = pair[1];
      }
      return out;
    };
  }
})();
