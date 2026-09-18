# External plugins folder

A place for "external" LV2 plugins. For the time being these are just likely to
be the ones I've ported. The hope is that eventually there will be genuine
third-party plugins that support PicoLV2

The Makefile fetches the configured repositories and delegates each repository's
Pico and desktop build to its own build recipe.

I haven't tested these much, but I can 100% promise you some of them won't work.

## What's done, what does and doesn't work

- LibreArp - basically useless as the gui doesn't work in either ingen or MOD
  Pedalboard, and it needs a gui to be useful. Maybe the pico part would work
  but I haven't tested it. Leaving it here for now because it'd be nice to get
  it working properly at some point.
