# A comment on the first line, then blank lines and comments in odd places.

  # an indented comment before any statement

def outer(a,
          b):  # brackets join lines
    total = (a +
             b)
# a comment at column 0 inside the block

    def inner():
        return total ; pass

    if total: return inner
    while False: pass
    return [
        1,
        2,
    ]


class Empty: ...


x = 1 \
    + 2
\

y = 3
if x:
    pass
elif x > 1:  # trailing comment
    pass
else :
    pass
values = 1..__class__, 1.e5, 0_0, 0xdead_beef, 1if x else 2
a[x:=1]
{y:=2}
f"{x:=10}"
if x:
    a = 1
    b = 2  # a form feed starts the line, and columns restart
# a final comment without a line break