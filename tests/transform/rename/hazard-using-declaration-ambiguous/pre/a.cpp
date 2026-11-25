namespace a { int f(int x) { return x; } }
namespace b { int g(int x) { return x + 1; } }
namespace c {
using a::f;
using b::g;
int u() { return g(1) + f(2); }
}
