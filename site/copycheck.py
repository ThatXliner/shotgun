#!/usr/bin/env python3
"""Check the assembled page's copy against the hard numbers.

  python3 copycheck.py

Rules enforced:
  * hero (the h1 plus its immediate lead paragraph) is under 20 words
  * no sentence anywhere on the page runs over 25 words
  * Flesch-Kincaid grade level of the body copy is at most 8.0
  * every heading is a flat statement: no question marks, no colons splicing
    two clauses, no trailing ellipsis, no marketing exclamation

Exits non-zero and lists every violation. This is a gate, not advice.
"""
import pathlib
import re
import sys
from html.parser import HTMLParser

ROOT = pathlib.Path(__file__).resolve().parent
MAX_HERO_WORDS = 20
MAX_SENTENCE_WORDS = 25
MAX_GRADE = 8.0

SKIP_TAGS = {"script", "style", "pre", "code", "kbd", "samp"}
HEADING_TAGS = {"h1", "h2", "h3", "h4", "h5", "h6"}


class Extract(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.stack = []
        self.blocks = []          # (tag, text) for every text-bearing block
        self._buf = []
        self._tag = None

    def handle_starttag(self, tag, attrs):
        self.stack.append(tag)
        if tag == "br":
            self._buf.append(" ")

    def handle_endtag(self, tag):
        if self.stack and tag in self.stack:
            while self.stack and self.stack.pop() != tag:
                pass
        if tag == self._tag:
            self._flush()

    def handle_data(self, data):
        if any(t in SKIP_TAGS for t in self.stack):
            return
        cur = self.stack[-1] if self.stack else None
        holder = None
        for t in reversed(self.stack):
            if t in HEADING_TAGS or t in ("p", "li", "figcaption", "blockquote", "dt", "dd"):
                holder = t
                break
        if holder is None:
            holder = cur or "text"
        if holder != self._tag:
            self._flush()
            self._tag = holder
        self._buf.append(data)

    def _flush(self):
        txt = re.sub(r"\s+", " ", "".join(self._buf)).strip()
        if txt:
            self.blocks.append((self._tag, txt))
        self._buf, self._tag = [], None

    def close(self):
        super().close()
        self._flush()


def words(s):
    return re.findall(r"[A-Za-z0-9][A-Za-z0-9'’./_-]*", s)


def syllables(w):
    w = re.sub(r"[^a-z]", "", w.lower())
    if not w:
        return 1
    if len(w) <= 3:
        return 1
    w = re.sub(r"(?:[^laeiouy]es|ed|[^laeiouy]e)$", "", w)
    w = re.sub(r"^y", "", w)
    n = len(re.findall(r"[aeiouy]{1,2}", w))
    return max(1, n)


def sentences(text):
    parts = re.split(r"(?<=[.!?])\s+(?=[A-Z0-9“‘])", text)
    return [p.strip() for p in parts if p.strip()]


def main():
    target = ROOT / "index.html"
    if "--file" in sys.argv:
        target = pathlib.Path(sys.argv[sys.argv.index("--file") + 1])
        if not target.is_absolute():
            target = ROOT / target
    print("checking %s" % target)
    html = target.read_text(encoding="utf-8")
    p = Extract()
    p.feed(html)
    p.close()

    problems = []

    # --- hero ---
    h1 = next((t for tag, t in p.blocks if tag == "h1"), None)
    if h1 is None:
        problems.append("no h1 found")
    else:
        idx = next(i for i, (tag, _) in enumerate(p.blocks) if tag == "h1")
        lead = next((t for tag, t in p.blocks[idx + 1: idx + 4] if tag == "p"), "")
        n = len(words(h1)) + len(words(lead))
        if n >= MAX_HERO_WORDS:
            problems.append(
                "hero is %d words, must be under %d\n      h1:   %s\n      lead: %s"
                % (n, MAX_HERO_WORDS, h1, lead))

    # --- sentence length ---
    for tag, text in p.blocks:
        for s in sentences(text):
            n = len(words(s))
            if n > MAX_SENTENCE_WORDS:
                problems.append("<%s> sentence is %d words (max %d): %s"
                                % (tag, n, MAX_SENTENCE_WORDS, s))

    # --- reading grade ---
    body = " ".join(t for tag, t in p.blocks if tag not in HEADING_TAGS)
    sents = sentences(body)
    ws = words(body)
    if sents and ws:
        syl = sum(syllables(w) for w in ws)
        grade = 0.39 * (len(ws) / len(sents)) + 11.8 * (syl / len(ws)) - 15.59
        if grade > MAX_GRADE:
            problems.append("Flesch-Kincaid grade %.2f, must be at most %.1f" % (grade, MAX_GRADE))
    else:
        grade = 0.0

    # --- flat headings ---
    for tag, text in p.blocks:
        if tag not in HEADING_TAGS:
            continue
        if "?" in text:
            problems.append("<%s> heading is a question: %s" % (tag, text))
        if "!" in text:
            problems.append("<%s> heading exclaims: %s" % (tag, text))
        if text.rstrip().endswith(("…", "...")):
            problems.append("<%s> heading trails off: %s" % (tag, text))
        if len(words(text)) > 12:
            problems.append("<%s> heading is %d words, keep headings short: %s"
                            % (tag, len(words(text)), text))

    print("blocks: %d   body words: %d   sentences: %d   grade: %.2f"
          % (len(p.blocks), len(ws), len(sents), grade))
    if problems:
        print("\n%d PROBLEM(S):" % len(problems))
        for x in problems:
            print("  - %s" % x)
        return 1
    print("\nOK — all copy constraints met.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
