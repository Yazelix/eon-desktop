# Changelog

## Unreleased

- Render one accepted Orbit session in a native Linux window with styled Unicode
  text, input-method preedit underlines sized from shaped text, stable blink
  timing, complete wide-cell cursor geometry, and conceal-aware drawing and
  accessibility.
- Send semantic keyboard, input-method, pointer, focus, and resize events to
  Orbit, preserving active keyboard layouts, synchronizing the latest pre-attachment focus,
  mapping all four wheel directions, requiring the current presented revision for
  pointer motion, wheel input, and new button presses, clamping captured motion to
  Orbit's coordinate bounds, and keeping held-button pairs coherent across presentation
  and queue recovery while clearing transient input on focus loss without owning the session or PTY.
- Normalize bounded wheel and trackpad input into Orbit-owned retained-history
  movement, keep active Shift-drag selection coherent through queue recovery and
  competing pointer input, render only Orbit-authored selection, and copy exact
  Orbit-returned text after the drag finishes.
- Explain bounded attachment, frame, and connection failures in the client and
  accessibility tree, preserve Orbit's input, protocol, and terminal failure classes,
  distinguish socket failures from invalid messages, retry interrupted reads,
  and keep unrelated traffic from erasing notices.
- Preserve resize and pointer readiness across queue and surface failures, report
  exact rendered grid geometry, and render Orbit colors in its declared space.
- Keep sustained complete-frame rendering within the accepted Linux CPU and
  memory envelope through superseded-frame replacement and cheap ASCII shaping
  while preserving revision failures, ordered acknowledgements, and Unicode.
