# Venus crate decisions

No dependency is selected and no Venus manifest exists. `ven-upt.1` completed
the comparison below and recommends one shape; explicit user acceptance is
still required before its status changes to selected or any manifest is added.

| Boundary | Recommended shape | Status | Owner consequence |
|---|---|---|---|
| Orbit protocol consumer | Exact Git revision `41894ed7e7ed22ae278e5d8feb22a1c85b589440` of the dependency-free, publish-false `orbit-protocol` 0.1.0 package | Orbit boundary accepted; Venus dependency awaiting approval | Orbit alone owns ORBS v1, ORBF v1, semantic values, bounds, and revision reduction. Venus keeps no mirror or adapter. |
| Native host | winit 0.30.13 with X11, Wayland, dynamic Wayland loading, and raw-window-handle 0.6 | Recommended; awaiting approval | The host owns window and event-loop lifecycle, native input and IME collection, resize, surface recovery, socket scheduling, and bounded client failure UX. |
| GPU and text | wgpu 30.0.0 with Vulkan, Metal, and WGSL; glyphon 0.12.0 with its cosmic-text 0.19.0 re-export; pollster 1.0.1 for bounded initialization | Recommended; awaiting approval | Venus owns a small rectangle/decorations pipeline. Glyphon owns shaping, fallback, clipping, raster cache, atlas, and text preparation. Neither sees transport or terminal state. |
| Accessibility | AccessKit 0.24.1 and accesskit_winit 0.33.2 with the Unix async-io adapter | Recommended; awaiting approval | Venus derives the accessibility tree from the same immutable, revision-tagged scene that supplies draw and hit inputs. |

## Measured comparison

Minimal Rust 2024 scratch binaries were resolved and checked on Rust 1.96.0.
Counts include the scratch root; lock counts include cross-target entries, while
tree counts are unique Linux normal/build output lines.

| Complete shape | Exact releases | Lock packages | Linux tree | Disposition |
|---|---|---:|---:|---|
| winit + wgpu + glyphon + AccessKit | 0.30.13, 30.0.0, 0.12.0, 0.24.1/0.33.2 | 324 | 267 | Recommended. Four packages over the owned-atlas shape remove its highest-risk custom subsystem. |
| winit + wgpu + cosmic-text + owned atlas | 0.30.13, 30.0.0, 0.19.0 | 320 | 262 | Rejected initially. It adds an estimated 700–1,200 specialized atlas, shader, upload, and cache LOC. |
| winit + softbuffer + cosmic-text + tiny-skia + AccessKit | 0.30.13, 0.4.8, 0.19.0, 0.12.0 | 297 | 232 | Rejected initially. It makes HiDPI composition and full-frame upload CPU work and weakens the intended GPU path. |
| winit + Vello + Parley + AccessKit | 0.30.13, 0.9.0, 0.11.0 | 336 | 301 | Rejected. It imports general vector/rich-layout policy, ICU and native fontconfig, and currently uses wgpu 29 rather than 30. |

All four shapes passed `cargo check --locked`. The recommended features avoid
wgpu defaults: Linux starts with Vulkan, macOS remains credible through Metal,
and GLES is conditional on a measured compatibility failure. X11, Wayland,
xkbcommon, the Vulkan loader/driver, font discovery, and AT-SPI/D-Bus are the
relevant Linux runtime/Nix surfaces; the stack adds no C++, Zig, or vendored
native rendering engine.

Full UI frameworks Iced 0.14.0 and Slint 1.17.1 own unnecessary widget,
layout, runtime, and renderer policy for one custom surface. Skia-safe 0.99.0
adds a large C++ binary/build surface; femtovg 0.26.0 makes OpenGL and another
text stack architectural. Sugarloaf at Rio 0.4.5 remains conditional on a
proved glyphon failure. Ghostling and libghostty-derived shapes import terminal
authority or demo-grade game rendering. A completely owned stack would
duplicate windowing, GPU, shaping, font fallback, accessibility, and future
macOS work.

## Proposed owner seam

One pure scene reducer maps canonical complete Orbit frames to immutable,
revision-tagged quads, exact grapheme runs, cursor data, approved hit geometry,
and accessibility inputs. It owns no socket, window, GPU, or terminal handle.
The native host maps winit events only into `orbit-protocol` values. The
renderer consumes scenes and advances candidate scene, hit, and accessibility
state to last-presented state only after a successful present.

The first slice rebuilds and redraws the complete scene. Damage is derived only
after measurement and never becomes wire state. Expected owned size is
1,500–2,400 production LOC plus 900–1,400 focused test and fixture LOC.

If corpus and native checks show that glyphon cannot preserve exact graphemes,
cell alignment, clipping, color glyphs, or acceptable sustained redraw latency,
retain the protocol, scene, winit, wgpu, and AccessKit owners and reopen only
the text-renderer gate against an owned cosmic-text atlas and current
Sugarloaf. Do not replace the application architecture.
