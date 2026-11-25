struct X {
  static int f(int) { return 1; }
  static int f(long) { return 2; }
  int mf(int) { return 1; }
  int mg(long) { return 2; }
};
template <class T> int c(T t) {
  auto l = [](auto u) { return u.mg(1L); };
  int (*p)(T) = l;
  return p(t);
}
int w() { return c(X()); }
int main() { return w() == 2 ? 0 : 1; }
