class Cell:
    def __init__(self, x, y, glyph='.', seen=False, items=None):
        self.x = x; self.y = y; self.glyph = glyph; self.seen = seen; self.items = [] if items is None else items
seen = 0
for i in range(1_000_000):
    c = Cell(i, i)
    if not c.seen: seen += 1
print(seen)
