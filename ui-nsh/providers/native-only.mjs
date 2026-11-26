function nativeOnly() {
  throw new Error("ui-nsh needs its native Wasmtime host. Run ./scripts/run-nsh.sh from the hdru workspace.");
}

export default {
  apply: nativeOnly,
  next_event_v2: nativeOnly,
};
