## New palette concept

Let's define default palette colors:

0: Black  (0,0,0)
1: Blue
2: Green
3: Cyan
4: Red
5: Magenta
6: Brown / Dark Yellow
7: White / Light Gray
8: Bright Black / Gray
9: Bright Blue
10: Bright Green
11: Bright Cyan
12: Bright Red
13: Bright Magenta
14: Yellow     
15: Bright White (255,255,255)

So, the default palette has 16 predefined colors, the rest are free to use (0..254 in total + 255-th which is transparency encoded). When images are loaded from assets, new colors can be added to the palette. If there are no more color slots available, the palette should find some color that is similar to the existing colors in the palette. The game is also free to add any fixed colors that it wants to use.

