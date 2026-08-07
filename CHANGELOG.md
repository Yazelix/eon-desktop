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
  movement, render Orbit-authored Shift-drag selection, expose that same
  selection to accessibility, and place only explicit Orbit-returned text on
  the native clipboard with Ctrl+Shift+C.
- Explain bounded attachment, protocol, frame, input, and connection failures
  accurately in the client window and accessibility tree without letting
  unrelated presentation, resize, or response traffic erase them.
- Preserve resize and pointer readiness across queue and surface failures, and
  render Orbit colors in the surface's declared color space.
- Keep sustained complete-frame rendering within the accepted Linux CPU and
  memory envelope through superseded-frame replacement and cheap ASCII shaping
  while preserving revision failures, ordered acknowledgements, and Unicode.
