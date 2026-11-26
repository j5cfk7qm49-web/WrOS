# Bevelled W r O S tiles in Chicago (direction for rc32)

Status: **direction agreed with the owner, 2026-11-25. Not scheduled yet.** It's meant for rc32, together with the PDF reader's pixel image layer.
rc30 ships the flat, boxed tiles from [the rc30 layout](2026-11-25-rc30-interface-layout-design.md). rc31 makes them two cells (16 pixels) a tile: a 14×14 square with 2-pixel gaps and the letter raised a pixel, so it clears the bottom line (owner, 2026-11-26). The bevelled 15×15 squares below were chosen 4 pixels apart; in 16-pixel slots they would be 1 pixel apart, so rc32 has to settle the spacing.

## Why later

A character cell has two colours. A real bevel needs four: the face, a highlight along the top and left, a shadow along the bottom and right, and the letter. Every cell-based attempt therefore looked like brackets or like flat blocks.

The native session's renderer (`wros-graphics`) can paint pixels after the text is drawn, as the start-up logo is painted. rc32's PDF reader needs exactly that layer for its page images, so the tiles become a small extra once the layer exists:
- The same widget can use sixel in Foot through `ratatui-image`.
- The plain console shows `W r O S`: since rc31 every tile is two cells, a letter and a space, in the console as in the native session.

## What the owner chose

- **Font: Chicago.** It comes from ChicagoFLF by Robin Casady. The font's own copyright and licence fields say: "You are free to use them as you wish. I consider those two in the public domain."
  - The menus use ChicagoFLF as it is, proportional, at its 12-pixel design size, unsmoothed.
  - The tile letters W, r, O and S are hand-set on Chicago's 12-pixel grid: 9-pixel capitals, 2-pixel stems, one shared baseline. ChicagoFLF's outlines land a pixel off at 12 pixels, which is why the letters are redrawn.
  - Credit Robin Casady in WrOS's notices.
  - Fonts considered and turned down:
    - ProFont (MIT): the owner preferred Chicago.
    - ZX Chicago (DamienG): liked, but its licence forbids "redistributing this font as a font".
    - A fixed-width Chicago made by WrOS: the narrow letters got too much space around them.
- **Tiles: equal 15×15 squares**, 4 pixels apart (`img/bevelled-tiles/e-centred.png`). The size is 15, not 16, because a bevel's highlight reads as part of the face and its shadow as the edge. That leaves 14 visible pixels, which centre an 8-pixel letter exactly. **Each letter is centred on the face you see:** away from the shadow side, which is the bottom and right when raised and the top and left when pressed.
  - The face is Platinum grey (`#CCCCCC`), with a near-white highlight and a mid-grey shadow.
  - The current tile is the accent colour and pressed: the highlight and shadow are swapped, and the letter moves one pixel down and to the right.
  - When cards are due, the S is red.
  - Letters never touch the bevel.
- **The W: two finalists** (`img/bevelled-tiles/w-finalists.png`). Both have three 2-pixel stems and ZX Chicago's notched bottom-right corner, where the base stops one pixel short:
  - **D:** 2-pixel gaps, 10 wide. Its rows are `##..##..##` repeated eight times, then `#########.`.
  - **E:** 1-pixel gaps, 8 wide. Its rows are `##.##.##` repeated eight times, then `#######.`.
  - Turned down: a 1-pixel middle stem, a 1-pixel third stem, the half-tone stem, and a cut bottom-left corner.

## Still open

1. ~~W: D or E~~: **E** (the owner, 2026-11-25).
2. Bevel only, or bevel with the 1-pixel dark Platinum outline (`img/bevelled-tiles/chicago-tiles.png`, both rows).
3. Whether the whole menu alphabet gets hand-tuned on the 12-pixel grid, as the four tile letters were.

## Mock-ups

- `img/bevelled-tiles/chicago-tiles.png`: the bar with Chicago tiles and menus, at actual size and at 3×.
- `img/bevelled-tiles/w-finalists.png`: W candidates D and E, at 8×, and at actual size and 2×, raised and pressed.
- `img/bevelled-tiles/bevel-styles.png`: the Window Maker bevel compared with the Platinum outline.

The scripts that drew them are in `rc30-work/bevel/` (`chicago2.py`, `wtwo.py`), outside the repository.
