def collatz(limit):
    steps = 0; n = 1
    while n < limit:
        x = n
        while x != 1:
            x = x // 2 if x % 2 == 0 else 3 * x + 1
            steps += 1
        n += 1
    return steps
print(collatz(60_000))
