# Sway Shell

The idea is that this will replace essentially all things that run after sway which are needed for a usable desktop, i.e.:
- Bar
- Notification Daemon
- Wob
- Other things that i might want
    - Music Player (replacing mpd)

## Current Problem: Text Rendering

### Current Solution

- The current solution is rendering SDFs on the CPU, but the problem is that there are weird artifacts.

### Alternative

- Encode the Bezier curve's points in a 2 Channel Texture, sampled using Nearest Neighbour sampling.
- Encode the type and offset of bezier curve in a Uniform per shape sequentially, ensuring that the curves for each shape are together
- Send the offset for the above buffer and number of curves in the instance data

- Send 3 offsets and len, for line, quadratic and cubics, in the instance


- In the fragment shader, for the pixel, calculate the winding number using the Dan Sunday's algorithm, essentially, for the pixel (x_p, y_p), find the intersections of all curves which intersects y = y_p, calculate the derivative at the intersection, if the derivative is positive, increment winding number, if the derivative is negative, decremenet winding number. If the winding number is 0, the point is outside, hence fill it with background color, if not, fill with foreground color

- To find the intersections, if the curve is P = B(t), then we need to convert it to implicit form, in the form of 0 = R(x, y), then find all possible values of x where y = y_p, i.e. zeroes of equation R(x, y_p) = 0
- R(x, y) would have 10 terms when solving for cubics, not ideal, however, for the function F(y_p) = x_p, Where x_p satisfies R(x_p, y_p) = 0, F(y) would be a cubic equation, which has a closed form solution
- if x_p < x, then discard this segment, since it is behind the current pixel, so it is irrelevant.
- Now we have a point (x_p, y_p), now we need to find a t, such that t = B^(-1)((x_p, y_p)), if t > 1.0 or t < 0.0, then this curve doesn't actually intersect the horizontal ray, so ignore this
- Now we can evaluate B'(t)

The idea is that the number of bezier curves is not going to be that much

This is going to be so much more expensive compared to SDFs, but atleast it will be correct

Sometime later, a proper implementation of msdf should be implemented, in theory, we should be able to cram the necessary data in 2 channels
