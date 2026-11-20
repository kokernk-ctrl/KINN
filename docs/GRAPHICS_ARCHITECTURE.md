# Graphics Architecture

**Status: planned. No graphics code ships in 0.1 — deliberately.**
This document records the committed plan; the first line of graphics
code lands in milestone 0.5, and only after its window/context spike is
verified on real hardware (desktop GL + one Android device).

## Principles

1. **One backend first.** OpenGL ES 3.0 is the only 0.5 target. Vulkan
   is planned *after* the GLES path is complete and verified (ADR-0010).
   We do not implement several backends before the first one works.
2. **Mobile-first capability set.** GLES 3.0 runs on Android and maps
   to desktop GL 4.1-core/ANGLE, so one API covers the machines
   learners actually have.
3. **C/C++ only where required.** Window/context creation and thin GL
   bindings live in a small C/C++ integration layer; everything above
   it (scene graph, resource management, the Kin graphics API) is Rust
   (ADR-0005 stack policy).
4. **Kin stays beginner-usable.** Graphics in Beginner mode means
   `window`, `clear`, `cube`, `camera` — one file, no shaders written
   by hand. Advanced mode exposes shaders, meshes and explicit GPU
   resource control as the API grows.

## Milestone 0.5 scope (in order)

1. **Window + context initialization** — EGL on Android/GBM, GLFW with
   a GLES 3.0 context on desktop; verified create/resize/destroy.
2. **Render loop** — vsync'd loop driven from the Kin runtime, clean
   teardown, frame metrics in A-mode diagnostics.
3. **Geometry** — vertex/index buffers, first primitives.
4. **Cubes and meshes** — built-in cube; mesh loading path decided by
   ADR at implementation time.
5. **Cameras and transformations** — perspective/ortho, model/view/
   projection matrices, `look-at`.
6. **Textures and materials** — KTX-first texture loading (format
   decision by ADR), material parameters.
7. **Shaders** — GLSL ES 3.00 pipeline stages; B-mode ships defaults,
   A-mode allows custom GLSL.
8. **GPU resource management** — handles, lifetimes, leak reporting;
   explicit release in A-mode.

Each step lands with tests where testable (math, resource logic) and
device verification logs where not (context creation), tracked in
PROBLEM_REGISTRY until closed.

## Android constraints (stated honestly)

- **Memory:** graphics workloads must budget for 2 GB-class devices;
  resource limits are part of the API design, not an afterthought.
- **Storage:** assets and toolchains live inside the app's private
  storage; no assumptions about shared storage permissions.
- **Permissions:** windowing via EGL needs no special permissions;
  anything requiring them (camera AR paths later) must be optional.
- **Toolchains:** on-device *compilation* is bounded by NDK
  availability and disk. We do not claim every desktop target can
  compile locally on Android. The verified 0.5 target is: Kin programs
  *run* on Android (KIR VM + native graphics); on-device compilation of
  Kin to native code is a later, explicitly-verified goal.

## Vulkan (later)

Revisit after GLES 3.0 is complete: swapchain management, SPIR-V
shader compilation (hand-written GLSL first), and a backend trait so
Kin code does not change when the backend does. The backend seam is
designed into the 0.5 API from day one (trait `GfxBackend`), but no
Vulkan code will exist until GLES is done.

## What will NOT be in 0.5

- Ray tracing, compute, VR.
- A visual scene editor inside Studio (candidate for post-1.0).
- Multi-window support.
