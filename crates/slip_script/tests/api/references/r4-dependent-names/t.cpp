namespace n {
int f(int);
struct S {
  int m;
  operator int() const { return m; }
};
template <class T>
int t(T x) {
  return f(x) + x.m;
}
int y = t(S{});
}
