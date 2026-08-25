# Venus crate decisions

`ven-upt.1` records the user-selected thin winit, wgpu, glyphon, and AccessKit
shape. `ven-upt.2` implements it with exact direct versions and features in
`Cargo.toml`. `ven-4sn` adds the minimum native text-clipboard owner.
`ven-consume-terminal-clipboard-writes-zgh` advances the canonical session
consumer to ORBS v3. `ven-native-clipboard-paste-uas` reuses that clipboard
owner for explicit reads. `ven-c87` adds the exact Eon-owned EONW v1 consumer
without a local protocol mirror. `ven-adopt-orbs-v4-without-interaction-expansion-zdc`
advances the same canonical session consumer to ORBS v4.
`ven-render-live-pane-title-cwd-8c7` advances it to ORBS v5 for read-only pane
metadata without another schema or runtime dependency.
`ven-venus-linux-wayland-only-ndj` selects native Linux Wayland and Vulkan as
the sole host and replaces the clipboard adapter's X11 fallback with its
already-locked Wayland data-control owner.

| Boundary | Selected shape | Status | Owner consequence |
|---|---|---|---|
| Eon workspace protocol consumer | Exact Git revision `4af395aea06c230ee6b18cf0755ae25915c0b88d` of the dependency-free, publish-false `eon-workspace-protocol` 0.1.0 package | Active for internal development | Eon alone owns EONW v1 values, validation, topology, selection, actions, and endpoint mappings. Venus owns only the Unix request worker and native projection. The user authorized Apache-2.0, matching Nova, if Eon needs a public license; the exact Eon revision has no durable license record, so public distribution remains blocked until Eon records it. |
| Orbit protocol consumer | Exact Git revision `780f5d746175b4a9b71df57c51ed4bfcc4c4c375` of the dependency-free, publish-false `orbit-protocol` 0.1.0 package | Active | Orbit alone owns ORBS v6, ORBF v1, semantic values, history, selection, typed wheel outcomes, signed scroll batches, adjacent-row previews, copied text, terminal clipboard effects, read-only pane metadata, bounds, revision reduction, and the accepted lifecycle contracts. Venus keeps no mirror terminal schema or viewport authority; one private kinetic state consumes exact preview and batch outcomes. |
| Native host | winit 0.30.13 with Wayland, dynamic Wayland loading, and raw-window-handle 0.6 only, patched to exact `chiyuki0325/winit-0.30` commit `fb45fbf901fbe70cc9a877b5d651d0b60c206b08` | Active on native Linux Wayland | The host owns window and event-loop lifecycle, native input and IME collection, compositor blur protocol selection, resize, surface recovery, socket scheduling, and bounded client failure UX. X11 and Xwayland compatibility are unsupported. Replace the patch with the first accepted stable winit containing upstream `c4afadbfabf7b1e7989b40b493db1a4c7bd8ff4e`. |
| GPU and text | wgpu 30.0.0 with Vulkan and WGSL only; glyphon 0.12.0 with its cosmic-text 0.19.0 re-export; pollster 1.0.1 for bounded initialization | Active on Linux/Vulkan | Venus owns a small rectangle/decorations pipeline. Glyphon owns shaping, fallback, clipping, raster cache, atlas, and text preparation. Neither sees transport or terminal state. |
| Accessibility | AccessKit 0.24.1 and accesskit_winit 0.33.2 with the Unix async-io adapter | Active | Venus derives native accessibility updates from each accepted immutable scene without creating another presentation model. |
| Native text clipboard | wl-clipboard-rs 0.9.3 with default features disabled | Active on native Linux Wayland | The host reads ordinary clipboard text once after an explicit paste shortcut and writes canonical bounded `CopiedText` and `ClipboardWrite` effects through Wayland data-control. Orbit owns paste encoding and terminal text. Missing data-control remains a visible bounded failure; there is no X11 fallback. |

The ORBS v5 advance keeps one direct package with no transitive, native, build,
feature, or Nix change. Orbit's canonical codec owns bounded metadata values and
the read-only observation role; Venus sends only `ObserveMetadata` and keeps the
latest accepted update per visible endpoint. Staying on v4 cannot observe the
accepted producer, while a backport, dual decoder, or adapter would duplicate
Orbit's schema. Replacement remains one exact pin plus exhaustive consumer matches.
The unpublished Orbit source still records no durable license metadata, so this
internal dependency decision makes no public-distribution claim.

## Measured comparison

Minimal Rust 2024 scratch binaries were resolved and checked on Rust 1.96.0.
Counts include the scratch root; lock counts include cross-target entries, while
tree counts are unique Linux normal/build output lines. The implemented lock
contains 324 packages including Venus, `eon-workspace-protocol`, and
`orbit-protocol`; its current Linux
normal/build tree has 272 unique lines.

| Complete shape | Exact releases | Lock packages | Linux tree | Disposition |
|---|---|---:|---:|---|
| winit + wgpu + glyphon + AccessKit | 0.30.13, 30.0.0, 0.12.0, 0.24.1/0.33.2 | 324 | 267 | Selected. Four packages over the owned-atlas shape remove its highest-risk custom subsystem. |
| winit + wgpu + cosmic-text + owned atlas | 0.30.13, 30.0.0, 0.19.0 | 320 | 262 | Text fallback only. It adds an estimated 700–1,200 specialized atlas, shader, upload, and cache LOC. |
| winit + softbuffer + cosmic-text + tiny-skia + AccessKit | 0.30.13, 0.4.8, 0.19.0, 0.12.0 | 297 | 232 | Rejected as the primary renderer. It makes HiDPI composition and full-frame upload CPU work and weakens the intended GPU path. |
| winit + Vello + Parley + AccessKit | 0.30.13, 0.9.0, 0.11.0 | 336 | 301 | Rejected. It imports general vector/rich-layout policy, ICU and native fontconfig, and uses wgpu 29 rather than 30. |

## Ranked tradeoffs

1. **winit + wgpu + glyphon + AccessKit** best fits the first Venus slice.
   Glyphon removes the risky custom atlas while preserving a narrow scene and
   renderer seam. The costs are a 324-package lock, wgpu device and surface
   recovery, and a required native proof for glyph fidelity and redraw latency.
2. **winit + wgpu + cosmic-text + an owned atlas** gives Venus control over
   cell placement, cache policy, uploads, and batching. It saves four lock
   packages but adds an estimated 700–1,200 specialized GPU and cache LOC. Use
   it only after a focused glyphon failure.
3. **winit + softbuffer + cosmic-text + tiny-skia + AccessKit** removes the GPU
   requirement and exposes a direct CPU framebuffer with damage support. CPU
   rasterization, HiDPI bandwidth, platform-dependent presentation copies, and
   a later renderer replacement make it weaker as the product architecture.
4. **winit + Vello + Parley + AccessKit** offers a rich vector scene and text
   layout foundation. Its alpha renderer, compute requirement, wgpu version
   skew, and general UI policy exceed the accepted one-window surface.

All four shapes passed `cargo check --locked`. The selected host enables only
Wayland and dynamic Wayland loading; the selected wgpu features enable only
Vulkan and WGSL. Wayland, xkbcommon, the Vulkan loader/driver, font discovery,
and AT-SPI/D-Bus are the relevant Linux runtime/Nix surfaces; the stack adds no
C++, Zig, or vendored native rendering engine.

Full UI frameworks Iced 0.14.0 and Slint 1.17.1 own unnecessary widget,
layout, runtime, and renderer policy for one custom surface. Skia-safe 0.99.0
adds a large C++ binary/build surface; femtovg 0.26.0 makes OpenGL and another
text stack architectural. Sugarloaf at Rio 0.4.5 remains conditional on a
proved glyphon failure. Ghostling and libghostty-derived shapes import terminal
authority or demo-grade game rendering. A completely owned stack would
duplicate windowing, GPU, shaping, font fallback, and accessibility.

## Native background blur decision

Stable winit 0.30.13 exposes blur but binds only KDE's Wayland protocol. The
selected exact fork commit is one commit above that release and backports the
merged upstream `ext-background-effect-v1` implementation: 200 additions and
30 deletions across eight winit files, with no new Venus dependency, native
library, build tool, service, unsafe code, or event-loop owner. One winit remains
shared with `accesskit_winit`.

The Git workspace resolves `dpi` 0.1.1 instead of registry 0.1.2. The older
source lacks only the later no-std and inset additions, which neither winit
0.30.13, AccessKit, nor Venus consumes. The locked package and Linux tree counts
remain unchanged. Direct Wayland protocol ownership, upstream master, and a
winit replacement were rejected because each adds substantially more lifecycle
and portability cost for the same request. Exact Venus proof
`7fc7e4ba97aaf48b586002934a580ef2d1c31694` resolves one shared winit, passes
the complete locked suite, and visibly applies blur on COSMIC Wayland 1.0.0 at
`091583ac84abac02967ae358cf9570ddfef63b31`. Other compositors remain best
effort because the public API exposes neither capability nor acceptance.

## Native clipboard decision

wl-clipboard-rs 0.9.3 owns the operation the Rust standard library does not
provide: read and host native plain-text clipboard values through Wayland
data-control. The package was already locked behind arboard, so selecting it
directly adds no resolved package and removes arboard's unconditional X11
implementation and fallback. Venus maps Orbit's standard destination to the
regular clipboard and selection or primary to the primary clipboard.
Ctrl+Shift+V or the native Paste key reads at most the canonical 1 MiB limit
plus one byte, validates UTF-8, and sends one bounded paste to Orbit. Missing
data-control remains a visible bounded failure.

External `wl-copy`, `xclip`, and `xsel` commands were rejected as undeclared
runtime dependencies. Retaining arboard was rejected because its Linux backend
always carries and can fall back to X11. smithay-clipboard would add another
package and raw-display lifecycle for no current contract gap. Handwritten
Wayland ownership was rejected as unsafe protocol code for one bounded effect.
A future browser client replaces this isolated effect with the browser
clipboard API rather than carrying a native backend across the boundary.

## Eon workspace protocol decision

The exact Eon owner package adds one direct lock entry and no transitive, native,
or build dependency. Its 496 production lines replace no acceptable Venus code:
copying them would duplicate every EONW tag, bound, identity rule, and snapshot
invariant. Parsing Eon CLI output, shelling out, and consuming the older private
control format remain rejected. Replacement is one source-pin edit if EONW is
later published unchanged; an adapter or local mirror is not a replacement path.

## Selected owner seam

One pure scene reducer maps canonical complete Orbit frames to immutable,
revision-tagged quads, exact grapheme runs, cursor data, and accessibility
inputs. It owns no socket, window, GPU, or terminal handle.
The native host maps winit events only into `orbit-protocol` values, keeps
transport outside the renderer, and sends typed wakeups through
`EventLoopProxy`. The selected shape adds no general async application runtime.
The same host owns one lazy native clipboard handle. It reads after an explicit
paste shortcut and writes the exact `CopiedText` or `ClipboardWrite` effect
returned by Orbit.

Venus derives drawing, accessibility, and deterministic contract snapshots from
the latest accepted scene. After a successful GPU present, the renderer
enables pointer events until resize or surface recovery invalidates that
presentation. AccessKit may publish the accepted scene while the surface is
occluded or recovering because assistive technology must not wait for GPU
presentation. Both views derive from the same scene and keep no independent
content model.

The first slice rebuilds and redraws the complete scene. Damage is derived only
after measurement and never becomes wire state. The implementation is smaller
than the gate estimate; the README scorecard records the exact owned size.

If corpus and native checks show that glyphon cannot preserve exact graphemes,
cell alignment, clipping, color glyphs, or acceptable sustained redraw latency,
retain the protocol, scene, winit, wgpu, and AccessKit owners and reopen only
the text-renderer gate against an owned cosmic-text atlas and current
Sugarloaf. Do not replace the application architecture.
