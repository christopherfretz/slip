int f() {
  int n = 1;
  auto get = [=] { return n; };
  n = 5;
  return get();
}
