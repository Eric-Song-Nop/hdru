// Wire values have the same named fields as the hd enums in protocol.hd.
const tags = {
  click: "Click", input: "Text", change: "Toggle", select: "Selected",
  key: "Key", enter: "Submit",
};

export function packet(generation, handler, event, value) {
  const tag = tags[event] ?? "Custom";
  return { generation, handler, payload: { tag, fields: { value } } };
}

export function legacy(result) {
  const convert = ({ handler, payload }) => ({ handler, payload: String(payload.fields.value) });
  return result instanceof Promise ? result.then(convert) : convert(result);
}

export function checkVersion(version) {
  if (version !== 2) throw new Error(`unsupported hdru surface protocol ${version}`);
}

// One mount can be blocked in host input while its owner closes it.
export function inputReader() {
  let pending = null;
  return {
    get waiting() { return pending !== null; },
    deliver(event) { pending.finish(event); },
    wait(context, generation) {
      const signal = context?.signal;
      if (pending && pending.signal === signal && pending.generation === generation) return pending.promise;
      pending?.cancel();
      const { promise, resolve } = Promise.withResolvers();
      const finish = (event) => {
        signal?.removeEventListener("abort", cancel);
        pending = null;
        resolve(event);
      };
      const cancel = () => finish(packet(generation, 0, "click", ""));
      pending = { signal, generation, promise, cancel, finish };
      if (signal?.aborted) cancel();
      else signal?.addEventListener("abort", cancel, { once: true });
      return promise;
    },
  };
}
