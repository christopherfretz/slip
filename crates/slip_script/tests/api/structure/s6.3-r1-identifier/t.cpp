namespace n {
struct S {
  S& operator=(const S&) { return *this; }
  operator int() { return 0; }
  int m = 0;
};
namespace {
int u;
}
}
int k(int);
int k2(n::S&);
int use() {
  n::S a, b;
  return k(a.operator int()) + k2(a.operator=(b)) + a.m + n::u;
}
