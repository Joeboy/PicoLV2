# Patches

These are [ingen](https://gitlab.com/drobilla/ingen) example patches that use
LV2 plugins. If you're trying to flash plugins to the Pico you'll need one of
these, or something like it.

"Patches" are basically combinations of one or more plugins, stuffed in a folder
called something like "mypatch.ingen/". A patch could be as simple as a single
plugin wired to the input and output, or it could be a complex effects chain or
modular synth.

The patch just describes how the plugins are wired together, you'll also need
the plugins themselves installed. The desktop versions should go under your
`$LV2_PATH` environment variable, the pico versions under `$PICOLV2_PATH`. If
the plugins are installed in the right place [picolv2-image](../picolv2-image/)
should find them automatically and include them in the image.

At the moment there are only synth patches. I'll add effects patches once the
hardware supports audio input.
