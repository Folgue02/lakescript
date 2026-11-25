class Point:
    def __init__(self, x, y): self.x = x; self.y = y
    def plus(self, o): return Point(self.x + o.x, self.y + o.y)
    def __eq__(self, o): return self.x == o.x and self.y == o.y
p = Point(0, 0); step = Point(1, 1); same = 0
for i in range(1_000_000):
    p = p.plus(step)
    if p == Point(i + 1, i + 1): same += 1
print(same)
