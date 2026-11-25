struct X {
  static int f(int) { return 1; }
  static int f(long) { return 2; }
  int mf(int) { return 1; }
  int mg(long) { return 2; }
};
int w() {
  auto l = [](auto u) { return u.mg(1L); };
  decltype(l(X())) x = 2;
  return x;
}
int main() { return w() == 2 ? 0 : 1; }
