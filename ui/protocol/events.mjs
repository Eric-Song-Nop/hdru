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
