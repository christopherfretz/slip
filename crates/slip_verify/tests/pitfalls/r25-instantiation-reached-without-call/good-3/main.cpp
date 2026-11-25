struct X {
  static int f(int) { return 1; }
  static int g(long) { return 2; }
  int mf(int) { return 1; }
  int mf(long) { return 2; }
};
template <class T> int c(T) {
  auto l = [](int k = T::g(1L)) { return k; };
  return l();
}
int w() { return c(X()); }
int main() { return w() == 2 ? 0 : 1; }
