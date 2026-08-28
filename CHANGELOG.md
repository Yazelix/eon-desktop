# Changelog

## Unreleased

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
