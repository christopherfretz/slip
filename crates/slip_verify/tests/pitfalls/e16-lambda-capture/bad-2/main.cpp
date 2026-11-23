int f() {
  int n = 1;
  auto get = [n] { return n; };
  n = 5;
  return get();
}
