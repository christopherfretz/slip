namespace a { int g(int x) { return x; } }
namespace b {
using a::g;
int u() { return g(1); }
}
int w() { return b::g(2); }
