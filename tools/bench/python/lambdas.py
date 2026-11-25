numbers = list(range(1_000_000))
even = list(filter(lambda n: n % 3 == 0, map(lambda n: n * 2, numbers)))
print(len(even))
