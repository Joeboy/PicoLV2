# Running ams-lv2 / ingen patches on the Pico

The idea is it should be possible to create modular synths using
[ingen](https://gitlab.com/drobilla/ingen) + ams-lv2 plugins and run them on the
pico. A couple of simple, working patches are included.

To use the ams-lv2 plugins you need
[this repo / branch](https://github.com/Joeboy/ams-lv2/tree/picolv2-build).

This is a proof of concept / work in progress! I just got as far as getting the
two included patches working, very likely there will be problems if you try to
do anything much else.

That said, if you want to try building cool patches and send them to me, maybe I
can fix stuff as necessary and include your patch here. I'm not really a modular
synth person, it'd be great if somebody could do that. Be aware that the Pico is
a humble microcontroller so you'll have to keep it pretty simple.
