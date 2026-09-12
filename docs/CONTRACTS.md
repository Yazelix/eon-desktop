# Venus contract index

This is the canonical current state of Venus behavior, ownership, proof, and
remaining limitations. Beads and Git retain execution history; `CHANGELOG.md`
retains accepted user-visible chronology.

`VEN-C16` owns platform scope. The current locked consumer uses ORBF v2 /
ORBS v11 at Orbit `ea9fd28ce0908f218cf65d4e6df368f0a4e565f5`, with source
acceptance indexed in `VEN-C7`. Older consumed revisions below identify their
named proofs; they do not select the current dependency.

## VEN-C19 — Startup typography and terminal geometry

- **Status:** Proved
- **Consumer:** One direct or caller-launched Venus surface.
- **Trigger:** Optional `--font-family`, repeatable `--font-fallback`,
  `--font-size`, `--line-height`, `--columns`, and `--rows` startup options.
- **Result:** Installed primary monospace and ordered fallback families feed the
  existing shaper. One `CellMetrics` grid owns rendering, resize, pointer and
  selection geometry, scrolling, cursor, IME and accessibility. Family names
  are nonempty trimmed UTF-8 without controls, at most 128 bytes; at most eight
  fallbacks are accepted. Nominal font size is 6–96 logical px (default 16);
  line height is 1–3 times that size (default 1.125). Named overrides precede
  specialty/platform fallback. No font override preserves existing font choice
  and specialty handling; no overrides preserve 10 by 18 logical cells and the
  960 by 600 logical initial window.
- **Initial geometry:** Optional positive `u16` columns/rows request terminal
  dimensions, with workspace headers, stack bottom margin and popup margins
  supplied by the existing Scene owner. Omitted dimensions retain the existing
  initial window dimension.
  Requests must fit Orbit's 100,000-cell limit and native pixel/GPU bounds.
  Compositor sizing policy may override the request; actual resize remains
  authoritative. Later user resizing is not constrained to the initial grid.
  Stack bottom-margin accounting is accepted at source
  `bc5a2bd3b2363abdea69c4cd89953b612bc970e8`: the existing initial-grid check
  passes for standalone/workspace/picker at simulated scales 1/1.25/1.5/2.
  Native installed proof is indexed with the pane-stack polish below; previous
  typography and fractional-scale qualifications remain in force.
- **Important failures:** Invalid values, duplicate singleton options, absent
  named families, a non-monospace primary, or impossible initial dimensions
  fail with bounded diagnostics before terminal presentation/attachment.
  Native output admission can require an initial window buffer to learn scale.
  Missing named fonts are not silently substituted. Availability does not
  promise every Unicode glyph or style variant. No live reload is provided.
- **Owner:** Venus launch, font fitter, `CellMetrics`, and workspace Scene;
  Orbit retains terminal authority and Eon retains product configuration.
- **Consumes:** ORBF v2 / ORBS v11 at Orbit
  `ea9fd28ce0908f218cf65d4e6df368f0a4e565f5`, EONW v5 at Eon
  `0cc8f477298681ae3945903e8fdb5852d487c5ab`, and the selected glyphon 0.12.0 /
  cosmic-text 0.19.0 APIs with exact unicode-script 0.5.8.
- **Compatibility order:** Prove Venus first with unchanged defaults and wire
  contracts, then let the separate Eon configuration issue consume that exact
  accepted source for Eon and EonTerm.
- **Supervised startup admission:**
  `EON_VENUS_PRESENTATION_CONTROL=stdin-ready-v1` uses private stdin/stdout.
  Workspace mode first reads one bounded canonical EONW v5 Snapshot from stdin;
  standalone mode has no snapshot prefix. After font/renderer initialization
  and real-window initial native-scale admission, Venus writes exactly
  `ready-v1` (eight bytes) and starts attachment. The caller may then start
  Orbit and the user command. Invalid fonts, geometry or snapshot produce no
  readiness; the caller bounds startup and closes a failed attempt. Existing
  `present\n` and EOF control retain their meanings. Later user/compositor
  resizing is outside this initial admission. An empty popup-only tab admits
  usable body space without requiring or inventing an attachment. The v5
  consumer refinement is indexed in VEN-C18; older startup proofs remain scoped
  to their exact snapshots and component revisions.
- **Startup-admission proof:** `ec80e36625dec73544c0cb64becf4b135c932a63`.
  Locked fmt/check/test/clippy pass (120 ordinary tests); five existing native
  checks pass in separate processes on x86_64 Linux, Sway 1.12 / lavapipe
  26.1.1 at 1.25 scale. The actual standalone/workspace window reports readiness
  for defaults and explicit grids without an Orbit process. Missing fonts,
  impossible workspace geometry and malformed snapshots exit without readiness;
  Present and EOF shutdown pass. The final native grid is checked before Ready.
  Reproduction inputs and observations are retained under
  `~/.local/state/eon/proofs/ven-c19-ec80e36625dec73544c0cb64becf4b135c932a63/`.
  Eon `b693a877cd4d630fe15aabf852de4e978fc166dc` accepts the exact Nix artifact
  `/nix/store/37v7m8kb88fgs4lgwk7ajcbf9hjb8pn7-yazelix-venus-0.1.0` through
  both refreshed product profiles. Installed configured initial/reopened grids,
  missing-font rejection before commands, native input, exact selection/copy,
  IME preedit/commit and accessible text/bounds pass. Eon's EON-C19 index owns
  exact artifacts, scale observations and unchanged Orbit/command identities.
  Earlier typography, fractional-scale, compositor and other partial evidence
  remains qualified by its recorded identities and observations below.
- **Proof:** `1b32e5ba7105d14f654136578a65c97a25b53fc1` (startup correction).
  Locked fmt/check/test/clippy and Nix package checks pass: 118 ordinary tests
  and five native regressions. Initial explicit-grid and font-only startup
  complete without input at scales 1, 1.25, 1.5 and 2 on Sway 1.12/lavapipe
  26.1.1. The watchdog can only fail startup; it cannot supply a successful
  admission wakeup as the earlier probe did.
  Package `/nix/store/k1pdjvzha5skyrp2nsvn0ih6gz6lj2j6-yazelix-venus-0.1.0`
  produces real PTY grids of 73 by 18 standalone, 73 by 12 in a three-pane
  workspace, and 71 by 15 in the picker at 1.25 with 20 px / 1.5 typography
  and omitted columns/rows. Both workspace modes reject 96 px / 3 typography
  with exit 1 and zero observed Orbit connections.
  Reproduction inputs and costly build evidence are retained under
  `~/.local/state/eon/proofs/ven-c19-1b32e5ba7105d14f654136578a65c97a25b53fc1/`.
  Eon `e10a01ee93d6d32d7312d1c82ff6f524aca31583` consumes this exact source;
  both refreshed profiles match their builds. Installed EonTerm PTY startup
  and Eon directory-picker presentation pass at 1.25. Eon's contract index
  records the artifacts and observation limits.
- **Typography matrix proof:** `9157f7fbed0318d94a0697c01a23de2bed86946a`;
  x86_64 Linux, isolated Sway 1.12,
  Mesa 26.1.1 lavapipe, DejaVu Sans Mono and Symbols Nerd Font Mono.
  - Locked fmt/check/test/clippy passed: 118 ordinary tests. All three ignored
    native tests passed, including configured live-output input/selection and
    fractional initial grid admission before attachment. Nix package tests pass.
  - Default and configured (20 px, 1.5 line height) standalone, plus configured
    three-pane and picker surfaces, produced exactly 100 by 30 in a real Orbit
    PTY at 1, 1.25, 1.5 and 2 scale. Latin, Unicode, wide/combining text,
    regular/bold/italic, Nerd symbols, Braille, boxes and cursor were inspected.
  - Missing primary/fallback, proportional primary, NaN and physical native-size
    overflow exited nonzero without any connection to the observed Orbit socket.
    A later manual resize and scale change retained the user-selected geometry.
  - Nix artifact `/nix/store/yc8w1y1q4g8m4wwbhvl749pkpa1vcnim-yazelix-venus-0.1.0`
    passed configured picker dogfood at 1.25 scale: native drag and exact copy
    of `Regular`, Wayland IME preedit `é界` and exact commit `ime-é界`, and
    AT-SPI text with terminal bounds `[15, 105, 1530, 1170]`, matching the
    physical 15 by 38 grid, padding and picker inset. IME cursor requests use
    the corresponding logical coordinates; the preedit capture was inspected.
  - **Downstream acceptance:** Eon `0461737272447fa976d13bd3376c8a894cebf324`
    consumes this exact source. Both installed profiles match their builds and
    pass isolated native default-geometry/input observations; Eon’s contract
    index owns the exact artifacts. Persistent typography configuration remains
    its separate issue.
  - Reproduction inputs, observations, captures and costly Nix log are retained
    under `~/.local/state/eon/proofs/ven-c19-9157f7fbed0318d94a0697c01a23de2bed86946a/`.
- **Limits:** This proves the named typography/geometry slice on Sway; it does
  not widen earlier fractional/HiDPI, VEN-C8, VEN-C18 or compositor coverage.
  The pinned winit destroys text input when a seat capability disappears;
  native IME proof uses a stable keyboard/pointer seat. That pre-existing
  hot-unplug limitation is outside this typography change.

The current proof advances `VEN-C1`, `VEN-C2` and `VEN-C4` only for startup.
Their distinct earlier proof identities and remaining limits stay qualified.

## Status

- **Planned:** accepted contract with no sufficient implementation evidence
- **Candidate:** implemented and verified without an accepted proof-bearing
  revision
- **Partially proved:** a useful exact slice is proved but required evidence is
  still open
- **Proved:** the listed immutable revision and evidence cover the contract
- **Retired:** explicitly replaced or removed; its ID is never reused

## VEN-C5 — Explicit native hyperlinks

- **Status:** Proved
- **Consumer/trigger:** A Venus user hovers an Orbit-authored OSC 8 link,
  inspects links with Ctrl+Shift+O, or explicitly activates one.
- **Result:** The presented link is highlighted and its actual target is
  inspectable. Tab/Shift+Tab chooses a link; Left/Right pages the complete
  escaped target; Enter opens, Ctrl+Shift+C copies, and Escape dismisses.
  Ctrl+Shift+left click opens only when press and release identify the same
  presented target. Ordinary terminal clicks and selection retain their owner.
- **Failures:** Revision, attachment, geometry, or presentation changes retire
  link actions. Opening accepts at most 4096 bytes of ASCII HTTP/HTTPS URI
  syntax with a host and without credentials. Copy accepts bounded target text
  without control characters. Unsupported or malformed targets, missing native
  handlers, and launch failures/timeouts have bounded visual and accessible
  notices. No URI is opened on hover or keyboard focus.
- **Owners:** Orbit owns URI attributes; Scene derives spans; Venus presentation
  identity gates actions; the existing renderer, input, AccessKit status and
  Wayland clipboard owners project them. Linux `gio open` on the host PATH
  dispatches the exact URI as one argument to the desktop's registered handler.
  One launch runs at a time with a ten-second dispatcher deadline.
- **Consumes:** Accepted Orbit `91999d79546422b49bdbc124166a65859d0bd872`,
  `ORB-C6` proof `a65e199e16e97330175e314cacf791fa00f53069`, ORBF v1/ORBS v10.
- **Boundary:** No heuristic detection, file/custom-scheme opening, URI rewrite,
  browser embedding, new crate, or protocol change. Host GIO and a registered
  handler are required for opening; copy remains available without them.
  Native Linux Wayland only; other platforms remain unsupported.
- **Proof:** `cf3a9169f2cd95d42c689134a52d51d9147ff37c`
  - **Inspection correction:** The real Application regression fails before
    this revision and passes after it: unidentified native presses are captured,
    their repeats/releases stay captured after Escape, and fresh presses return
    to terminal routing. Locked checks (115 ordinary tests) and both native
    checks pass. Earlier hyperlink dogfood below retains its exact source.
  - **Installed correction:** Eon
    `07867f99d176012a9d34a61aa37b5de9ce0888af` selects this exact source;
    both profiles launch `/nix/store/fxf4xyadmiwn7m8ndc0i0gxzcbrq5r2p-yazelix-venus-0.1.0/bin/yazelix-venus`.
    Installed EonTerm on Sway 1.12/Nix Mesa 26.1.2 captured real native keycode
    240 carrying text during inspection, admitted it before/after inspection,
    and copied the exact link. Eon's contract index records both artifacts.
  - **Environment:** x86_64 Linux, private Sway 1.12, Vulkan Mesa lavapipe,
    host GIO 2.80.0, Wayland clipboard and AT-SPI.
  - **Checks:** Locked fmt/check/test/clippy; 115 ordinary tests and both native
    renderer/Application checks. The native Application check rejects unpainted
    and replaced link targets, preserves wide tails, refreshes hover after
    presentation and retires the attachment. A real stalled child proves the
    dispatcher deadline and reaping.
  - **Initial dogfood (`d1223463b2b513c04242df50a4bd948d0533cfde`):**
    Native keyboard traversal/paging, exact clipboard and real GIO
    dispatch of `https://example.com/exact?x=%26&y=2#part`, unsupported-scheme
    and malformed-percent refusal, mouse-reporting press/release preservation,
    explicit click capture, bottom-row hover, replacement during a held click,
    and native missing-handler failure. Target and failure text were read
    through AT-SPI.
  - **Integration:** Accepted Eon
    `be9d0e37c7d0029d6832ff609082faa885280762` installed source
    `d1223463b2b513c04242df50a4bd948d0533cfde` as
    `/nix/store/746dv1wk3pmmizz18d4v238p0s69qmf9-yazelix-venus-0.1.0`.
    Both Eon and EonTerm profiles matched their built artifacts; installed
    native checks passed on Sway 1.12/Nix Mesa 26.1.2 lavapipe. EonTerm repeated
    the target, mouse, failure and stale-click dogfood above; full Eon proved
    picker cancellation, target inspection, exact copy and native dispatch.
    Host-only XDG handler preferences were preserved. Eon's contract index and
    `ven-lq4` record the exact artifacts. Fractional scale and broader compositor
    quality remain unproved.

The `VEN-C1`, `VEN-C2` and `VEN-C4` proof revisions advance for this exact
hyperlink slice. Their distinct earlier evidence and listed proof gaps remain
qualified by the identities and boundaries below.

## VEN-C1 — Authoritative native presentation

- **Status:** Proved
- **Consumer:** One Venus native surface consuming canonical Orbit frames and an
  optional Eon cursor profile.
- **Trigger:** Venus accepts an initial frame, an ordered later revision, or a
  cursor-profile value.
- **Result:**
  - Venus validates and materializes one coherent Orbit-authored structured
    frame into native draw inputs without terminal authority.
  - An omitted cursor profile uses the Venus `#89b4fa`, duration-`1.0` tail;
    `none` is static; a complete `tail` supplies one validated color and duration.
  - A bounded four-corner trail approaches Orbit's exact cursor destination while
    preserving authoritative shape, visibility, blink, wide-cell geometry, and
    color.
  - The immutable Scene alone owns accessible text and selection. Each visible,
    nonblank canonical head cell is one selectable UTF-8 unit; text above the
    AccessKit 255-byte unit limit becomes one U+FFFD accessibility unit without
    changing visual or protocol text.
- **Important failures:** Invalid frames, topology, cursor profiles, renderer
  admission, focus, occlusion, or presentation failure cannot publish stale draw
  or accessibility state.
- **Owner:** Venus Scene, renderer, and cursor-animation state; Orbit retains
  terminal and cursor authority.
- **Consumes:** Orbit `ORB-C4` and `ORB-C6` at accepted proof
  `9d6d2bb37f20ab4ad9e186c7bc715eabef43e757`; ORBF v1 in canonical ORBS v4
  at `7f067b30e97d0b4787a7c6c0bbe3dd8a80a61c2c`; Eon cursor-profile v1 values
  owned by Yazelix Cursors `f97d0e7d3badf37ce3c01c1eba99b6b2bd17a7bf`.
- **Boundary:** Fractional/HiDPI native quality remains outside the proof.
- **Proof:** `ec80e36625dec73544c0cb64becf4b135c932a63`
  - **Startup slice:** Exact typography, geometry and failure evidence is
    indexed in VEN-C19 above; earlier distinct evidence below is retained.
  - **Environment:** x86_64 Linux Wayland, with retained deterministic and native
    evidence from the accepted presentation lineage
  - **Evidence:**
    - Static presentation `74ab5a0b661210f0afec94086f5358fe50b01f05`
      with native predecessor `534e47908b7bb7e8286949fe39bf4b0f7cc811f9`
    - Typography `846daf8fb7846b0e8dc227e533aa8d51a691f76f` and
      cell placement `99539163ae901413c56388ae8959c49c608dd46e`, retaining
      `457b3da837f07d186a99ca20930f2146c3d78f36`,
      `3e27bb7e377da9d9eb7e5cbdbfcfa5add3de0cad`, and
      `038cc2129d7cb4047350a37bb4aa6e4c3b93ccee`
    - Cursor tail `254beec194fe5cac208cced34d346014b0484319`, native
      dogfood `c618cfd087da7c64adf8cd093c3f4b458e8fd5d4`, and wide
      timing comparison `9a1658561a130f6d4656a3049a707feb7cecaa72`
    - Omitted-profile tail and explicit-`none` proof at
      `5d22b09e323212693a8e54c4c63089784b660cad`
    - Cell-buffer allocation proof at
      `541a9cb43c155b8b97069904593dc81c73682613`: the
      [memory comparison](benchmarks/venus-memory-2026-09-08.md) records
      repeated PSS/USS savings and identical baseline/candidate pixels on
      isolated Sway/software Vulkan. It preserves the earlier presentation
      proofs and does not extend COSMIC, fractional-scale or compositor coverage.
- **Open proof:** Composed Eon cursor-profile serialization and broader Linux
  compositor coverage remain separately tracked.

## VEN-C2 — Native semantic interaction

- **Status:** Proved
- **Consumer:** One focused Venus terminal surface.
- **Trigger:** Native key, mouse, focus, paste, input-method, or resize activity.
- **Result:** Venus maps native activity to canonical Orbit semantic input and
  resize messages using one shared scaled geometry owner. Compatible output
  advances render content without withdrawing the last presented input geometry.
- **Important failures:** Invalid, stale, unsettled, or non-presented geometry
  cannot reach Orbit; capture and focus transitions withdraw obsolete input.
- **Owner:** Venus native input and geometry owners; Orbit encodes terminal
  behavior.
- **Consumes:** Orbit `ORB-C5` proof
  `c905bf9610581747f1b07565814b501ca66cfaa6` through canonical ORBS v4 at
  `7f067b30e97d0b4787a7c6c0bbe3dd8a80a61c2c`.
- **Boundary:** Candidate-list IMEs, physical mixed-monitor hardware, and broader
  Linux Wayland compositor behavior remain manual quality surfaces.
- **Proof:** `ec80e36625dec73544c0cb64becf4b135c932a63`
  - **Startup slice:** Exact typography, geometry and failure evidence is
    indexed in VEN-C19 above; earlier distinct evidence below is retained.
  - **Environment:** Accepted deterministic host coverage plus retained native
    geometry and input evidence
  - **Evidence:** Native Application input-between-frames regression;
    Sway focus/selection/scroll checks; retained geometry proof
    `c0766532c7669a2ae9dac6a26ae94d467110896b`. Shared geometry,
    resize-settlement, semantic mapping,
    preedit, focus cleanup, and fullscreen checks, retaining typography proof
    `846daf8fb7846b0e8dc227e533aa8d51a691f76f`, semantic-input proof
    `a17d100d11d38acf515cf96af34df6bbfef9850b`, and focused geometry proofs
    `f84493781956a1f5c24be3dab148bef02748cdfe`,
    `6df14c7e2ca8c27d587d6c138200f4d39d6b12a9`,
    `6f919a6e3d2f51b661f76be644a4354a003a3404`,
    `ba7177ca9380f2f5800bcea20da3f81e96b090cd`, and
    `8929c9f9d151641a343813ddeb6005cb9c771286`

## VEN-C3 — Transient client recovery

- **Status:** Proved
- **Consumer:** Standalone or Eon-supervised Venus attached to one selected live
  Orbit endpoint.
- **Trigger:** Client close or crash, retryable connection loss, control-stream
  loss, or replacement attachment.
- **Result:**
  - Client loss never owns or terminates the Orbit Session.
  - Reopening materializes the coherent current state.
  - An open client retries the same live endpoint with bounded backoff while
    retaining the last coherent scene.
  - In supervised mode, control loss exits Venus and a replacement may retry
    transient Busy while the departing attachment releases.
  - Endpoint replacement, non-live workspace state, successful attachment, or
    exit cancels obsolete retry state.
- **Important failures:** Standalone Busy and incompatible, exited, malformed,
  or replaced endpoints remain terminal rather than creating duplicate clients.
- **Owner:** Venus connection/retry and native event-loop lifecycle; Orbit owns
  Session survival.
- **Consumes:** Orbit `ORB-C1`, `ORB-C3`, `ORB-C4`, and `ORB-C7` through canonical
  ORBS v4 at `7f067b30e97d0b4787a7c6c0bbe3dd8a80a61c2c`.
- **Boundary:** Broader Linux compositor coverage remains unproved.
- **Proof:** `a768e9a1bcb61eac5a21d25b7463c9dc44aa2df8`
  - **Environment:** x86_64 Linux with deterministic host and composed Eon
    acceptance
  - **Evidence:** Supervised control EOF, managed-only Busy retry, exact current
    reattachment, and unrelated-Session survival; native predecessor
    `754eb057ea71ddee57b2f543d85d08d66edc953e`, prior loss proof
    `8e2d22a36ae6f6cab74d3556204a2a537265bff7`, exact Orbit
    `86aa130629c09dce61d0f232150298656fa5cef4`, and composed Eon acceptance
    `0bf0b165d06b4a8162be497011070f61f6c2000a`

## VEN-C4 — Bounded explicit failure UX

- **Status:** Proved
- **Consumer:** A Venus user observing connection, protocol, model, renderer, or
  Orbit failure.
- **Trigger:** Attachment rejection, incompatibility, invalid frame, Orbit loss,
  renderer failure, or retry transition.
- **Result:** Venus exposes bounded truthful failure notices without inventing
  state. Retryable loss remains visible during backoff. Routine connection and
  first-frame progress is quiet inside an accepted workspace; standalone
  attachment retains progress messages. Actual attachment and rendering failures
  remain visible in both modes.
- **Important failures:** Standalone Busy is terminal; only supervised
  replacement retries it. Incompatible, Exited, model, queue, invalid-input,
  worker-start, and persistent-renderer failures remain terminal and withdraw
  presented input.
- **Owner:** Venus connection state, notice projection, and renderer lifecycle.
- **Consumes:** Orbit `ORB-C3`, `ORB-C4`, and `ORB-C7` through canonical ORBS v11
  at `ea9fd28ce0908f218cf65d4e6df368f0a4e565f5`.
- **Boundary:** Compositor title-bar visibility and broader Linux compositor
  proof remain outside the accepted slice.
- **Quiet workspace proof:** `b7404dae6c59a8e6de6ca9efbf2c907eedf1262b`.
  Locked fmt/check/test/clippy/build pass (124 ordinary tests). The existing
  native Application input/presentation check goes red for routine connection
  progress, then passes with silent workspace Connecting and first-frame wait,
  visible offline failure and retained standalone progress on x86_64 Linux,
  private Sway 1.12 / host lavapipe at scale 1. Source and native evidence lives
  under `~/.local/state/eon/proofs/eon-quiet-workspace-navigation-y7a/venus/`.
  Consumes accepted Orbit `ea9fd28ce0908f218cf65d4e6df368f0a4e565f5`
  (ORBF v2 / ORBS v11). This does not widen VEN-C8 fractional-scale,
  compositor, screen-reader or earlier distinct failure evidence.
  Eon `07dc210a49cd30f3b58fa65fd9e9a45b421e9b36` accepts this source through
  both refreshed products. Its contract index owns installed identities,
  navigation observations and the retained intermittent headless-capture limits.
- **Proof:** `ec80e36625dec73544c0cb64becf4b135c932a63`
  - **Startup slice:** Exact typography, geometry and failure evidence is
    indexed in VEN-C19 above; earlier distinct evidence below is retained.
  - **Environment:** x86_64 Linux deterministic host coverage with retained
    native title and supervised-loss evidence
  - **Evidence:** First-renderer-error retention, best-effort diagnostics,
    presented-input withdrawal, terminal-versus-retryable classification, and
    later recovery; native title proof `4c95652b2fe1af28df5964b5686daf44676a171e`,
    exact winit `fb45fbf901fbe70cc9a877b5d651d0b60c206b08`,
    supervised proof `a768e9a1bcb61eac5a21d25b7463c9dc44aa2df8`, native
    predecessor `754eb057ea71ddee57b2f543d85d08d66edc953e`, prior loss
    proof `8e2d22a36ae6f6cab74d3556204a2a537265bff7`, and composed Eon
    acceptance `0bf0b165d06b4a8162be497011070f61f6c2000a`

## VEN-C7 — Authoritative history and selection interaction

- **Status:** Proved
- **Consumer:** One presented Venus terminal surface.
- **Trigger:** Native wheel or touchpad movement, one left-pointer sequence, or
  explicit copy.
- **Result:**
  - While away from live output, Venus displays `↑ N rows` (`↑ 1 row` for one)
    from the accepted frame's authoritative wrapped display-row distance. Live
    bottom, alternate screen, recovery and known terminal-owned routing hide it.
    Idle preview cleanup and rejected input preserve routing evidence until a
    fresh frame or preview replaces it.
    Preview movement never changes the count; accessibility exposes committed
    position as a description rather than an alert. The selected pane header
    reserves space for the count and fits its directory label into the remaining
    width using the existing cluster-safe header fitter. Standalone and picker
    terminals use a small top-right overlay that preserves grid dimensions and yields to selection,
    links, notices, picker tab previews and an overlapping terminal cursor.
    Pending reflow hides the old count;
    labels that cannot fit in full remain accessible without clipped digits.
  - Precision movement translates and clips an accepted complete Orbit frame
    plus its bounded revision-bound row window at two presented pixels per
    native input pixel.
  - Venus retains a sub-row remainder, commits crossed rows through at most one
    coalesced signed Orbit batch, and installs only its atomic authoritative
    frame and next preview.
  - Lagging relative preview and commit revisions remain admissible while PTY
    output advances; after history entry, later output leaves the authoritative
    viewport pinned and further relative movement remains available.
  - When a later output frame preserves screen, geometry, default cell colors,
    and palette while fractional movement remains, Venus keeps only that
    bounded revision-bound preview until Orbit supplies its replacement instead
    of presenting the remainder as zero or freezing finger movement. Every
    plain frame retires transient terminal-routing evidence.
  - A phase-complete precision gesture may continue with bounded elapsed-time
    exponential decay; discrete wheel input moves three rows per logical step
    without synthetic momentum.
  - A transient presentation timeout preserves gesture estimation, kinetic
    velocity, and distance; an occluded surface still cancels them.
  - Ordinary output preserving terminal dimensions and active screen keeps
    wheel start/movement/end and pointer phases admissible between repaints.
    Input carries an actually presented revision; render caching still advances
    on each accepted frame. Orbit keeps parsing and owns anchored history.
  - Redraw follows native compositor callbacks.
  - Venus sends every left-pointer phase to Orbit. Orbit routes uncaptured
    input to cell, word, or logical-line selection, preserves terminal mouse
    capture, and treats Shift as a host-selection override.
  - Pointer phases and explicit copy that arrive before Orbit completes the
    preceding sequence retain their order. Venus resumes them only after
    presenting the authoritative completion revision reported by Orbit.
  - A successful selection release writes Orbit's frozen text to both the
    ordinary Wayland clipboard and primary selection. `Ctrl+Shift+C` remains an
    explicit ordinary-clipboard copy.
- **Important failures:** Future authority, stale coordinate- or phase-bound
  input, failed gesture admission, resize, capture loss, lifecycle change,
  terminal-owned routing, history edge, or Orbit rejection cancels synthetic
  motion and cannot fabricate cells, selection, copied text, or viewport state.
  A change to screen, geometry, default cell colors, or palette retires the
  bounded preview before presentation. An unsolicited or mismatched scroll
  outcome is a protocol-order failure.
- **Owner:** Venus owns native fractional presentation, bounded kinetic state,
  gesture cancellation, and clipboard effects; Orbit alone owns history,
  viewport movement, routing, cells, revisions, selection, and copied text.
- **Consumes:** Orbit `ORB-C4`, `ORB-C5`, `ORB-C6`, `ORB-C8`, and `ORB-C9`
  through canonical ORBF v2 / ORBS v11 at
  `ea9fd28ce0908f218cf65d4e6df368f0a4e565f5`; exact patched winit
  `fb45fbf901fbe70cc9a877b5d651d0b60c206b08`, wgpu 30.0.0, and glyphon
  0.12.0.
- **Boundary:** Client-owned history caches, bounce, device/source heuristics,
  public physics tuning, presentation feedback, unreleased winit, GPU-layer
  translation, and additional platform support are outside this contract.
- **Scrollback review proof:** `74d22276e9039dc3ee6151d69b9283114ab02ea1`
  - Idle cleanup and rejected input preserve terminal-routing evidence; the
    selected pane label keeps its directory ending beside the count. Existing
    canonical-model and native shaped-header checks reproduce both regressions
    before the corrections and pass afterward. Locked Rust checks pass (124
    ordinary tests), as do the native header and continuous-output checks.
  - Private Sway 1.12, scale 1 and host lavapipe repeat the real-Orbit scenarios
    below. Terminal-owned wheel input hides the count at an unchanged distance
    of 15, and a fresh frame restores it. The narrowed workspace preserves the
    long path's `/leaf` ending beside counts 10 / 20 / 10 across pane switches.
    The reused workspace projection preserves geometry and input admission.
    Source/lock and binary hashes, logs, captures and process-preservation
    records are retained in
    `~/.local/state/eon/proofs/ven-scrollback-position-ddo-review-2026-09-09/`.
    All 19 pre-existing product process identities survive; owned processes
    are reaped. The source-consumer, accessibility and platform limits below
    remain unchanged; this does not prove installed Eon delivery.
- **Initial scrollback indicator proof:** `2bbb7029a84de783357ef2db0a68a145086b37b2`
  - **Environment:** Rust 1.96.0, x86_64 Linux, private Sway 1.12 at native scale
    1 and host lavapipe Vulkan; real exact Orbit sessions with a canonical EONW
    v4 workspace fixture.
  - **Evidence:** Locked fmt/check/test/clippy/build pass, including 124 ordinary
    tests. The canonical codec/model regression covers exact distance, singular
    wording, the full `u64` range, preview, routing, alternate screen and recovery.
    The native renderer check covers header spacing, complete counts, clipping
    and cursor collision; the existing native continuous-output Application
    regression passes. Real native counts track output (10 to 15), reflow (63),
    history top (177), pruning (36,135), live bottom and clear (zero), reconnect,
    and pane switches (10 / 20 / 10). Selection and link inspection hide the
    standalone overlay. Source/lock hashes, scripts, captures, observations and
    process-preservation records are retained under
    `~/.local/state/eon/proofs/ven-scrollback-position-ddo-2026-09-09/` and
    indexed by `ven-scrollback-position-ddo`.
  - **Limits:** This is source consumer acceptance. Paired installed Eon delivery
    belongs to `eon-accept-scrollback-position-j7f`; the workspace topology here
    is a fixture. AccessKit is tree-verified, not screen-reader dogfood. Native
    fractional-scale and broader-compositor qualifications remain unchanged;
    this proof makes no new latency or kinetic-performance claim.
- **Retained gesture proof:** `e13970e90289d0d86f0adcbf350e4b9c1d5e5219`
  - **Environment:** Locked deterministic checks and optimized x86_64 Linux
    native Wayland dogfood
  - **Evidence:** Complete Rust checks and Nix build; native Application red/green
    admission regression; standalone cell/word/line drag and repeat-click
    gestures, both clipboard destinations and explicit copy during output;
    anchored history with continuing terminal replies under appends, active
    redraws, and DEC 2026. Native artifacts and exact observations are recorded
    in `ven-select-during-live-output-752`. Retained scroll proof
    `d3e63ae6e9fa0b72df426dbfc9bd29a96148577a` covers the exact ORBS v10
    pin; bounded multi-row
    conversion, clipping, lagging commit, compatible viewport retention,
    terminal-routing and incompatible-frame retirement, exact in-flight outcome
    admission, edge, cancellation, routed cell/word/line gestures, terminal
    capture with Shift override, separate primary and ordinary clipboard
    effects, resize, reattachment, and client-loss coverage. Bead comment 711
    records user acceptance of optimized native continuous-output hard flings on
    `d3e63ae6e9fa0b72df426dbfc9bd29a96148577a`; that revision preserves
    transient timeout motion while cancelling an occluded surface explicitly.
    Normal Eon manifest/profile adoption is accepted at Eon
    `91c6ed5d51b5a09d5c9e1d2e30aebc191c10223f` in
    `eon-accept-live-output-input-nxh`: both installed products use this exact
    Venus source and Orbit `91999d79546422b49bdbc124166a65859d0bd872`.
    Isolated Sway 1.12 proof covers anchored history, cell/word/line selection,
    both clipboard destinations, explicit frozen copy, and busy-workspace
    focus under appends, DEC 2026 batches, and active-screen redraws. The
    separate Eon Input Dogfood launcher remains prior evidence.

## VEN-C8 — Eon workspace presentation

- **Status:** Partially proved
- **Consumer:** One Venus surface controlled by an Eon workspace.
- **Trigger:** Outside visible `VEN-C18` popup presentation, Eon supplies
  durable topology or a workspace action changes tab, pane, focus, order,
  lifetime, liveness, endpoint, or metadata.
- **Result:**
  - Venus materializes ordered horizontal tabs and every fitting header for the
    active tab around exactly one expanded pane. Each tab shows its current
    one-based position plus the leaf, `~`, or `/` derived from Eon's
    authoritative launch directory; positions update when Eon reorders tabs.
  - Tabs have pill-shaped corners and separated hit targets. Width follows shaped
    label text plus padding, capped near 280 logical pixels at default
    typography. Long labels use a cluster-safe middle ellipsis; when even that
    cannot fit, preserve the positional number whenever it fits. Overflow stays
    horizontally scrollable across the whole strip, including its gaps, and
    selection reveals the active tab. Hover shows
    its control-sanitized launch path within available window space; the
    accessible name retains that path. Selection and keyboard focus remain
    distinct: selected fill and brighter text identify the active tab without
    an underline; keyboard focus adds a rounded outline. Pane shell navigation
    does not rename or resize tabs.
  - Every visible live pane endpoint has one bounded read-only metadata observer.
  - Pane frames connect the visible accordion within one thin rounded outer
    border, with horizontal separators reaching its sides and a small gap below
    the stack (4 logical pixels at default typography). The gap shares terminal
    background color/opacity and the surface-wide blur request. Selected and
    hovered header fills follow the stack's corners, with square internal edges.
    Separators follow the rounded corners during scrolling;
    the shared border stays fixed while overflowing headers scroll within it.
    `--pane-frames true|false` defaults to `true`; `false` removes the shared
    border and separators while retaining headers, a lighter selected fill,
    hover feedback and a distinct rounded keyboard-focus outline. Both modes
    preserve the same terminal grid, header hit targets and AccessKit bounds.
    Initial requested grids include the bottom gap in their window overhead;
    the gap accepts no pane or terminal input.
    Changed pane lists resize the terminal even when the endpoint stays selected.
    The option has no visual effect in standalone or popup presentation.
    Missing, duplicate or invalid values fail before window creation. Eon owns
    persistent configuration and composed-launch policy.
  - Headers and AccessKit names show the opaque `pN` identity, two ASCII spaces,
    and one compact working-directory label.
  - Home displays the packaged home marker; descendants use `~/`; paths outside
    home remain absolute; overlong paths elide from the left to preserve the
    leaf; an unset or empty `HOME` keeps absolute paths.
  - Terminal title remains available to the selected window and is not repeated
    in pane chrome. Empty, offline, unavailable, or incompatible metadata falls
    back to pane identity without exposing mapped Session identity.
  - Hidden tabs have no observers, obsolete observations retire, and a 250 ms
    reinspection exposes accepted external workspace changes without native
    input.
  - Pointer input, Alt+H/L tab traversal, Alt+K/J pane traversal, Alt+M pane
    creation, Alt+Shift+T tab requests, focused arrow traversal, and
    selected-terminal attachment retain their existing owners. Alt+H/L remains
    admitted while a popup is visible; inactive popup selections do not replace
    the active tab's projection. Popup-only tabs may have no pane or selected
    pane. Hiding their last visible popup yields an empty body with actionable
    tabs and catalog shortcuts (VEN-C18).
  - Physical Alt+1 through Alt+9 focus the corresponding current tab position;
    Alt+0 focuses position 10. Venus resolves the position from the accepted
    snapshot and sends the existing stable `tN` through `FocusId`. A missing
    position is consumed without an action or terminal input. Alt+H/L remains
    the traversal path for tabs after position 10.
  - Ctrl+Alt+H/L sends one non-repeating semantic move for the active tab;
    Ctrl+Alt+K/J does the same for its selected pane. Alt+Shift+W sends one
    non-repeating close naming the snapshot's active stable `tN`. Venus consumes
    each exact press and matching release without forwarding either to the
    selected terminal. Ctrl+T, Ctrl+Shift+W, and lowercase Ctrl+W remain terminal
    input, and none of these shortcuts enters a mode.
  - Compatible terminal output does not withdraw tab/pane focus actions or
    presented header hit targets while a repaint is pending.
  - Hit testing, AccessKit node identity, and actions retain the exact `tN`.
    AccessKit names pair the current position with a bounded full launch path,
    so duplicate leaves remain distinguishable without storing another tab
    name.
- **Important failures:** Workspace loss, Orbit exit, endpoint replacement,
  liveness change, rejected close or movement, or incompatible metadata retires
  stale observations and never grants Venus topology or Session-lifecycle
  ownership. Repeats create no second structural action.
- **Owner:** Venus workspace materialization, bounded metadata observation,
  clipping, and accessibility projection; Eon owns topology and Orbit owns
  terminal metadata and Session lifetime.
- **Consumes:** Eon `EON-C10`, `EON-C17`, `EON-C18`, EONW v5, and
  `eon-workspace-protocol` 0.1.0 at
  `0cc8f477298681ae3945903e8fdb5852d487c5ab`; Orbit metadata observation in
  ORBS v11 and `orbit-protocol` 0.1.0 at
  `ea9fd28ce0908f218cf65d4e6df368f0a4e565f5`.
- **Boundary:** Other Linux Wayland compositors remain outside the current
  proof.
- **Proof:** `0791f00926cd5cc4fedcc7737aeac7bb68569aff` for tab shortcuts;
  `94b15af20d1798b648f4d9945fd6bb647f10add8` for one-line header layout;
  `e13970e90289d0d86f0adcbf350e4b9c1d5e5219` for workspace interaction
  - Positional labels and direct Alt-digit focus are accepted at source
    `f4845f1d0dd5c31301fd0c64b28fa8a907d1eae0`
    (`ven-positional-tabs-alt-digit-b3j`). The focused red/green checks and the
    complete locked Rust route pass with 127 ordinary tests. This source proof
    covers deterministic scene, shortcut, tooltip and AccessKit projection; it
    does not claim native input, screen-reader, installed Eon or macOS proof.
  - EONW v5 shared-popup and empty-body projection is accepted at
    `f7fb5a071edc446d04e629837b6e277f28d93709`; VEN-C18 indexes its exact
    Rust/Nix and isolated native checks. This consumer proof does not widen
    fractional-scale or full-Eon acceptance.
  - Scrollback count reservation preserves the selected pane directory ending
    at source `74d22276e9039dc3ee6151d69b9283114ab02ea1`. The focused shaped-header
    red/green check and private native pane-switch captures are indexed in the
    VEN-C7 review proof. Fractional scale and the other limits remain qualified.
  - Small painted bottom spacing and stack-shaped header fills are accepted
    at source `bc5a2bd3b2363abdea69c4cd89953b612bc970e8`. The focused margin
    regression fails on the preceding source and passes on this candidate;
    all 123 ordinary Rust tests and strict clippy pass. Private Sway 1.12 /
    Vulkan lavapipe scale-1 compares the rejected `ff426a9` baseline and this
    source over a colored underlay in both frame modes. Pixels prove a 4px gap
    matching terminal background color/opacity, top outer corners and square
    internal selected/hovered edges, and full-width dividers. Pointer/keyboard
    input, focus, overflow/reveal, picker and actual PTY grids pass. All 33
    pre-existing runtime identities survive; private process cleanup is empty.
    Evidence:
    `~/.local/state/eon/proofs/ven-connected-pane-stack-0ub-corrections-2026-09-09/REPORT.md`.
    Installed Eon/EonTerm generation `g1-5626ef23db3adb92c987e2c6179a793c` passes
    default/off pixels, native interaction and real initial/reopened 100x30 PTYs.
    Reopened workspaces retain the same supervisor, Orbit Sessions and command;
    standalone retains its grid without workspace chrome. Both profile elements
    and every observed Venus launch match the new build. All 33 pre-existing
    runtime identities survive installed proof; private process cleanup is empty.
    Sway does not prove compositor blur; pinned winit's surface-wide blur request
    is unchanged. The colored underlay exposes the alpha-zero hole hidden by the
    earlier black proof background. Fractional native scale, other compositors,
    screen-reader usage and offline lifecycle remain qualified.
  - Earlier polish proof at source
    `ff426a9f3d6e1acbb7ae1ce2388df7c75f7bb271` covered full-width separators,
    bottom spacing and lighter selected headers. Its margin and highlight
    design is superseded by the correction above.
    Locked Rust checks pass (123 tests), with a red/green margin/hit regression.
    Private Sway 1.12 / Vulkan lavapipe scale-1 true/false proof covers selection,
    hover, native focus/input, long labels, narrow overflow, picker and PTY grids.
    Pixel checks prove side-to-side separators, selected fill in both modes and
    the 12-logical-pixel bottom gap at default typography. All 30 existing runtime
    identities survive; private processes stop cleanly. Inputs, captures and
    limits are in
    `~/.local/state/eon/proofs/ven-connected-pane-stack-0ub-polish-2026-09-09/REPORT.md`.
    Installed Eon/EonTerm generation `g1-23256a5c404a78d4a68164edae640415`
    passes default/off visual and native interaction checks. Reopened 100x30
    workspace PTYs preserve their supervisor, Orbit Sessions and command;
    standalone remains 100x30 without workspace chrome. All 28 pre-existing
    runtime identities survive installed proof. Fractional native scale, other
    compositors, screen-reader usage and offline lifecycle remain qualified.
  - Connected pane stacks are accepted at source
    `8389cc011adbbf9c390a912d6c93b0040e47e21d`
    (`ven-connected-pane-stack-0ub`). Locked Rust checks (123 ordinary tests)
    and the native header regression pass. Private Sway 1.12 / Vulkan lavapipe
    at scale 1 proves top/middle/bottom selection, native focus/input, long
    labels, overflow, picker and unchanged PTY grids. Five frame-off captures
    and the single-pane frame-on capture match the prior renderer exactly;
    multi-pane captures show a continuous side border and two inset separators.
    Exact inputs, captures and limits are retained in
    `~/.local/state/eon/proofs/ven-connected-pane-stack-0ub-2026-09-09/REPORT.md`.
    Installed Eon and EonTerm delivery passes with generation
    `g1-216b179a560fe9af5c70a15dce222bf1`: exact profiles, default/off,
    connected border pixels, focus/input, overflow, picker and real PTY grids.
    All 25 existing runtime processes survive; private processes are reaped.
    Fractional native scale, other compositors and actual screen-reader use
    remain qualified.
  - Pane-frame review is accepted at source
    `5c23b0fe206fb2ed9e027603ae61ba366913cbe1`. Selected offline labels stay
    distinct with frames off; rounded fills and outlines share edge coverage.
    Locked Rust checks (123 ordinary tests) and the native renderer regression
    pass. Eight scale-1 Sway captures match the prior live-pane appearance
    pixel-for-pixel in both modes. Offline selection has native renderer-fixture
    red/green proof; it is not an installed Eon offline-lifecycle claim. Evidence:
    `~/.local/state/eon/proofs/ven-pane-stack-chrome-suu-review-2026-09-09/REPORT.md`.
  - Rounded pane frames and their startup option are accepted at source
    `bc3a01e0cae503429ab513e81fe2c95ea516b949` (`ven-pane-stack-chrome-suu`).
    Checks pass: locked Rust (123 ordinary tests), native header/input and Sway
    scale-1 default/on/off, focus, overflow and picker observations. Rapid changes
    resize retained PTYs correctly. Exact hashes and qualifications are in
    `~/.local/state/eon/proofs/ven-pane-stack-chrome-suu-2026-09-09/REPORT.md`.
    Initial Eon delivery is recorded in
    `~/.local/state/eon/proofs/eon-pane-stack-delivery-jfo-2026-09-09/REPORT.md`.
    Source acceptance covers the recorded native boundary; earlier proofs do not
    cover this change or widen its scale/compositor/accessibility evidence.
  - Pill-shaped corners without a selection underline are accepted at source
    `8211e8f773140429398154ca3f2230bd09fed1df`
    (`ven-pill-workspace-tabs-rg4`). Locked Rust checks pass, including 121
    ordinary tests. Private Sway 1.12/Vulkan at scale 1 shows selected/idle tabs,
    rounded focus, hover paths, overflow scrolling and narrow clipping.
    Exact source/binary hashes, captures and cleanup are retained in
    `~/.local/state/eon/proofs/ven-pill-workspace-tabs-rg4/REPORT.md`.
    This substituted debug-child check does not update installed Eon or extend
    fractional-scale, compositor or accessibility proof. Prior rounded-tab
    evidence remains scoped to its source below.
    Installed Eon acceptance at `57fb875cc02c7fe8c2cd1134315e777a17f15f73`
    verifies this exact child through both package builds/profile identities and
    one private scale-1 native style/focus observation, without substitution.
    Evidence is retained in the same archive's `installed/REPORT.md`; wider
    compositor, fractional-scale and accessibility qualifications remain.
  - Rounded adaptive tabs and hover paths are accepted at source
    `f5c679443b2fbda1a7a8f91c98d312811bee2cdf`
    (`ven-rounded-adaptive-tabs-vdf`). Retained evidence is in
    `~/.local/state/eon/proofs/ven-rounded-adaptive-tabs-vdf-2026-09-08-review-2/`:
    exact source/binary hashes, 121 ordinary Rust tests, native application
    red/green for wheel scrolling in padding and between tabs, and an Eon-backed
    wheel observation. Rendering, hover, focus, resize, picker and 1x/1.25x/2x
    observations in the sibling `-review-1/` directory remain prior-candidate
    evidence. All native observations use private Sway 1.12/Vulkan; they do not
    close the existing fractional-scale or other compositor gaps. Substituted
    debug-child proof does not establish installed Eon pin/profile behavior.
  - **Environment:** Exact-source x86_64 Linux checks and isolated Sway;
    COSMIC observations belong to the retained prior proof.
  - **Evidence:** Complete locked Rust checks and isolated Nix workspace
    acceptance: two completed tabs without a picker, native Alt+H/L, F6 and
    tab/pane arrows, and tab clicks while an attached Session appends. Retained
    proof `d2d798099934dcf8037bfad6ab856e40c9b989fe` covers exact EONW v4;
    bounded
    active-tab projection, metadata, action, scene, AccessKit, and direct
    structural-shortcut checks while a picker remains bound to another tab;
    isolated native picker-to-durable-to-picker attachment, AT-SPI projection,
    and installed Eon input for all five structural chords; prior complete EONW v3 proof
    `9b2a527d3581f40569f58e24130cc8f8f1222162`
  - **Header evidence:** `ven-2fq` records actual-renderer red/green checks for
    long directory labels and underscore ink inside the header, preserved
    multiline failures, complete locked Rust checks, and native Sway 1.12
    scale-1 installed Eon screenshot acceptance of two real Orbit-backed tabs.
    Eon `/nix/store/j4n3j5r9mfj7xpkj6sjnb9cns11ffsly-eon-0.1.0` consumes
    the header proof source with Orbit `91999d79546422b49bdbc124166a65859d0bd872`.
    Long labels remain on one horizontally clipped line; this does not extend
    scale proof.
  - **Tab shortcut evidence:** `ven-yp4` records focused red/green and complete
    locked Rust checks plus native Sway 1.12 scale-1 composition. Alt+Shift+T
    opens an inherited-directory pending tab; Alt+Shift+W cancels it or closes
    an accepted durable tab while preserving the original Session and final-tab
    protection. Ctrl+T and Ctrl+Shift+W produce exactly `14 17` PTY bytes, with
    no workspace mutation or new-shortcut leakage. Eon source
    `fea679260dd81fcdbb907e804156b00244af2324` accepts the same observations
    through installed `/nix/store/578xxkf17cin8sdn6pb5rr10hxz4yfqg-eon-0.1.0`
    with unchanged Orbit `91999d79546422b49bdbc124166a65859d0bd872`; both Eon
    profiles resolve to their current-tree builds. This changes no scale proof.
- **Open proof:** Fractional native scale remains to be dogfooded.

## VEN-C9 — Optional native decorations

- **Status:** Proved
- **Consumer:** A user launching Venus.
- **Trigger:** Launch with or without one `--no-decorations` option.
- **Result:** The option requests an undecorated native window before creation;
  omission retains decorations.
- **Important failures:** Unknown, duplicate, or excess arguments fail with
  bounded usage before window or transport creation.
- **Owner:** Venus CLI admission and winit native window attributes.
- **Consumes:** Exact locked winit 0.30.13; no Orbit or Eon protocol boundary.
- **Boundary:** Runtime decoration changes are outside this launch-only contract.
- **Proof:** `90988f6ebcde68338e202a9c637c59398aafe93d`
  - **Environment:** x86_64 Linux Wayland
  - **Evidence:** Decorated default, both socket forms, invalid-input rejection,
    and pre-window attribute application

## VEN-C10 — Native paste

- **Status:** Partially proved
- **Consumer:** One attached focused Linux terminal surface.
- **Trigger:** Native Paste or logical Ctrl+Shift+V.
- **Result:** Venus reads ordinary native clipboard text once, admits at most
  1 MiB, and submits exactly one canonical semantic paste to Orbit; Orbit alone
  owns normal and bracketed terminal encoding.
- **Important failures:** Oversized, invalid, unavailable, unfocused, detached,
  or overlapping shortcut input fails visibly without duplicate submission.
- **Owner:** Venus shortcut precedence, native clipboard read, admission bound,
  and visible failure.
- **Consumes:** Orbit `ORB-C5` and canonical semantic paste at
  `9d6d2bb37f20ab4ad9e186c7bc715eabef43e757`, exact winit 0.30.13, and the
  accepted Linux native clipboard owner.
- **Boundary:** A hardware Paste key and native Wayland without data-control
  remain unproved.
- **Proof:** `9c56eb17613e10ef7712a1852b049ed63cf22b18`
  - **Environment:** Deterministic checks plus isolated Sway 1.12 Wayland proof
  - **Evidence:** Overlapping shortcuts, layout-independent release pairing,
    multiline Unicode, normal mode, bracketed mode, and reattachment; native
    Sway source `eb67dac509d0c6033e4373caf8c63eb6e4c88868`
- **Open proof:** Manual Zellij, Helix, and Yazi acceptance remains assigned to
  the user.

## VEN-C11 — Terminal-background opacity

- **Status:** Proved
- **Consumer:** One Venus native surface.
- **Trigger:** Launch with an optional finite `--background-opacity VALUE` in
  `0.0..=1.0`.
- **Result:** Omission equals `1.0`; the value applies only to terminal default
  background, later Orbit-authored background changes, and terminal padding.
  Explicit cells, selection, inverse video, workspace chrome, notices, focus,
  foreground effects, input, hit testing, and accessibility remain unchanged.
- **Important failures:** Invalid values fail before window creation; values
  below `1.0` require a proved premultiplied surface and otherwise fail before
  presentation.
- **Owner:** Venus launch policy and renderer surface composition.
- **Consumes:** Exact winit 0.30.13, wgpu 30.0.0, glyphon 0.12.0, ORBS v4 at
  `7f067b30e97d0b4787a7c6c0bbe3dd8a80a61c2c`, and EONW v3 at
  `96119f29ca2e3ec4ad19bbe272708b07d588429a`.
- **Boundary:** Broader Linux compositor proof remains open.
- **Proof:** `74ab5a0b661210f0afec94086f5358fe50b01f05`
  - **Environment:** x86_64 Linux deterministic checks with retained COSMIC
    native opacity dogfood
  - **Evidence:** Default/background provenance, later background revision,
    opaque chrome and notices, selection, resize, reattachment, and invalid NaN
    with native predecessor `534e47908b7bb7e8286949fe39bf4b0f7cc811f9`

## VEN-C13 — Terminal-authored clipboard delivery

- **Status:** Partially proved
- **Consumer:** One attached Venus client and its native clipboard owner.
- **Trigger:** Orbit emits one canonical terminal clipboard-write effect.
- **Result:** Venus delivers it once without storing, replaying, parsing, or
  reconstructing terminal text; Linux standard maps to ordinary clipboard and
  selection or primary maps to primary selection.
- **Important failures:** Pre-attachment effect, native failure, invalid
  destination, or Orbit rejection remains bounded and visible without changing
  Scene state or selection-copy behavior.
- **Owner:** Venus native clipboard effect delivery; Orbit owns terminal
  interpretation and text.
- **Consumes:** Orbit `ORB-C11` through canonical ORBS v4 at
  `7f067b30e97d0b4787a7c6c0bbe3dd8a80a61c2c` and the accepted Linux native
  clipboard owner.
- **Boundary:** Ordinary clipboard delivery, Wayland without data-control, and
  broader Linux compositor coverage remain unproved.
- **Proof:** `2d3498258920736eb1bdae2b8869b6547b9735d4`
  - **Environment:** Deterministic Venus checks with accepted x86_64 Linux
    Wayland primary-selection composition
  - **Evidence:** One-shot destination/text forwarding, unchanged Scene,
    preserved selection copy, and exact primary-selection dogfood through Eon
    `0e25ebc2311d7e41edf90c940f8211dd5839bb83` and Eonova
    `4fda9b67b0faa33561624633229135e5e2d579ea`

## VEN-C14 — Supervised native presentation lifecycle

- **Status:** Proved
- **Consumer:** One Eon-supervised Venus process.
- **Trigger:** A complete private Present command, control EOF, or terminal read
  failure.
- **Result:** Present preserves process and terminal attachment while requesting
  presentation of the existing native window; control loss exits through the
  native event loop, releases attachments, and leaves Orbit live; replacement
  may retry transient Busy while the old client releases.
- **Important failures:** Malformed or partial commands do not present; standalone
  Busy remains terminal; Present followed by EOF preserves exact Present-then-
  Exit ordering.
- **Owner:** Venus supervised control stream and native event-loop lifecycle.
- **Consumes:** Eon `EON-C11`, one bounded private byte stream, Unix EOF, and
  exact winit 0.30.13 event/exit APIs.
- **Boundary:** Direct focus and unminimize are unavailable through winit;
  xdg activation remains compositor-controlled.
- **Proof:** `a768e9a1bcb61eac5a21d25b7463c9dc44aa2df8`
  - **Environment:** x86_64 Linux deterministic host and composed Eon acceptance
  - **Evidence:** Complete-command admission, malformed/partial rejection,
    Present-then-Exit, ORBS release, replacement presentation, and live Orbit;
    native predecessor `754eb057ea71ddee57b2f543d85d08d66edc953e`, exact
    Orbit `86aa130629c09dce61d0f232150298656fa5cef4`, prior native
    Present proof `50b7ef7f6c9d5b531b79ecca67c9c8fdf40f355f`, and composed
    Eon acceptance `0bf0b165d06b4a8162be497011070f61f6c2000a`

## VEN-C15 — Compositor-owned background blur

- **Status:** Proved
- **Consumer:** One Venus native Wayland surface.
- **Trigger:** Launch with an optional `--background-blur` flag.
- **Result:** Before showing the window, Venus asks its native host for full-
  surface compositor blur; omission makes no request. Translucent terminal
  pixels reveal compositor blur while explicit cells, selection, inverse,
  chrome, notices, focus, text, cursor, input, IME, and accessibility retain
  their owners. The compositor owns capability, algorithm, strength, and policy.
- **Important failures:** Unsupported or policy-disabled compositors remain best
  effort without failing launch; duplicate, unknown, or excess arguments fail
  before window creation.
- **Owner:** Venus owns one boolean request, winit owns native protocol lifecycle,
  and the compositor owns the effect.
- **Consumes:** Exact patched winit 0.30.13 at
  `fb45fbf901fbe70cc9a877b5d651d0b60c206b08`, backporting upstream
  `c4afadbfabf7b1e7989b40b493db1a4c7bd8ff4e`; unchanged `VEN-C11`, ORBS v4,
  and EONW v3.
- **Boundary:** Stable crates.io winit lacks the mechanism; non-COSMIC Linux
  compositors remain unproved; no API reports capability or compositor
  acceptance or controls strength.
- **Proof:** `7fc7e4ba97aaf48b586002934a580ef2d1c31694`
  - **Environment:** COSMIC Wayland 1.0.0 on x86_64 Linux
  - **Evidence:** Flag admission, hidden-window attributes, blur off/on at
    opacity `0.0`, `0.65`, and `0.88`, and preserved terminal/chrome semantics
    on COSMIC revision `091583ac84abac02967ae358cf9570ddfef63b31`
- **Open proof:** A populated current Eon workspace was not re-dogfooded; this
  proof reuses accepted VEN-C11 composition evidence.

## VEN-C16 — Native Linux Wayland and Apple Silicon macOS platforms

- **Status:** Proved for x86_64 Linux native Wayland; Apple Silicon macOS is
  planned and unproved.
- **Consumer:** Every packaged Venus application launch.
- **Trigger:** Build or launch the native application on an approved target.
- **Result:** Venus uses native Wayland with Vulkan on Linux and will use the
  native application/window lifecycle with Metal on `aarch64-darwin`. Both
  targets share protocol validation, model, scene, draw inputs, renderer,
  accessibility tree, semantic actions, and workspace composition; only native
  host mechanics vary.
- **Important failures:** An unavailable native host or graphics backend,
  invalid platform mapping, or unsupported target fails before presentation
  attachment with a bounded explicit error. Evaluation or compilation alone is
  not native runtime proof.
- **Owner:** Venus native host and dependency feature selection.
- **Consumes:** User-approved `EON-C7`; proved Linux uses exact winit 0.30.13 at
  `fb45fbf901fbe70cc9a877b5d651d0b60c206b08`, wgpu 30.0.0, and
  wl-clipboard-rs 0.9.3 with unchanged ORBS v4 and EONW v3. The macOS path must
  consume accepted Orbit `ORB-C14` at `0233f4d34b294a50c5bc7f373859cf4ec04d2414`;
  native feature or crate changes remain gated on the stack qualification.
- **Boundary:** X11, Xwayland, Intel macOS, signing, notarization, packaging,
  distribution, and other native platforms remain unsupported.
- **Linux proof:** `e033efadf023492ae02eb1e9036de98ad93d2f98`
  - **Environment:** x86_64 Linux native Wayland
  - **Evidence:** Locked format/check/test/Clippy, exact dependency features,
    no-Wayland failure, and live native event-loop launch.
    Installed Eon/EonTerm integration is accepted at Eon
    `91c6ed5d51b5a09d5c9e1d2e30aebc191c10223f`, consuming Venus
    `e13970e90289d0d86f0adcbf350e4b9c1d5e5219` with the same Wayland/Vulkan
    host. `eon-accept-live-output-input-nxh` records exact package/profile
    identity and isolated Sway 1.12 selection-copy delivery to ordinary and
    primary clipboards. VEN-C10 native-paste and VEN-C13 terminal-authored
    clipboard gaps remain separate; this does not prove non-systemd delivery
    or broader compositor coverage.
- **Open macOS proof:** `ven-prove-venus-apple-silicon-macos-nm7` must record
  exact hardware, OS, toolchain, artifacts, automated checks, interactive
  observations, failures, and limitations.

## VEN-C17 — Caller-owned native application identity

- **Status:** Proved
- **Consumer:** Eon, EonTerm, or one approved EonTerm composition.
- **Trigger:** The caller supplies one optional validated application ID before
  Venus creates its window.
- **Result:** The accepted value becomes winit's Wayland general name and native
  xdg-toplevel app ID; omission remains `eon`; Orbit-authored titles remain
  independent presentation state.
- **Important failures:** Empty, non-UTF-8, oversized, or non-token values fail
  before window creation; Venus never infers identity from child argv,
  environment, titles, executable names, runtime paths, or desktop files.
- **Owner:** The caller owns identity choice and desktop metadata; Venus owns
  direct CLI validation and native Wayland materialization.
- **Consumes:** Exact winit 0.30.13 commit
  `fb45fbf901fbe70cc9a877b5d651d0b60c206b08`; Eon owns the selected value.
- **Boundary:** Native mapping only; no branding framework or mutable identity.
- **Proof:** `ab24961bd6b2f9403736e52ebbac8cc266488a41`
  - **Environment:** x86_64 Linux Wayland with installed Eonova acceptance
  - **Evidence:** Locked format/check/test/Clippy and exact live
    `--application-id eonova` grouping without Session duplication

## VEN-C18 — Shared popup and picker-first presentation

- **Status:** Proved for the v5 Venus consumer; full-Eon activation pending
- **Consumer:** One person using a Venus workspace supplied through EONW v5.
- **Trigger:** The active tab selects a popup, its selection changes, or the
  person invokes an Eon-supplied catalog shortcut.
- **Result:** Workspace-only startup waits for a canonical snapshot before
  attaching its sole terminal endpoint. A selected popup replaces the entire
  pane stack while tabs remain visible and actionable. Each inactive tab's
  selection and hidden popup instances remain Eon-owned. A popup-only tab may
  have an empty body: no placeholder pane, attachment, resize failure, or alert.
  Its selected tab remains the only keyboard and accessibility focus target;
  reopening work restores terminal focus.
  A catalog shortcut names the exact tab, entry and current instance, with
  Toggle intent from terminal focus and Focus intent from workspace chrome.
  Structural repeats remain consumed without sending duplicate actions.
  Tab traversal and structural actions use the existing semantic workspace
  path; Eon decides their admission. F6 cycles visible focus regions, and
  Escape returns chrome focus to the terminal. While the terminal is focused,
  Escape, Ctrl+C, Tab and Enter reach its application through ordinary Orbit
  input. Popup entry labels also name the accessible tab panel.
- **Geometry:** A selected popup reuses the pane stack's exact outer rounded
  frame, so toggling preserves every chrome edge. Shared Eon-supplied logical
  margins inset the popup terminal content within that stable shell. A compact
  border label keeps a small top gutter before terminal padding. Scene owns
  both rectangles: chrome drives the outline, while terminal drives resize,
  clipping, pointer input, IME and accessibility. Margins and the title gutter
  shrink to preserve a usable cell grid; tiny surfaces omit the label. Exposed
  margins share the terminal default background and opacity, with the existing
  best-effort surface-wide compositor blur request. Pane-frame configuration
  has no effect on the popup outline.
- **Important failures:** Hidden or replaced endpoints retain no active
  presentation or input. Stale popup actions carry exact instance guards;
  canonical EONW validation and failure responses preserve the last coherent
  snapshot. Invalid frames, aliased endpoints, missing selections and malformed
  catalog values fail in the owner codec. Unavailable attachments retain the
  existing bounded recovery/failure path. Standalone Venus preserves raw
  workspace and catalog keys. Invalid or mixed launch forms fail before a
  window or transport starts.
- **Owner:** Venus owns native projection, shortcut dispatch and focus. Eon owns
  catalog, geometry settings, commands, cwd, popup/tab lifecycle, selection,
  action admission and cleanup. Orbit owns terminal Sessions and PTYs.
- **Consumes:** EONW v5 and `eon-workspace-protocol` 0.1.0 at exact Eon source
  `0cc8f477298681ae3945903e8fdb5852d487c5ab`; ORBF v2 / ORBS v11 at unchanged
  Orbit `ea9fd28ce0908f218cf65d4e6df368f0a4e565f5`.
- **Boundary:** No local command launch, chooser-mode recognition, cwd or
  dismissal policy, second schema, simultaneous terminals, compatibility
  negotiation, Eon runtime activation, or additional platform.
- **Proof:** `2d81d50d6decb4e3702e5cd7ff84e0a7c5642c00`.
  - **Environment:** x86_64 Linux, isolated Sway 1.12 scale 1 and Nix Mesa
    26.1.2 lavapipe; bounded canonical EONW v5 producer with real Orbit Sessions.
  - **Evidence:** Locked fmt/check/test/Clippy/build and the affected Nix build
    pass all 125 ordinary tests. Canonical projection, targeted shortcuts,
    AccessKit actions/focus, empty-body focus and failure-alert regressions pass.
    The native Nix artifact
    `/nix/store/88k1kx4ilckx3yk841zp2rlc21rb9i2j-venus-popup-proof-0.1.0`
    passes ordinary/Agent/Project switching, Focus/Toggle, application-owned
    Escape/Tab/Ctrl+C, normal/narrow/tiny geometry, replacement endpoints and
    supervised empty-body startup/reopen. The existing native application
    regression passes. Margins share background/opacity; final captures have
    no stale empty-body terminal or resize notice. All 12 observed live product
    process identities survive. Reproduction inputs, logs and captures:
    `~/.local/state/eon/proofs/ven-shared-popup-surface-ehk-2d81d50/REPORT.md`.
- **Open proof:** Full Eon command/lifetime and installed popup acceptance
  remain downstream in `eon-tool-popups-e13.2`.
  Native fractional scale, other compositors, actual compositor blur and
  screen-reader interaction remain qualified by their separately indexed proof.
  Previous v4 picker acceptance is retained in Git and
  `ven-consume-picker-first-eonw-v4-zd1`: Venus
  `e13970e90289d0d86f0adcbf350e4b9c1d5e5219` and Eon distinct-endpoint
  acceptance `4298fbb8868752e3d6c8eb4fd79fae067ab3e2a1` do not prove v5.

## Rules

- Each contract uses one `## VEN-CN — Name` heading and the required fields
  `Status`, `Consumer`, `Trigger`, `Result`, `Important failures`, `Owner`,
  `Boundary`, and `Proof`.
- Add optional `Consumes` and `Open proof` fields when applicable; nest
  `Environment` and `Evidence` under `Proof`, and use nested bullets instead of
  prose table cells.
- Contract IDs are stable and repository-qualified. Never renumber or reuse an
  ID; mark an explicitly removed contract retired.
- Only current user-visible behavior, correctness boundaries, ownership
  invariants, and cross-repository interfaces belong here.
- Every implementation Bead names contracts it changes, proves, consumes,
  hardens, or preserves and maps consumed Orbit contracts to exact revisions.
- Historical execution evidence belongs in Beads and Git, not this current-state
  index. User-visible chronology belongs in `CHANGELOG.md`.
- Later work touching a proved owner reruns its indexed checks and advances the
  proof revision or records the remaining gap.
