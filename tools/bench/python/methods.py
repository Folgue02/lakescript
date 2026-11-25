class Counter:
    def __init__(self): self.n = 0
    def add(self, by): self.n += by
    def total(self): return self.n
c = Counter()
for i in range(2_000_000): c.add(1)
print(c.total())
