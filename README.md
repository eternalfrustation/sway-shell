# Sway Shell

The idea is that this will replace essentially all things that run after sway which are needed for a usable desktop, i.e.:
- Bar
- Notification Daemon
- Wob
- Other things that i might want
    - Music Player (replacing mpd)

## Current Problem: Text Rendering

### Current Solution

- The current solution is sending the bezier data to the GPU and rendering them there, I personally don't like this because of repeated work that is being done

### Alternative

- Render mSDF on the CPU, update/replace the texture when new glyphs are introduced and send the rendered texture to the GPU along with texture coordinates to the GPU, Will need a way to distinguish between doing text rendering and just drawing boxes
