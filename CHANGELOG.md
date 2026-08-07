# Changelog

## Unreleased

- Render one accepted Orbit session in a native Linux window with styled Unicode
  text, input-method preedit underlines sized from shaped text, stable blink
  timing, cursor state, and conceal-aware drawing and accessibility.
- Send semantic keyboard, input-method, pointer, focus, and resize events to
  Orbit, preserving active keyboard layouts, mapping all four wheel directions,
  requiring the current presented revision for pointer motion, wheel input, and
  new button presses, clamping captured motion to Orbit's coordinate bounds, and
  keeping held-button pairs coherent across presentation and queue recovery while
  clearing transient input on focus loss without owning the session or PTY.
- Normalize bounded wheel and trackpad input into Orbit-owned retained-history
  movement, keep competing pointer input inside an active Shift-drag selection,
  render that Orbit-authored selection for sighted and accessibility users, and
  copy only explicit Orbit-returned text after the drag finishes.
- Explain bounded attachment, protocol, frame, input, and connection failures in
  the client and accessibility tree, retry interrupted reads, distinguish socket
  failures from invalid messages, and keep unrelated traffic from erasing them.
- Preserve resize and pointer readiness across queue and surface failures, report
  exact rendered grid geometry, and render Orbit colors in its declared space.
- Keep sustained complete-frame rendering within the accepted Linux CPU and
  memory envelope through superseded-frame replacement and cheap ASCII shaping
  while preserving revision failures, ordered acknowledgements, and Unicode.
