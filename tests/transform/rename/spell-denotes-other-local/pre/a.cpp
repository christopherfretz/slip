namespace a {
int f(int x) { return x; }
int u() {
  auto g = [](int x) { return x + 1; };
  return f(1) + g(2);
}
}
