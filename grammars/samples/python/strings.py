plain = "double" 'single' """triple
double""" '''triple
single''' r"raw \d" u"unicode"
data = b"bytes" Rb'raw bytes' rb"\x00"
escaped = "tab\t quote\" newline\n \N{EM DASH} € backslash\\ \
continued"
name = "fastpeg"
width, precision = 10, 3
simple = f"hello {name}!"
nested = f"{f"{f"{name}"}"}"
spec = f"{3.14159:{width}.{precision}f} {name!r:>{width}} {name=} {name = !s:^20}"
braces = f"{{literal}} {name}"
expression = f"{', '.join(str(n) for n in range(3))} {name['key'] if False else 0}"
named = f"\N{GREEK SMALL LETTER ALPHA} {name}"
raw = rf"\d{name}\N{name}" fr'{name}\w'
lone = f"\{name}"
multiline = f"""first {name}
second {
    name  # a comment inside a replacement field
}"""
template = t"Hello {name}" tr"\{name}"
conversion = f"{name!a} {name!s}"
dictionary = f"{ {'a': 1}['a'] }"
lambda_inside = f"{(lambda x: x * 2)(3)}"
