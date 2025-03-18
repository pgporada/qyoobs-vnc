# qyoobs-vnc

Opens a VNC viewer to another Qube. This is useful for facilitating screensharing of another qube
through Google Meet, Zoom, etc.

Install `qyoobs-vnc` to your source and target qubes, preferably their templates.
```bash
```

In Dom0, apply a policy that permits `qyoobs-vnc` to work.
```bash
# Configure this policy to your taste.
qyoobs.VNC * @anyvm @anyvm ask
```

Then, run `qyoobs-vnc connect`. Let's try connecting to the `personal` VM.
```bash
qyoobs-vnc connect personal
```

You'll get a dom0 prompt to allow the operation. Allow it, then a VNC window with the contents of
the target qube should appear.

Maximize and drag the window to another workspace. You should then be able to screenshare the VNC
window from your source qube.

If you have multiple monitors, and want to limit the capture to a single monitor, use the `--window`
option. Identify your monitors using `xrandr`.
```
qyoobs-vnc connect personal --screen 1
```

## Alternatives

### Manually

This is basically a wrapper around https://www.d10.dev/blog/qubes-vnc-screenshare/. You can follow
those steps instead, if you want.

### qubes-video-companion

If [qubes-video-companion](https://github.com/QubesOS/qubes-video-companion) works for you, you
should use that instead. I found it consumed a lot of CPU, and having the screen shared through
a camera device is kind of awkward.

Maybe I could have improved that instead, but I don't like writing Python ;)
