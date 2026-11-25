namespace b {
struct S {};
int f(S) { return 2; }
}
namespace a {
int g(int x) { return x; }
int u() {
  b::S s;
  return f(s) + g(1);
}
}
