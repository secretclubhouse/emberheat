"""Read and write KiCad s-expressions.

A tree is nested lists. Bare atoms are `str`; quoted strings are `Q`, a `str`
subclass holding the text with escapes resolved, so writing it back restores
the quotes. Standard library only.
"""


class Q(str):
    """A quoted string atom."""


_ESC_IN = {"\\": "\\", '"': '"', "n": "\n"}
_ESC_OUT = {"\\": "\\\\", '"': '\\"', "\n": "\\n"}


def loads(text):
    """Parse one s-expression and return it as nested lists."""
    stack = [[]]
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if c in " \t\r\n":
            i += 1
        elif c == "(":
            stack.append([])
            i += 1
        elif c == ")":
            if len(stack) < 2:
                raise ValueError("unbalanced ')' at offset %d" % i)
            done = stack.pop()
            stack[-1].append(done)
            i += 1
        elif c == '"':
            i += 1
            buf = []
            while True:
                if i >= n:
                    raise ValueError("unterminated string")
                c = text[i]
                if c == "\\" and i + 1 < n:
                    nxt = text[i + 1]
                    if nxt in _ESC_IN:
                        buf.append(_ESC_IN[nxt])
                    else:
                        buf.append(c + nxt)
                    i += 2
                elif c == '"':
                    i += 1
                    break
                else:
                    buf.append(c)
                    i += 1
            stack[-1].append(Q("".join(buf)))
        else:
            j = i
            while j < n and text[j] not in " \t\r\n()":
                j += 1
            stack[-1].append(text[i:j])
            i = j
    if len(stack) != 1:
        raise ValueError("unbalanced '(': %d unclosed" % (len(stack) - 1))
    if len(stack[0]) != 1:
        raise ValueError("expected one top-level expression, found %d" % len(stack[0]))
    return stack[0][0]


def _atom(x):
    if isinstance(x, Q):
        return '"' + "".join(_ESC_OUT.get(ch, ch) for ch in x) + '"'
    return str(x)


def _inline(x):
    if isinstance(x, list):
        return "(" + " ".join(_inline(y) for y in x) + ")"
    return _atom(x)


def dumps(tree, indent=0, width=88):
    """Write a tree as KiCad-style text: short lists on one line, longer ones
    with the leading atoms on the first line and each child list indented."""
    if not isinstance(tree, list):
        return _atom(tree)
    flat = _inline(tree)
    if len(flat) + indent * 2 <= width and not any(isinstance(y, list) and any(isinstance(z, list) for z in y) for y in tree):
        return flat
    head = []
    rest = []
    for y in tree:
        if not rest and not isinstance(y, list):
            head.append(_atom(y))
        else:
            rest.append(y)
    pad = "\t" * (indent + 1)
    lines = ["(" + " ".join(head)]
    for y in rest:
        lines.append(pad + dumps(y, indent + 1, width))
    return "\n".join(lines) + "\n" + "\t" * indent + ")"


def shape(tree):
    """Comparable form of a tree that keeps the quoted / bare distinction."""
    if isinstance(tree, list):
        return [shape(y) for y in tree]
    return ("q", str(tree)) if isinstance(tree, Q) else ("a", str(tree))
