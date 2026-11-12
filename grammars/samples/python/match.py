match = {"case": 1, "type": 2}
type = match["type"]
_ = case = 3


def evaluate(command):
    match command.split():
        case [action]:
            return action
        case ["go", direction] | ["move", direction] if direction in DIRECTIONS:
            return direction
        case ["drop", *objects]:
            return objects
        case Point(x=0, y=0) as origin:
            return origin
        case Color.RED | Color.GREEN:
            return "warm"
        case {"x": x, "y": y, **rest}:
            return x, y, rest
        case (1, 2) | -3 | +4 | 1.5 + 2j | "text" | None | True:
            return "literal"
        case (inner, *_):
            return inner
        case _:
            return None


match (a, b):
    case (0, _): pass
