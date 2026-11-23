int counter() {
  static int n = 0;
  return ++n;
}

int helper_a() { return 1; }
int helper_b() { return 2; }

int first = counter();
int second = counter();
