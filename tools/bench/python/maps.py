counts = {}
for i in range(300_000):
    key = f"k{i % 1000}"
    counts[key] = counts.get(key, 0) + 1
print(len(counts))
