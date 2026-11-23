int limit() { return 10; }

int f(int x) {
  auto limit = [] { return 5; };
  return x + limit();
}
