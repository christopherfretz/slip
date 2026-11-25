struct X {
  static int f(int) { return 1; }
  static int f(long) { return 2; }
  int mf(int) { return 1; }
  int mf(long) { return 2; }
};
template <class T> int c(T t) {
  auto l = [](auto u) { return u.mf(1L); };
  decltype(l(t)) x = 2;
  return x;
}
int w() { return c(X()); }
int main() { return w() == 2 ? 0 : 1; }
