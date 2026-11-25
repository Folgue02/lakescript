class Point:
    def __init__(self, x, y): self.x = x; self.y = y
p = Point(1, 2)
s = 0
for i in range(3_000_000): s += p.x
print(s)
