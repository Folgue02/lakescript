class Grid:
    def __init__(self, w, h): self.width = w; self.height = h; self.cells = [0] * (w * h)
    def get(self, x, y): return self.cells[y * self.width + x]
    def set(self, x, y, v): self.cells[y * self.width + x] = v
g = Grid(100, 100)
for p in range(200):
    for y in range(g.height):
        for x in range(g.width):
            g.set(x, y, g.get(x, y) + 1)
print(g.get(5, 5))
