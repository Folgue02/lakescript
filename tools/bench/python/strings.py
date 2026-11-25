total = 0
for i in range(300_000):
    line = f"item-{i},{i * 2},done"
    total += len(line.split(','))
print(total)
