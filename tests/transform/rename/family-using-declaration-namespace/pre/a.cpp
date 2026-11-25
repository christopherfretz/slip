namespace a { int f(int x) { return x; } }
namespace b {
using a::f;
int u() { return f(1); }
}
int w() { return b::f(2); }
