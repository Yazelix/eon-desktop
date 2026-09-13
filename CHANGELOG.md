# Changelog

## Unreleased

- Add an Alt+/ native shortcut viewer for the Eon surface. Fixed Venus
  bindings share one owner with dispatch, enabled Project/tool rows follow the
  live Eon catalog, and bounded scrolling keeps the dialog usable at narrow
  sizes. Escape or Alt+/ closes it without sending terminal input or workspace
  actions, and AccessKit exposes the grouped rows as a modal help dialog.

- Number workspace tabs by their current position and update those numbers after
  reordering. Alt+1 through Alt+9 select positions 1 through 9; Alt+0 selects
  position 10. Stable Eon tab identities remain internal to actions and
  accessibility nodes.

- Consume exact EONW v5 through one shared popup surface for tools and Project:
  rounded stack-covering geometry, compact labels, translucent margins, catalog
  shortcuts with exact targets, actionable tabs, and empty popup-only bodies.
  Popup outlines retain the pane stack's exact edges across toggles. Empty bodies
  keep keyboard and accessibility focus on the selected tab. A small top gutter
  separates each border label from the first terminal row.
  Eon command/lifetime policy and composed v5 activation remain downstream.

- Keep routine workspace attachment and first-frame progress silent while
  retaining actual failures and standalone connection progress.

- Show authoritative scrollback distance as `↑ N rows` in the selected pane
  header, or a small standalone/picker overlay. Hide it at live output and
  expose committed position as an accessible description. Overlays yield to
  selection, link previews, notices, tab previews and overlapping cursors.
  Consume exact Orbit ORBF v2 / ORBS v11; older session versions are rejected.
  Preserve the directory ending beside the count. Idle cleanup and rejected
  input keep the count hidden during terminal-owned scrolling.

- Connect accordion panes within one rounded outer frame, with full-width
  separators, a small bottom gap with terminal background color/opacity, and a
  lighter selected header whose fill follows the shared stack corners. Keep hover and focus cues;
  `--pane-frames false` hides the shared frame and separators.

- Give each workspace pane a thin rounded frame and quieter header by default.
  Accept `--pane-frames true|false`; hiding frames preserves grid dimensions,
  pane controls, hover feedback and rounded keyboard-focus indicators.
  Selected offline pane labels stay distinct with decorative frames off.
  Resize the terminal when a pane-list change retains its selected attachment.

- Give workspace tabs pill-shaped corners and remove the selection underline.
  Preserve selected fill, bright labels and a distinct rounded focus outline.

- Use rounded, separated workspace tabs sized to their shaped directory labels.
  Cap long tabs with middle ellipsis, expose the launch path on hover, and
  preserve horizontal scrolling, selection reveal and distinct keyboard focus.
  Keep a fitting tab number visible when the name and ellipsis cannot fit.
  Scroll across the whole tab strip, including padding and gaps between tabs.

- Reduce per-cell text-buffer allocations by reserving one line while preserving
  glyph selection and cell geometry.

- Admit the actual supervised window before reporting `stdin-ready-v1`
  readiness, so Eon can validate fonts and initial native geometry before
  starting a user command. Workspace admission consumes the canonical snapshot;
  existing presentation commands and EOF shutdown remain on the same stream.

- Validate font-only workspace geometry before attachment; reject typography
  that leaves no terminal cell in the initial window. Complete native startup
  without waiting for keyboard input or an unrelated event.

- Accept startup font family, ordered fallback families, font size, line height,
  and initial columns/rows through VEN-C19. Preserve defaults, share one physical
  cell grid, include workspace/picker overhead, and reject invalid settings or
  impossible native sizing before terminal attachment.

- Preview explicit OSC 8 targets on hover with only two actions: Ctrl+click opens
  HTTP/HTTPS and Ctrl+Shift+C copies the hovered target unless terminal text is
  selected. Preserve ordinary terminal mouse behavior, reject stale targets and
  unsafe opening, and expose each current visible target as an Open link and
  Copy button through accessibility. Opening uses host GIO.

- Use Alt+Shift+T to open a new tab and Alt+Shift+W to close the active
  non-final tab, matching Nova. Ctrl+T and Ctrl+Shift+W reach terminal programs.

- Keep long workspace directory labels on the visible header line, including
  underscores, instead of wrapping the name below its clip bounds.
- Keep scrolling, selection, and tab/pane focus admitted across compatible
  terminal output while refreshing rendered content. Consume Orbit
  `91999d79546422b49bdbc124166a65859d0bd872` for nonfuture selection admission;
  geometry, attachment, and authoritative Finish presentation gates remain.
  Installed Eon/EonTerm acceptance at Eon
  `91c6ed5d51b5a09d5c9e1d2e30aebc191c10223f` covers native history, selection,
  copy, and workspace focus with this exact Venus source.
- Complete `VEN-C18` rapid-picker-reopen proof with unchanged Venus
  `e13970e90289d0d86f0adcbf350e4b9c1d5e5219` and Eon's distinct endpoints at
  `4298fbb8868752e3d6c8eb4fd79fae067ab3e2a1`. Older compositions that reuse
  endpoints retain the known attachment failure; fractional scale stays open.

- Keep active precision gesture and kinetic scroll state across a transient GPU
  texture timeout without weakening occluded-surface cancellation, so the
  skipped visual sample does not truncate momentum during live output.
- Consume accepted Orbit `a65e199e16e97330175e314cacf791fa00f53069`
  under unchanged ORBS v10 so its bounded output queue reserves one atomic
  scroll outcome behind a partially transmitted frame. Venus behavior remains
  unchanged; Eon accepts the exact pair separately.
- Consume Eon's stable-target tab close and workspace movement actions.
  Ctrl+Shift+W closes the expected active non-final tab, Ctrl+Alt+H/L moves the
  active tab, and Ctrl+Alt+K/J moves the selected pane without a mode; exact
  presses and releases remain outside terminal input.
- Consume EONW v4, admit workspace-only startup through `--workspace`, and
  project Eon's initial, new-tab, or Alt+Z directory picker as the only tab-body
  terminal with one-cell gutters, modal input, and AccessKit focus only on its
  bound active tab. Alt+H/L traverses to durable tabs without ending the picker,
  and returning restores the same picker while retaining the tab bar and prior
  workspace focus.
- Consume EONW v2 tab launch directories, render directory-derived numbered tab
  labels, retain stable `tN` action targets, and expose bounded full-path context
  through AccessKit.
- Scale precision retained-history input to twice its native pixel distance,
  continue phase-complete gestures with bounded elapsed-time momentum, move
  discrete wheel steps by three rows without synthetic inertia, coalesce crossed
  rows through one Orbit batch, admit only its matching outcome, retain bounded
  multi-row previews across advancing frames, commit against their exact lagging
  revisions without zero-offset shakes or input stalls, retire transient
  terminal-routing previews on every plain frame and bounded previews after
  changes to screen, geometry, default cell colors, or palette, keep an entered
  history viewport pinned, and pace active redraws through native Wayland
  compositor callbacks.
- Accept one bounded caller-owned application ID before native Wayland window
  creation, preserving `eon` as the direct default and keeping terminal titles
  independent from launcher grouping.
- Show each visible live pane's exact Eon identity, two spaces, then a
  `~/`-anchored path below `HOME`, an absolute path elsewhere, or Nova's marker
  at exact `HOME`; unset or empty `HOME` leaves paths absolute.
  Overlong labels preserve their rightmost components, while terminal titles
  stay in the selected native window without title-only pane redraws.
- Make Venus native Linux Wayland/Vulkan-only, remove X11 and Metal features and
  unsupported compatibility calls, and replace arboard's X11 fallback with
  direct Wayland data-control clipboard ownership.
- Admit AccessKit tab and pane actions against the latest published workspace
  tree during GPU occlusion or recovery, and change local focus only after Eon
  queue admission while retaining presented-composite input gating.
- Keep visible, nonblank Orbit cells atomic in accessibility text and selection,
  and expose an otherwise unrepresentable greater-than-255-byte cell as one
  replacement unit without changing its visual text or Orbit's protocol.
- Bound retained decoded Orbit server events by their validated framed byte
  lengths, preserving ordered effects and frame replacement while failing once
  through the existing explicit loss path when the byte ceiling is exceeded.
- Align AccessKit terminal rows and workspace descendants with the exact
  geometry and clipping already used by rendering and native hit testing.
- Report callback-observed whole-device GPU loss through the bounded renderer
  failure path, including OOM reports that would otherwise panic, while
  preserving ordinary surface-only recovery.
- Report a rejected non-empty native input-method commit through the existing
  bounded Input notice instead of silently dropping it.
- Keep AccessKit tab and pane identities stable across Eon topology changes so
  delayed actions for removed nodes cannot select a sibling.
- Preserve still-held native modifiers and ordinary key pairing when workspace
  modifiers change, while retiring stale key, button, shortcut, composition,
  selection, and focus state before another Orbit attachment receives input.
- Bind renderer cache reuse, workspace actions, and terminal coordinate or
  selection input to one successfully presented composite generation across
  attachment, workspace, scrolling, resize, recovery, and failure transitions.
- Expose a persistent renderer failure immediately through the native window
  title, stderr, and AccessKit without waiting for another GPU frame, then
  restore Orbit's accepted title after the next successful presentation.
- Exit supervised Venus when its private presentation stream closes without
  stopping Orbit, and let only an immediate supervised replacement retry the
  departing client's transient Busy response.
- Keep transport-loss state independent from retry policy: every ended Orbit
  transport becomes explicitly non-attached, while Protocol and Terminal
  failures still suppress retry and retain only a current constraining notice.
- Preserve Orbit's flushed authoritative terminal completion when concurrent
  Venus input observes socket loss, while retaining prompt bounded loss when no
  complete authoritative terminal message is available.
- Render supported packaged Nerd Font symbols completely through adjacent blank
  cells without moving or covering later nonblank cells or changing geometry
  between regular and bold terminal presentation.
- Consume exact canonical ORBS v4 and its accepted ORB-C12 Orbit revision,
  mapping typed whole-row wheel outcomes into the existing result and frame
  owners while retaining explicit protocol-incompatibility UX and adding no
  preview or kinetic interaction.
- Animate an omitted cursor profile with the documented `#89b4fa` tail at
  duration `1.0`, while retaining explicit `none` and strict custom profiles.
- Shape each complete terminal cell at its Orbit-authored grid start so complex
  Unicode cannot displace later cells or delimiters.
- Accept `--background-blur` as one best-effort full-surface native compositor
  request, prove it on COSMIC Wayland, and keep opacity, scene layers, input,
  input methods, accessibility, and compositor-owned blur policy independent.
- Accept a strict versioned launch profile for a static cursor or one bounded
  Rio-style cursor tail, using a single trail color and duration multiplier
  while preserving Orbit's authoritative cursor state and stopping redraws when
  motion settles, the window loses focus, or the surface is occluded.
- Accept a finite `--background-opacity` value from `0.0` through `1.0`, apply
  it only to the terminal default background, and keep explicit cell
  backgrounds, workspace chrome, selection, inverse video, text, and cursors
  independent from that launch setting.
- Use a nominal 16 px terminal font with one shared 10×18 logical cell grid so
  rendering, resizing, pointer mapping, input methods, and accessibility retain
  coherent geometry at supported display scales.
- Preserve the existing supervised Venus window and request native presentation
  when Eon presents the same live generation again.
- Render adjacent Unicode full-block cells as exact cell rectangles so terminal
  graphics meet without visible seams.
- Read ordinary native clipboard text on Ctrl+Shift+V or the Paste key and send
  one bounded semantic paste to Orbit for authoritative terminal-mode encoding,
  while retaining each accepted shortcut through its matching release.
- Deliver bounded Orbit terminal clipboard-write effects through the existing
  native text clipboard owner, preserving standard and Linux primary targets.
- Render Unicode Braille cells with distinct dots while preserving adjacent
  text, box-drawing cells, and authoritative cell-grid bounds.
- Render terminal `g`, `j`, `p`, `q`, and `y` descenders in regular, bold, and
  italic text inside each cell row while preserving grid advances and clipping.
- Keep Eon workspace pane headers focused on pane identity and offline state
  while retaining Session mappings in EONW and Eon diagnostics.
- Preserve Shift-produced layout text, including punctuation and non-ASCII
  characters, through Kitty keyboard mode without changing modified shortcuts.
- Accept `--no-decorations` at launch while retaining decorated windows as the
  default.
- Materialize accepted Eon workspace snapshots as a horizontally scrollable tab
  strip and a vertically scrollable accordion that shows every fitting pane
  header around one expanded terminal, route pointer, Alt+H/L and Alt+K/J
  traversal, Alt+M pane creation, and Ctrl+T tab creation through EONW v1,
  attach only the selected Orbit endpoint, re-inspect Eon every 250 ms so
  external workspace changes appear without native input, preserve the last
  coherent view on failures, and expose matching AccessKit order and actions.
- Render one accepted Orbit session in a native Linux window with exact
  cell-grid advances for plain and styled Unicode text, connected table borders,
  cursor-anchored shaped input-method preedit text and underlines, aligned
  wide-cell cursor and input-method geometry, stable blink timing, and
  conceal-aware drawing and accessibility.
- Send semantic keyboard, input-method, pointer, focus, and resize events to
  Orbit, preserving active keyboard layouts, synchronizing the latest pre-attachment focus,
  mapping all four wheel directions, requiring the current presented revision for
  pointer motion, wheel input, and new button presses, clamping captured motion to
  Orbit's coordinate bounds, and keeping held-button pairs coherent across presentation
  and queue recovery while clearing transient input on focus loss without owning the session or PTY.
- Normalize bounded wheel and trackpad input into Orbit-owned retained-history
  movement; route cell, word, and logical-line gestures through Orbit while
  preserving terminal mouse capture with Shift override; retain native gesture
  order through queue and resize recovery; and automatically copy a successful
  selection to both Wayland clipboard targets.
- Explain bounded attachment, frame, and connection failures in the client and
  accessibility tree, preserve Orbit's input, protocol, and terminal failure classes,
  distinguish retryable socket failures from terminal invalid messages and resource
  failures, retry interrupted reads, automatically recover the same selected live
  endpoint with bounded backoff while retaining the last coherent scene, and keep
  unrelated traffic from erasing notices.
- Preserve resize and pointer readiness across queue and surface failures, report
  exact rendered grid geometry, and render Orbit colors in its declared space.
- Keep sustained complete-frame rendering within the accepted Linux CPU and
  memory envelope through superseded-frame replacement and cheap ASCII shaping
  while preserving revision failures, ordered acknowledgements, and Unicode.
