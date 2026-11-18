import { checkVersion, legacy, packet } from "../../ui/protocol/events.mjs";
// The provider for `ui-dom`'s `surface.Surface`.
//
// In a browser it drives the real DOM under the element with id "app".
// Anywhere else (hd run NAME --engine node) it keeps an in-memory tree, prints
// each command batch, and answers the events listed in UI_CLICKS, then
// handler 0 to stop. Each item is a handler id, optionally with a payload:
// "2,2,1" clicks, "4=milk,5" types "milk" into the field of handler 4.
//
// This is the DOM's half of rendering.md: the node kinds and the property
// keys of its tables mapped onto elements and CSS, the way
// react-native-web maps React Native's View and style. Unknown kinds render
// as "box", unknown keys and events are dropped (each logged once), and names with
// a "dom:" prefix are this renderer's own: the keys "dom:class" and "dom:attr:NAME",
// the kinds "dom:textarea" and "dom:a", and "dom:EVENT" for the DOM's own events.
//
// `take_keys` is applied here, on keydown, because a handler in hd runs too late
// to prevent a default. Host code that was given a node id (`on_ready`) finds its
// element in the `nodes` table it passes to `createDomSurface`.
//
// Commands arrive as { tag, fields }; an optional field is a value or
// null; node and handler ids are Numbers; a PropValue is
// { tag: "Number" | "Text" | "Flag" | "Paint" | "Extent" | "Unset",
// fields }. `next_event` may return a Promise, which lets the program wait.
//
// The factories, `cssFor` and `chordOf` are exported for providers/dom.test.mjs.

const px = (n) => `${n}px`;

// The text of a Size: { tag: "Px" | "Pct" | "Auto" | "Fill", fields }.
function sizeText(size) {
  switch (size.tag) {
    case "Px":
      return px(size.fields.value);
    case "Pct":
      return `${size.fields.value}%`;
    case "Auto":
      return "auto";
    default:
      return null;
  }
}

function colorText({ r, g, b, a }) {
  return a === 255 ? `rgb(${r}, ${g}, ${b})` : `rgba(${r}, ${g}, ${b}, ${+(a / 255).toFixed(3)})`;
}

const words = {
  justify: { start: "flex-start", center: "center", end: "flex-end", between: "space-between", around: "space-around" },
  align: { start: "flex-start", center: "center", end: "flex-end", stretch: "stretch" },
};

// Font family tokens; anything else is passed through as a family name.
const fonts = {
  sans: "system-ui, sans-serif",
  serif: "Georgia, serif",
  mono: "ui-monospace, SFMono-Regular, Menlo, monospace",
};

const extentKeys = {
  width: "width",
  height: "height",
  min_width: "minWidth",
  min_height: "minHeight",
  max_width: "maxWidth",
  max_height: "maxHeight",
};

const pixelKeys = {
  gap: "gap",
  padding_top: "paddingTop",
  padding_right: "paddingRight",
  padding_bottom: "paddingBottom",
  padding_left: "paddingLeft",
  margin_top: "marginTop",
  margin_right: "marginRight",
  margin_bottom: "marginBottom",
  margin_left: "marginLeft",
  radius: "borderRadius",
  font_size: "fontSize",
};

const plainKeys = {
  grow: "flexGrow",
  shrink: "flexShrink",
  opacity: "opacity",
  font_weight: "fontWeight",
  overflow: "overflow",
  cursor: "cursor",
};

const paintKeys = { border_color: "borderColor", background: "backgroundColor", color: "color" };

// The CSS one portable property sets: a list of [style property, value]
// pairs, with "" for each property an Unset clears; null for a key that is
// not a style key.
export function cssFor(key, value) {
  const unset = value.tag === "Unset";
  const f = value.fields;
  if (key === "direction") return [["flexDirection", unset ? "" : f.value]];
  if (key === "wrap") return [["flexWrap", unset ? "" : f.value ? "wrap" : "nowrap"]];
  if (key in words) return [[key === "justify" ? "justifyContent" : "alignItems", unset ? "" : words[key][f.value] ?? ""]];
  if (key in pixelKeys) return [[pixelKeys[key], unset ? "" : px(f.value)]];
  if (key in plainKeys) return [[plainKeys[key], unset ? "" : String(f.value)]];
  if (key in paintKeys) return [[paintKeys[key], unset ? "" : colorText(f.color)]];
  if (key === "border_width") {
    return [
      ["borderWidth", unset ? "" : px(f.value)],
      ["borderStyle", unset ? "" : "solid"],
    ];
  }
  if (key === "font_family") return [["fontFamily", unset ? "" : fonts[f.value] ?? f.value]];
  if (key in extentKeys) {
    const property = extentKeys[key];
    if (unset) return [[property, ""]];
    const text = sizeText(f.size);
    // Fill takes the room left along the parent's main axis (flex: 1 1 0)
    // and stretches across it.
    if (text === null) return [["flex", "1 1 0"], ["alignSelf", "stretch"], [property, ""]];
    return [[property, text]];
  }
  return null;
}

// The style a kind starts with, and the element it makes.
const kinds = {
  label: { tag: "label", css: {} },
  select: { tag: "select", css: {} },
  box: { tag: "div", css: { display: "flex", flexDirection: "column" } },
  text: { tag: "span", css: {} },
  button: { tag: "button", css: {} },
  input: { tag: "input", css: {}, type: "text" },
  "dom:textarea": { tag: "textarea", css: {} },
  "dom:a": { tag: "a", css: {} },
  checkbox: { tag: "input", css: {}, type: "checkbox" },
  fragment: { tag: "div", css: { display: "contents" } },
};

// What an event on `target` reports: a checkbox's state, a field's text,
// or nothing for a button.
function payloadOf(target) {
  if (!target) return "";
  if (target.type === "checkbox") return String(target.checked);
  return "value" in target ? String(target.value) : "";
}

// A keydown as a key name: `KeyboardEvent.key`, with `Ctrl+`, `Alt+` and `Meta+`
// before it, and `Shift+` before a named key, whose name does not show the shift.
export function chordOf(event) {
  const { key } = event;
  if (["Control", "Alt", "Meta", "Shift"].includes(key)) return key;
  let chord = key;
  if (event.shiftKey && key.length > 1) chord = `Shift+${chord}`;
  if (event.metaKey) chord = `Meta+${chord}`;
  if (event.altKey) chord = `Alt+${chord}`;
  if (event.ctrlKey) chord = `Ctrl+${chord}`;
  return chord;
}

// What a node takes from the browser. One listener per element, reading the
// current set, so `take_keys` can change without adding listeners.
const taken = new WeakMap();
function takeKeys(element, names) {
  if (!taken.has(element)) {
    element.addEventListener("keydown", (domEvent) => {
      if (!domEvent.isComposing && domEvent.keyCode !== 229 && taken.get(element).has(chordOf(domEvent))) domEvent.preventDefault();
    });
  }
  taken.set(element, new Set(names === "" ? [] : names.split("\n")));
}

function warnOnce(seen, what, log) {
  if (seen.has(what)) return;
  seen.add(what);
  log(`ui: ignoring unknown ${what}`);
}

// `trace` receives a line per batch and command; `log` what the renderer ignored.
let surfaceSerial = 0;

export function createDomSurface(root, nodes = new Map(), log = (line) => console.warn(line), trace = (line) => console.log(line)) {
  nodes.set(0, root);
  const prefix = `hdru-${++surfaceSerial}-`;
  let generation = 0;
  const queue = [];
  const seen = new Set();
  let waiting = null;
  const push = (event) => {
    if (waiting) {
      const resolve = waiting;
      waiting = null;
      resolve(event);
    } else {
      queue.push(event);
    }
  };
  const node = (id) => {
    const found = nodes.get(id);
    if (!found) throw new Error(`ui: unknown node ${id}`);
    return found;
  };
  // Drops the map entry of every runtime node under `element`, so a removed
  // or cleared subtree can be collected.
  const forgetDescendants = (element) => {
    for (const child of element.querySelectorAll("[data-hd]")) nodes.delete(Number(child.dataset.hd));
  };
  const setProp = (element, key, value) => {
    const unset = value.tag === "Unset";
    const f = value.fields;
    if (key === "value") {
      element.value = unset ? "" : f.value;
    } else if (key === "checked") {
      element.checked = !unset && f.value;
    } else if (key === "disabled") {
      element.disabled = !unset && f.value;
    } else if (key === "choices") {
      element.replaceChildren(...(unset ? [] : f.items).map(({ key, label }) => {
        const option = document.createElement("option");
        option.value = key;
        option.textContent = label;
        return option;
      }));
    } else if (key === "field_id" || key === "description_id") {
      element.id = prefix + (key === "field_id" ? "field-" : "error-") + f.value;
    } else if (key === "label_for") {
      element.htmlFor = prefix + "field-" + f.value;
    } else if (key === "described_by") {
      if (unset) element.removeAttribute("aria-describedby");
      else element.setAttribute("aria-describedby", prefix + "error-" + f.value);
    } else if (key === "invalid" || key === "accessible_name") {
      const attribute = key === "invalid" ? "aria-invalid" : "aria-label";
      if (unset) element.removeAttribute(attribute);
      else element.setAttribute(attribute, String(f.value));
    } else if (key === "take_keys") {
      takeKeys(element, unset ? "" : f.value);
    } else if (key === "dom:class") {
      element.className = unset ? "" : f.value;
    } else if (key.startsWith("dom:attr:")) {
      const name = key.slice("dom:attr:".length);
      if (unset) element.removeAttribute(name);
      else element.setAttribute(name, String(f.value));
    } else {
      const css = cssFor(key, value);
      if (css === null) warnOnce(seen, `property ${key}`, log);
      else for (const [property, text] of css) element.style[property] = text;
    }
  };
  const apply = (command) => {
    const f = command.fields;
    switch (command.tag) {
      case "Begin":
        checkVersion(f.version);
        if (generation && generation !== f.generation) {
          forgetDescendants(root);
          root.replaceChildren();
        }
        generation = f.generation;
        queue.length = 0;
        break;
      case "Focus": {
        const target = nodes.get(f.node);
        if (target && !target.disabled && target.isConnected) target.focus();
        break;
      }
      case "Create": {
        let kind = kinds[f.kind];
        if (!kind) {
          warnOnce(seen, `kind ${f.kind}`, log);
          kind = kinds.box;
        }
        const element = document.createElement(kind.tag);
        if (kind.type) element.type = kind.type;
        Object.assign(element.style, kind.css);
        element.dataset.hd = String(f.node);
        nodes.set(f.node, element);
        break;
      }
      case "Append":
        node(f.parent).appendChild(node(f.child));
        break;
      case "MoveBefore":
        node(f.parent).insertBefore(node(f.child), f.before === null ? null : node(f.before));
        break;
      case "SetText":
        node(f.node).textContent = f.text;
        break;
      case "SetProp":
        setProp(node(f.node), f.key, f.value);
        break;
      case "Listen": {
        const handler = f.handler;
        const element = node(f.node);
        const mounted = generation;
        const send = (name, value) => push(packet(mounted, handler, name, value));
        if (f.event === "enter" || f.event === "key") {
          element.addEventListener("keydown", (event) => {
            if (event.isComposing || event.keyCode === 229) return;
            if (f.event === "key") send("key", chordOf(event));
            else if (event.key === "Enter") send("enter", payloadOf(event.target));
          });
        } else if (["click", "input", "change", "select"].includes(f.event)) {
          element.addEventListener(f.event === "select" ? "change" : f.event, (event) => {
            if (f.event === "change" && event.target?.type === "checkbox") send("change", event.target.checked);
            else if (f.event === "change") send("dom:change", payloadOf(event.target));
            else send(f.event, payloadOf(event.target));
          });
        } else if (f.event.startsWith("dom:")) {
          element.addEventListener(f.event.slice(4), (event) => {
            send(f.event, typeof event.detail === "string" ? event.detail : payloadOf(event.target));
          });
        } else {
          warnOnce(seen, `event ${f.event}`, log);
        }
        break;
      }
      case "Remove": {
        const element = node(f.node);
        forgetDescendants(element);
        element.remove();
        nodes.delete(f.node);
        break;
      }
      case "Clear": {
        const element = node(f.node);
        forgetDescendants(element);
        element.replaceChildren();
        break;
      }
      default:
        throw new Error(`ui: unknown command ${command.tag}`);
    }
  };
  let batch = 0;
  return {
    apply(commands) {
      batch += 1;
      trace(`ui: batch ${batch}: ${commands.length} command(s)`);
      for (const command of commands) {
        apply(command);
        trace(`ui:   ${command.tag} ${JSON.stringify(command.fields)}`);
      }
    },
    next_event() { return legacy(this.next_event_v2()); },
    next_event_v2() {
      if (queue.length) return queue.shift();
      return new Promise((resolve) => {
        waiting = resolve;
      });
    },
  };
}

// A PropValue as short text for the printed tree.
function valueText(value) {
  const f = value.fields;
  switch (value.tag) {
    case "Number":
    case "Flag":
      return String(f.value);
    case "Text":
      return f.value;
    case "Paint":
      return colorText(f.color);
    case "Extent":
      return sizeText(f.size) ?? "fill";
    default:
      return "unset";
  }
}

export function createScriptedSurface(clicks, print, nodes = new Map()) {
  nodes.set(0, { id: 0, kind: "root", children: [], text: "", attrs: {} });
  let generation = 0;
  const listeners = new Map();
  const node = (id) => {
    const found = nodes.get(id);
    if (!found) throw new Error(`ui: unknown node ${id}`);
    return found;
  };
  const forget = (n) => {
    for (const child of n.children) forget(child);
    nodes.delete(n.id);
  };
  const detach = (child) => {
    for (const parent of nodes.values()) {
      const index = parent.children.indexOf(child);
      if (index >= 0) parent.children.splice(index, 1);
    }
  };
  const describe = (command) => {
    const f = command.fields;
    if (command.tag === "SetProp") return `SetProp node=${f.node} ${f.key}=${JSON.stringify(valueText(f.value))}`;
    const parts = Object.entries(f).map(([k, v]) => `${k}=${v === null ? "none" : JSON.stringify(v)}`);
    return `${command.tag} ${parts.join(" ")}`;
  };
  const apply = (command) => {
    const f = command.fields;
    switch (command.tag) {
      case "Begin":
        checkVersion(f.version);
        if (generation && generation !== f.generation) apply({ tag: "Clear", fields: { node: 0 } });
        generation = f.generation;
        listeners.clear();
        break;
      case "Focus": break;
      case "Create":
        nodes.set(f.node, { id: f.node, kind: f.kind, children: [], text: "", attrs: {} });
        break;
      case "Append":
        detach(node(f.child));
        node(f.parent).children.push(node(f.child));
        break;
      case "MoveBefore": {
        const child = node(f.child);
        detach(child);
        const siblings = node(f.parent).children;
        const index = f.before === null ? siblings.length : siblings.indexOf(node(f.before));
        siblings.splice(index < 0 ? siblings.length : index, 0, child);
        break;
      }
      case "SetText":
        node(f.node).text = f.text;
        break;
      case "SetProp":
        if (f.value.tag === "Unset") delete node(f.node).attrs[f.key];
        else node(f.node).attrs[f.key] = valueText(f.value);
        break;
      case "Listen":
        listeners.set(f.handler, f.event);
        node(f.node).attrs[`on:${f.event}`] = String(f.handler);
        break;
      case "Remove": {
        const target = node(f.node);
        detach(target);
        forget(target);
        break;
      }
      case "Clear": {
        const target = node(f.node);
        for (const child of target.children) forget(child);
        target.children = [];
        break;
      }
      default:
        throw new Error(`ui: unknown command ${command.tag}`);
    }
  };
  const render = (n, depth) => {
    const pad = "  ".repeat(depth);
    const attrs = Object.entries(n.attrs).map(([k, v]) => ` ${k}="${v}"`).join("");
    const text = n.text ? ` "${n.text}"` : "";
    const lines = [`${pad}<${n.kind}#${n.id}${attrs}>${text}`];
    for (const child of n.children) lines.push(...render(child, depth + 1));
    return lines;
  };
  let batch = 0;
  return {
    apply(commands) {
      batch += 1;
      print(`-- batch ${batch}: ${commands.length} command(s)`);
      for (const command of commands) {
        apply(command);
        print(`   ${describe(command)}`);
      }
      print(render(node(0), 0).join("\n"));
    },
    next_event() { return legacy(this.next_event_v2()); },
    next_event_v2() {
      if (!clicks.length) {
        print("-- no more scripted events; stopping");
        return packet(generation, 0, "click", "");
      }
      const { handler, payload } = clicks.shift();
      print(payload === "" ? `-- click handler ${handler}` : `-- event for handler ${handler}: ${JSON.stringify(payload)}`);
      const name = listeners.get(handler) ?? "click";
      return packet(generation, handler, name, name === "change" ? payload === "true" : payload);
    },
  };
}

const inBrowser = typeof document !== "undefined" && typeof document.getElementById === "function";

export default inBrowser
  ? createDomSurface(document.getElementById("app") ?? document.body)
  : createScriptedSurface(
      (globalThis.process?.env?.UI_CLICKS ?? "")
        .split(",")
        .map((item) => item.trim())
        .filter((item) => item.length)
        .map((item) => {
          const at = item.indexOf("=");
          return at < 0
            ? { handler: Number(item), payload: "" }
            : { handler: Number(item.slice(0, at)), payload: item.slice(at + 1) };
        }),
      (line) => console.log(line),
    );
