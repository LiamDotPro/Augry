Icons: the master is `app-icon.html`, the Augry icon from the design project
(App Shell board 04): three glass panes on a plain dark tile, nothing else. It
has two variants, the glass one and, with `?small`, a flat one in solid colours
for 48px and below, where the glass turns muddy. Render it to the 2048px
`app-icon.png` with headless Chrome:

    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless=new \
      --hide-scrollbars --force-device-scale-factor=2 --default-background-color=00000000 \
      --window-size=1024,1024 --screenshot="$PWD/src-tauri/icons/app-icon.png" \
      "file://$PWD/src-tauri/icons/app-icon.html"

and the same with `app-icon.html?small` to a scratch `app-icon-small.png`.

Then regenerate the platform set with
`npm run tauri icon src-tauri/icons/app-icon.png -- -o src-tauri/icons`, drop the
generated `android/` and `ios/` folders and `icon.icns` (not shipped), and redraw
the small sizes from the flat render: `32x32.png`, `Square30x30Logo.png`,
`Square44x44Logo.png`, and the 16, 24, 32 and 48px entries of `icon.ico` (64 and
256 stay glass).

The title bar mark in `src/components/Chrome.tsx` is a hand-drawn 15px version
of the flat variant; keep the two in step.
