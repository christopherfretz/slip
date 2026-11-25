namespace a { int f(int x) { return x; } }
namespace b {
using a::f;
int f(long x) { return (int)x; }
}
namespace c { using b::f; }
int u() { return c::f(1) + c::f(2L); }
