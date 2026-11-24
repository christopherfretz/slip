namespace a {
struct C { int m(int x) { return x; } };
}
int (a::C::*pm)(int) = &a::C::m;
int u(a::C &c) { return (c.*pm)(1); }
namespace a {
auto q = &C::m;
}
