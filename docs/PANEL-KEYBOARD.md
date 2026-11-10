# Keyboard panel sizing

🟢 [Done] macOS native app, 2026-11-10.

From the toolbar or a panel WebView, press **Cmd+Option+Left / Right / Down** to focus the corresponding dock boundary. Ctrl+Alt is also accepted by the UI. Hidden boundaries do not take focus. The shortcut can leave an inspector input without changing its value. Clicking a boundary also focuses it.

- Left/right boundaries: Left and Right move the boundary in that direction.
- Bottom boundary: Up increases panel height; Down decreases it.
- Arrows change by 10 logical points; Shift changes by 50.
- Home selects the minimum; End selects the current window's maximum.

Rust serializes keyboard changes against active layout gestures, enforces limits and saves through the existing layout store. A reported save failure restores the previous in-memory dimensions. The normal minimum is 180 pt for side panels and 120 pt for the bottom. Maximum dimensions are 700 pt, additionally limited to 35% of window width or half its height. Existing saved larger dimensions can still be visually clamped on a smaller window.

The WebView only sends key commands and displays Rust's confirmed dimensions. Initial state and changes from pointer drag/window resize update separator accessibility values; notifications and subscriptions target the individual splitter WebView. Labels and shortcut hints support English, Japanese and Simplified Chinese. Keyboard focus has a visible highlight. Tab focuses the separator within its child WebView; use the focus shortcuts to move between native child WebViews rather than relying on a single cross-WebView tab sequence.

## Native verification

An isolated default circuit/workspace was used. The macOS app verified:

- Left boundary 310→360 with Shift+Right; Home→180; End→504 in a 1440 pt window.
- Right boundary 300→310 with Left, then 260 with Shift+Right.
- Bottom 230→240→290 with Up/Shift+Up; Home→120; End→450; Shift+Down→400.
- Cmd+Option focus moves from toolbar to left, then right and bottom; arrow adjustments affect only the corresponding accessibility value.
- Pointer drag bottom 400→300, followed by keyboard Up→310; independent left/right values remain 240/270.
- Quit/relaunch restores **240 / 270 / 310 pt**, including the accessibility values.
- The 320-vector ALU check passes; inspector floating and right re-docking preserve dimensions.

The new flow was exercised in Japanese. Actual VoiceOver speech, Windows/Linux keyboard mapping and mixed-DPI/multi-monitor interaction are ⭕️ [Pending].
