# Presentation and ownership

## Native window and workspace

Venus presents one native Wayland window on Linux. It can show one local Orbit
Session or an Eon workspace. In workspace mode Eon supplies the tabs, pane
order, selection, and popups; Venus draws the visible scene.

### Eon Bar

- Tabs show their one-based position and the directory leaf or `/` from Eon's tab
  launch directory. Long labels use middle ellipsis; hover shows the full path.
  Selection brightens the tab, and keyboard focus adds an outline.
- New tab sits beside the last tab and pins to the tab-region edge when tabs
  overflow. Shortcut help and Close stay at the right. A small empty area lets
  users drag an undecorated window without taking clicks from tabs or controls.
- When Eon supplies Codex quota facts, a read-only chip shows the OpenAI Blossom,
  elapsed/total window positions, and remaining percentages. It condenses to
  the lowest-remaining window, then disappears before tabs or controls lose
  space. Stale, blocked, and unknown states remain explicit; hover gives human
  time.

### Panes and popups

One pane expands in a vertical stack while every fitting pane header stays
visible. A rounded frame connects the stack; selection, hover, and keyboard
focus have distinct fills or outlines. Visible headers pair Eon's opaque pane
identity with Orbit's working directory. Home displays `~`, and its descendants
use `~/`; without `HOME`, paths stay absolute. Only the selected endpoint receives
terminal presentation and input.

A fresh workspace or tab can begin with a Project popup and no pane. Tool popups
and the chooser cover the stack in one rounded terminal surface with Eon's
logical margins. Tabs remain available. A tab with only hidden popups has an
empty body and keeps focus on its tab until a catalog shortcut reopens work.
Venus recovers retryable socket loss and detaches without ending Sessions.

The cursor tail follows pane and tab switches and workspace layout changes
within the window. During attachment it waits without drawing the old pane;
motion resumes toward the new pane's fresh cursor. Failures, hidden cursors,
focus loss, occlusion, native resize, and GPU recovery reset the effect.

## Ownership

```text
Eon                     -> product policy, composition, distribution
Eon Runtime             -> workspace mechanisms, canonical EONW
Eon Desktop / Venus     -> native presentation, interaction, client failure UX
Eon Sessions / Orbit    -> PTYs, terminal state, session lifetime, wire authority
```

Venus consumes EONW v7 through `eon-workspace-protocol` 0.1.0 at exact Eon Runtime
source `b8f18b4374ca0818a64d9cac6a3fcd02d9f2f2aa`. Eon supplies popup definitions,
commands and geometry defaults. The runtime validates configuration and owns
live workspace and popup state, Session mappings, and Codex quota collection
and normalization. Venus consumes
`orbit-protocol` 0.1.0, ORBF v2, and ORBS v13 at exact Orbit proof
`b6cecf8f2ee35570b41cfdc578b095889d917fe2`. One reducer turns complete canonical
frames into immutable scene data used by drawing and accessibility. The native
host owns the local socket, window, input mapping, and redraw lifecycle; it owns
no terminal state.

Each Eon workspace pane references an independent Orbit Session. Venus keeps
the EONW connection for workspace state
and actions, one presentation connection to the selected popup or pane, and one read-only
metadata observer for each visible live pane. Hidden and offline Sessions have no
Venus observer and remain alive independently.

## Platform and limits

This slice excludes arbitrary split trees, simultaneous expanded panes,
sidebars, popup command/lifetime policy, persistent Venus configuration,
background images, plugins, remote or web access, packaging, and distribution.
Blur strength and live blur changes are also outside its scope.

The proved Linux host uses winit, wgpu, glyphon, AccessKit, and wl-clipboard-rs
with native Wayland and Vulkan. The unsupported Apple Silicon host reuses the
same renderer/model path with AppKit, AccessKit, Metal, and target-only arboard.
Its opaque foundation and a bounded clipboard/key slice have native M1 proof;
held-key repeat, dead keys, other IMEs, physical trackpad gesture quality, a
host-access failure notice, lifecycle acceptance, effects, and Eon composition remain open.
