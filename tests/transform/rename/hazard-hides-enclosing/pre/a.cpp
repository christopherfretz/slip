int g(int x) { return x; }
namespace a {
int f(long x) { return (int)x + 1; }
int u() { return g(1); }
int k(int y = g(2)) { return y; }
}
int w() { return a::f(3L) + a::k(); }
