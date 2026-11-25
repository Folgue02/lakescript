l = []
for i in range(2_000_000): l.append(i)
s = 0
for i in range(len(l)): s += l[i]
print(s)
