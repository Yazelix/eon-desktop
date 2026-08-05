# Changelog

## Unreleased

- Render one accepted Orbit session in a native Linux window with styled Unicode
  text, stable blink timing, cursor state, and conceal-aware drawing and
  accessibility.
- Send semantic keyboard, input-method, pointer, focus, and resize events to
  Orbit, preserving active keyboard layouts, mapping all four wheel directions,
  keeping held-button pairs coherent across presentation and queue recovery,
  and clearing transient input on focus loss without owning the session or PTY.
- Explain bounded attachment, protocol, frame, input, and connection failures
  accurately in the client window and accessibility tree without letting
  unrelated presentation, resize, or response traffic erase them.
- Preserve resize and pointer readiness across queue and surface failures, and
  render Orbit colors in the surface's declared color space.
