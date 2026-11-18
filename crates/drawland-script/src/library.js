// The draw.land library, as JavaScript sees it. The library itself is Rust
// (`drawland-graph`): every check, default and message is there, the same for
// Python and Rust. This is only its JavaScript face: the namespaces, handles
// as objects, and the three calls that take a function (`view`, a `remap`
// warp, `burin.terrain`), which run here, in JavaScript.
//
// It is a function of `dl`, the library's calls: a profile's interpreter
// (drawland-script) gives it the graph the script builds and puts what it
// returns in the script's globals; the Node package gives it a `Graph` of its
// own and returns it as that graph's face.
(function (dl) {
  "use strict";

  // A handle from the library: features, an area, lines. `$` is what the
  // library knows it by.
  class Handle {
    constructor(marked) { this.$ = marked; Object.freeze(this); }
  }

  // A value on its way to the library: handles as they are marked, functions
  // as a mark (the library says where it takes none).
  const enc = (v) => {
    if (v instanceof Handle) return v.$;
    if (typeof v === "function") return { $function: true };
    if (Array.isArray(v)) return v.map(enc);
    if (v !== null && typeof v === "object" && !(v instanceof Part)) {
      const out = {};
      for (const k of Object.keys(v)) if (v[k] !== undefined) out[k] = enc(v[k]);
      return out;
    }
    if (v instanceof Part) return v.$;
    return v;
  };

  // A module's parts made when first read (`graticule.keys`, `labels.area`).
  class Part {
    constructor(marked) {
      this.$ = marked;
      for (const name of marked.parts) {
        let made;
        Object.defineProperty(this, name, { enumerable: true, get: () => made || (made = dec(dl.get(marked, name))) });
      }
      Object.freeze(this);
    }
  }

  // A value from the library.
  const dec = (v) => {
    if (Array.isArray(v)) return v.map(dec);
    if (v === null || typeof v !== "object") return v;
    if ("$features" in v || "$area" in v || "$lines" in v) return new Handle(v);
    if ("$lazy" in v) return new Part(v);
    const out = {};
    for (const k of Object.keys(v)) out[k] = dec(v[k]);
    return Object.freeze(out);
  };

  const call = (name) => (...args) => dec(dl.call(name, enc(args)));
  const prop = (name) => () => dec(dl.prop(name));

  const lib = {};
  for (const name of dl.functions()) {
    const [space, fn] = name.split(".");
    if (fn === undefined) lib[space] = call(name);
    else (lib[space] || (lib[space] = {}))[fn] = call(name);
  }
  for (const name of dl.properties()) {
    const [space, key] = name.split(".");
    Object.defineProperty(lib[space] || (lib[space] = {}), key, { enumerable: true, get: prop(name) });
  }

  // `output` samples a `remap` warp here: the function runs on a grid of
  // page points, `page.toWorld` answering through the warps before it, and
  // what it gives goes to the engine as a mesh.
  lib.output = (spec) => {
    if (!dl.outputBegin(enc(spec))) return;
    const warps = spec.warp == null ? [] : Array.isArray(spec.warp) ? spec.warp : [spec.warp];
    const done = [];
    warps.forEach((w, n) => {
      if (w != null && typeof w === "object" && "remap" in w) {
        dl.outputWarps(enc(done));
        const { columns, rows, points } = dl.remapGrid(enc(w), n);
        const went = points.map((p) => w.remap(p));
        done.push(dec(dl.remapMesh(n, columns, rows, points, enc(went))));
      } else {
        done.push(w);
      }
    });
    dl.outputWarps(enc(done));
  };

  // Another place, at its own scale, drawn into a rectangle of this page:
  //   view({ id: "beach", at: [0, 0], size: [74, 50], lat, lng, scale: "1:6000" }, "city-grain")
  //   view({ id: "park", … }, () => { draw({ … }) })
  // The style is a script the host lends by name, or a function whose calls
  // build the view's own graph. Returns the view's id.
  lib.view = (spec, style) => {
    if (typeof style === "string") return dl.viewStyle(enc(spec), style);
    if (typeof style !== "function") {
      dl.fail(`view: the style is a name the host knows ("city-grain") or a function that draws, got ${dl.describe(enc(style))}`);
    }
    dl.viewBegin(enc(spec));
    let ok = false;
    try {
      style();
      ok = true;
    } finally {
      dl.viewEnd(ok);
    }
    return spec.id;
  };

  // A burin document, written here in burin's own language: the callback
  // runs in this engine, so it is linted, completed and its errors carry
  // this script's positions. The atlas it builds is drawn by the engine.
  //   burin.terrain((b) => { b.batch({ angle: 90 }); b.shore({ lines: 2 }) })
  // Or the document's JSON, as burin's own front ends build it.
  const burinTerrain = (build, options) => {
    if (typeof build !== "function" && (build === null || typeof build !== "object")) {
      dl.fail(`burin.terrain expects a function: burin.terrain((b) => { … }), got ${dl.describe(enc(build))}`);
    }
    const atlas = typeof build === "function" ? dl.burin(build) : build;
    return dec(dl.call("burin.terrain", enc([atlas, options])));
  };
  lib.burin.terrain = burinTerrain;
  lib.burin.script = burinTerrain;

  for (const value of Object.values(lib)) if (typeof value === "object") Object.freeze(value);
  return Object.freeze(lib);
})
