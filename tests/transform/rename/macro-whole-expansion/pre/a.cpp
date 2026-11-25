#define ID(x) x
namespace a {
int f(int x) { return x; }
int u() { return ID(f)(1); }
}
