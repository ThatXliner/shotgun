# site

The Shotgun landing page. Published at <https://bryanhu.com/shotgun/>.

## Building

`index.html` is generated. Do not edit it by hand — your changes will be
overwritten on the next build.

```sh
python3 site/build.py
```

`build.py` concatenates the pieces listed in `src/order.json`. Each piece owns
one directory under `src/pieces/<name>/`:

| file          | goes where                       |
| ------------- | -------------------------------- |
| `style.css`   | the page's single `<style>` block |
| `markup.html` | the `<body>`, in order            |
| `head.html`   | the `<head>` (font links)         |

Shared parts live in `src/head.html` and `src/tail.html`. Splitting the page
this way means several people (or agents) can work on different sections
without touching the same file.

Order matters. `substrate`, `type` and `ornament` define the design tokens,
`layout` sets the shared container, rhythm and panel treatment, `motion` adds
reveal and hover behaviour, then the content sections follow.

## Copy

All wording lives in `src/copy.html` and is mirrored into the section markup.
It is written to a numeric bar, checked by:

```sh
python3 site/copycheck.py            # the built page
python3 site/copycheck.py --file src/copy.html
```

The gate: hero under 20 words, no sentence over 25 words, Flesch-Kincaid grade
at most 8.0, and every heading a flat statement. CI runs this on every push, so
a change that hurts readability fails the build rather than shipping quietly.

## Screenshots

```sh
node site/shoot.mjs --out shots/page.png --width 1440 --full
node site/shoot.mjs --out shots/hero.png --sel "#masthead" --width 1440
```

Requires `npm i playwright` and a local Chrome. It clips from a viewport
capture rather than using `elementHandle.screenshot()`, which silently drops
animated absolutely-positioned children.

## Deploying

`.github/workflows/pages.yml` builds the page, runs the copy gate, and
publishes `index.html` plus `assets/` to GitHub Pages on every push to `main`
that touches `site/`. The source directories are not published.
