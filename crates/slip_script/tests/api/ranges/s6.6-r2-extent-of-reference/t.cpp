#define ID(x) x
namespace n {
namespace m {
template <class T>
struct V {
  static int k;
};
int f();
}
}
int a = ::n::m::f();
int b = n::m::V<int>::k;
typename n::m::V<long> c;
using namespace n::m;
int d = ID(f)();
int e = f();
struct S {
  S operator+(S) const;
};
int k2(S);
int g(S s, int caf\u00e9) {
  return k2(s.operator+(s)) + caf\u00e9;
}
