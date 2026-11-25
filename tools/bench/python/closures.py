total = 0
def add(n):
    global total
    total += n
for i in range(2_000_000): add(i)
print(total)
