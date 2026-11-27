/* kinnrt.js — the KINN web runtime (0.4.0 S5/S6).
   Zero dependencies, plain script (ES2020), sets globalThis.KinnRT.
   No eval, no new Function, no innerHTML, no framework (A6, C.6).
   Copyright (c) KINN contributors — same license as the compiler. */
"use strict";
(function (g) {
  var I64_MIN = -9223372036854775808; /* exact in f64 (= -2^63) */
  var I64_TOP = 9223372036854775808; /* exact 2^63: the overflow edge */
  var B_MIN = -9223372036854775808n;
  var B_MAX = 9223372036854775807n;
  var FAST = 4294967296; /* operands within ±2^32: exact float64 math */

  /* -- errors (C.7): Kin-level runtime errors map back to .kin lines -- */
  function KinRTError(code, message, line) {
    this.name = "KinRTError";
    this.code = code;
    this.message = message;
    this.line = line;
    this.kinLine = line;
    this.kinFile = "";
    this.kin = true;
  }
  KinRTError.prototype = Object.create(Error.prototype);
  KinRTError.prototype.constructor = KinRTError;
  function err(code, msg, line) { throw new KinRTError(code, msg, line); }

  /* -- checked i64 arithmetic through float64/BigInt (C.3) --
     Fast path for small operands (exact in f64); slow path is exact
     via BigInt so i64::MIN boundary cases behave like the Rust VM.
     Documented limit: literals/results beyond ±2^53 round like every
     f64 — the differential corpus keeps printed values exact. */
  function k_add(a, b, l) {
    if (a > -FAST && a < FAST && b > -FAST && b < FAST) return a + b;
    var r = BigInt(a) + BigInt(b);
    if (r > B_MAX || r < B_MIN) err("E4004", "integer overflow in `+`", l);
    return Number(r);
  }
  function k_sub(a, b, l) {
    if (a > -FAST && a < FAST && b > -FAST && b < FAST) return a - b;
    var r = BigInt(a) - BigInt(b);
    if (r > B_MAX || r < B_MIN) err("E4004", "integer overflow in `-`", l);
    return Number(r);
  }
  function k_mul(a, b, l) {
    if (a > -FAST && a < FAST && b > -FAST && b < FAST) {
      var r0 = a * b;
      /* Products of two <2^32 operands can still exceed i64; for
         in-range products the float64 product is exact. */
      if (r0 >= I64_TOP || r0 < I64_MIN) err("E4004", "integer overflow in `*`", l);
      return r0;
    }
    var r = BigInt(a) * BigInt(b);
    if (r > B_MAX || r < B_MIN) err("E4004", "integer overflow in `*`", l);
    return Number(r);
  }
  function k_div(a, b, l) {
    if (b === 0) err("E4001", "division by zero", l);
    if (a > -FAST && a < FAST && b > -FAST && b < FAST) return MT(a / b);
    var r = BigInt(a) / BigInt(b);
    if (r > B_MAX || r < B_MIN) err("E4004", "integer overflow in `/`", l);
    return Number(r);
  }
  function k_mod(a, b, l) {
    if (b === 0) err("E4001", "modulo by zero", l);
    if (a > -FAST && a < FAST && b > -FAST && b < FAST) return a % b;
    if (a === I64_MIN && b === -1) err("E4004", "integer overflow in `%`", l);
    return Number(BigInt(a) % BigInt(b));
  }
  function k_neg(a, l) {
    if (a === I64_MIN) err("E4004", "integer overflow in unary `-`", l);
    return -a;
  }
  function k_iabs(a, l) {
    if (a === I64_MIN) err("E4004", "integer overflow in `abs`", l);
    return a < 0 ? -a : a;
  }
  /* Rust f64::min/max: NaN yields the other operand (Math.min yields NaN). */
  function k_fmin(a, b) { return a !== a ? b : b !== b ? a : Math.min(a, b); }
  function k_fmax(a, b) { return a !== a ? b : b !== b ? a : Math.max(a, b); }
  /* Float division by zero is an error in Kin (not IEEE infinity). */
  function k_fdiv(a, b, l) {
    if (b === 0) err("E4001", "division by zero", l);
    return a / b;
  }

  /* -- values: formatting parity with kinrt::Value::format ------------ */
  /* Expand shortest-round-trip exponent forms to plain decimals:
     Rust's Display never uses exponents, JS String() does. */
  function k_expand(s) {
    var i = s.indexOf("e");
    var mant = s.slice(0, i);
    var exp = parseInt(s.slice(i + 1), 10);
    var neg = mant.charCodeAt(0) === 45;
    if (neg) mant = mant.slice(1);
    var dot = mant.indexOf(".");
    var digits = dot === -1 ? mant : mant.slice(0, dot) + mant.slice(dot + 1);
    var point = (dot === -1 ? digits.length : dot) + exp;
    var out;
    if (point <= 0) out = "0." + "0".repeat(-point) + digits;
    else if (point >= digits.length) out = digits + "0".repeat(point - digits.length);
    else out = digits.slice(0, point) + "." + digits.slice(point);
    return neg ? "-" + out : out;
  }
  function k_ffmt(v) {
    if (v !== v) return "NaN";
    if (v === Infinity) return "inf";
    if (v === -Infinity) return "-inf";
    if (v === MT(v) && Math.abs(v) < 1e16) {
      return Object.is(v, -0) ? "-0.0" : v.toFixed(1);
    }
    var s = String(v);
    return s.indexOf("e") === -1 ? s : k_expand(s);
  }
  /* k_fmt(v, tag): Value::format parity. Tags: "i" "f" "b" "s",
     arrays "a" + element tag ("ai", "aaf", …). */
  function k_fmt(v, t) {
    if (t === "i") return String(v);
    if (t === "f") return k_ffmt(v);
    if (t === "b") return v ? "true" : "false";
    if (t === "s") return v;
    if (t.charAt(0) === "a") {
      var et = t.slice(1);
      var r = "[";
      for (var i = 0; i < v.length; i++) {
        if (i) r += ", ";
        r += k_fmt(v[i], et);
      }
      return r + "]";
    }
    return String(v);
  }

  /* -- arrays: reference semantics + deep structural equality --------- */
  function k_eq(a, b) {
    var aa = AI(a), ba = AI(b);
    if (aa !== ba) return false;
    if (!aa) return a === b;
    if (a.length !== b.length) return false;
    for (var i = 0; i < a.length; i++) if (!k_eq(a[i], b[i])) return false;
    return true;
  }
  function k_acat(a, b) { return a.concat(b); }
  function k_aget(a, i, l) {
    if (i < 0 || i >= a.length) {
      err("E4005", "array index " + i + " out of bounds (len " + a.length + ")", l);
    }
    return a[i];
  }
  function k_aset(a, i, v, l) {
    if (i < 0 || i >= a.length) {
      err("E4005", "array index " + i + " out of bounds (len " + a.length + ")", l);
    }
    a[i] = v;
  }

  /* -- strings: B.10 helpers, Unicode-honest --------------------------- */
  /* Code-point length (Rust chars().count(), not UTF-16 units). */
  function k_len(s) {
    var n = 0;
    for (var i = 0; i < s.length; i++) {
      var c = s.charCodeAt(i);
      if (c >= 0xd800 && c <= 0xdbff) i++;
      n++;
    }
    return n;
  }
  /* One-to-one case mapping: a code point whose mapping expands to
     several code points keeps its original form (same rule as the VM). */
  function k_case(s, up) {
    var cps = Array.from(s);
    for (var i = 0; i < cps.length; i++) {
      var m = up ? cps[i].toUpperCase() : cps[i].toLowerCase();
      if (Array.from(m).length === 1) cps[i] = m;
    }
    return cps.join("");
  }
  function k_upper(s) { return k_case(s, true); }
  function k_lower(s) { return k_case(s, false); }
  /* Unicode White_Space (the Rust char::is_whitespace set — U+FEFF is
     deliberately NOT whitespace here, unlike JS String.trim). */
  var WS = /[\t\n\v\f\r \u0085\u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000]/;
  function k_trim(s) {
    var a = 0, b = s.length;
    while (a < b && WS.test(s.charAt(a))) a++;
    while (b > a && WS.test(s.charAt(b - 1))) b--;
    return s.slice(a, b);
  }
  function k_contains(s, sub) { return s.indexOf(sub) !== -1; }
  function k_split(s, sep) {
    return sep === "" ? Array.from(s) : s.split(sep);
  }
  function k_join(a, sep) { return a.join(sep); }

  /* -- casts ------------------------------------------------------------ */
  function k_f2i(f) {
    if (f !== f) return 0;
    if (f >= 9223372036854775808) return 9223372036854775807;
    if (f <= I64_MIN) return I64_MIN;
    return MT(f);
  }

  /* -- module table and call discipline ---------------------------------- */
  var K = {
    version: "0.4.0",
    fns: {},            /* name → function (and numeric index → same) */
    depth: 0,           /* entered KIR-JS frames (E4002 parity, 1024) */
    files: {},          /* fn name → .kin file label (C.7 envelopes) */
    out: function (t) { /* output sink; replaced per host */
      if (typeof g.process !== "undefined" && g.process && g.process.stdout) {
        g.process.stdout.write(t + "\n");
      }
    },
    call: function (name, args) {
      var f = K.fns[name];
      if (!f) err("E4000", "module has no `" + name + "` function", 0);
      return k_call(f, args, 0);
    },
    reset: function () { K.depth = 0; },
    /* Register one generated function: numeric index (KIR Call) and
       deterministic name (manifest/"main") both resolve to it; the
       source file label powers C.7 error envelopes. */
    reg: function (idx, name, file, fn) {
      K.fns[idx] = fn;
      K.fns[name] = fn;
      K.files[name] = file;
      fn.kinn_name = name;
      return fn;
    }
  };

  function k_call(f, args, l) {
    if (K.depth >= 1024) {
      err("E4002", "call stack overflow (depth limit is 1024)", l);
    }
    K.depth++;
    try {
      return f.apply(null, args);
    } catch (e) {
      if (e && e.kin && !e.kinFile && f && f.kinn_name) {
        e.kinFile = K.files[f.kinn_name] || "";
      }
      throw e;
    } finally {
      K.depth--;
    }
  }

  /* -- error reporting (C.7): envelope for embedders, console otherwise - */
  K.__report = function (e) {
    if (e && e.kin) {
      var env = {
        kinnRuntimeError: { code: e.code, message: e.message, kinFile: e.kinFile || "", kinLine: e.line }
      };
      if (typeof W !== "undefined" && W.parent && W.parent !== W) {
        W.parent.postMessage(env, "*");
      } else {
        var fn = g.console && g.console.error ? g.console.error.bind(g.console) : function () {};
        fn("[" + e.code + "] " + e.message + " (line " + e.line + ")");
      }
      return;
    }
    var raw = e && e.message ? e.message : String(e);
    if (typeof W !== "undefined" && W.parent && W.parent !== W) {
      W.parent.postMessage({ kinnRuntimeError: { code: "E4099", message: raw, kinFile: "", kinLine: 0 } }, "*");
    } else if (g.console && g.console.error) {
      g.console.error("[E4099] " + raw);
    }
  };

  /* Exported helper surface: the generated app.js/console module reads
     these off K (K.k_add, …) so no globals leak beyond KinnRT. */
  K.KinRTError = KinRTError;
  K.k_call = k_call;
  var X = [k_add, k_sub, k_mul, k_div, k_mod, k_neg, k_iabs, k_fmin, k_fmax, k_fdiv, k_fmt, k_eq, k_acat, k_aget, k_aset, k_len, k_upper, k_lower, k_trim, k_contains, k_split, k_join, k_f2i];
  var XN = "k_add k_sub k_mul k_div k_mod k_neg k_iabs k_fmin k_fmax k_fdiv k_fmt k_eq k_acat k_aget k_aset k_len k_upper k_lower k_trim k_contains k_split k_join k_f2i".split(" ");
  for (var xi = 0; xi < X.length; xi++) K[XN[xi]] = X[xi];

  /* -- S6: the reactive core (C.2/C.5/C.6) --------------------------------
     Layers: State (values + computed caches), Flush (microtask batch over
     the static dependency map), Binder (re-evaluate dirty slots, patch
     only changed text/attrs), Lists (keyed reconciliation), Events (one
     delegated listener set + builtins), Lifecycle (mount/patch — the ONE
     DOM renderer), Hydrator (MPA state via sessionStorage), Store
     (localStorage + schema-hash guard; language surface is S9), Anim
     (class toggles only — visible motion waits for S8 CSS), Reporter
     (already above). prefers-reduced-motion disables the JS animation
     path entirely. */
  var HOP = Object.prototype.hasOwnProperty;
  var AI = Array.isArray;
  var MT = Math.trunc;
  var W = g.window, D = g.document;
  var JSS = JSON.stringify, JSP = JSON.parse;
  var M = null;      /* manifest */
  var st = {};       /* state values (name → value) */
  var cv = {};       /* computed cache (name → value) */
  var cdirty = {};   /* computed name → needs recompute */
  var recs = [];     /* binding records (one per dynamic slot) */
  var flushQ = false;
  var dirty = {};    /* state name → changed since last flush */
  var REDUCED = false;

  function nowin() { return !W; }

  function resolve(name, env) {
    for (var e = env; e; e = e.p) {
      if (e.v && HOP.call(e.v, name)) return e.v[name];
    }
    if (HOP.call(st, name)) return st[name];
    if (HOP.call(cv, name)) return cv[name];
    return undefined;
  }

  function evalRec(r) {
    var args = [], ps = r.p;
    for (var i = 0; i < ps.length; i++) args.push(resolve(ps[i], r.env));
    return k_call(K.fns[r.fn], args, 0);
  }

  function same(a, b) {
    var aa = AI(a), ba = AI(b);
    if (aa !== ba) return false;
    if (!aa) return a === b;
    if (a.length !== b.length) return false;
    for (var i = 0; i < a.length; i++) if (!same(a[i], b[i])) return false;
    return true;
  }

  /* Patch one record (Binder). `r.kind`: text, attr, input, when, each,
     prop (component prop slots re-evaluate at instantiation only). */
  function patch(r, v) {
    if (r.kind === "text") {
      var s = k_fmt(v, r.t);
      if (r.l !== s) { r.l = s; r.nd.data = s; }
    } else if (r.kind === "attr") {
      var t = k_fmt(v, r.t);
      if (r.l !== t) { r.l = t; r.el.setAttribute(r.at, t); }
    } else if (r.kind === "input") {
      var u = k_fmt(v, r.t);
      if (r.l !== u) { r.l = u; r.el.value = u; }
    } else if (r.kind === "prop") {
      if (!same(r.l, v)) {
        r.l = v;
        r.te.v[r.pp] = v;
        forceEnv(r.te);
      }
    } else if (r.kind === "when") {
      var on = v === true;
      if (r.l === on) return;
      r.l = on;
      setLive(r.tA, on); setLive(r.oA, !on && r.ho);
      r.tEl.setAttribute("data-off", on ? "0" : "1");
      if (r.ho) r.oEl.setAttribute("data-off", on ? "1" : "0");
    } else if (r.kind === "each") {
      reconcile(r, v);
    }
  }

  function setLive(rs, live) {
    for (var i = 0; i < rs.length; i++) {
      rs[i].li = live;
      if (live) { rs[i].fo = true; }
    }
  }

/* Lists: keyed reconciliation — children match by key (stable across
     reorders); a duplicate key keeps the newest item in the FIRST slot
     position it appeared at; index-keyed lists (no `key` expression)
     reuse by position. */
  function reconcile(r, v) {
    if (same(r.l, v)) return;
    r.l = v ? v.slice() : v;
    var items = AI(v) ? v : [];
    var i;
    var wanted = []; /* [{key, ks, item}] in list order */
    var seen = {};
    for (i = 0; i < items.length; i++) {
      var key = r.kf ? r.kf(items[i]) : i;
      var ks = JSS(key);
      if (HOP.call(seen, ks)) {
        wanted[seen[ks]].item = items[i]; /* dup key: newest value wins */
      } else {
        seen[ks] = wanted.length;
        wanted.push({ key: key, ks: ks, item: items[i] });
      }
    }
    /* New children list in WANTED order: reuse the child with the same
       key (newest item), mount fresh ones, drop the rest. */
    var childByKs = {};
    for (i = 0; i < r.cd.length; i++) childByKs[r.cd[i].ks] = r.cd[i];
    var reused = {};
    var newKids = [];
    for (i = 0; i < wanted.length; i++) {
      var w = wanted[i];
      var ch = childByKs[w.ks];
      if (ch && !reused[w.ks]) {
        reused[w.ks] = 1;
        ch.env.v[r.var] = w.item;
        newKids.push(ch);
      } else {
        var child = renderChild(r, w.key, w.ks, w.item);
        newKids.push(child);
        reused[w.ks] = 1;
      }
    }
    for (i = 0; i < r.cd.length; i++) {
      if (!reused[r.cd[i].ks]) removeChild(r, r.cd[i]);
    }
    /* Minimal moves: place every child at its list position. */
    var at = r.el.firstChild;
    for (i = 0; i < newKids.length; i++) {
      var node = newKids[i].nodes[0];
      if (at === node) {
        at = at.nextSibling;
      } else {
        r.el.insertBefore(node, at);
      }
    }
    r.cd = newKids;
    for (i = 0; i < newKids.length; i++) {
      var c2 = newKids[i];
      for (var j = 0; j < c2.recs.length; j++) c2.recs[j].fo = true;
    }
  }

  function removeChild(r, child) {
    for (var i = 0; i < child.nodes.length; i++) {
      if (child.nodes[i].parentNode) child.nodes[i].parentNode.removeChild(child.nodes[i]);
    }
    for (var j = 0; j < child.recs.length; j++) {
      var idx = recs.indexOf(child.recs[j]);
      if (idx >= 0) recs.splice(idx, 1);
    }
  }

  /* Flush (C.5): one microtask; recompute dirty computeds in declaration
     order, re-evaluate dirty bindings (static dep map + force), patch. */
  function flush() {
    if (nowin()) { flushNow(); return; }
    if (flushQ) return;
    flushQ = true;
    Promise.resolve().then(function () { flushQ = false; flushNow(); });
  }

  function flushNow() {
    if (!M) return;
    var i, j;
    /* 1. computed values whose deps changed (declaration order). */
    for (i = 0; i < M.computed.length; i++) {
      var c = M.computed[i];
      if (!cdirty[c.name]) continue;
      var deps = c.d;
      for (j = 0; j < deps.length; j++) if (dirty[deps[j]]) { break; }
      if (j < deps.length || !HOP.call(cv, c.name)) {
        var args = [];
        for (var p = 0; p < c.p.length; p++) args.push(resolve(c.p[p], null));
        cv[c.name] = k_call(K.fns[c.fn], args, 0);
        cdirty[c.name] = false;
      }
    }
    /* 2. dirty bindings → patch. */
    for (i = 0; i < recs.length; i++) {
      var r = recs[i];
      if (!r.li) continue;
      var hot = r.fo;
      if (!hot && r.d) {
        for (j = 0; j < r.d.length; j++) if (dirty[r.d[j]]) { hot = true; break; }
      }
      if (!hot) continue;
      r.fo = false;
      try {
        patch(r, evalRec(r));
      } catch (e) {
        if (e && e.kin) { r.fo = false; K.__report(e); }
        else throw e;
      }
    }
    /* 3. one write-through snapshot (Hydrator, MPA). */
    dirty = {};
    hydrateSave();
  }

  function hydrateSave() {
    if (nowin() || !M) return;
    try { W.sessionStorage.setItem("kinn:" + M.name, JSS(st)); }
    catch (e) { /* privacy mode: state is simply not persisted */ }
  }

  function hydrateRestore() {
    if (nowin() || !M) return;
    try {
      var raw = W.sessionStorage.getItem("kinn:" + M.name);
      if (!raw) return;
      var snap = JSP(raw);
      for (var i = 0; i < M.state.length; i++) {
        var n = M.state[i].name;
        if (HOP.call(snap, n)) st[n] = snap[n];
      }
    } catch (e) { /* corrupted snapshot: defaults win */ }
  }

  /* Store layer (present; the language surface arrives with S9). */
  function storeLoad(name, keys, tags) {
    if (nowin()) return;
    try {
      var raw = W.localStorage.getItem("kinn:store:" + name);
      if (!raw) return;
      var obj = JSP(raw);
      /* Schema-hash guard: the key set must match exactly. */
      var ok = keys.length === Object.keys(obj).length;
      for (var i = 0; ok && i < keys.length; i++) {
        if (!HOP.call(obj, keys[i])) ok = false;
      }
      if (!ok) { W.localStorage.removeItem("kinn:store:" + name); return; }
      for (var j = 0; j < keys.length; j++) st[keys[j]] = obj[keys[j]];
    } catch (e) {}
  }
  K.storeLoad = storeLoad;

  /* -- Lifecycle: the ONE DOM renderer (C.2/C.6) --------------------------
     createElement/textContent/setAttribute/insertBefore only; static
     strings are already inlined in templates. `env` is the scope chain
     ({v: values, p: parent}); the page root resolves against state. */
  function el(tag, cls) {
    var e = D.createElement(tag);
    if (cls) e.className = cls;
    return e;
  }

  function addSlot(x, into, kind, extra) {
    /* x = literal string | {b: bindingIndex}; returns a text node or the
       binding record's target once evaluated. */
    if (typeof x === "string") {
      var t = D.createTextNode(x);
      into.appendChild(t);
      return null;
    }
    var b = M.bindings[x.b];
    var r = {
      kind: kind, fn: b.fn, p: b.p, d: b.d, t: b.t,
      env: extra.env, li: true, fo: true, l: undefined,
    };
    if (kind === "text") {
      var node = D.createTextNode("");
      r.nd = node;
      into.appendChild(node);
    } else if (kind === "attr") {
      r.el = extra.el; r.at = extra.at;
    } else if (kind === "input") {
      r.el = extra.el;
    }
    recs.push(r);
    return r;
  }

  /* Render one node of the template; returns the created element. */
  function render(n, parent, env) {
    var k = n.k;
    var node;
    if (k === "title" || k === "h" || k === "text") {
      node = el(k === "title" ? "h1" : k === "h" ? "h2" : "p", k === "title" ? "kinn-title" : null);
      addSlot(n.x, node, "text", { env: env });
    } else if (k === "button" || k === "link") {
      node = el(k === "button" ? "button" : "a", k === "button" ? "kinn-button" : "kinn-link");
      if (k === "button") node.type = "button";
      if (n.o) node.setAttribute("href", n.o + ".html");
      addSlot(n.x, node, "text", { env: env });
      attachHandlers(node, n.h, env);
    } else if (k === "image") {
      node = el("img", null);
      addSlot(n.s, node, "attr", { env: env, el: node, attr: "src" });
      addSlot(n.l, node, "attr", { env: env, el: node, attr: "alt" });
      attachHandlers(node, n.h, env);
    } else if (k === "hr") {
      node = el("hr", null);
    } else if (k === "input") {
      node = el("input", "kinn-input");
      node.setAttribute("data-kb", n.n);
      var r = addSlot(n.x, node, "input", { env: env, el: node });
      if (r) r.t = r.t || "s";
      attachHandlers(node, n.h, env);
    } else if (k === "when") {
      node = el("div", "kinn-when");
      var tEl = el("div", null), oEl = el("div", null);
      var tRecs = renderView(n.t, tEl, env);
      var hasO = n.o && n.o.length;
      var oRecs = hasO ? renderView(n.o, oEl, env) : [];
      node.appendChild(tEl); node.appendChild(oEl);
      var b = M.bindings[n.c.b];
      var rec = {
        kind: "when", fn: b.fn, p: b.p, d: b.d, t: b.t,
        env: env, li: true, fo: true, l: undefined,
        tEl: tEl, oEl: oEl, tA: tRecs, oA: oRecs, ho: hasO,
      };
      setLive(oRecs, false);
      oEl.setAttribute("data-off", "1");
      recs.push(rec);
    } else if (k === "each") {
      node = el("div", "kinn-each");
      var eb = M.bindings[n.i.b];
      var kb = n.y ? M.bindings[n.y.b] : null;
      var rec2 = {
        kind: "each", fn: eb.fn, p: eb.p, d: eb.d, t: eb.t,
        env: env, li: true, fo: true, l: undefined,
        el: node, var: n.v, tp: n.b, cd: [],
        kf: kb ? function (item) { return k_call(K.fns[kb.fn], [item], 0); } : null,
      };
      recs.push(rec2);
    } else if (k === "comp") {
      node = instantiate(n, parent, env);
    }
    if (n.a && n.a.enter && !REDUCED && node) {
      /* Anim (S6): class toggle only — visible motion waits for S8 CSS. */
      node.setAttribute("data-ka", n.a.enter);
    }
    if (node) {
      if (node.nodeType === 1 && env && env.inst) node.__kenv = env;
      parent.appendChild(node);
    }
    return node;
  }

  function renderView(nodes, parent, env) {
    var made = [];
    for (var i = 0; i < nodes.length; i++) {
      var before = recs.length;
      render(nodes[i], parent, env);
      for (var j = before; j < recs.length; j++) made.push(recs[j]);
    }
    return made;
  }

  function renderChild(listRec, key, ks, item) {
    var env = { v: {}, p: null, inst: true };
    env.v[listRec.var] = item;
    var holder = el("div", "kinn-item");
    holder.__kenv = env;
    var recsBefore = recs.length;
    renderView(listRec.tp, holder, env);
    var childRecs = recs.slice(recsBefore);
    listRec.el.appendChild(holder);
    return { key: key, ks: ks, env: env, nodes: [holder], recs: childRecs };
  }

  function instantiate(n, parent, parentEnv) {
    var comp = null;
    for (var i = 0; i < M.components.length; i++) {
      if (M.components[i].name === n.c) { comp = M.components[i]; break; }
    }
    var node = el("div", "kinn-comp");
    if (!comp) return node;
    var env = { v: {}, p: null, inst: true };
    /* Prop slots: literals copy in; dynamic slots become live records
       (evaluated in the PARENT scope, writing into the instance env). */
    for (var pname in n.p) {
      if (!HOP.call(n.p, pname)) continue;
      var x = n.p[pname];
      if (typeof x === "string" || typeof x === "number" || typeof x === "boolean") {
        env.v[pname] = x;
        continue;
      }
      var b = M.bindings[x.b];
      var args = [];
      for (var q = 0; q < b.p.length; q++) args.push(resolve(b.p[q], parentEnv));
      env.v[pname] = k_call(K.fns[b.fn], args, 0);
      recs.push({
        kind: "prop", fn: b.fn, p: b.p, d: b.d, t: b.t,
        env: parentEnv, te: env, pp: pname, li: true, fo: true,
        l: env.v[pname],
      });
    }
    /* Prop defaults (literals) for missing ones. */
    for (var d in n.d) {
      if (HOP.call(n.d, d) && !HOP.call(env.v, d)) {
        env.v[d] = n.d[d];
      }
    }
    /* Per-instance state via app:init:<Comp> (params are the props the
       initializers read — carried in the manifest as `initparams`). */
    if (comp.init && K.fns[comp.init]) {
      var iargv = [];
      for (var z = 0; z < comp.ip.length; z++) {
        iargv.push(env.v[comp.ip[z]]);
      }
      var vals = k_call(K.fns[comp.init], iargv, 0);
      for (var s2 = 0; s2 < comp.state.length; s2++) env.v[comp.state[s2].name] = vals[s2];
    }
    renderView(comp.view, node, env);
    attachHandlers(node, n.h, parentEnv);
    /* Every element inside the instance carries the instance env for
       handler resolution (child elements' own handlers resolve through
       their own envs first — nested instances own theirs). */
    return node;
  }

  /* -- Events (C.6): one delegated listener set on the document -------- */

  function attachHandlers(node, hIdxs, env) {
    if (!hIdxs || !hIdxs.length) return;
    var s = "";
    for (var i = 0; i < hIdxs.length; i++) s += (i ? " " : "") + hIdxs[i];
    node.setAttribute("data-kh", s);
    if (env && env.inst) node.__kenv = env;
  }

  function builtinValue(name, e) {
    var t = e.target;
    if (name === "event_value") return t && "value" in t ? t.value : "";
    if (name === "event_key") return e.key || "";
    if (name === "event_x") return (e.clientX || 0) | 0;
    if (name === "event_y") return (e.clientY || 0) | 0;
  }

  function dispatch(ev) {
    if (!M) return;
    var t = ev.target;
    if (!t || !t.closest) return;
    t = t.closest("[data-kh]");
    if (!t) return;
    var attr = t.getAttribute("data-kh");
    if (!attr) return;
    var idxs = attr.split(" ");
    for (var i = 0; i < idxs.length; i++) {
      var h = M.handlers[+idxs[i]];
      if (!h || h.event !== ev.type) continue;
      if (h.key && h.key !== ev.key) continue;
      runHandler(h, t, ev);
    }
  }

  function runHandler(h, target, ev) {
    try {
      K.reset();
      var env = target.__kenv || null;
      var args = [];
      for (var i = 0; i < h.p.length; i++) args.push(resolve(h.p[i], env));
      for (var j = 0; j < h.bi.length; j++) args.push(builtinValue(h.bi[j], ev));
      var ret = k_call(K.fns[h.fn], args, 0);
      if (!AI(ret)) return;
      var touchedEnv = false;
      for (var p = 0; p < h.p.length; p++) {
        var n = h.p[p];
        if (env && env.v && HOP.call(env.v, n)) {
          env.v[n] = ret[p];
          touchedEnv = true;
        } else if (HOP.call(st, n)) {
          if (st[n] !== ret[p] || AI(st[n])) {
            st[n] = ret[p];
            dirty[n] = true;
            for (var c = 0; c < M.computed.length; c++) {
              if (M.computed[c].d.indexOf(n) >= 0) cdirty[M.computed[c].name] = true;
            }
          }
        }
      }
      if (touchedEnv) forceEnv(env);
      flush();
    } catch (e) {
      if (e && e.kin) K.__report(e);
      else if (g.console && g.console.error) g.console.error(String(e && e.stack ? e.stack : e));
    }
  }

  function forceEnv(env) {
    for (var i = 0; i < recs.length; i++) {
      if (recs[i].env === env) recs[i].fo = true;
    }
    /* Instance state writes also affect computed values that read them
       through bindings — re-evaluate everything in that env chain. */
    for (var j = 0; j < M.computed.length; j++) cdirty[M.computed[j].name] = true;
  }

  /* -- mount (C.4): boot an MPA document ------------------------------- */
  K.mount = function (manifest) {
    /* A mounted document is a fresh app instance: clear any previous
       mount's state (tests, re-mounts). */
    M = manifest;
    st = {}; cv = {}; cdirty = {}; recs = []; dirty = {}; flushQ = false;
    if (nowin()) return; /* headless (tests) — nothing to render */
    
    REDUCED = W.matchMedia && W.matchMedia("(prefers-reduced-motion: reduce)").matches;
    /* State defaults from `app:init`. */
    var defaults = K.fns["app:init"] ? k_call(K.fns["app:init"], [], 0) : [];
    for (var i = 0; i < M.state.length; i++) st[M.state[i].name] = defaults[i];
    /* Seed computed caches on the first flush (declaration order). */
    for (var ci = 0; ci < M.computed.length; ci++) cdirty[M.computed[ci].name] = true;
    /* Hydrate (MPA state transfer) + store layers. */
    hydrateRestore();
    for (var s = 0; s < M.stores.length; s++) {
      storeLoad(M.stores[s].name, M.stores[s].keys, M.state);
    }
    /* Computed caches seed lazily on first read. */
    /* Which page is this document? */
    var main = D.querySelector("[data-kpage]");
    var pname = main ? main.getAttribute("data-kpage") : M.pages[0].name;
    var page = M.pages[0];
    for (var p = 0; p < M.pages.length; p++) {
      if (M.pages[p].name === pname) { page = M.pages[p]; break; }
    }
    if (main) renderView(page.view, main, null);
    /* Delegated listeners (one set per document). */
    var evs = "click input change submit keydown keyup".split(" ");
    for (var e = 0; e < evs.length; e++) {
      D.addEventListener(evs[e], dispatch);
    }
    /* Reporter: browser errors become envelopes. */
    W.addEventListener("error", function (ev) {
      K.__report(ev.error || new Error(ev.message || "script error"));
    });
    W.addEventListener("unhandledrejection", function (ev) {
      K.__report(ev.reason instanceof Error ? ev.reason : new Error(String(ev.reason)));
    });
    /* console.* forwarding (embedded only). */
    if (W.parent && W.parent !== W) {
      var fwd = function (level) {
        var orig = W.console[level].bind(W.console);
        W.console[level] = function () {
          var parts = [];
          for (var i = 0; i < arguments.length; i++) parts.push(String(arguments[i]));
          try {
            W.parent.postMessage({ kinnConsole: { level: level, text: parts.join(" ") } }, "*");
          } catch (e) {}
          orig.apply(null, arguments);
        };
      };
      var levels = "log info warn error".split(" ");
      for (var L = 0; L < levels.length; L++) fwd(levels[L]);
    }
    /* print output in the browser flows through console.log → the
       forwarding above carries it to the host terminal. */
    K.out = function (t) { W.console.log(t); };
    flush();
  };

  g.KinnRT = K;
})(typeof globalThis !== "undefined" ? globalThis : this);
