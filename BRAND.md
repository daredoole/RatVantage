# Brand Notes

## 5 possible project names

1. **Legion Control**
2. **Legion Deck**
3. **VantageX Linux**
4. **ForgeFan**
5. **Legion Pilot**

Recommended working name: **Legion Control**.

Why: direct, searchable, not too cute, and clear about what the app does. Avoid using Lenovo trademarks in a way that implies official affiliation.

## 5 taglines

1. “Safe Fedora controls for Lenovo Legion laptops.”
2. “Native Linux tuning for Legion power, fans, and battery modes.”
3. “A Fedora-first dashboard for Legion hardware controls.”
4. “Probe-driven, polkit-gated Legion control.”
5. “Linux hardware controls without root GUIs.”

## Visual direction

- Dark, technical, minimal.
- Fedora/GNOME-friendly spacing and typography.
- Use libadwaita cards and rows rather than dense custom widgets.
- Avoid gamer clutter in the main UI.
- Use risk badges for advanced controls:
  - Safe
  - Needs confirmation
  - Reboot required
  - Experimental
  - Unsupported

Suggested palette direction:

- Neutral base matching Adwaita.
- Accent inspired by Lenovo Legion red, but not overpowering.
- Temperature and fan charts should use accessible contrast and avoid alarm colors except for true warnings.

## App icon ideas

1. Abstract laptop outline with a small fan rotor.
2. Shield plus fan blades, emphasizing safety.
3. Minimal “L” monogram with airflow lines.
4. Dashboard gauge with a small lightning bolt.
5. Hexagonal control badge with a profile LED dot.

Icon requirements:

- Must work as a symbolic tray icon.
- Must still read at 16 px.
- Avoid official Lenovo logos.
- Avoid copying the Legion “Y” mark.

## Tone of voice

Direct, technical, and honest.

Use:

- “Detected on this boot.”
- “Probe-only.”
- “Requires reboot.”
- “Hidden because this path is not exposed.”
- “Experimental; read-back required.”

Avoid:

- “Fully supports all Legion laptops.”
- “Safe overclocking.”
- “Lenovo Vantage replacement” as a promise.
- “One-click max performance” marketing.
- Corporate filler.

## GitHub repo description

Short description:

```text
Fedora-native, probe-driven control dashboard for Lenovo Legion laptop profiles, fans, battery modes, LEDs, and GPU switching.
```

Longer description:

```text
Legion Control is a Fedora-first GTK/libadwaita dashboard and optional tray/status tool for Lenovo Legion laptops. It uses a privileged D-Bus daemon, polkit authorization, runtime capability probing, and strict validation instead of root GUIs or arbitrary sysfs writes.
```

## Buy Me a Coffee blurb

```text
I’m building Legion Control to make Lenovo Legion laptops safer and nicer to use on Fedora/Linux: native UI, no root GUI, runtime hardware probing, fan presets, battery modes, profile switching, and clear safety boundaries. If this saves you time or helps your laptop run better, you can support development here.
```

## README badge ideas

- Fedora 43 target
- Rust
- GTK4/libadwaita
- D-Bus
- polkit
- pre-alpha

## Naming cautions

- Do not imply the project is official Lenovo software.
- Do not use Lenovo or Legion logos.
- If the name includes “Legion,” include a disclaimer:

```text
This project is not affiliated with or endorsed by Lenovo.
```
