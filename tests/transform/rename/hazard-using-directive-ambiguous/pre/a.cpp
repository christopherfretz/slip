namespace a { int f(int x) { return x; } }
namespace b { int g(int x) { return x + 1; } }
namespace c {
using namespace a;
using namespace b;
int u() { return g(1) + f(2); }
}
