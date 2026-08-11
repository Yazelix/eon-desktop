# Changelog

## Unreleased

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
  movement, keep Shift-drag gestures coherent through queue and resize recovery, rejected
  actions, competing pointer input, and Orbit-authored cancellation, render only
  authoritative selected presentation, and copy exact text returned by Orbit after the drag.
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
