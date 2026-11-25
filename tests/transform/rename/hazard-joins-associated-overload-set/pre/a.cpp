namespace a { struct T {}; }
namespace b {
struct S {};
int g(S, const a::T &) { return 1; }
}
namespace a { int f(b::S, T &) { return 2; } }
int u() {
  b::S s;
  a::T t;
  return g(s, t) + a::f(s, t);
}
