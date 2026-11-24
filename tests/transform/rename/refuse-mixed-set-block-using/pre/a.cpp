namespace a { int f(int x) { return x; } }
namespace b {
using a::f;
int f(long x) { return (int)x; }
}
int u() {
  using b::f;
  return f(1) + f(2L);
}
