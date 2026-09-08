# Patches

These are [ingen](https://gitlab.com/drobilla/ingen) example patches that use
LV2 plugins. If you're trying to flash a patch to the Pico you'll need one of
these, or something like it.

The "patches" are basically combinations of one or more LV2 plugins, stuffed in
a directory called something like "mypatch.ingen".

The patch just describes how the plugins are wired together, you'll also need
the plugins themselves installed. The desktop versions should go under your
`$LV2_PATH` environment variable, the pico versions under `$PICOLV2_PATH`. If
the plugins are installed in the right place [picolv2-image](../picolv2-image/)
should find them automatically and include them in the image.

## The actual patches

As yet all I have is a couple of AlsaModularSynth patches, the idea is I'll be
adding more. There's also the [graphs](../graphs/) folder with a couple of
ad-hoc testing patches which should work but I'll be deleting them one I have
more "real" LV2 plugins working.
