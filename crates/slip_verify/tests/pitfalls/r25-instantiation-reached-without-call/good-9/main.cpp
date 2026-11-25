struct X {
  static int f(int) { return 1; }
  static int f(long) { return 2; }
  int mi(int) { return 1; }
  int mf(long) { return 2; }
};
int w() {
  auto l = [](auto u) { return u.mf(1L); };
  int (*p)(X) = l;
  return p(X());
}
int main() { return w() == 2 ? 0 : 1; }
